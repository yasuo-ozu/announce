//! Rewrites the paths of a trait cut out of std's source to absolute public paths
//! (`::core::option::Option`), the way the mocks of announce-std write them.
//!
//! A name is resolved through, in this order: the generics in scope and `Self` (kept),
//! a primitive type (a single-segment path), the items of its module, the `use` items of
//! its file, `core`/`alloc`/`std`, and the std prelude. A name that none of these has is
//! an error. `crate::`, `self::` and `super::` are the paths of the file's crate and module.

use crate::config::{DROPPED_BOUNDS, PRELUDE, PRIMITIVES, PUBLIC_PATHS, SIZED_HIERARCHY};
use crate::source::{FileInfo, Res};
use std::collections::BTreeSet;
use syn::punctuated::Punctuated;
use syn::visit_mut::VisitMut;

pub struct Resolver<'a> {
    info: &'a FileInfo,
    scopes: Vec<BTreeSet<String>>,
    /// Extra prefix rewrites of the finished paths (`Variant`).
    rewrites: Vec<(String, String)>,
    pub errors: Vec<String>,
}

fn path_string(segs: &[String]) -> String {
    let mut s = String::new();
    for seg in segs {
        s.push_str("::");
        s.push_str(seg);
    }
    s
}

/// Replaces the longest of `table` that is a prefix of `path` (at a `::` boundary).
fn rewrite_prefix_once(path: &str, table: &[(String, String)]) -> String {
    let mut best: Option<&(String, String)> = None;
    for e in table {
        let boundary =
            path.len() == e.0.len() || path.get(e.0.len()..).is_some_and(|r| r.starts_with("::"));
        if path.starts_with(&e.0) && boundary && best.is_none_or(|b| e.0.len() > b.0.len()) {
            best = Some(e);
        }
    }
    match best {
        Some((from, to)) => format!("{}{}", to, &path[from.len()..]),
        None => path.to_string(),
    }
}

/// Rewrites until no entry applies (a private path may map to another private one).
fn rewrite_prefix(path: &str, table: &[(String, String)]) -> String {
    let mut p = path.to_string();
    for _ in 0..8 {
        let n = rewrite_prefix_once(&p, table);
        if n == p {
            return p;
        }
        p = n;
    }
    p
}

fn public_table() -> Vec<(String, String)> {
    PUBLIC_PATHS
        .iter()
        .map(|(a, b)| (a.to_string(), b.to_string()))
        .collect()
}

