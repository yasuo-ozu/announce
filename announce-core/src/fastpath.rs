//! The fast path of the type-context analysis.
//!
//! `leak::analyze` runs type-leak over the whole item, which is by far the most
//! expensive step of the `Capture` derive for an item with many methods. Most items
//! need none of it: every path in them is one that resolves everywhere, so
//! type-leak finds nothing to carry and nothing to reject. [`accepts`] reads
//! the item once, with a visitor that knows only the forms below, and says
//! whether that is certainly so. When it is not sure it says no, and the full
//! analysis runs as before: the scan never decides an error, and it must never
//! accept what the full analysis would reject or carry
//! (`leak::tests::fast_path_never_accepts_more_than_the_analysis` checks this
//! on the items of the test suites and on random items, and the analysis checks
//! it again on every item it sees in a test build).
//!
//! # What is accepted
//!
//! The item is a struct, enum, union, trait or type alias (a module is not
//! analysed at all), and every one of these holds, anywhere in the item that
//! type-leak reads (attributes, visibilities, default method bodies and the
//! rest of what it does not read are not scanned):
//!
//! - A type is a path, reference, raw pointer, slice, array, tuple, `!`, a
//!   parenthesised or grouped type, or a bare function pointer without `for<..>`
//!   and without a C-variadic. The output of a function pointer and of an
//!   `Fn(..) -> ..` bound has no reference without a lifetime (type-leak follows
//!   the elision of those, and leaks the pointer whole when it cannot). `impl Trait`, `dyn Trait`, `_`, a type macro, a
//!   `<T as Trait>::X` and anything `syn` does not parse are not accepted.
//! - A type path has no `<Q as ..>` and is one of
//!   - an absolute path of `std`, `core` or `alloc` (`::std::vec::Vec<T>`) or a
//!     path of `crate` (`crate::Local`), whose arguments are accepted types, in
//!     turn;
//!   - `Self` alone; and, in a trait only, `Self::A` with the one associated type
//!     `A` and no arguments (not in a type alias; never `Self::A::B`);
//!   - a type parameter of the item, of the method or of the associated type
//!     that declares the scope, alone or with one associated type (`T::Output`);
//!   - a primitive type, with `allow_primitive` (as type-leak's `allow_primitive()`
//!     lists them: `bool`, `char`, `str`, `u8`..`u128`, `usize`, `i8`..`i128`,
//!     `isize`, `f32`, `f64`);
//!   - a name of the std prelude ([`crate::leak::PRELUDE`]), with
//!     `allow_std_prelude`, with arguments that are accepted types.
//! - A lifetime is `'static` or declared by the item, the method or the associated
//!   type that it is in; it is not `'_`.
//! - A trait bound has no `for<..>` and is a path of `std`, `core` or `alloc`
//!   with a leading `::`, a path of `crate`, a prelude name with
//!   `allow_std_prelude`, or `Sized` / `?Sized`, with arguments that are accepted
//!   types (an `Fn(A) -> B` bound included). It may be a lifetime bound. These
//!   are also the paths that `leak::check_trait_paths` accepts.
//! - The bounded type of a `where` predicate is a type parameter in scope or
//!   `Self`, and has no `for<..>`.
//! - A value (an array length, an enum discriminant, a const generic argument, the
//!   default of a const parameter or of an associated const) is a literal, a
//!   const parameter in scope, an absolute path of `std`, `core`, `alloc` or
//!   `crate`, `Self::X`, or a unary or binary operation or parentheses around
//!   those. These are also the paths that `leak::check_values` accepts.
//!
//! A const parameter must have a primitive type. An associated type may have bounds
//! and a default; a method may have its own generics. Everything else is a
//! reason to run the full analysis.

use crate::leak::{PRELUDE, PRIMITIVES};
use crate::options::Options;
use syn::visit::Visit;
use syn::{
    Attribute, Expr, GenericArgument, GenericParam, Generics, Ident, Item, Lit, Macro, Pat, Path,
    PathArguments, Signature, TraitBound, TraitBoundModifier, TraitItem, Type, TypeParamBound,
    Visibility, WherePredicate,
};
#[cfg(test)]
use template_quote::ToTokens;

/// Whether every path in `item` certainly resolves everywhere, so that
/// type-leak would find no required entry and report no error. Independent of
/// `repeater`: with one, the caller still needs the table, and runs the full analysis.
pub(crate) fn accepts(item: &Item, opts: &Options) -> bool {
    let mut v = Fast {
        ok: true,
        allow_primitive: opts.allow_primitive,
        allow_prelude: opts.allow_std_prelude,
        in_trait: false,
        in_alias: false,
        in_bound: 0,
        in_type: 0,
        scopes: Vec::new(),
    };
    match item {
        // Not analysed.
        Item::Mod(_) => return true,
        Item::Struct(_) | Item::Enum(_) | Item::Union(_) | Item::Trait(_) | Item::Type(_) => {
            v.visit_item(item)
        }
        _ => return false,
    }
    v.ok
}

/// The names a scope declares: type parameters, const parameters and lifetimes.
#[derive(Default)]
struct Scope {
    types: Vec<String>,
    consts: Vec<String>,
    lifetimes: Vec<String>,
}

struct Fast {
    ok: bool,
    allow_primitive: bool,
    allow_prelude: bool,
    in_trait: bool,
    in_alias: bool,
    /// Inside the arguments of a trait bound (a predicate of the item).
    in_bound: usize,
    /// Inside a type (not in a bound alone, nor in the value of a discriminant).
    in_type: usize,
    scopes: Vec<Scope>,
}

fn is_one_of(id: &Ident, names: &[&str]) -> bool {
    names.iter().any(|n| id == n)
}

/// Whether the output of a signature has a reference without a lifetime, whose
/// lifetime is the one that elision takes from the inputs. type-leak leaks a
/// function pointer whose elision it cannot follow (when `Self` has lifetime
/// parameters, say) as a whole, and `Self` cannot be leaked; so the scan leaves
/// the signatures of function pointers and of `Fn(..)` bounds with such an output to
/// the analysis. (The methods of a trait are not affected: its `Self` is the
/// implementor.)
fn output_is_elided(output: &syn::ReturnType) -> bool {
    struct V(bool);
    impl<'ast> Visit<'ast> for V {
        fn visit_type_reference(&mut self, r: &'ast syn::TypeReference) {
            self.0 |= r.lifetime.is_none();
            syn::visit::visit_type_reference(self, r);
        }
    }
    let mut v = V(false);
    v.visit_return_type(output);
    v.0
}

