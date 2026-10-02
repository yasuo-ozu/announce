//! Type context of a definition: `#[announce(repeater = path)]` and the
//! type-leak table (DESIGN.md section 4.5).
//!
//! On the cfg-evaluated item, type-leak finds the types that would not
//! resolve at a caller: relative paths such as `Local` or `super::X`.
//! The decision is type-leak's own: `Referrer::is_empty()` after
//! `reduce_roots()`, with primitives and `crate::` paths allowed. Std items
//! must be written absolutely (`::std::vec::Vec<u8>`); prelude names such as
//! `Vec` or `Option` count as relative unless `allow_std_prelude = true`.
//!
//! type-leak runs on an *analysis copy* of the item, in which
//! - higher-ranked lifetimes (`for<'b>`) are replaced by `'static`,
//! - array lengths, enum discriminants, const generic arguments and const
//!   defaults are erased (after checking that the value paths in them are
//!   absolute: a repeater only carries types),
//! - with `allow_std_prelude = true`, bare prelude names are temporarily made
//!   absolute (and restored in the result).
//!
//! If any relative type remains, `repeater = ..` is **required** (for type
//! aliases, which are not a type-leak target, it is an error instead). Each
//! type `T_I` then gets an impl of the definer's repeater trait,
//!
//! ```text
//! impl<P..> REPEATER<ID, I, (P..)> for () { type Type = T_I; }
//! ```
//!
//! where `P..` are only the generic parameters `T_I` mentions (from any scope;
//! see `announce_helper::scope`), with their own bounds. The payload stays the
//! original item; the entry's META gets
//! `type_leak(repeater = .., id = .., generics = [..], types = [..])`, and a
//! callback rewrites `types[I]` itself (`announce_helper::TypeLeak::apply`).
//! The repeater trait must be local and have the shape
//! `pub trait Repeater<const ID: u64, const I: usize, G: ?Sized> { type Type: ?Sized; }`.

use announce_helper::scope;
use proc_macro2::{Ident, Span, TokenStream};
use syn::visit_mut::VisitMut;
use syn::{GenericParam, Item, Path, Result, Type};
use template_quote::{quote, ToTokens};

pub struct Leaked {
    /// Impls emitted at the definition site.
    pub impls: TokenStream,
    /// `type_leak(repeater = .., id = .., generics = [..], types = [..])`.
    pub meta: TokenStream,
}

/// The repeater must be a local trait (`crate::..` / `$crate::..`): the table
/// implements it for `()`, which the orphan rule only allows for a local
/// trait (AUDIT 3 #7). Callers resolve it through `$crate`.
pub fn check_repeater(repeater: &Path) -> Result<()> {
    let first = repeater.segments.first().map(|s| s.ident.to_string());
    if repeater.leading_colon.is_none() && matches!(first.as_deref(), Some("crate") | Some("$crate")) {
        Ok(())
    } else {
        Err(syn::Error::new_spanned(
            repeater,
            "`repeater` must be a trait of this crate, written `crate::path::to::Repeater` \
             (callers resolve it through `$crate`, and the table implements it for `()`, \
             which the orphan rule only allows for a local trait)",
        ))
    }
}

/// Std prelude names (types and traits) accepted bare with
/// `allow_std_prelude = true`.
const PRELUDE: &[&str] = &[
    "Box", "String", "Vec", "Option", "Result", "Copy", "Send", "Sized", "Sync", "Unpin", "Drop", "Fn",
    "FnMut", "FnOnce", "Clone", "ToOwned", "ToString", "PartialEq", "PartialOrd", "Eq", "Ord", "AsRef",
    "AsMut", "Into", "From", "Default", "Iterator", "Extend", "IntoIterator", "DoubleEndedIterator",
    "ExactSizeIterator", "TryFrom", "TryInto", "FromIterator",
];

/// Marker root for the reversible prelude rewrite.
const PRELUDE_ROOT: &str = "__announce_prelude";

fn is_absolute(p: &Path) -> bool {
    let first = p.segments.first().map(|s| s.ident.to_string()).unwrap_or_default();
    p.leading_colon.is_some() || first == "crate" || first == "$crate"
}

