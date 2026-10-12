//! Attributes that a check owns, and the query that collects them.
//!
//! A library can put its own attributes inside an announced item:
//!
//! ```text
//! #[announce(my_lib::check)]
//! pub struct S { #[my_lib(skip)] pub a: u8, pub b: u8 }
//! ```
//!
//! Rustc rejects `#[my_lib(skip)]` ("cannot find attribute"), because no derive
//! declares it. So `#[announce]` asks every check which attribute names it owns
//! (the *query*, below), removes the owned attributes from the item it emits, and
//! keeps them in the recorded payload. The check, which runs at the definition,
//! and the callbacks, which run at the call sites, see the attributes.
//!
//! # The query
//!
//! `#[announce(a::check, b::check)]` does not emit the item at once. It emits
//! a call to the first check that carries the rest of the work:
//!
//! ```text
//! a::check! { @__announce_attrs [ b::check!, ::announce::__private::finish! ] { STATE } }
//! ```
//!
//! `STATE` is opaque to the checks. Each check *answers* by calling the first
//! element of the list with the rest of the list, and `STATE` followed by one
//! `attrs(name, ..)` group when it owns attribute names (or unchanged when it
//! owns none):
//!
//! ```text
//! b::check! { @__announce_attrs [ ::announce::__private::finish! ] { STATE attrs(my_lib) } }
//! ```
//!
//! The last element is a hidden macro of `announce`, `finish!`. It reads the names,
//! rejects a name that rustc or a standard tool uses (`cfg`, `doc`, `derive`,
//! `allow`, `rustfmt`, `rustc_*`, ..), and expands to what `#[announce]` expands to
//! without checks, except that the item it emits has no attribute of the owned
//! names. The `META` of the item's trampoline lists the names in one element,
//! `attrs(my_lib, other)` ([`Entry::owned_attrs`](crate::Entry::owned_attrs)).
//!
//! A check that does not answer stops the chain: the item is not emitted, and
//! rustc reports the call of that check (a `macro_rules!` check says "no rules
//! expected the token `@`", a proc-macro check that parses [`CheckInput`](crate::CheckInput)
//! by hand says that it did not answer). **Every check must answer**, also one
//! that owns no attributes. This is a breaking change for checks written before.
//!
//! * A proc-macro check uses [`check!`](crate::check) (or [`CheckCall`] and
//!   [`Query::answer`] by hand):
//!
//! ```ignore
//! announce_helper::check! {
//!     pub fn my_check(input) attributes(my_lib, other) {
//!         for attr in input.owned_attributes() { /* validate */ }
//!         Ok(proc_macro2::TokenStream::new())
//!     }
//! }
//! ```
//!
//! * A `macro_rules!` check has one more arm, which forwards `STATE` and adds the
//!   names (leave out `attrs(..)` when the check owns none). The path of the next
//!   element is matched as in the arms of a trampoline, so `crate::`, `$crate::`
//!   and `::a::b` work:
//!
//! ```text
//! macro_rules! my_check {
//!     (@__announce_attrs [ $($np:ident)? $(:: $ns:ident)* ! $(, $($rest:tt)*)? ] { $($state:tt)* }) => {
//!         $($np)? $(:: $ns)* ! { @__announce_attrs [ $($($rest)*)? ] { $($state)* attrs(my_lib, other) } }
//!     };
//!     (($($args:tt)*), $($item:tt)*) => { /* the check itself */ };
//! }
//! ```
//!
//! # What the check, `#[announce]` and the callbacks do
//!
//! * The check is called again, as before, as `my_check! { (ARGS), ITEM }`, and
//!   `ITEM` has the owned attributes (`#[cfg]` and `#[cfg_attr]` evaluated). It
//!   validates them and returns a `compile_error!` at the offending attribute.
//! * `#[announce]` removes the owned attributes from the item it emits, after the
//!   item's cfgs are evaluated, so an owned attribute inside `#[cfg_attr(..)]`
//!   works. Put `#[announce]` above the item's own owned attributes: rustc
//!   resolves attributes from the top.
//! * The trampoline's payload keeps the attributes. A callback reads them from
//!   [`Entry::item`](crate::Entry::item) ([`OwnedAttrs::find`]). A callback that
//!   emits the item itself, in another crate, must remove them first, since no
//!   macro declares them there either:
//!   [`Entry::item_without_owned_attrs`](crate::Entry::item_without_owned_attrs).
//! * `#[derive(Announce)]` cannot remove attributes from its input: a library
//!   declares its attributes with its own derive (`attributes(my_lib)`). The derive
//!   sends the same query, so every check answers in the same way, and the callbacks
//!   see `attrs(..)`, but nothing is removed.
//!
//! # What is matched
//!
//! An owned name `my_lib` matches an attribute `#[my_lib]`, `#[my_lib(..)]`,
//! `#[my_lib = ..]` and `#[my_lib::..]` (the first segment of its path, with no
//! leading `::`: `#[::my_lib::x]` names a crate and is rustc's business). Both
//! outer (`#[..]`) and inner (`#![..]`) attributes are matched, wherever syn
//! parses attributes in the item: on the item, its fields, variants, generic
//! parameters, trait and impl items, the function parameters and statements of
//! the bodies, and on everything inside an inline `mod`. Attributes inside macro
//! invocations and `macro_rules!` bodies are token trees, which are not looked
//! at. An owned name must not be read by another macro on the item (a derive's
//! helper attribute would be removed before the derive runs).

