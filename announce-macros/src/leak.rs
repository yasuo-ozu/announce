//! Type context of a definition: `#[announce(repeater = path)]` and the
//! type-leak table (DESIGN.md section 4.5).
//!
//! On the cfg-evaluated item, type-leak finds the types that would not
//! resolve at a caller: relative paths such as `Local` or `super::X`.
//! The decision is type-leak's own: `Referrer::required()` after
//! `Leaker::finish()`, with primitives and `crate::` paths allowed. Std items
//! must be written absolutely (`::std::vec::Vec<u8>`); prelude names such as
//! `Vec` or `Option` count as relative unless `allow_std_prelude = true`
//! (type-leak's `allowed_paths`, with the names in [`PRELUDE`]).
//!
//! type-leak sees the item as written. `Self` is not internable
//! (`allow_self = false`): it has no meaning in an `impl .. for ()`, and it
//! resolves everywhere. On top of type-leak, announce checks
//! - trait paths (supertraits, bounds, `impl`/`dyn` bounds): absolute only,
//! - value paths in array lengths, enum discriminants, const generic arguments
//!   and const defaults: absolute, `Self::..` or a const parameter (type-leak
//!   does not read const defaults, and a repeater only carries types),
//! - that no type to carry names a lifetime bound by a `for<..>` of the item.
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
use syn::visit::Visit;
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
/// (AUDIT 3 #5). type-leak reads the values in types, but not const defaults,
/// and a relative array length would make it carry the whole array type.
fn check_values(item: &Item, const_params: &[String]) -> Result<()> {
    struct V<'a> {
        consts: &'a [String],
        err: Option<syn::Error>,
    }
    impl V<'_> {
        fn check(&mut self, e: &syn::Expr) {
            struct P<'b, 'a>(&'b mut V<'a>);
            impl<'ast> Visit<'ast> for P<'_, '_> {
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
            Visit::visit_expr(&mut P(self), e);
        }
    }
    impl<'ast> Visit<'ast> for V<'_> {
        fn visit_type_array(&mut self, a: &'ast syn::TypeArray) {
            self.check(&a.len);
            syn::visit::visit_type_array(self, a);
        }
        fn visit_variant(&mut self, v: &'ast syn::Variant) {
            if let Some((_, e)) = &v.discriminant {
                self.check(e);
            }
            syn::visit::visit_variant(self, v);
        }
        fn visit_trait_item_const(&mut self, c: &'ast syn::TraitItemConst) {
            if let Some((_, e)) = &c.default {
                self.check(e);
            }
            syn::visit::visit_trait_item_const(self, c);
        }
        fn visit_const_param(&mut self, c: &'ast syn::ConstParam) {
            if let Some(e) = &c.default {
                self.check(e);
            }
            syn::visit::visit_const_param(self, c);
        }
        fn visit_generic_argument(&mut self, a: &'ast syn::GenericArgument) {
            if let syn::GenericArgument::Const(e) = a {
                self.check(e);
            }
            syn::visit::visit_generic_argument(self, a);
        }
        // Provided-method bodies are not analysed (documented gap).
        fn visit_block(&mut self, _: &'ast syn::Block) {}
    }
    let mut v = V { consts: const_params, err: None };
    Visit::visit_item(&mut v, item);
    v.err.map_or(Ok(()), Err)
}

/// The first lifetime of `ty` that is neither `'static` nor `'_`, nor declared
/// by a generic parameter of the item, nor bound inside `ty` itself: one bound
/// by a `for<..>` of the item (`for<'b> fn(Local<'b>)`). The table could not
/// declare it.
fn undeclared_lifetime(ty: &Type, params: &[GenericParam]) -> Option<syn::Lifetime> {
    struct V<'a> {
        declared: &'a [GenericParam],
        bound: Vec<String>,
        found: Option<syn::Lifetime>,
    }
    impl V<'_> {
        fn binder(&mut self, b: &Option<syn::BoundLifetimes>, f: impl FnOnce(&mut Self)) {
            let depth = self.bound.len();
            for p in b.iter().flat_map(|b| b.lifetimes.iter()) {
                if let GenericParam::Lifetime(l) = p {
                    self.bound.push(l.lifetime.to_string());
                }
            }
            f(self);
            self.bound.truncate(depth);
        }
    }
    impl<'ast> Visit<'ast> for V<'_> {
        fn visit_lifetime(&mut self, l: &'ast syn::Lifetime) {
            let name = l.to_string();
            let declared = self.bound.contains(&name)
                || name == "'static"
                || name == "'_"
                || self.declared.iter().any(|p| matches!(p, GenericParam::Lifetime(d) if d.lifetime.to_string() == name));
            if !declared && self.found.is_none() {
                self.found = Some(l.clone());
            }
        }
        fn visit_type_bare_fn(&mut self, f: &'ast syn::TypeBareFn) {
            self.binder(&f.lifetimes, |v| syn::visit::visit_type_bare_fn(v, f));
        }
        fn visit_trait_bound(&mut self, b: &'ast syn::TraitBound) {
            self.binder(&b.lifetimes, |v| syn::visit::visit_trait_bound(v, b));
        }
    }
    let mut v = V { declared: params, bound: Vec::new(), found: None };
    v.visit_type(ty);
    v.found
}