fn has_arguments(p: &Path) -> bool {
    p.segments
        .iter()
        .any(|s| !matches!(s.arguments, PathArguments::None))
}

impl Fast {
    fn reject(&mut self) {
        self.ok = false;
    }

    fn with_generics(&mut self, g: &Generics, f: impl FnOnce(&mut Self)) {
        let mut scope = Scope::default();
        for p in &g.params {
            match p {
                GenericParam::Type(t) => scope.types.push(t.ident.to_string()),
                GenericParam::Const(c) => scope.consts.push(c.ident.to_string()),
                GenericParam::Lifetime(l) => scope.lifetimes.push(l.lifetime.to_string()),
            }
        }
        self.scopes.push(scope);
        f(self);
        self.scopes.pop();
    }

    /// Runs `f` with the scope of `g` reduced to its first `n` parameters.
    fn with_prefix(&mut self, g: &Generics, n: usize, f: impl FnOnce(&mut Self)) {
        let full = self.scopes.pop();
        let mut prefix = Scope::default();
        for p in g.params.iter().take(n) {
            match p {
                GenericParam::Type(t) => prefix.types.push(t.ident.to_string()),
                GenericParam::Const(c) => prefix.consts.push(c.ident.to_string()),
                GenericParam::Lifetime(l) => prefix.lifetimes.push(l.lifetime.to_string()),
            }
        }
        self.scopes.push(prefix);
        f(self);
        self.scopes.pop();
        self.scopes.extend(full);
    }

    fn is_type_param(&self, id: &Ident) -> bool {
        let name = id.to_string();
        self.scopes.iter().any(|s| s.types.contains(&name))
    }

    fn is_const_param(&self, id: &Ident) -> bool {
        let name = id.to_string();
        self.scopes.iter().any(|s| s.consts.contains(&name))
    }

    /// `::std::..`, `::core::..`, `::alloc::..` or `crate::..`, whose arguments the
    /// caller visits.
    fn is_absolute(p: &Path) -> bool {
        match (p.leading_colon.is_some(), p.segments.first()) {
            (true, Some(s)) => is_one_of(&s.ident, &["std", "core", "alloc"]),
            (false, Some(s)) => s.ident == "crate",
            _ => false,
        }
    }

    /// The arguments of the segments of a path, as types and values.
    fn visit_arguments(&mut self, p: &Path) {
        for seg in &p.segments {
            match &seg.arguments {
                PathArguments::None => {}
                PathArguments::AngleBracketed(a) => {
                    for arg in &a.args {
                        self.visit_generic_argument(arg);
                    }
                }
                PathArguments::Parenthesized(a) => {
                    if output_is_elided(&a.output) {
                        return self.reject();
                    }
                    for ty in &a.inputs {
                        self.visit_type(ty);
                    }
                    self.visit_return_type(&a.output);
                }
            }
        }
    }

    /// Whether `ty` is a path to a primitive type that `allow_primitive` admits,
    /// the only type of a const parameter that is accepted.
    fn is_primitive(&self, p: &Path) -> bool {
        self.allow_primitive
            && p.leading_colon.is_none()
            && p.segments.len() == 1
            && p.segments[0].arguments.is_empty()
            && is_one_of(&p.segments[0].ident, PRIMITIVES)
    }

    fn type_path(&mut self, p: &Path) {
        let first = match p.segments.first() {
            Some(s) => s,
            None => return self.reject(),
        };
        let n = p.segments.len();
        if Self::is_absolute(p) {
            return self.visit_arguments(p);
        }
        if p.leading_colon.is_some() {
            return self.reject();
        }
        // `Self`, `Self::A`; not in a bound (type-leak follows the environment of the
        // item through them and can find a cycle).
        if first.ident == "Self" {
            let ok = !self.in_alias
                && self.in_bound == 0
                && first.arguments.is_empty()
                && (n == 1 || (n == 2 && self.in_trait && p.segments[1].arguments.is_empty()));
            if !ok {
                self.reject();
            }
            return;
        }
        // `T`, `T::Output` (not in a bound); a type parameter takes no arguments.
        if self.is_type_param(&first.ident) {
            let ok = first.arguments.is_empty()
                && (n == 1 || (n == 2 && self.in_bound == 0 && p.segments[1].arguments.is_empty()));
            if !ok {
                self.reject();
            }
            return;
        }
        if n != 1 {
            return self.reject();
        }
        if self.is_primitive(p) {
            return;
        }
        if self.allow_prelude && is_one_of(&first.ident, PRELUDE) {
            return self.visit_arguments(p);
        }
        self.reject()
    }

    /// A value: an array length, a discriminant, a const argument, a default.
    fn value(&mut self, e: &Expr) {
        match e {
            Expr::Lit(l) => {
                if !l.attrs.is_empty() || matches!(l.lit, Lit::Verbatim(_)) {
                    self.reject()
                }
            }
            Expr::Path(p) => {
                let path = &p.path;
                let ok = p.qself.is_none()
                    && p.attrs.is_empty()
                    && !has_arguments(path)
                    && match path.segments.first() {
                        None => false,
                        Some(first) => {
                            Self::is_absolute(path)
                                || (path.leading_colon.is_none()
                                    && first.ident == "Self"
                                    && path.segments.len() > 1
                                    && self.in_type > 0
                                    && !self.in_alias)
                                || (path.leading_colon.is_none()
                                    && path.segments.len() == 1
                                    && self.is_const_param(&first.ident))
                        }
                    };
                if !ok {
                    self.reject()
                }
            }
            Expr::Paren(p) => self.value(&p.expr),
            Expr::Group(g) => self.value(&g.expr),
            Expr::Unary(u) => self.value(&u.expr),
            Expr::Binary(b) => {
                self.value(&b.left);
                self.value(&b.right);
            }
            _ => self.reject(),
        }
    }

    /// A bound: a trait path (the modifier `?` on `Sized` only) or a lifetime.
    fn bound(&mut self, b: &TraitBound) {
        if b.lifetimes.is_some() {
            return self.reject();
        }
        let p = &b.path;
        let bare_sized = p.leading_colon.is_none()
            && p.segments.len() == 1
            && p.segments[0].ident == "Sized"
            && p.segments[0].arguments.is_empty();
        match b.modifier {
            TraitBoundModifier::None => {}
            TraitBoundModifier::Maybe(_) => {
                if !bare_sized {
                    self.reject();
                }
                return;
            }
            #[allow(unreachable_patterns)]
            _ => return self.reject(),
        }
        if bare_sized {
            return;
        }
        if Self::is_absolute(p) {
            return self.bound_arguments(p);
        }
        let prelude = self.allow_prelude
            && p.leading_colon.is_none()
            && p.segments.len() == 1
            && is_one_of(&p.segments[0].ident, PRELUDE);
        if prelude {
            self.bound_arguments(p)
        } else {
            self.reject()
        }
    }