use crate::{Cont, ContList};
use proc_macro2::{Ident, Span, TokenStream};
use std::collections::HashSet;
use syn::ext::IdentExt;
use syn::parse::{Parse, ParseStream};
use syn::punctuated::Punctuated;
use syn::visit::Visit;
use syn::visit_mut::VisitMut;
use syn::{Attribute, Item, Result, Token};
use template_quote::{quote, ToTokens};

/// The word after `@` in the query.
pub(crate) const QUERY_TAG: &str = "__announce_attrs";

/// The name of the META element and of the query answer: `attrs(a, b)`.
pub(crate) const META_NAME: &str = "attrs";

/// Attribute names that rustc, a standard tool or announce itself uses. An owned
/// name must not be one of them: the item would lose an attribute that rustc
/// needs. Names that start with `rustc_` or `__announce` are reserved as well.
const RESERVED: &[&str] = &[
    // Conditional compilation, macros and tests.
    "cfg",
    "cfg_attr",
    "cfg_eval",
    "cfg_accessible",
    "derive",
    "derive_const",
    "test",
    "bench",
    "test_case",
    "global_allocator",
    "proc_macro",
    "proc_macro_derive",
    "proc_macro_attribute",
    "macro_use",
    "macro_export",
    "macro_reexport",
    "path",
    // Lints, documentation and diagnostics.
    "allow",
    "warn",
    "deny",
    "forbid",
    "expect",
    "doc",
    "deprecated",
    "must_use",
    "must_not_suspend",
    "ignore",
    "should_panic",
    "automatically_derived",
    "diagnostic",
    "stable",
    "unstable",
    "allow_internal_unstable",
    "allow_internal_unsafe",
    // Code generation and layout.
    "inline",
    "cold",
    "track_caller",
    "target_feature",
    "instruction_set",
    "optimize",
    "naked",
    "no_mangle",
    "export_name",
    "link_name",
    "link_section",
    "link_ordinal",
    "link",
    "linkage",
    "used",
    "repr",
    "non_exhaustive",
    "thread_local",
    "coverage",
    "no_sanitize",
    "sanitize",
    "cfi_encoding",
    "patchable_function_entry",
    "ffi_const",
    "ffi_pure",
    "ffi_returns_twice",
    "collapse_debuginfo",
    "debugger_visualizer",
    "export_stable",
    "pointee",
    "const_trait",
    "marker",
    "fundamental",
    "lang",
    "unsafe",
    "define_opaque",
    "custom_mir",
    "loop_match",
    "const_continue",
    // Crate level.
    "crate_name",
    "crate_type",
    "no_std",
    "no_core",
    "no_main",
    "no_implicit_prelude",
    "no_link",
    "no_builtins",
    "recursion_limit",
    "type_length_limit",
    "windows_subsystem",
    "panic_handler",
    "global_asm",
    "feature",
    "register_tool",
    "register_attr",
    "plugin",
    "prelude_import",
    "start",
    "default_lib_allocator",
    "needs_allocator",
    "panic_runtime",
    // Tools.
    "rustfmt",
    "clippy",
    "rustc",
    "rustdoc",
    "miri",
    "rust_analyzer",
    // announce itself.
    "announce",
];

