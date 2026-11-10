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
//! (`tests/fast_path.rs` checks this, through `check_fast_path`, on the items of the
//! test suites and on random items, and the analysis checks it again on every item it
//! sees in a test build of this crate).
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

use crate::leak::PRELUDE;
use crate::options::Options;
use syn::visit::Visit;
use syn::{
    Attribute, Expr, GenericArgument, GenericParam, Generics, Ident, Item, Lit, Macro, Pat, Path,
    PathArguments, Signature, TraitBound, TraitBoundModifier, TraitItem, Type, TypeParamBound,
    Visibility, WherePredicate,
};
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

pub(crate) const PRIMITIVES: &[&str] = &[
    "bool", "char", "str", "u8", "u16", "u32", "u64", "u128", "usize", "i8", "i16", "i32", "i64",
    "i128", "isize", "f32", "f64",
];

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

/// What [`accepts`] says of `item`, checked against `result`, the full analysis of
/// the same item: an item that is accepted must have no required entry and no error.
/// `Ok(accepted)` when they agree; `Err` with a message when the fast path accepted an
/// item that the analysis needs an entry for or rejects.
pub(crate) fn compare(
    item: &Item,
    opts: &Options,
    result: &crate::leak::Analysed,
) -> Result<bool, String> {
    if !accepts(item, opts) {
        return Ok(false);
    }
    match result {
        Ok(None) => {}
        Ok(Some(a)) => {
            if let Some((ix, ty)) = a.required().next() {
                return Err(format!(
                    "the fast path accepted an item whose analysis needs entry {}: {}\n{}",
                    ix,
                    ty.to_token_stream(),
                    item.to_token_stream()
                ));
            }
        }
        Err(e) => {
            return Err(format!(
                "the fast path accepted an item that the analysis rejects: {:?}\n{}",
                e,
                item.to_token_stream()
            ))
        }
    }
    Ok(true)
}

/// In a test build, the full analysis checks every answer of [`accepts`] that it
/// is given the item for (`leak::analyze`), with [`compare`].
#[cfg(test)]
pub(crate) fn check_analysis(item: &Item, opts: &Options, result: &crate::leak::Analysed) {
    if let Err(message) = compare(item, opts, result) {
        panic!("{}", message);
    }
}

/// The analysis of an item that the fast path accepted, in a test build, to
/// check the answer (see [`check_analysis`]).
#[cfg(test)]
pub(crate) fn assert_agrees(item: &Item, opts: &Options) {
    let _ = crate::leak::analyze(item, opts);
}