/// Trait paths (supertraits, generic and where-clause bounds, `impl`/`dyn`
/// bounds) must be absolute: a repeater only carries *types*, so a relative
/// trait path could never resolve at a caller (decided 2026-10-01). With
/// `allow_std_prelude = true`, bare prelude traits are accepted too.
fn check_trait_paths(item: &Item, allow_prelude: bool) -> Result<()> {
    struct V {
        allow_prelude: bool,
        err: Option<syn::Error>,
    }
    impl<'ast> syn::visit::Visit<'ast> for V {
        fn visit_trait_bound(&mut self, b: &'ast syn::TraitBound) {
            let p = &b.path;
            let first = p.segments.first().map(|s| s.ident.to_string()).unwrap_or_default();
            let prelude = self.allow_prelude && p.segments.len() == 1 && PRELUDE.contains(&first.as_str());
            if !(is_absolute(p) || prelude) && self.err.is_none() {
                self.err = Some(syn::Error::new_spanned(
                    p,
                    "announce: trait paths must be absolute (`::core::fmt::Display`, `crate::Base`, \
                     `?::core::marker::Sized`); a repeater only covers types, not traits",
                ));
            }
            syn::visit::visit_trait_bound(self, b);
        }
    }
    let mut v = V { allow_prelude, err: None };
    syn::visit::Visit::visit_item(&mut v, item);
    v.err.map_or(Ok(()), Err)
}

/// Value expressions inside types and signatures (array lengths, enum
/// discriminants, const generic arguments, const defaults) are copied to
/// callers like the types around them, but a repeater cannot carry values:
/// every path in them must be absolute, `Self::..`, or a const parameter
/// (AUDIT 3 #5). Then they are erased in the analysis copy.
fn check_and_erase_values(item: &mut Item, const_params: &[String]) -> Result<Vec<syn::Expr>> {
    struct V<'a> {
        consts: &'a [String],
        err: Option<syn::Error>,
        /// Original expressions of the array lengths / const arguments that
        /// were replaced by `::__announce_len::L{k}` (restored afterwards).
        saved: Vec<syn::Expr>,
    }
    impl V<'_> {
        fn marker(&mut self, e: &syn::Expr) -> syn::Expr {
            let k = Ident::new(&format!("L{}", self.saved.len()), Span::call_site());
            self.saved.push(e.clone());
            syn::parse_quote!(::__announce_len::#k)
        }
    }
    impl V<'_> {
        fn check(&mut self, e: &syn::Expr) {
            struct P<'b, 'a>(&'b mut V<'a>);
            impl<'ast> syn::visit::Visit<'ast> for P<'_, '_> {
                fn visit_expr_path(&mut self, p: &'ast syn::ExprPath) {
                    let path = &p.path;
                    let first = path.segments.first().map(|s| s.ident.to_string()).unwrap_or_default();
                    let ok = p.qself.is_some()
                        || is_absolute(path)
                        || first == "Self"
                        || (path.segments.len() == 1 && self.0.consts.contains(&first));
                    if !ok && self.0.err.is_none() {
                        self.0.err = Some(syn::Error::new_spanned(
                            path,
                            "announce: paths in array lengths, discriminants and const defaults must be \
                             absolute (`crate::N`, `::core::mem::size_of::<T>()`), `Self::..` or a const \
                             parameter; a repeater only covers types, not values",
                        ));
                    }
                    syn::visit::visit_expr_path(self, p);
                }
            }
            syn::visit::Visit::visit_expr(&mut P(self), e);
        }
    }
    impl VisitMut for V<'_> {
        fn visit_type_array_mut(&mut self, a: &mut syn::TypeArray) {
            self.check(&a.len);
            let len = a.len.clone();
            a.len = self.marker(&len);
            syn::visit_mut::visit_type_array_mut(self, a);
        }
        fn visit_variant_mut(&mut self, v: &mut syn::Variant) {
            if let Some((_, e)) = &v.discriminant {
                self.check(e);
            }
            v.discriminant = None;
            syn::visit_mut::visit_variant_mut(self, v);
        }
        fn visit_trait_item_const_mut(&mut self, c: &mut syn::TraitItemConst) {
            if let Some((_, e)) = &c.default {
                self.check(e);
            }
            c.default = None;
            syn::visit_mut::visit_trait_item_const_mut(self, c);
        }
        fn visit_const_param_mut(&mut self, c: &mut syn::ConstParam) {
            if let Some(e) = &c.default {
                self.check(e);
            }
            c.default = None;
            c.eq_token = None;
            syn::visit_mut::visit_const_param_mut(self, c);
        }
        fn visit_generic_argument_mut(&mut self, a: &mut syn::GenericArgument) {
            if let syn::GenericArgument::Const(e) = a {
                self.check(e);
                let e = e.clone();
                let m = self.marker(&e);
                *a = syn::GenericArgument::Const(syn::parse_quote!({ #m }));
                return;
            }
            syn::visit_mut::visit_generic_argument_mut(self, a);
        }
        // Provided-method bodies are not analysed (documented gap).
        fn visit_block_mut(&mut self, _: &mut syn::Block) {}
    }
    let mut v = V { consts: const_params, err: None, saved: Vec::new() };
    v.visit_item_mut(item);
    match v.err {
        Some(e) => Err(e),
        None => Ok(v.saved),
    }
}

/// Puts the original array lengths / const arguments back into an interned
/// type (inverse of the markers of `check_and_erase_values`).
fn restore_values(ty: &mut Type, saved: &[syn::Expr]) {
    struct V<'a>(&'a [syn::Expr]);
    impl V<'_> {
        fn lookup(&self, e: &syn::Expr) -> Option<syn::Expr> {
            let p = match e {
                syn::Expr::Path(p) => p,
                syn::Expr::Block(b) if b.block.stmts.len() == 1 => match &b.block.stmts[0] {
                    syn::Stmt::Expr(syn::Expr::Path(p), None) => p,
                    _ => return None,
                },
                _ => return None,
            };
            let segs: Vec<String> = p.path.segments.iter().map(|s| s.ident.to_string()).collect();
            if p.path.leading_colon.is_some() && segs.len() == 2 && segs[0] == "__announce_len" {
                let k: usize = segs[1].trim_start_matches('L').parse().ok()?;
                return self.0.get(k).cloned();
            }
            None
        }
    }
    impl VisitMut for V<'_> {
        fn visit_type_array_mut(&mut self, a: &mut syn::TypeArray) {
            if let Some(e) = self.lookup(&a.len) {
                a.len = e;
            }
            syn::visit_mut::visit_type_array_mut(self, a);
        }
        fn visit_generic_argument_mut(&mut self, a: &mut syn::GenericArgument) {
            if let syn::GenericArgument::Const(e) = a {
                if let Some(orig) = self.lookup(e) {
                    *e = orig;
                    return;
                }
            }
            syn::visit_mut::visit_generic_argument_mut(self, a);
        }
    }
    V(saved).visit_type_mut(ty);
}