impl<'a> Resolver<'a> {
    pub fn new(info: &'a FileInfo, variant: Option<(&str, &str)>) -> Resolver<'a> {
        Resolver {
            info,
            scopes: Vec::new(),
            rewrites: variant
                .into_iter()
                .map(|(a, b)| (a.to_string(), b.to_string()))
                .collect(),
            errors: Vec::new(),
        }
    }

    /// Runs `f` with the generic parameters of `generics` in scope.
    pub fn with_generics<R>(
        &mut self,
        generics: &syn::Generics,
        f: impl FnOnce(&mut Self) -> R,
    ) -> R {
        let mut names = BTreeSet::new();
        for p in &generics.params {
            match p {
                syn::GenericParam::Type(t) => {
                    names.insert(t.ident.to_string());
                }
                syn::GenericParam::Const(c) => {
                    names.insert(c.ident.to_string());
                }
                syn::GenericParam::Lifetime(_) => {}
            }
        }
        self.scopes.push(names);
        let r = f(self);
        self.scopes.pop();
        r
    }

    fn in_scope(&self, name: &str) -> bool {
        name == "Self" || self.scopes.iter().any(|s| s.contains(name))
    }

    /// The absolute path (crate name first) that `segs` stands for in the file.
    fn resolve_names(&self, segs: &[String], depth: usize) -> Res<Vec<String>> {
        if depth > 16 {
            return Err(format!("`use` loop at {}", segs.join("::")));
        }
        let info = self.info;
        let first = segs[0].as_str();
        let rest = &segs[1..];
        let mut out: Vec<String> = Vec::new();
        match first {
            "crate" => out.push(info.krate.clone()),
            "self" => {
                out.push(info.krate.clone());
                out.extend(info.module.iter().cloned());
            }
            "super" => {
                let ups = segs.iter().take_while(|s| *s == "super").count();
                if ups > info.module.len() {
                    return Err(format!(
                        "`super` above the crate root in {}",
                        segs.join("::")
                    ));
                }
                out.push(info.krate.clone());
                out.extend(info.module[..info.module.len() - ups].iter().cloned());
                out.extend(segs[ups..].iter().cloned());
                return Ok(out);
            }
            _ => {
                // A single-segment type path of a primitive name is the primitive, also
                // when a module of that name (`use crate::str;`) is in scope.
                if rest.is_empty() && PRIMITIVES.contains(&first) {
                    out.extend([
                        "core".to_string(),
                        "primitive".to_string(),
                        first.to_string(),
                    ]);
                    return Ok(out);
                }
                // The items of the module come before its `use`s: a trait and a derive
                // macro of one name (`trait Hash` and `pub use macros::Hash`) are in
                // different namespaces, and a type path means the trait.
                if info.items.contains(first) {
                    out.push(info.krate.clone());
                    out.extend(info.module.iter().cloned());
                    out.push(first.to_string());
                } else if let Some(u) = info.uses.get(first) {
                    let mut p = u.clone();
                    p.extend(rest.iter().cloned());
                    return self.resolve_names(&p, depth + 1);
                } else if matches!(first, "core" | "alloc" | "std" | "alloc_crate") {
                    out.push(first.to_string());
                } else if let Some((_, path)) = PRELUDE.iter().find(|(n, _)| *n == first) {
                    out.extend(
                        path.trim_start_matches("::")
                            .split("::")
                            .map(str::to_string),
                    );
                } else {
                    let globs = if info.globs.is_empty() {
                        String::new()
                    } else {
                        format!(
                            " (the file has glob imports: {})",
                            info.globs
                                .iter()
                                .map(|g| format!("{}::*", g.join("::")))
                                .collect::<Vec<_>>()
                                .join(", ")
                        )
                    };
                    return Err(format!(
                        "cannot resolve the name `{}` in {}::{}{}",
                        first,
                        info.krate,
                        info.module.join("::"),
                        globs
                    ));
                }
            }
        }
        out.extend(rest.iter().cloned());
        Ok(out)
    }

    /// The public absolute path (as `::a::b`) of `segs`.
    fn public_path(&self, segs: &[String]) -> Res<String> {
        let mut abs = self.resolve_names(segs, 0)?;
        if matches!(abs[0].as_str(), "alloc" | "alloc_crate") {
            abs[0] = "std".to_string();
        }
        let p = rewrite_prefix(&path_string(&abs), &public_table());
        Ok(rewrite_prefix(&p, &self.rewrites))
    }

    /// Resolves the first `n` segments of `path` and returns how many segments they are now.
    fn resolve_path(&mut self, path: &mut syn::Path, n: usize) -> usize {
        if path.leading_colon.is_some() || n == 0 {
            // `::core::..` is absolute (only the public path table and the variant apply).
            if path.leading_colon.is_some() {
                let names: Vec<String> = path
                    .segments
                    .iter()
                    .take(n)
                    .map(|s| s.ident.to_string())
                    .collect();
                if !names.is_empty() {
                    let p = rewrite_prefix(&path_string(&names), &public_table());
                    let p = rewrite_prefix(&p, &self.rewrites);
                    return self.splice(path, n, &p);
                }
            }
            return n;
        }
        let first = path.segments[0].ident.to_string();
        if self.in_scope(&first) {
            return n;
        }
        let names: Vec<String> = path
            .segments
            .iter()
            .take(n)
            .map(|s| s.ident.to_string())
            .collect();
        match self.public_path(&names) {
            Ok(p) => self.splice(path, n, &p),
            Err(e) => {
                self.errors.push(e);
                n
            }
        }
    }

    /// Replaces the first `n` segments of `path` by the segments of the absolute path
    /// `abs`; the generic arguments of the old last of them go to the new last one.
    fn splice(&mut self, path: &mut syn::Path, n: usize, abs: &str) -> usize {
        let new: Vec<&str> = abs.trim_start_matches("::").split("::").collect();
        let old: Vec<syn::PathSegment> = path.segments.iter().cloned().collect();
        let mut segs: Punctuated<syn::PathSegment, syn::Token![::]> = Punctuated::new();
        for (i, name) in new.iter().enumerate() {
            let mut seg =
                syn::PathSegment::from(syn::Ident::new(name, proc_macro2::Span::call_site()));
            if i + 1 == new.len() {
                seg.arguments = old[n - 1].arguments.clone();
            }
            segs.push(seg);
        }
        for seg in &old[n..] {
            segs.push(seg.clone());
        }
        path.segments = segs;
        path.leading_colon = Some(Default::default());
        new.len()
    }

    fn visit_args(&mut self, path: &mut syn::Path) {
        for seg in path.segments.iter_mut() {
            self.visit_path_arguments_mut(&mut seg.arguments);
        }
    }
}

impl VisitMut for Resolver<'_> {
    fn visit_path_mut(&mut self, path: &mut syn::Path) {
        let n = path.segments.len();
        self.resolve_path(path, n);
        self.visit_args(path);
    }