/// Why `name` cannot be an owned attribute name, if it cannot.
fn reserved(name: &str) -> Option<&'static str> {
    if RESERVED.contains(&name) {
        Some("an attribute that rustc or a standard tool uses")
    } else if name.starts_with("rustc_") {
        Some("a name reserved for rustc (`rustc_*`)")
    } else if name.starts_with("__announce") {
        Some("a name reserved for announce (`__announce*`)")
    } else {
        None
    }
}

/// A set of attribute names that a check owns: `my_lib`, `other`.
///
/// It is what a check answers to the query ([`Query::answer`]), what META lists as
/// `attrs(..)` ([`Entry::owned_attrs`](crate::Entry::owned_attrs)), and what finds
/// ([`find`](Self::find)) and removes ([`strip`](Self::strip)) the attributes
/// in an item. See the [module](self) docs for what a name matches.
///
/// ```
/// use announce_helper::OwnedAttrs;
/// let owned = OwnedAttrs::from_strs(&["my_lib"]);
/// let mut item: syn::Item = syn::parse_quote! {
///     #[derive(Debug)]
///     struct S { #[my_lib(skip)] a: u8, #[my_lib::tag = "x"] #[doc = "kept"] b: u8 }
/// };
/// assert_eq!(owned.find(&item).len(), 2);
/// assert!(owned.strip(&mut item).unwrap());
/// let out = template_quote::ToTokens::to_token_stream(&item).to_string();
/// assert!(out.contains("derive (Debug)") && out.contains("doc") && !out.contains("my_lib"));
/// ```
#[derive(Clone, Debug, Default)]
pub struct OwnedAttrs {
    names: Vec<Ident>,
}

impl PartialEq for OwnedAttrs {
    fn eq(&self, other: &Self) -> bool {
        self.keys() == other.keys()
    }
}
impl Eq for OwnedAttrs {}

impl OwnedAttrs {
    /// No names.
    pub fn new() -> Self {
        Self::default()
    }

    /// From identifiers; a repeated name is kept once, in the order of first use.
    pub fn from_idents(names: impl IntoIterator<Item = Ident>) -> Self {
        let mut out = OwnedAttrs::new();
        out.extend(names);
        out
    }

    /// From names written as text (`"my_lib"`; a raw identifier `"r#type"` is its
    /// plain name).
    pub fn from_strs(names: &[&str]) -> Self {
        Self::from_idents(
            names
                .iter()
                .map(|n| Ident::new(n.strip_prefix("r#").unwrap_or(n), Span::call_site())),
        )
    }

    /// Adds names; a name that is already present is skipped.
    pub fn extend(&mut self, names: impl IntoIterator<Item = Ident>) {
        for n in names {
            if !self.contains(&n.unraw().to_string()) {
                self.names.push(n);
            }
        }
    }

    /// The names, in the order they were added.
    pub fn names(&self) -> &[Ident] {
        &self.names
    }

    /// Whether there are no names.
    pub fn is_empty(&self) -> bool {
        self.names.is_empty()
    }

    /// Whether `name` is one of the names.
    pub fn contains(&self, name: &str) -> bool {
        self.names.iter().any(|n| n.unraw() == name)
    }

    fn keys(&self) -> Vec<String> {
        self.names.iter().map(|n| n.unraw().to_string()).collect()
    }

    /// Whether `attr` is owned: the first segment of its path (without a leading
    /// `::`) is one of the names.
    pub fn owns(&self, attr: &Attribute) -> bool {
        let path = attr.path();
        path.leading_colon.is_none()
            && path
                .segments
                .first()
                .map_or(false, |s| self.contains(&s.ident.unraw().to_string()))
    }