/// Higher-ranked lifetimes (`for<'b> Fn(&'b u8)`) are not in type-leak's
/// scope; replace them by `'static` in the analysis copy (AUDIT 3 #4).
fn erase_hrtb(item: &mut Item) {
    struct Rename(Vec<String>);
    impl VisitMut for Rename {
        fn visit_lifetime_mut(&mut self, l: &mut syn::Lifetime) {
            if self.0.contains(&l.to_string()) {
                *l = syn::Lifetime::new("'static", l.span());
            }
        }
    }
    struct V;
    impl V {
        fn names(b: &Option<syn::BoundLifetimes>) -> Vec<String> {
            b.as_ref()
                .map(|b| b.lifetimes.iter().map(|p| match p {
                    GenericParam::Lifetime(l) => l.lifetime.to_string(),
                    other => other.to_token_stream().to_string(),
                }).collect())
                .unwrap_or_default()
        }
    }
    impl VisitMut for V {
        fn visit_trait_bound_mut(&mut self, b: &mut syn::TraitBound) {
            let names = Self::names(&b.lifetimes);
            if !names.is_empty() {
                b.lifetimes = None;
                Rename(names).visit_path_mut(&mut b.path);
            }
            syn::visit_mut::visit_trait_bound_mut(self, b);
        }
        fn visit_type_bare_fn_mut(&mut self, f: &mut syn::TypeBareFn) {
            let names = Self::names(&f.lifetimes);
            if !names.is_empty() {
                f.lifetimes = None;
                let mut r = Rename(names);
                for i in &mut f.inputs {
                    r.visit_type_mut(&mut i.ty);
                }
                r.visit_return_type_mut(&mut f.output);
            }
            syn::visit_mut::visit_type_bare_fn_mut(self, f);
        }
        fn visit_predicate_type_mut(&mut self, p: &mut syn::PredicateType) {
            let names = Self::names(&p.lifetimes);
            if !names.is_empty() {
                p.lifetimes = None;
                let mut r = Rename(names);
                r.visit_type_mut(&mut p.bounded_ty);
                for b in &mut p.bounds {
                    r.visit_type_param_bound_mut(b);
                }
            }
            syn::visit_mut::visit_predicate_type_mut(self, p);
        }
    }
    V.visit_item_mut(item);
}