    fn visit_type_path_mut(&mut self, tp: &mut syn::TypePath) {
        match &mut tp.qself {
            None => self.visit_path_mut(&mut tp.path),
            Some(q) => {
                self.visit_type_mut(&mut q.ty);
                let old = q.position;
                let new = if old == 0 {
                    0
                } else {
                    self.resolve_path(&mut tp.path, old)
                };
                q.position = new;
                self.visit_args(&mut tp.path);
            }
        }
    }

    fn visit_attribute_mut(&mut self, _: &mut syn::Attribute) {}
}

fn bound_path(b: &syn::TypeParamBound) -> Option<String> {
    match b {
        syn::TypeParamBound::Trait(t)
            if t.modifier == syn::TraitBoundModifier::None && t.path.leading_colon.is_some() =>
        {
            Some(path_string(
                &t.path
                    .segments
                    .iter()
                    .map(|s| s.ident.to_string())
                    .collect::<Vec<_>>(),
            ))
        }
        _ => None,
    }
}

/// `PointeeSized` and `MetaSized` of the unstable sized hierarchy: they stand for `?Sized`
/// in a bound, and are no supertraits.
pub fn is_sized_hierarchy(b: &syn::TypeParamBound) -> bool {
    bound_path(b).is_some_and(|p| SIZED_HIERARCHY.contains(&p.as_str()))
}

/// Unstable marker traits (`Destruct`) that std's source puts in bounds of const traits.
fn is_dropped(b: &syn::TypeParamBound) -> bool {
    bound_path(b).is_some_and(|p| DROPPED_BOUNDS.contains(&p.as_str()))
}

/// The supertraits of a mock: no unstable marker traits.
pub fn fix_supertraits(t: &mut syn::ItemTrait) {
    t.supertraits = t
        .supertraits
        .iter()
        .filter(|b| !is_sized_hierarchy(b) && !is_dropped(b))
        .cloned()
        .collect();
    if t.supertraits.is_empty() {
        t.colon_token = None;
    }
}

fn retain_stable(bounds: &mut Punctuated<syn::TypeParamBound, syn::Token![+]>) {
    *bounds = bounds.iter().filter(|b| !is_dropped(b)).cloned().collect();
}

/// Rewrites the bounds of std's unstable marker traits: `PointeeSized` to `?Sized`,
/// `Destruct` dropped (with a where predicate or clause that it leaves empty).
pub struct BoundFix;

impl VisitMut for BoundFix {
    fn visit_type_param_bound_mut(&mut self, b: &mut syn::TypeParamBound) {
        if is_sized_hierarchy(b) {
            *b = syn::parse_quote!(?::core::marker::Sized);
        } else {
            syn::visit_mut::visit_type_param_bound_mut(self, b);
        }
    }

    fn visit_generics_mut(&mut self, g: &mut syn::Generics) {
        syn::visit_mut::visit_generics_mut(self, g);
        for p in g.params.iter_mut() {
            if let syn::GenericParam::Type(t) = p {
                retain_stable(&mut t.bounds);
                if t.bounds.is_empty() {
                    t.colon_token = None;
                }
            }
        }
        if let Some(w) = &mut g.where_clause {
            let mut preds: Punctuated<syn::WherePredicate, syn::Token![,]> = Punctuated::new();
            for pred in w.predicates.iter() {
                let mut pred = pred.clone();
                if let syn::WherePredicate::Type(pt) = &mut pred {
                    retain_stable(&mut pt.bounds);
                    if pt.bounds.is_empty() {
                        continue;
                    }
                }
                preds.push(pred);
            }
            w.predicates = preds;
            if w.predicates.is_empty() {
                g.where_clause = None;
            }
        }
    }

    fn visit_trait_item_type_mut(&mut self, t: &mut syn::TraitItemType) {
        syn::visit_mut::visit_trait_item_type_mut(self, t);
        retain_stable(&mut t.bounds);
        if t.bounds.is_empty() {
            t.colon_token = None;
        }
    }
}