    /// The arguments of a bound: types and values that are part of a predicate of the
    /// item.
    fn bound_arguments(&mut self, p: &Path) {
        self.in_bound += 1;
        self.visit_arguments(p);
        self.in_bound -= 1;
    }
}

impl<'ast> Visit<'ast> for Fast {
    // Not read by type-leak.
    fn visit_attribute(&mut self, _: &'ast Attribute) {}

    fn visit_visibility(&mut self, v: &'ast Visibility) {
        if let Visibility::Restricted(r) = v {
            if r.in_token.is_some() {
                self.reject();
            }
        }
    }

    fn visit_item_struct(&mut self, i: &'ast syn::ItemStruct) {
        self.with_generics(&i.generics, |s| syn::visit::visit_item_struct(s, i))
    }

    fn visit_item_enum(&mut self, i: &'ast syn::ItemEnum) {
        self.with_generics(&i.generics, |s| syn::visit::visit_item_enum(s, i))
    }

    fn visit_item_union(&mut self, i: &'ast syn::ItemUnion) {
        self.with_generics(&i.generics, |s| syn::visit::visit_item_union(s, i))
    }

    fn visit_item_trait(&mut self, i: &'ast syn::ItemTrait) {
        self.in_trait = true;
        self.with_generics(&i.generics, |s| syn::visit::visit_item_trait(s, i));
        self.in_trait = false;
    }

    fn visit_item_type(&mut self, i: &'ast syn::ItemType) {
        self.in_alias = true;
        self.with_generics(&i.generics, |s| syn::visit::visit_item_type(s, i));
        self.in_alias = false;
    }

    fn visit_trait_item(&mut self, i: &'ast TraitItem) {
        match i {
            TraitItem::Fn(_) | TraitItem::Type(_) | TraitItem::Const(_) => {
                syn::visit::visit_trait_item(self, i)
            }
            _ => self.reject(),
        }
    }

    fn visit_trait_item_type(&mut self, i: &'ast syn::TraitItemType) {
        self.with_generics(&i.generics, |s| syn::visit::visit_trait_item_type(s, i))
    }

    fn visit_trait_item_const(&mut self, i: &'ast syn::TraitItemConst) {
        self.with_generics(&i.generics, |s| {
            s.visit_generics(&i.generics);
            s.visit_type(&i.ty);
            if let Some((_, e)) = &i.default {
                s.value(e);
            }
        })
    }

    fn visit_signature(&mut self, i: &'ast Signature) {
        if i.variadic.is_some() || i.asyncness.is_some() {
            return self.reject();
        }
        self.with_generics(&i.generics, |s| syn::visit::visit_signature(s, i))
    }

    // The bodies of provided methods are not analysed.
    fn visit_block(&mut self, _: &'ast syn::Block) {}

    fn visit_pat(&mut self, p: &'ast Pat) {
        if !matches!(p, Pat::Ident(_) | Pat::Wild(_)) {
            self.reject();
        }
    }

    fn visit_variant(&mut self, v: &'ast syn::Variant) {
        if let Some((_, e)) = &v.discriminant {
            self.value(e);
        }
        self.visit_fields(&v.fields);
    }

    /// The bounds see all the parameters; the default of a parameter sees the
    /// ones before it only (type-leak reads each default alone, with those).
    fn visit_generics(&mut self, g: &'ast Generics) {
        for (i, p) in g.params.iter().enumerate() {
            match p {
                GenericParam::Lifetime(l) => {
                    self.visit_lifetime(&l.lifetime);
                    for b in &l.bounds {
                        self.visit_lifetime(b);
                    }
                }
                GenericParam::Type(t) => {
                    for b in &t.bounds {
                        self.visit_type_param_bound(b);
                    }
                    if let Some(d) = &t.default {
                        self.with_prefix(g, i, |s| s.visit_type(d));
                    }
                }
                GenericParam::Const(c) => {
                    match &c.ty {
                        Type::Path(t) if t.qself.is_none() && self.is_primitive(&t.path) => {}
                        _ => self.reject(),
                    }
                    if let Some(e) = &c.default {
                        self.with_prefix(g, i, |s| s.value(e));
                    }
                }
            }
        }
        if let Some(w) = &g.where_clause {
            self.visit_where_clause(w);
        }
    }

    fn visit_type(&mut self, ty: &'ast Type) {
        self.in_type += 1;
        match ty {
            Type::Path(t) => {
                if t.qself.is_some() {
                    self.reject();
                } else {
                    self.type_path(&t.path);
                }
            }
            Type::Array(a) => {
                self.visit_type(&a.elem);
                self.value(&a.len);
            }
            Type::BareFn(f) => {
                if f.lifetimes.is_some() || f.variadic.is_some() || output_is_elided(&f.output) {
                    self.reject();
                } else {
                    for arg in &f.inputs {
                        self.visit_type(&arg.ty);
                    }
                    self.visit_return_type(&f.output);
                }
            }
            Type::Reference(r) => {
                if let Some(l) = &r.lifetime {
                    self.visit_lifetime(l);
                }
                self.visit_type(&r.elem);
            }
            Type::Ptr(p) => self.visit_type(&p.elem),
            Type::Slice(s) => self.visit_type(&s.elem),
            Type::Paren(p) => self.visit_type(&p.elem),
            Type::Group(g) => self.visit_type(&g.elem),
            Type::Tuple(t) => {
                for e in &t.elems {
                    self.visit_type(e);
                }
            }
            Type::Never(_) => {}
            // `impl Trait`, `dyn Trait`, `_`, a type macro, anything else.
            _ => self.reject(),
        }
        self.in_type -= 1;
    }

    fn visit_generic_argument(&mut self, a: &'ast GenericArgument) {
        match a {
            GenericArgument::Lifetime(l) => self.visit_lifetime(l),
            GenericArgument::Type(t) => self.visit_type(t),
            GenericArgument::Const(e) => self.value(e),
            GenericArgument::AssocType(a) => {
                if a.generics.is_some() {
                    return self.reject();
                }
                self.visit_type(&a.ty);
            }
            _ => self.reject(),
        }
    }

    fn visit_lifetime(&mut self, l: &'ast syn::Lifetime) {
        // `'_` is elided, `'static` is everywhere; any other lifetime must be
        // declared by the item, the method or the associated type (type-leak
        // takes an undeclared one for a path that is relative).
        if l.ident == "_" {
            return self.reject();
        }
        if l.ident != "static" {
            let name = l.to_string();
            if !self.scopes.iter().any(|s| s.lifetimes.contains(&name)) {
                self.reject();
            }
        }
    }

    fn visit_type_param_bound(&mut self, b: &'ast TypeParamBound) {
        match b {
            TypeParamBound::Trait(t) => self.bound(t),
            TypeParamBound::Lifetime(l) => self.visit_lifetime(l),
            _ => self.reject(),
        }
    }

    fn visit_where_predicate(&mut self, p: &'ast WherePredicate) {
        match p {
            WherePredicate::Type(t) => {
                if t.lifetimes.is_some() {
                    return self.reject();
                }
                let bounded_ok = match &t.bounded_ty {
                    Type::Path(tp) if tp.qself.is_none() && tp.path.segments.len() == 1 => {
                        let seg = &tp.path.segments[0];
                        tp.path.leading_colon.is_none()
                            && seg.arguments.is_empty()
                            && ((seg.ident == "Self" && !self.in_alias)
                                || self.is_type_param(&seg.ident))
                    }
                    _ => false,
                };
                if !bounded_ok {
                    return self.reject();
                }
                for b in &t.bounds {
                    self.visit_type_param_bound(b);
                }
            }
            WherePredicate::Lifetime(_) => {}
            _ => self.reject(),
        }
    }

    // Anything below that is not handled above is not a form this scan knows.
    fn visit_path(&mut self, _: &'ast Path) {
        self.reject();
    }

    fn visit_expr(&mut self, _: &'ast Expr) {
        self.reject();
    }

    fn visit_macro(&mut self, _: &'ast Macro) {
        self.reject();
    }
}

/// In a test build, the full analysis checks every answer of [`accepts`] that it
/// is given the item for (`leak::analyze`): an item that is accepted must have no
/// required entry and no error.
#[cfg(test)]
pub(crate) fn check_analysis(
    item: &Item,
    opts: &Options,
    result: &Result<Option<crate::leak::Analysis>, crate::leak::Unleakable>,
) {
    if !accepts(item, opts) {
        return;
    }
    match result {
        Ok(None) => {}
        Ok(Some(a)) => {
            if let Some((ix, ty)) = a.required().next() {
                panic!(
                    "the fast path accepted an item whose analysis needs entry {}: {}\n{}",
                    ix,
                    ty.to_token_stream(),
                    item.to_token_stream()
                );
            }
        }
        Err(e) => panic!(
            "the fast path accepted an item that the analysis rejects: {}\n{}",
            e,
            item.to_token_stream()
        ),
    }
}

/// The analysis of an item that the fast path accepted, in a test build, to
/// check the answer (see [`check_analysis`]).
#[cfg(test)]
pub(crate) fn assert_agrees(item: &Item, opts: &Options) {
    let _ = crate::leak::analyze(item, opts);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::leak::analyze;
    use std::path::{Path as FsPath, PathBuf};

    fn item(src: &str) -> Item {
        syn::parse_str(src).unwrap_or_else(|e| panic!("{}: {}", e, src))
    }

    fn opts(allow_primitive: bool, allow_std_prelude: bool) -> Options {
        Options {
            allow_primitive,
            allow_std_prelude,
            ..Options::default()
        }
    }

    /// Whether the fast path accepts `src` with the default options.
    fn fast(src: &str) -> bool {
        accepts(&item(src), &Options::default())
    }

    /// Runs the full analysis of every item in `src` under the four combinations of
    /// options. In a test build `analyze` checks the answer of the fast path itself
    /// (`check_analysis`), so a disagreement panics. Returns how many (item,
    /// options) pairs the fast path accepted, and how many it saw.
    fn agree_on(items: impl IntoIterator<Item = Item>) -> (usize, usize) {
        let (mut accepted, mut seen) = (0, 0);
        for it in items {
            for (p, s) in [(true, false), (true, true), (false, false), (false, true)] {
                let o = opts(p, s);
                seen += 1;
                if accepts(&it, &o) {
                    accepted += 1;
                }
                let _ = analyze(&it, &o);
            }
        }
        (accepted, seen)
    }

    /// The struct, enum, union, trait and type items of `file`, at any depth of
    /// inline modules.
    fn items_of(file: &syn::File) -> Vec<Item> {
        fn walk(items: &[Item], out: &mut Vec<Item>) {
            for i in items {
                match i {
                    Item::Struct(_)
                    | Item::Enum(_)
                    | Item::Union(_)
                    | Item::Trait(_)
                    | Item::Type(_) => out.push(i.clone()),
                    Item::Mod(m) => {
                        if let Some((_, inner)) = &m.content {
                            walk(inner, out);
                        }
                    }
                    Item::Fn(f) => {
                        // Items declared in a function body (the UI tests do that).
                        for s in &f.block.stmts {
                            if let syn::Stmt::Item(it) = s {
                                walk(std::slice::from_ref(it), out);
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
        let mut out = Vec::new();
        walk(&file.items, &mut out);
        out
    }

    fn rust_files(dir: &FsPath, out: &mut Vec<PathBuf>) {
        let entries = match std::fs::read_dir(dir) {
            Ok(e) => e,
            Err(_) => return,
        };
        let mut paths: Vec<PathBuf> = entries.filter_map(|e| e.ok()).map(|e| e.path()).collect();
        paths.sort();
        for p in paths {
            let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if p.is_dir() {
                if name != "target" {
                    rust_files(&p, out);
                }
            } else if p.extension().map_or(false, |e| e == "rs") {
                out.push(p);
            }
        }
    }

    /// Every definition in the source of the test suites of the repository
    /// (`tests/`, the std mocks of `announce-std`, and this crate's own sources,
    /// whose tests are full of items in strings that the unit tests below cover
    /// separately). The items are those a user could pass to `#[announce]`, and more.
    /// Without the repository (a packaged crate) there are none.
    #[test]
    fn fast_path_never_accepts_more_than_the_analysis_on_the_test_suites() {
        let root = FsPath::new(env!("CARGO_MANIFEST_DIR")).join("..");
        let mut files = Vec::new();
        for dir in [
            "tests",
            "announce-std/src",
            "announce-core/src/helper",
            "src",
        ] {
            rust_files(&root.join(dir), &mut files);
        }
        if files.is_empty() {
            return;
        }
        let (mut accepted, mut seen, mut parsed) = (0, 0, 0);
        for f in &files {
            let text = std::fs::read_to_string(f).unwrap();
            let file = match syn::parse_file(&text) {
                Ok(file) => file,
                Err(_) => continue,
            };
            parsed += 1;
            let (a, s) = agree_on(items_of(&file));
            accepted += a;
            seen += s;
        }
        assert!(parsed > 20, "only {} files parsed", parsed);
        // The check is not empty: many items are accepted, many are not.
        assert!(seen > 200, "only {} items seen", seen);
        assert!(accepted * 10 > seen, "{} of {} accepted", accepted, seen);
        assert!(accepted < seen, "{} of {} accepted", accepted, seen);
    }

    /// The items the code of this crate writes in its own tests: those of `leak`,
    /// `generate` and `lib`, as the strings and `quote!` blocks they are, are in
    /// the files above; these are the shapes the fast path is designed for.
    const ACCEPTED: &[&str] = &[
        // The benchmark's items.
        "pub trait D { fn m0(&self, x: u32) -> u32; fn m1(&self, x: u32) -> u32; }",
        "pub struct D { pub f0: u32, pub f1: u32 }",
        "struct S;",
        "struct S(u8, bool, str);",
        "pub enum E { A, B(u8), C { x: u8, y: (u8, bool) }, D = 4 }",
        "pub union U { a: u8, b: [u8; 4] }",
        "type A = (u8, [u8; 3], &'static str, fn(u8) -> bool);",
        "type B<T> = ::std::vec::Vec<T>;",
        // Paths that resolve everywhere.
        "struct S { a: ::std::vec::Vec<u8>, b: ::core::option::Option<crate::Local>, c: crate::m::T<u8> }",
        "struct S<T> { a: T, b: ::std::boxed::Box<T>, c: T::Output }",
        "struct S { a: *const u8, b: *mut u8, c: &'static u8, d: &'static mut u8, e: [u8] }",
        "trait Tr { type Out; fn f(&self) -> Self::Out; fn g(&self, o: &Self) -> bool; fn h(self) -> Self; }",
        "trait Tr { type Out; fn f(&self) -> (Self::Out, Self); fn g(&mut self) -> &Self::Out; }",
        "trait Tr { type Out; fn f(&self) -> ::std::boxed::Box<Self::Out>; fn g(&self) -> [Self::Out; 2]; }",
        "trait Tr<T> { fn f(&self, t: T) -> T; fn g<U>(&self, u: U) -> (T, U); }",
        "trait Tr<'a> { fn f(&'a self) -> &'a u8; fn g<'b>(&self, x: &'b u8) -> &'b u8; }",
        "trait Tr: ::core::fmt::Debug + Sized + 'static { fn f(&self) where Self: Sized; }",
        "trait Tr { type G<'a>: ::core::fmt::Debug where Self: 'a; const C: usize; const D: u8 = 3; }",
        "trait Tr { fn f(&self) -> u8 { helper() } }",
        "struct S<const N: usize> { a: [u8; N], b: [u8; 2 * N + 1] }",
        "struct S<T: ?Sized + ::core::fmt::Debug, const N: usize = 3> where T: ::core::clone::Clone { a: [u8; N], b: &'static T }",
        "struct S<'a, T: 'a> { a: &'a T, b: &'a [T] }",
        "trait Tr { fn f<const N: usize>(&self) -> [u8; N]; }",
        "struct S { a: [u8; crate::N], b: [u8; ::core::primitive::usize::MAX] }",
        "enum E { A = crate::N, B = 3, C = 1 << 3 }",
        "trait Tr { fn f(&self, g: fn(u8, bool) -> ::std::vec::Vec<u8>); }",
        "trait Tr { fn f<F: ::core::ops::Fn(u8) -> u8>(&self, f: F) -> u8; }",
        "trait Tr { fn f<T>(&self, t: T) where T: ::core::iter::Iterator<Item = u8>; }",
        "struct S { a: (), b: ! }",
        "trait Tr { fn f(&self, _: u8, mut x: u16) -> u32; }",
        "mod m { pub struct S(Local); }",
    ];

    /// With the options: `Vec` and the primitives need them.
    const ACCEPTED_WITH_PRELUDE: &[&str] = &[
        "struct S { a: Vec<u8>, b: Option<Box<Self>>, c: String }",
        "trait Tr: Clone + Send + Sync { fn f(&self) -> Option<u8>; }",
        "struct S<T: Clone + Default> where T: PartialEq { a: Vec<T>, b: Result<T, String> }",
        "trait Tr { fn f<I: IntoIterator<Item = u8>>(&self, i: I) -> Box<Self>; }",
    ];

    /// Not accepted: every one of them takes the full analysis (the first ones are
    /// errors or entries there, the last ones are only unknown to the scan).
    const NOT_ACCEPTED: &[&str] = &[
        // Relative paths.
        "struct S { a: Local }",
        "struct S { a: inner::Item }",
        "struct S { a: super::X }",
        "struct S { a: std::vec::Vec<u8> }",
        "struct S { a: Vec<u8> }",
        "struct S { a: Option<u8> }",
        "struct S { a: ::std::vec::Vec<Local> }",
        "struct S<T> { a: Vec<T> }",
        "struct S<T> { a: Local<T> }",
        "struct S { a: [u8; N] }",
        "struct S { a: [u8; LOCAL] }",
        "struct S { a: [u8; ::core::mem::size_of::<u8>()] }",
        "enum E { A = LOCAL }",
        "enum E { A = f() }",
        "trait Tr: Base { }",
        "trait Tr { fn f<T: Display>(&self, t: T); }",
        "struct S<T: Clone>(T);",
        "struct S<T> where T: Debug { a: T }",
        "trait Tr { fn f(&self) where Self: Clone; }",
        "trait Tr { fn f(&self, x: Local); }",
        // `Self` where it is not accepted.
        "struct S { a: Self::A }",
        "struct S<T> { a: T::A::B }",
        "type A = Self;",
        "type A<T> = T::Out::X;",
        "trait Tr { type Out; fn f(&self) -> Self::Out::X; }",
        "trait Tr { fn f(&self) -> Local<Self>; }",
        "trait Tr { type Out; fn f(&self) -> Local<Self::Out>; }",
        "trait Tr { type G<'a>; fn f<'a>(&'a self) -> Self::G<'a>; }",
        "trait Tr { fn f(&self) where Self::Out: Clone; }",
        // Forms the scan does not know.
        "struct S { a: Box<dyn ::core::fmt::Debug> }",
        "struct S { a: &'static dyn ::core::fmt::Debug }",
        "trait Tr { fn f(&self) -> impl ::core::fmt::Debug; }",
        "trait Tr { fn f(&self, x: impl ::core::fmt::Debug); }",
        "struct S { a: <u8 as ::core::ops::Add>::Output }",
        "trait Tr { fn f<T: ::core::ops::Add>(&self) -> <T as ::core::ops::Add>::Output; }",
        "struct S { a: ::other::Wrap<u8> }",
        "struct S { a: _ }",
        "struct S { a: m!() }",
        "struct S<'a> { a: &'_ u8, b: &'a u8 }",
        "struct S { a: for<'a> fn(&'a u8) -> &'a u8 }",
        "struct S<F: for<'a> ::core::ops::Fn(&'a u8)>(F);",
        "struct S<T> where for<'a> &'a T: ::core::marker::Copy { a: T }",
        "struct S<T> where Vec<T>: ::core::clone::Clone { a: T }",
        "struct S<const N: Idx>;",
        "struct S<T, const N: usize> { a: Foo<N> }",
        "trait Tr { fn f(&self, (a, b): (u8, u8)); }",
        "trait Tr { async fn f(&self) -> u8; }",
        "trait Tr { unsafe extern \"C\" fn f(&self, ...); }",
        "trait Tr { m!(); }",
        "trait Tr { const N: usize = size_of::<u8>(); }",
        "trait Tr { type Out: Local; }",
        "pub(in crate::m) struct S;",
        "fn f() {}",
    ];

    #[test]
    fn the_shapes_it_is_designed_for_are_accepted() {
        for src in ACCEPTED {
            assert!(fast(src), "not accepted: {}", src);
        }
        for src in ACCEPTED_WITH_PRELUDE {
            assert!(!fast(src), "accepted without the prelude: {}", src);
            assert!(
                accepts(&item(src), &opts(true, true)),
                "not accepted with the prelude: {}",
                src
            );
        }
        // Primitives need `allow_primitive`.
        let u8_only = item("struct S { a: u8, b: ::core::primitive::u8 }");
        assert!(accepts(&u8_only, &opts(true, false)));
        assert!(!accepts(&u8_only, &opts(false, false)));
    }

    #[test]
    fn other_shapes_are_left_to_the_analysis() {
        for src in NOT_ACCEPTED {
            let it = item(src);
            assert!(!accepts(&it, &Options::default()), "accepted: {}", src);
        }
    }

    #[test]
    fn the_analysis_agrees_on_the_listed_items() {
        let all: Vec<Item> = ACCEPTED
            .iter()
            .chain(ACCEPTED_WITH_PRELUDE)
            .chain(NOT_ACCEPTED)
            .map(|s| item(s))
            .collect();
        let (accepted, seen) = agree_on(all);
        assert!(accepted > 60 && accepted < seen, "{} of {}", accepted, seen);
        // The listed accepted items really pass the full analysis, with no entry.
        for src in ACCEPTED {
            let it = item(src);
            if matches!(it, Item::Mod(_)) {
                continue;
            }
            let a = analyze(&it, &Options::default())
                .unwrap_or_else(|e| panic!("{}: {}", src, e))
                .unwrap();
            assert_eq!(a.required().count(), 0, "{}", src);
        }
    }

    #[test]
    fn a_module_is_not_analysed() {
        assert!(fast("mod m { struct S { a: Local } }"));
        assert!(analyze(
            &item("mod m { struct S { a: Local } }"),
            &Options::default()
        )
        .unwrap()
        .is_none());
        assert!(!fast("fn f() {}"));
    }

    // ---- random items ------------------------------------------------------

    /// xorshift64*: the same items on every run.
    struct Rng(u64);

    impl Rng {
        fn next(&mut self) -> u64 {
            self.0 ^= self.0 >> 12;
            self.0 ^= self.0 << 25;
            self.0 ^= self.0 >> 27;
            self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
        }
        fn below(&mut self, n: usize) -> usize {
            (self.next() % n as u64) as usize
        }
        fn pick<'a>(&mut self, xs: &[&'a str]) -> &'a str {
            xs[self.below(xs.len())]
        }
        fn chance(&mut self, percent: usize) -> bool {
            self.below(100) < percent
        }
    }

    /// What a type may name where it is written.
    #[derive(Clone)]
    struct Ctx {
        /// Type parameters in scope.
        types: Vec<&'static str>,
        /// Const parameters in scope.
        consts: Vec<&'static str>,
        /// A trait item (`Self::Out` and the like make sense).
        in_trait: bool,
        /// `'a` is declared.
        lifetime: bool,
        /// Only forms that resolve everywhere, mostly: the share of "good" leaves.
        good: usize,
    }

    const PRIMS: &[&str] = &["u8", "u32", "bool", "char", "str", "f64", "usize", "i128"];
    const GOOD_PATHS: &[&str] = &[
        "::std::string::String",
        "::core::marker::PhantomData<u8>",
        "crate::Local",
        "crate::m::Item",
        "::core::option::Option<u8>",
    ];
    const BAD_LEAVES: &[&str] = &[
        "Local",
        "inner::Item",
        "super::X",
        "std::vec::Vec<u8>",
        "Self::A::B",
        "::other::Wrap<u8>",
        "_",
        "m!()",
        "dyn ::core::fmt::Debug",
        "dyn Local",
        "impl ::core::fmt::Debug",
        "impl Local",
        "<u8 as ::core::ops::Add>::Output",
        "for<'a> fn(&'a u8) -> &'a u8",
        "Local<'_>",
        "&'_ u8",
        "[u8; LOCAL_N]",
        "[u8; ::core::mem::size_of::<u8>()]",
        "(Local, u8)",
        "fn(Local) -> u8",
        "[Local; 2]",
        "$crate::Local",
    ];
    const PRELUDE_LEAVES: &[&str] = &[
        "Vec<u8>",
        "Option<u8>",
        "String",
        "Box<u8>",
        "Result<u8, u8>",
    ];

    fn gen_type(r: &mut Rng, c: &Ctx, depth: usize) -> String {
        // Leaves.
        if depth == 0 || r.chance(35) {
            if r.chance(c.good) {
                let mut leaves: Vec<String> = PRIMS.iter().map(|s| s.to_string()).collect();
                leaves.extend(GOOD_PATHS.iter().map(|s| s.to_string()));
                leaves.push("Self".into());
                for t in &c.types {
                    leaves.push(t.to_string());
                    leaves.push(format!("{}::Output", t));
                }
                if c.in_trait {
                    leaves.push("Self::Out".into());
                }
                return leaves[r.below(leaves.len())].clone();
            }
            return match r.below(10) {
                0..=3 => r.pick(BAD_LEAVES).to_string(),
                4..=6 => r.pick(PRELUDE_LEAVES).to_string(),
                7 => "Self::Out".into(),
                8 => "Self".into(),
                _ => format!("{}::Out", c.types.first().copied().unwrap_or("Local")),
            };
        }
        let inner = |r: &mut Rng| gen_type(r, c, depth - 1);
        match r.below(16) {
            0 => format!("&{}", inner(r)),
            1 if c.lifetime => format!("&'a {}", inner(r)),
            1 => format!("&'b {}", inner(r)),
            2 => format!("&'static {}", inner(r)),
            3 => format!("&mut {}", inner(r)),
            4 => format!("*const {}", inner(r)),
            5 => format!("[{}]", inner(r)),
            6 => {
                let len = if c.consts.is_empty() || r.chance(40) {
                    r.pick(&[
                        "3",
                        "crate::N",
                        "Self::N",
                        "LOCAL_N",
                        "2 * 3",
                        "::core::mem::size_of::<u8>()",
                    ])
                    .to_string()
                } else {
                    c.consts[r.below(c.consts.len())].to_string()
                };
                format!("[{}; {}]", inner(r), len)
            }
            7 => format!("({}, {})", inner(r), inner(r)),
            8 => format!("({},)", inner(r)),
            9 => format!("fn({}) -> {}", inner(r), inner(r)),
            10 => format!("::std::vec::Vec<{}>", inner(r)),
            11 => format!("::core::option::Option<{}>", inner(r)),
            12 => format!("::std::boxed::Box<{}>", inner(r)),
            13 => format!("crate::Wrap<{}, {}>", inner(r), inner(r)),
            14 => format!("Local<{}>", inner(r)),
            _ => format!("Vec<{}>", inner(r)),
        }
    }

    const BOUNDS: &[&str] = &[
        "::core::convert::From<Self>",
        "::core::convert::Into<Self::Out>",
        "::core::iter::Iterator<Item = Self::Out>",
        "::core::ops::Fn(&Self) -> u8",
        "::core::ops::Fn(&u8) -> &u8",
        "::core::ops::FnMut(&mut u8, &u8) -> u8",
        "::core::convert::AsRef<[u8]>",
        "::core::ops::Add<u8, Output = u8>",
        "::core::borrow::Borrow<T>",
        "::core::borrow::Borrow<T::Output>",
        "::core::convert::From<[u8; N]>",
        "::core::convert::From<[u8; Self::N]>",
        "::core::fmt::Debug",
        "::core::clone::Clone + ::core::marker::Copy",
        "crate::Tr",
        "::core::ops::Add<Output = u8>",
        "::core::ops::Fn(u8) -> u8",
        "::core::iter::Iterator<Item = u8>",
        "::core::convert::From<u8>",
        "Sized",
        "?Sized",
        "'a",
    ];
    const BAD_BOUNDS: &[&str] = &[
        "Clone",
        "Local",
        "std::fmt::Debug",
        "Iterator<Item = Local>",
        "Fn(Local)",
        "for<'b> ::core::ops::Fn(&'b u8)",
        "::other::Tr",
    ];

    fn gen_bound(r: &mut Rng, good: usize) -> String {
        if r.chance(good) {
            r.pick(BOUNDS).to_string()
        } else {
            r.pick(BAD_BOUNDS).to_string()
        }
    }

    /// `<..>` and its names: type parameters and const parameters.
    fn gen_generics(r: &mut Rng, c: &mut Ctx, allow_lifetime: bool) -> String {
        let mut params = Vec::new();
        if allow_lifetime && r.chance(30) {
            params.push("'a".to_string());
            c.lifetime = true;
        }
        if r.chance(45) {
            let name = r.pick(&["T", "U"]);
            let g = c.good;
            let p = match r.below(5) {
                0 => name.to_string(),
                1 => format!("{}: {}", name, gen_bound(r, g)),
                2 => format!("{}: {} + {}", name, gen_bound(r, g), gen_bound(r, g)),
                3 => format!(
                    "{} = {}",
                    name,
                    r.pick(&[
                        "u8",
                        "Local",
                        "crate::Local",
                        "Self",
                        "Self::Out",
                        "&'a u8",
                        "&'static u8",
                        "::std::vec::Vec<Self>",
                        "T",
                        "[u8; 2]"
                    ])
                ),
                _ => format!("{}: ?Sized", name),
            };
            c.types.push(if name == "T" { "T" } else { "U" });
            params.push(p);
        }
        if r.chance(25) {
            let ty = r.pick(&["usize", "u8", "bool", "Idx", "crate::Idx"]);
            let default = if r.chance(30) { " = 3" } else { "" };
            params.push(format!("const N: {}{}", ty, default));
            c.consts.push("N");
        }
        if params.is_empty() {
            String::new()
        } else {
            format!("<{}>", params.join(", "))
        }
    }

    fn gen_where(r: &mut Rng, c: &Ctx) -> String {
        if !r.chance(30) {
            return String::new();
        }
        let mut preds = Vec::new();
        for _ in 0..1 + r.below(2) {
            let bounded = match r.below(9) {
                0 | 1 if !c.types.is_empty() => c.types[r.below(c.types.len())].to_string(),
                2 => "Self".to_string(),
                3 => "Self::Out".to_string(),
                4 if !c.types.is_empty() => format!("{}::Output", c.types[0]),
                5 => "&'a T".to_string(),
                6 => "(T, u8)".to_string(),
                _ => gen_type(r, c, 1),
            };
            preds.push(format!("{}: {}", bounded, gen_bound(r, c.good)));
        }
        format!(" where {}", preds.join(", "))
    }

    fn gen_fields(r: &mut Rng, c: &Ctx, named: bool) -> String {
        let n = r.below(4);
        let fields: Vec<String> = (0..n)
            .map(|i| {
                let ty = gen_type(r, c, 2);
                if named {
                    format!("pub f{}: {}", i, ty)
                } else {
                    ty
                }
            })
            .collect();
        fields.join(", ")
    }

    fn gen_method(r: &mut Rng, c: &Ctx, i: usize) -> String {
        let mut c = c.clone();
        let g = gen_generics(r, &mut c, true);
        let g = g.replace("const N", "const M").replace("= 3", "");
        if g.contains("const M") {
            c.consts.push("M");
        }
        let recv = if c.lifetime {
            r.pick(&[
                "&self",
                "&mut self",
                "self",
                "&'a self",
                "&'a mut self",
                "self: ::std::boxed::Box<Self>",
                "self: Box<Self>",
                "self: &Self",
                "self: &'a Self",
                "self: ::std::rc::Rc<Self>",
                "self: ::core::pin::Pin<&mut Self>",
                "",
            ])
        } else {
            r.pick(&[
                "&self",
                "&mut self",
                "self",
                "&'b self",
                "self: ::std::boxed::Box<Self>",
                "self: &mut Self",
                "",
            ])
        };
        let mut args = vec![recv.to_string()];
        for k in 0..r.below(3) {
            args.push(format!("a{}: {}", k, gen_type(r, &c, 2)));
        }
        let args: Vec<String> = args.into_iter().filter(|a| !a.is_empty()).collect();
        let ret = if r.chance(70) {
            format!(" -> {}", gen_type(r, &c, 2))
        } else {
            String::new()
        };
        let wh = gen_where(r, &c);
        let body = if r.chance(15) { " { helper() }" } else { ";" };
        let wh_body = if body == ";" {
            format!("{};", wh)
        } else {
            format!("{}{}", wh, body)
        };
        let _ = wh_body;
        format!("fn m{}{}({}){}{}{}", i, g, args.join(", "), ret, wh, body)
    }

    fn gen_item(r: &mut Rng) -> String {
        let good = if r.chance(50) { 95 } else { 70 };
        let mut c = Ctx {
            types: Vec::new(),
            consts: Vec::new(),
            in_trait: false,
            lifetime: false,
            good,
        };
        let kind = r.below(10);
        let attrs = r.pick(&[
            "",
            "",
            "#[derive(Clone)] ",
            "#[doc = \"x\"] ",
            "#[cfg(a)] #[repr(C)] ",
        ]);
        let generics = gen_generics(r, &mut c, true);
        match kind {
            0..=2 => {
                let named = r.chance(70);
                if r.chance(10) {
                    format!("{}struct S{}{};", attrs, generics, gen_where(r, &c))
                } else if named {
                    format!(
                        "{}struct S{}{} {{ {} }}",
                        attrs,
                        generics,
                        gen_where(r, &c),
                        gen_fields(r, &c, true)
                    )
                } else {
                    format!(
                        "{}struct S{}({}){};",
                        attrs,
                        generics,
                        gen_fields(r, &c, false),
                        gen_where(r, &c)
                    )
                }
            }
            3..=4 => {
                let n = 1 + r.below(3);
                let variants: Vec<String> = (0..n)
                    .map(|i| match r.below(4) {
                        0 => format!("V{}", i),
                        1 => format!("V{}({})", i, gen_fields(r, &c, false)),
                        2 => format!("V{} {{ {} }}", i, gen_fields(r, &c, true)),
                        _ => format!(
                            "V{} = {}",
                            i,
                            r.pick(&[
                                "1",
                                "crate::N",
                                "LOCAL",
                                "-2",
                                "1 << 2",
                                "Self::N",
                                "N",
                                "::core::primitive::u8::MAX"
                            ])
                        ),
                    })
                    .collect();
                format!(
                    "{}enum E{}{} {{ {} }}",
                    attrs,
                    generics,
                    gen_where(r, &c),
                    variants.join(", ")
                )
            }
            5 => format!(
                "{}union U{} {{ {} }}",
                attrs,
                generics,
                gen_fields(r, &c, true)
            ),
            6 => {
                let ctype = gen_type(r, &c, 3);
                format!("{}type A{} = {};", attrs, generics, ctype)
            }
            _ => {
                c.in_trait = true;
                let supers = if r.chance(30) {
                    let mut b = gen_bound(r, c.good);
                    if r.chance(30) {
                        b = format!("{} + {}", b, gen_bound(r, c.good));
                    }
                    format!(": {}", b)
                } else {
                    String::new()
                };
                let mut items = Vec::new();
                for i in 0..r.below(5) {
                    match r.below(8) {
                        0 => items.push(format!(
                            "type Out{};",
                            if r.chance(50) {
                                format!(": {}", gen_bound(r, c.good))
                            } else {
                                String::new()
                            }
                        )),
                        1 => items.push("type Out = u8;".to_string()),
                        2 => items.push(format!(
                            "type G<'a>: {} where Self: 'a;",
                            gen_bound(r, c.good)
                        )),
                        3 => items.push(format!("const C{}: {};", i, gen_type(r, &c, 1))),
                        4 => items.push(format!(
                            "const D{}: usize = {};",
                            i,
                            r.pick(&["3", "crate::N", "LOCAL", "Self::N", "2 + 1"])
                        )),
                        _ => items.push(gen_method(r, &c, i)),
                    }
                }
                // `Out` is declared once.
                let mut seen_out = false;
                items.retain(|i| {
                    if i.starts_with("type Out") {
                        let keep = !seen_out;
                        seen_out = true;
                        keep
                    } else {
                        true
                    }
                });
                format!(
                    "{}trait Tr{}{}{} {{ {} }}",
                    attrs,
                    generics,
                    supers,
                    gen_where(r, &c),
                    items.join(" ")
                )
            }
        }
    }

    /// `ANNOUNCE_FASTPATH_ITEMS=n` and `ANNOUNCE_FASTPATH_SEED=s` run more items,
    /// other ones (a hunt for disagreements; 1200 items take a few seconds).
    fn env_number(name: &str, default: u64) -> u64 {
        std::env::var(name)
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(default)
    }

    #[test]
    fn fast_path_never_accepts_more_than_the_analysis_on_random_items() {
        let count = env_number("ANNOUNCE_FASTPATH_ITEMS", 1200);
        let mut r = Rng(0x9E37_79B9_7F4A_7C15 ^ env_number("ANNOUNCE_FASTPATH_SEED", 0));
        let (mut accepted, mut seen, mut parsed) = (0, 0, 0);
        for _ in 0..count {
            let src = gen_item(&mut r);
            let it = match syn::parse_str::<Item>(&src) {
                Ok(it) => it,
                Err(_) => continue,
            };
            parsed += 1;
            let (a, s) = agree_on([it]);
            accepted += a;
            seen += s;
        }
        assert!(
            parsed * 10 > count * 8,
            "only {} of {} items parsed",
            parsed,
            count
        );
        // The test has something to say: a fair share of the items is accepted, and
        // a fair share is not.
        assert!(accepted * 8 > seen, "{} of {} accepted", accepted, seen);
        assert!(accepted * 2 < seen, "{} of {} accepted", accepted, seen);
    }
}