/// With `allow_std_prelude = true`: bare prelude names become
/// `::__announce_prelude::Name` in the analysis copy, so type-leak treats them
/// as absolute even with generic arguments (AUDIT 3 #6). [`unprelude`]
/// restores them in the result.
fn prelude_to_marker(item: &mut Item) {
    struct V;
    impl VisitMut for V {
        fn visit_path_mut(&mut self, p: &mut Path) {
            if p.leading_colon.is_none() && p.segments.len() == 1 {
                let name = p.segments[0].ident.to_string();
                if PRELUDE.contains(&name.as_str()) {
                    let root = Ident::new(PRELUDE_ROOT, p.segments[0].ident.span());
                    p.leading_colon = Some(Default::default());
                    p.segments.insert(0, syn::PathSegment::from(root));
                }
            }
            syn::visit_mut::visit_path_mut(self, p);
        }
    }
    V.visit_item_mut(item);
}

fn unprelude(ty: &mut Type) {
    struct V;
    impl VisitMut for V {
        fn visit_path_mut(&mut self, p: &mut Path) {
            if p.leading_colon.is_some() && p.segments.len() >= 2 && p.segments[0].ident == PRELUDE_ROOT {
                p.leading_colon = None;
                let rest: syn::punctuated::Punctuated<syn::PathSegment, syn::Token![::]> =
                    p.segments.iter().skip(1).cloned().collect();
                p.segments = rest;
            }
            syn::visit_mut::visit_path_mut(self, p);
        }
    }
    V.visit_type_mut(ty);
}

/// Every type occurring in `item` (for checking that an interned type maps
/// back to the original item).
fn all_types(item: &Item) -> Vec<Type> {
    struct V(Vec<Type>);
    impl<'ast> syn::visit::Visit<'ast> for V {
        fn visit_type(&mut self, t: &'ast Type) {
            self.0.push(t.clone());
            syn::visit::visit_type(self, t);
        }
    }
    let mut v = V(Vec::new());
    syn::visit::Visit::visit_item(&mut v, item);
    v.0
}

/// The types of `item` that would not resolve at a caller, as they appear in
/// `item`. `None` for items that are not analysed (inline modules: the
/// documented wrapper pattern).
pub fn analyze(item: &Item, opts: &crate::options::Options) -> Result<Option<Vec<Type>>> {
    let generics = match item {
        Item::Struct(s) => &s.generics,
        Item::Enum(s) => &s.generics,
        Item::Union(s) => &s.generics,
        Item::Trait(s) => &s.generics,
        Item::Type(s) => &s.generics,
        _ => return Ok(None),
    };
    check_trait_paths(item, opts.allow_std_prelude)?;
    let params = scope::collect_params(item);
    let consts: Vec<String> = params
        .iter()
        .filter_map(|p| match p {
            GenericParam::Const(c) => Some(c.ident.to_string()),
            _ => None,
        })
        .collect();
    let mut analysed = item.clone();
    let saved = check_and_erase_values(&mut analysed, &consts)?;
    erase_hrtb(&mut analysed);
    if opts.allow_std_prelude {
        prelude_to_marker(&mut analysed);
    }
    // Primitives (optionally) and `crate::` paths are allowed; the prelude is
    // handled by the marker rewrite above.
    let mut config = type_leak::LeakerConfig::new();
    config.allow_crate();
    if opts.allow_primitive {
        config.allow_primitive();
    }
    // `Self` has no meaning in an `impl .. for ()`, and resolves everywhere.
    config.self_ty_can_be_interned = false;
    let mut leaker = type_leak::Leaker::from_config(config);
    leaker.intern_with(generics, |v| v.visit_item(&analysed)).map_err(|e| {
        syn::Error::new(
            e.span(),
            "announce: type-leak cannot analyse this type; write it with absolute paths \
             (a reference with an elided lifetime, `&Local`, needs a named lifetime or `&crate::Local`)",
        )
    })?;
    // `finish()` is empty unless `reduce_roots()` ran first.
    leaker.reduce_roots();
    let originals = all_types(item);
    let mut types = Vec::new();
    for t in leaker.finish().iter() {
        let mut t = t.clone();
        unprelude(&mut t);
        restore_values(&mut t, &saved);
        if !originals.contains(&t) {
            return Err(syn::Error::new_spanned(
                &t,
                "announce: this type could not be mapped back to the item after analysis; \
                 write it with absolute paths",
            ));
        }
        types.push(t);
    }
    // type-leak's order comes from a `HashSet`; sort for reproducible output.
    types.sort_by_cached_key(|t| t.to_token_stream().to_string());
    Ok(Some(types))
}