    /// The owned attributes of `item`, in source order, wherever syn parses them
    /// (see the [module](self) docs).
    pub fn find(&self, item: &Item) -> Vec<Attribute> {
        struct V<'a>(&'a OwnedAttrs, Vec<Attribute>);
        impl<'a, 'ast> Visit<'ast> for V<'a> {
            fn visit_attribute(&mut self, a: &'ast Attribute) {
                if self.0.owns(a) {
                    self.1.push(a.clone());
                }
            }
        }
        let mut v = V(self, Vec::new());
        v.visit_item(item);
        v.1
    }

    /// Removes the owned attributes from `item`. Returns whether there were any.
    pub fn strip(&self, item: &mut Item) -> Result<bool> {
        if self.is_empty() {
            return Ok(false);
        }
        remove_attributes(item, |a| self.owns(a))
    }

    /// An error at the first name that rustc, a standard tool or announce uses.
    pub fn check_names(&self) -> Result<()> {
        for n in &self.names {
            if let Some(why) = reserved(&n.unraw().to_string()) {
                return Err(syn::Error::new(
                    n.span(),
                    format!(
                        "announce: `{}` cannot be an owned attribute name: it is {}",
                        n, why
                    ),
                ));
            }
        }
        Ok(())
    }

    /// Reads `attrs(a, b)` (the answer of a check and the META element): the
    /// tokens inside the parentheses are the names.
    pub fn parse_names(tokens: TokenStream) -> Result<Self> {
        let p: Punctuated<Ident, Token![,]> =
            syn::parse::Parser::parse2(Punctuated::parse_terminated, tokens)?;
        Ok(Self::from_idents(p))
    }
}