/// The errors of type-leak, one `syn::Error` per offending path or type: the
/// headline, the notes that say why, and what to write instead.
fn report(e: type_leak::NotInternableError) -> syn::Error {
    let mut out: Option<syn::Error> = None;
    for item in e.items() {
        let mut message = format!("announce: {}", item.message);
        for (_, note) in &item.notes {
            message.push_str(&format!("\n  = note: {}", note));
        }
        // The options of type-leak's help that name its own API (`LeakerConfig`,
        // `allow_self`) mean nothing to a user of `#[announce]`.
        let help = item
            .help_text()
            .unwrap_or("write the paths in it as absolute paths")
            .replace(", or allow it with `LeakerConfig::allowed_paths`", "")
            .replace(", allow it with `LeakerConfig::allowed_paths`", "")
            .replace(", or analyze the item with `allow_self = true`", "")
            .replace(", analyze the item with `allow_self = true`", "");
        message.push_str(&format!("\n  = help: {}", help));
        let err = syn::Error::new(item.span, message);
        match &mut out {
            Some(o) => o.combine(err),
            None => out = Some(err),
        }
    }
    out.expect("a NotInternableError has at least one item")
}

/// The types of `item` that would not resolve at a caller, as they appear in
/// `item`. `None` for items that are not analysed (inline modules: the
/// documented wrapper pattern).
pub fn analyze(item: &Item, opts: &crate::options::Options) -> Result<Option<Vec<Type>>> {
    if !matches!(item, Item::Struct(_) | Item::Enum(_) | Item::Union(_) | Item::Trait(_) | Item::Type(_)) {
        return Ok(None);
    }
    check_trait_paths(item, opts.allow_std_prelude)?;
    let params = scope::collect_params(item);
    let consts: Vec<String> = params
        .iter()
        .filter_map(|p| match p {
            GenericParam::Const(c) => Some(c.ident.to_string()),
            _ => None,
        })
        .collect();
    check_values(item, &consts)?;
    // Primitives (optionally), `crate::` paths and (optionally) the prelude
    // are allowed.
    let mut config = type_leak::LeakerConfig::new();
    config.allow_crate();
    if opts.allow_primitive {
        config.allow_primitive();
    }
    if opts.allow_std_prelude {
        config
            .allowed_paths
            .extend(PRELUDE.iter().map(|n| Path::from(Ident::new(n, Span::call_site()))));
    }
    let mut leaker = type_leak::Leaker::from_config(config);
    // `Self` has no meaning in an `impl .. for ()`, and resolves everywhere:
    // `allow_self = false`.
    let analysed = match item {
        Item::Struct(s) => leaker.intern_struct(s, false),
        Item::Enum(e) => leaker.intern_enum(e, false),
        Item::Union(u) => leaker.intern_union(u, false),
        Item::Trait(t) => leaker.intern_trait(t, false),
        Item::Type(a) => leaker
            .enter_generics(&a.generics, |view| syn::visit::visit_item_type(view, a))
            .map(|(context, ())| context),
        _ => return Ok(None),
    };
    analysed.map_err(report)?;
    // Only the required entries need a repeater; absolute paths of other
    // crates (`::std::vec::Vec<u8>`) are optional entries. The order is
    // type-leak's own, which is deterministic. The same type can be an entry
    // in several contexts (method generics with different bounds); the table
    // has one impl for it.
    let referrer = leaker.finish();
    let mut types: Vec<Type> = Vec::new();
    for (_, t) in referrer.required() {
        if let Some(lifetime) = undeclared_lifetime(t, &params) {
            return Err(syn::Error::new_spanned(
                t,
                format!(
                    "announce: this type could not be mapped back to the item after analysis; \
                     write it with absolute paths (it names the higher-ranked lifetime `{}`)",
                    lifetime
                ),
            ));
        }
        if !types.contains(t) {
            types.push(t.clone());
        }
    }
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