/// Enforces `repeater` when it is needed, and builds the table.
pub fn table(item: &Item, opts: &crate::options::Options, random: u64) -> Result<Option<Leaked>> {
    let repeater = opts.repeater.as_ref();
    // Type aliases are not a type-leak target: their right-hand side must be
    // written with paths that resolve everywhere.
    if let Item::Type(alias) = item {
        if let Some(a) = repeater {
            return Err(syn::Error::new_spanned(
                a,
                "`repeater` is not supported on type aliases; write the right-hand side with absolute paths",
            ));
        }
        if let Some(types) = analyze(item, opts)? {
            if let Some(ty) = types.first() {
                return Err(syn::Error::new_spanned(
                    ty,
                    format!(
                        "announce: the right-hand side of type alias `{}` must not contain relative paths; \
                         write this type as an absolute path (`crate::..`, `$crate::..` or `::other_crate::..`)",
                        alias.ident
                    ),
                ));
            }
        }
        return Ok(None);
    }
    let analysis = analyze(item, opts)?;
    let (repeater, types) = match (repeater, analysis) {
        (Some(a), None) => {
            return Err(syn::Error::new_spanned(
                a,
                "`repeater` is supported on struct, enum, union and trait items",
            ))
        }
        (None, None) => return Ok(None),
        (None, Some(types)) => {
            let ty = match types.first() {
                None => return Ok(None),
                Some(t) => t,
            };
            return Err(syn::Error::new_spanned(
                ty,
                format!(
                    "announce: this type does not resolve outside the defining crate{}; \
                     add `#[announce(repeater = crate::path::to::Repeater)]` (with \
                     `pub trait Repeater<const ID: u64, const I: usize, G: ?Sized> {{ type Type: ?Sized; }}`), \
                     or write it as an absolute path (`crate::..`, `::std::vec::Vec<..>`, `::other_crate::..`; \
                     std prelude names count as relative)",
                    if types.len() > 1 { format!(" ({} such types)", types.len()) } else { String::new() }
                ),
            ));
        }
        (Some(a), Some(types)) => (a, types),
    };
    let params = scope::collect_params(item);
    let id = syn::LitInt::new(&format!("{}u64", random), Span::call_site());
    let mut impls = TokenStream::new();
    let mut gs = Vec::new();
    for (i, ty) in types.iter().enumerate() {
        // Only the parameters this type mentions, with their own bounds
        // (AUDIT 3 #1-#3): no `Self` predicates, no cycles through the
        // item's bounds, valid for method/GAT parameters too.
        let ps = scope::mentioned_params(ty, &params);
        for p in &ps {
            if let GenericParam::Const(c) = p {
                if !matches!(&c.ty, Type::Path(t) if t.path.is_ident("usize")) {
                    return Err(syn::Error::new_spanned(
                        &c.ty,
                        "`repeater`: only `usize` const generics are supported in interned types",
                    ));
                }
            }
        }
        let decl: Vec<TokenStream> = ps
            .iter()
            .map(|p| {
                let mut p = p.clone();
                match &mut p {
                    GenericParam::Type(t) => {
                        t.default = None;
                        t.eq_token = None;
                        let names: Vec<String> = ps.iter().map(scope::param_name).collect();
                        // Keep only bounds that mention no `Self` and no
                        // parameter outside `ps`.
                        let keep: syn::punctuated::Punctuated<syn::TypeParamBound, syn::Token![+]> = t
                            .bounds
                            .iter()
                            .filter(|b| {
                                let s = b.to_token_stream().to_string();
                                !s.split(|c: char| !c.is_alphanumeric() && c != '_').any(|w| w == "Self")
                                    && scope::collect_params(&syn::parse_quote!(fn __x<__P: #b>() {}))
                                        .iter()
                                        .all(|q| names.contains(&scope::param_name(q)) || scope::param_name(q) == "__P")
                            })
                            .cloned()
                            .collect();
                        t.bounds = keep;
                        if t.bounds.is_empty() {
                            t.colon_token = None;
                        }
                        let ident = &t.ident;
                        let bounds = &t.bounds;
                        if bounds.is_empty() {
                            quote! { #ident }
                        } else {
                            quote! { #ident: #bounds }
                        }
                    }
                    GenericParam::Lifetime(l) => {
                        let lt = &l.lifetime;
                        quote! { #lt }
                    }
                    GenericParam::Const(c) => {
                        let ident = &c.ident;
                        quote! { const #ident: usize }
                    }
                }
            })
            .collect();
        let g = scope::encode(&ps);
        impls.extend(quote! {
            #[doc(hidden)]
            impl<#(#decl),*> #repeater<#id, #i, #g> for () {
                type Type = #ty;
            }
        });
        gs.push(g);
    }
    let meta = quote! {
        type_leak(repeater = #repeater, id = #id, generics = [#(#gs),*], types = [#(#types),*])
    };
    Ok(Some(Leaked { impls, meta }))
}