impl ToTokens for OwnedAttrs {
    /// `attrs(a, b)`, or nothing when there are no names.
    fn to_tokens(&self, tokens: &mut TokenStream) {
        if self.is_empty() {
            return;
        }
        let name = Ident::new(META_NAME, Span::call_site());
        let names = &self.names;
        tokens.extend(quote! { #name(#(#names),*) });
    }
}

/// The path that stands in for an attribute about to be removed.
const MARK: &str = "__announce_removed_attribute";

/// Removes every attribute of `item` for which `remove` returns true, at any
/// depth that syn parses (not inside macro invocations). Returns whether any
/// was removed.
///
/// `syn`'s `VisitMut` can change an attribute but not take it out of its list
/// (the hook for that is recent), so a removed attribute is renamed to a mark,
/// the marks are dropped from the printed tokens, and the item is parsed again.
///
/// ```
/// let mut item: syn::Item = syn::parse_quote! { #[a] struct S { #[b] x: u8 } };
/// let removed = announce_helper::remove_attributes(&mut item, |a| a.path().is_ident("b")).unwrap();
/// assert!(removed);
/// let out = template_quote::ToTokens::to_token_stream(&item).to_string();
/// assert!(out.contains("# [a]") && !out.contains("# [b]"));
/// ```
pub fn remove_attributes(item: &mut Item, remove: impl FnMut(&Attribute) -> bool) -> Result<bool> {
    struct V<F>(F, bool);
    impl<F: FnMut(&Attribute) -> bool> VisitMut for V<F> {
        fn visit_attribute_mut(&mut self, a: &mut Attribute) {
            if (self.0)(a) {
                let mark: syn::Path = Ident::new(MARK, Span::call_site()).into();
                match &mut a.meta {
                    syn::Meta::Path(p) => *p = mark,
                    syn::Meta::List(l) => l.path = mark,
                    syn::Meta::NameValue(nv) => nv.path = mark,
                }
                self.1 = true;
            }
            syn::visit_mut::visit_attribute_mut(self, a);
        }
    }
    let mut v = V(remove, false);
    v.visit_item_mut(item);
    if !v.1 {
        return Ok(false);
    }
    *item = syn::parse2(drop_marked_attributes(item.to_token_stream()))?;
    Ok(true)
}

/// Removes the `#[MARK ..]` and `#![MARK ..]` attributes, at any depth.
fn drop_marked_attributes(ts: TokenStream) -> TokenStream {
    use proc_macro2::{Delimiter, Group, TokenTree};
    let tts: Vec<TokenTree> = ts.into_iter().collect();
    let mut out = Vec::with_capacity(tts.len());
    let mut i = 0;
    while i < tts.len() {
        match &tts[i] {
            TokenTree::Punct(p) if p.as_char() == '#' => {
                let mut j = i + 1;
                if matches!(tts.get(j), Some(TokenTree::Punct(b)) if b.as_char() == '!') {
                    j += 1;
                }
                if let Some(TokenTree::Group(g)) = tts.get(j) {
                    let marked = g.delimiter() == Delimiter::Bracket
                        && matches!(
                            g.stream().into_iter().next(),
                            Some(TokenTree::Ident(id)) if id == MARK
                        );
                    if marked {
                        i = j + 1;
                        continue;
                    }
                }
                out.push(tts[i].clone());
            }
            TokenTree::Group(g) => {
                let mut ng = Group::new(g.delimiter(), drop_marked_attributes(g.stream()));
                ng.set_span(g.span());
                out.push(TokenTree::Group(ng));
            }
            t => out.push(t.clone()),
        }
        i += 1;
    }
    out.into_iter().collect()
}

/// The query a check receives: `@__announce_attrs [ NEXT!, .. ] { STATE }`.
///
/// Answer it with [`answer`](Self::answer); see the [module](self) docs for the
/// protocol. [`check!`](crate::check) does this for a proc-macro check.
#[derive(Clone, Debug)]
pub struct Query {
    /// The elements still to be called, in order; the last one is `finish!`.
    pub list: ContList,
    /// The state that travels along. Opaque: pass it on unchanged.
    pub state: TokenStream,
}

impl Parse for Query {
    fn parse(input: ParseStream) -> Result<Self> {
        input.parse::<Token![@]>()?;
        let tag: Ident = input.parse()?;
        if tag != QUERY_TAG {
            return Err(syn::Error::new(
                tag.span(),
                format!("announce: expected `@{}`", QUERY_TAG),
            ));
        }
        let list;
        syn::bracketed!(list in input);
        let state;
        syn::braced!(state in input);
        let q = Query {
            list: list.parse()?,
            state: state.parse()?,
        };
        if !input.is_empty() {
            return Err(input.error("announce: unexpected tokens after the attribute query"));
        }
        Ok(q)
    }
}

impl Query {
    /// The call that answers the query for a check owning `owned`: the next
    /// element of the list, with the rest of the list and the state plus
    /// `attrs(..)` (the state unchanged if `owned` is empty). A name that
    /// collides with a reserved attribute is a `compile_error!`.
    pub fn answer(&self, owned: &OwnedAttrs) -> TokenStream {
        if let Err(e) = owned.check_names() {
            return e.to_compile_error();
        }
        let mut it = self.list.0.iter();
        let next = match it.next() {
            Some(c @ Cont::Bang(_)) => c,
            _ => {
                return quote! {
                    ::core::compile_error! { "announce: the attribute query has no macro to call next: a check before this one did not forward the rest of its list (`finish!` ends it)" }
                }
            }
        };
        let rest: ContList = it.cloned().collect();
        let state = &self.state;
        quote! { #next { @__announce_attrs #rest { #state #owned } } }
    }
}

/// What a proc-macro check receives: the attribute [`Query`], or its
/// own input (`(ARGS), ITEM`).
///
/// [`check!`](crate::check) parses this and answers the query for you.
#[allow(clippy::large_enum_variant)]
pub enum CheckCall {
    /// The query of `#[announce]`: answer it with [`Query::answer`].
    Query(Query),
    /// The check itself.
    Run(crate::CheckInput),
}

impl Parse for CheckCall {
    fn parse(input: ParseStream) -> Result<Self> {
        if input.peek(Token![@]) {
            Ok(CheckCall::Query(input.parse()?))
        } else {
            Ok(CheckCall::Run(input.parse()?))
        }
    }
}

/// The attribute names of a META `attrs(..)` element, if it is one.
pub(crate) fn owned_from_meta(meta: &crate::Meta) -> Option<OwnedAttrs> {
    if !meta.path.is_ident(META_NAME) {
        return None;
    }
    OwnedAttrs::parse_names(meta.args.clone().unwrap_or_default()).ok()
}

/// All the owned names that the META of an entry lists, merged.
pub(crate) fn owned_from_metas(metas: &[crate::Meta]) -> OwnedAttrs {
    let mut out = OwnedAttrs::new();
    let mut seen = HashSet::new();
    for m in metas {
        if let Some(o) = owned_from_meta(m) {
            for n in o.names {
                if seen.insert(n.unraw().to_string()) {
                    out.names.push(n);
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(ts: impl ToTokens) -> String {
        ts.to_token_stream()
            .to_string()
            .split_whitespace()
            .collect()
    }

    fn item(ts: TokenStream) -> Item {
        syn::parse2(ts).unwrap()
    }

    #[test]
    fn matches_the_first_path_segment() {
        let owned = OwnedAttrs::from_strs(&["my_lib"]);
        let attrs: Vec<Attribute> = vec![
            syn::parse_quote!(#[my_lib]),
            syn::parse_quote!(#[my_lib(skip)]),
            syn::parse_quote!(#[my_lib = "x"]),
            syn::parse_quote!(#[my_lib::skip]),
            syn::parse_quote!(#[my_lib::a::b(1)]),
        ];
        assert!(attrs.iter().all(|a| owned.owns(a)));
        let others: Vec<Attribute> = vec![
            syn::parse_quote!(#[other]),
            syn::parse_quote!(#[my_libs]),
            syn::parse_quote!(#[a::my_lib]),
            syn::parse_quote!(#[::my_lib::x]),
            syn::parse_quote!(#[doc = "my_lib"]),
            syn::parse_quote!(#[cfg(my_lib)]),
        ];
        assert!(others.iter().all(|a| !owned.owns(a)));
    }

    #[test]
    fn finds_attributes_in_every_position() {
        let owned = OwnedAttrs::from_strs(&["t"]);
        let it = item(quote! {
            #[t(0)]
            pub struct S<#[t(1)] T, #[t(2)] const N: usize> {
                #[t(3)] a: T,
                b: [u8; N],
            }
        });
        assert_eq!(owned.find(&it).len(), 4);

        let it = item(quote! {
            #[t(0)]
            pub enum E { #[t(1)] A, B(#[t(2)] u8), #[t(3)] C { #[t(4)] x: u8 } }
        });
        assert_eq!(owned.find(&it).len(), 5);

        let it = item(quote! {
            #[t(0)]
            pub trait Tr {
                #[t(1)] type A;
                #[t(2)] const C: u8;
                #[t(3)] fn f(&self, #[t(4)] x: u8) -> u8 { #[t(5)] let y = x; y }
            }
        });
        assert_eq!(owned.find(&it).len(), 6);

        let it = item(quote! {
            #[t(0)]
            mod m {
                #![t(1)]
                #[t(2)] pub struct S { #[t(3)] a: u8 }
                #[t(4)] fn f() {}
            }
        });
        let found = owned.find(&it);
        assert_eq!(found.len(), 5);
        assert!(found
            .iter()
            .any(|a| matches!(a.style, syn::AttrStyle::Inner(_))));
    }

    #[test]
    fn leaves_macro_bodies_alone() {
        let owned = OwnedAttrs::from_strs(&["t"]);
        let mut it = item(quote! {
            mod m { m!(#[t] x); macro_rules! k { () => { #[t] struct Z; } } #[t] struct Y; }
        });
        assert_eq!(owned.find(&it).len(), 1);
        assert!(owned.strip(&mut it).unwrap());
        let out = s(&it);
        assert!(
            out.contains("m!(#[t]x)") && out.contains("#[t]structZ"),
            "{}",
            out
        );
        assert!(
            out.contains("structY") && !out.contains("#[t]structY"),
            "{}",
            out
        );
    }

    #[test]
    fn strips_and_keeps_everything_else() {
        let owned = OwnedAttrs::from_strs(&["my_lib", "other"]);
        let mut it = item(quote! {
            /// docs
            #[derive(Clone)]
            #[my_lib(a)]
            pub struct S {
                #[other] #[serde(rename = "x")] pub a: u8,
                #[my_lib::tag = "y"] pub b: u8,
            }
        });
        assert!(owned.strip(&mut it).unwrap());
        assert_eq!(
            s(&it),
            "#[doc=\"docs\"]#[derive(Clone)]pubstructS{#[serde(rename=\"x\")]puba:u8,pubb:u8,}"
        );
        // Nothing to strip: the item is not touched.
        assert!(!owned.strip(&mut it).unwrap());
        assert!(!OwnedAttrs::new().strip(&mut it).unwrap());
    }

    #[test]
    fn names_are_unique_ordered_and_printed() {
        let o = OwnedAttrs::from_strs(&["b", "a", "b", "r#a"]);
        assert_eq!(s(&o), "attrs(b,a)");
        assert_eq!(s(OwnedAttrs::new()), "");
        let back = OwnedAttrs::parse_names(quote! { b, a }).unwrap();
        assert_eq!(back, o);
        assert!(o.contains("a") && !o.contains("c"));
    }

    #[test]
    fn reserved_names_are_errors() {
        for bad in [
            "cfg",
            "doc",
            "derive",
            "allow",
            "inline",
            "repr",
            "rustc_foo",
            "__announce_x",
            "announce",
            "clippy",
        ] {
            let e = OwnedAttrs::from_strs(&[bad]).check_names().unwrap_err();
            assert!(e.to_string().contains(bad), "{}: {}", bad, e);
        }
        for ok in ["my_lib", "skip", "sumtype", "serde", "doc_hidden"] {
            OwnedAttrs::from_strs(&[ok]).check_names().unwrap();
        }
    }

    #[test]
    fn the_query_is_answered_by_calling_the_next_element() {
        let q: Query = syn::parse_quote! {
            @__announce_attrs [ b::check!, ::announce::__private::finish! ] { [1u64; attr (x) { struct S; }] }
        };
        assert_eq!(q.list.0.len(), 2);
        assert_eq!(
            s(q.answer(&OwnedAttrs::from_strs(&["a", "c"]))),
            "b::check!{@__announce_attrs[::announce::__private::finish!]{[1u64;attr(x){structS;}]attrs(a,c)}}"
        );
        // Owning nothing: the state is passed on as it is.
        assert_eq!(
            s(q.answer(&OwnedAttrs::new())),
            "b::check!{@__announce_attrs[::announce::__private::finish!]{[1u64;attr(x){structS;}]}}"
        );
        // The last element gets an empty list.
        let q: Query = syn::parse_quote! { @__announce_attrs [ f::finish! ] { st } };
        assert_eq!(
            s(q.answer(&OwnedAttrs::from_strs(&["a"]))),
            "f::finish!{@__announce_attrs[]{stattrs(a)}}"
        );
        // Nothing to call: an error, not a panic.
        let q: Query = syn::parse_quote! { @__announce_attrs [] { st } };
        assert!(s(q.answer(&OwnedAttrs::new())).contains("compile_error"));
        // A reserved name is reported where the check answers.
        let q: Query = syn::parse_quote! { @__announce_attrs [ f::finish! ] { st } };
        let out = s(q.answer(&OwnedAttrs::from_strs(&["doc"])));
        assert!(
            out.contains("compile_error") && out.contains("cannotbeanownedattributename"),
            "{}",
            out
        );
    }

    #[test]
    fn malformed_queries_are_errors() {
        for bad in [
            quote! { @other [ f! ] { } },
            quote! { @__announce_attrs { } },
            quote! { @__announce_attrs [ f! ] { } extra },
            quote! { (a), struct S; },
        ] {
            assert!(syn::parse2::<Query>(bad).is_err());
        }
    }

    #[test]
    fn a_check_call_is_a_query_or_its_input() {
        let q: CheckCall = syn::parse_quote! { @__announce_attrs [ f! ] { } };
        assert!(matches!(q, CheckCall::Query(_)));
        let r: CheckCall = syn::parse_quote! { (a, b), struct S; };
        match r {
            CheckCall::Run(i) => assert_eq!(s(i.args), "a,b"),
            _ => panic!(),
        }
    }
}
