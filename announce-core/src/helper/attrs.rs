//! Attributes that a check owns, and the query that collects them.
//!
//! A check can own attribute names. An announced item may then carry attributes with
//! these names (`#[my_lib(skip)]`). `#[announce]` removes them from the item it
//! emits, so rustc never sees them, and keeps them in the payload that callbacks
//! receive. What the definer writes and gets is described in
//! [`#[announce]`][announce-checks]. This page has the protocol between `#[announce]`
//! and the checks, called the *query*, and the types that find and remove owned
//! attributes.
//!
//! # The query
//!
//! `#[announce(a::check, b::check)]` does not emit the item at once. It calls the
//! first check with a list of the calls to make. The list is headed by the macro
//! being called, like the continuation lists of trampolines. It holds the checks in
//! the order written, and ends with a hidden macro of `announce`, `finish!`:
//!
//! ```text
//! a::check! { @announce_format "1" @__announce_attrs [ a::check!, b::check!, ::announce::__private::finish! ] { STATE } }
//! ```
//!
//! Like every call that announce makes, it starts with the format prefix
//! `@announce_format "1"` ([the wire format](crate::helper::Input#the-wire-format)).
//! `STATE` is opaque to the checks. A check *answers* by dropping its own element
//! (the head) and calling the next element with the rest of the list, which is now
//! headed by the macro that is called. `STATE` is unchanged, followed by one
//! `attrs(name, ..)` group if the check owns names:
//!
//! ```text
//! b::check! { @announce_format "1" @__announce_attrs [ b::check!, ::announce::__private::finish! ] { STATE attrs(my_lib) } }
//! ::announce::__private::finish! { @announce_format "1" @__announce_attrs [ ::announce::__private::finish! ] { STATE attrs(my_lib) } }
//! ```
//!
//! A check that forwards the query writes the prefix again, as above
//! ([`Query::answer`] does). The parsers of announce, and `finish!`, also take a query
//! without the prefix, as a check written by hand may forward it; another version
//! (`@announce_format "2"`) is an error that names both versions.
//!
//! `finish!` reads the names, rejects a reserved one ([`OwnedAttrs::check_names`]),
//! and expands to what `#[announce]` expands to without checks, except that the item
//! it emits has no attribute of the owned names. The `META` of the trampoline lists
//! the names in one element, `attrs(my_lib, other)`
//! ([`Entry::owned_attrs`](crate::helper::Entry::owned_attrs)).
//!
//! **Every check must answer**, also one that owns no attributes. A check that does
//! not answer stops the chain: the item is not emitted, and rustc reports the call of
//! that check. A `macro_rules!` check says "no rules expected the token `@`" (when it
//! has no arm for the query, or no arm for the prefix `@announce_format "1"`). A
//! proc-macro check that parses [`CheckInput`](crate::helper::CheckInput) by hand says that
//! it did not answer.
//!
//! A list that does not fit is an error where it is noticed:
//!
//! * a check answers a list that is empty, or has nothing after its head (an earlier
//!   check forwarded too little; `finish!` must end the list);
//! * `finish!` receives anything but `[ finish! ]`: an empty list, or "elements left
//!   in its list" (a check forwarded its head or more);
//! * a check forwards its list without dropping its head, so it calls itself again
//!   without end: rustc stops it at the recursion limit.
//!
//! Listing a check twice, `#[announce(a::check, a::check)]`, is not an error: the
//! list is `[a!, a!, finish!]`.
//!
//! # Writing a check
//!
//! A proc-macro check calls [`check`](crate::helper::check) with its input. The function
//! answers the query with the names the check owns. It parses the check's own input,
//! `(ARGS), ITEM` ([`CheckInput`](crate::helper::CheckInput)), and runs a closure on it.
//! (By hand, parse a [`CheckCall`] and answer its [`Query`] with [`Query::answer`].)
//!
//! ```ignore
//! #[proc_macro]
//! pub fn my_check(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
//!     announce::helper::check(input.into(), &["my_lib", "other"], |c| {
//!         for attr in c.owned_attributes() { /* validate */ }
//!         Ok(proc_macro2::TokenStream::new())
//!     })
//!     .into()
//! }
//! ```
//!
//! A `macro_rules!` check has one more arm. It skips its own head, forwards `STATE`
//! and adds the names (leave out `attrs(..)` when the check owns none). The paths of
//! the head and of the next element are matched as in the arms of a trampoline, so
//! `crate::`, `$crate::` and `::a::b` work in both:
//!
//! ```text
//! macro_rules! my_check {
//!     (@announce_format "1" @__announce_attrs [ $($hp:ident)? $(:: $hs:ident)* ! , $($np:ident)? $(:: $ns:ident)* ! $(, $($rest:tt)*)? ] { $($state:tt)* }) => {
//!         $($np)? $(:: $ns)* ! { @announce_format "1" @__announce_attrs [ $($np)? $(:: $ns)* ! $(, $($rest)*)? ] { $($state)* attrs(my_lib, other) } }
//!     };
//!     (@announce_format "1" ($($args:tt)*), $($item:tt)*) => { /* the check itself */ };
//! }
//! ```
//!
//! Both arms start with the prefix, because announce writes it in every call to a
//! check. (A check that should also be callable by hand writes `$(@announce_format "1")?`
//! instead.)
//!
//! The arm matches the head only to skip it (`$hp` and `$hs` are not used). A check
//! that wants to see its own path in the head can write that path in the pattern
//! instead (`[ crate::my_check ! , ..`).
//!
//! The check itself runs as `my_check! { @announce_format "1" (ARGS), ITEM }`. `ITEM`
//! has the owned attributes, with `#[cfg]` and `#[cfg_attr]` evaluated. The check
//! validates them and returns a `compile_error!` at the offending attribute.
//!
//! # What is matched
//!
//! An owned name `my_lib` matches an attribute `#[my_lib]`, `#[my_lib(..)]`,
//! `#[my_lib = ..]` and `#[my_lib::..]` (the first segment of its path, with no
//! leading `::`: `#[::my_lib::x]` names a crate and is rustc's business). Both outer
//! (`#[..]`) and inner (`#![..]`) attributes match, wherever syn parses attributes in
//! the item: on the item, its fields, variants, generic parameters, trait and impl
//! items, the function parameters and statements of the bodies, and on everything
//! inside an inline `mod`. Attributes inside macro invocations and `macro_rules!`
//! bodies are token trees, which are not looked at.
//!
//! [announce-checks]: https://docs.rs/announce/latest/announce/attr.announce.html#checks-and-attributes-of-your-own

use crate::helper::{format_prefix, parse_format, Cont, ContList};
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
/// name must not be one of them, because the item would lose an attribute that
/// rustc needs. Names that start with `rustc_` or `__announce` are reserved too.
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
/// A check answers the query with it ([`Query::answer`]), META lists it as
/// `attrs(..)` ([`Entry::owned_attrs`](crate::helper::Entry::owned_attrs)), and it finds
/// ([`find`](Self::find)) and removes ([`strip`](Self::strip)) the attributes in
/// an item. The [module docs](self#what-is-matched) say what a name matches.
///
/// ```
/// # use announce_core as announce;
/// use announce::helper::OwnedAttrs;
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
    /// (see [what is matched](self#what-is-matched)).
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

/// Removes every attribute of `item` for which `remove` returns true, at any depth
/// that syn parses (not inside macro invocations). Returns whether any was removed.
///
/// `syn`'s `VisitMut` cannot take an attribute out of its list. So this renames
/// each removed attribute to a mark, drops the marks from the printed tokens, and
/// parses the item again.
///
/// ```
/// # use announce_core as announce;
/// let mut item: syn::Item = syn::parse_quote! { #[a] struct S { #[b] x: u8 } };
/// let removed = announce::helper::remove_attributes(&mut item, |a| a.path().is_ident("b")).unwrap();
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

/// The query a check receives:
/// `@announce_format "1" @__announce_attrs [ THIS!, NEXT!, .. ] { STATE }`. The prefix is
/// optional when parsing (a query written by hand), and always written by
/// [`answer`](Self::answer).
///
/// Answer it with [`answer`](Self::answer). The [module docs](self#the-query) have
/// the protocol. [`check`](crate::helper::check) does this for a proc-macro check.
#[derive(Clone, Debug)]
pub struct Query {
    /// The macro being called (the head, the check itself), then the elements still
    /// to be called, in order. The last one is `finish!`.
    pub list: ContList,
    /// The state that travels along. Opaque: pass it on unchanged.
    pub state: TokenStream,
}

impl Parse for Query {
    fn parse(input: ParseStream) -> Result<Self> {
        // `@announce_format "1"` comes first, unless the query was written by hand.
        parse_format(input)?;
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
    /// The call that answers the query for a check that owns `owned`. It calls the
    /// element after the head, with the prefix `@announce_format "1"`, the list from
    /// there on and the state plus `attrs(..)` (the state unchanged if `owned` is
    /// empty). A reserved name gives
    /// a `compile_error!`, and so does a list that is empty or has no element after
    /// its head.
    pub fn answer(&self, owned: &OwnedAttrs) -> TokenStream {
        if let Err(e) = owned.check_names() {
            return e.to_compile_error();
        }
        let mut it = self.list.0.iter();
        match it.next() {
            Some(Cont::Bang(_)) => {}
            _ => {
                return quote! {
                    ::core::compile_error! { "announce: the list of the attribute query is empty or does not start with a macro: it must start with the macro that is called (this check) and end with `finish!`. A check before this one did not forward its list (without its own first element)" }
                }
            }
        }
        let next = match it.clone().next() {
            Some(c @ Cont::Bang(_)) => c,
            _ => {
                return quote! {
                    ::core::compile_error! { "announce: the attribute query has no macro to call after this check: the list ends with this check, and `finish!` must end it. A check before this one forwarded too little of its list" }
                }
            }
        };
        let rest: ContList = it.cloned().collect();
        let (format, state) = (format_prefix(), &self.state);
        quote! { #next { #format @__announce_attrs #rest { #state #owned } } }
    }
}

/// What a proc-macro check receives: the attribute [`Query`], or its own input
/// (`(ARGS), ITEM`). Both may start with the prefix `@announce_format "1"`.
///
/// [`check`](crate::helper::check) parses this and answers the query for you.
#[allow(clippy::large_enum_variant)]
pub enum CheckCall {
    /// The query of `#[announce]`: answer it with [`Query::answer`].
    Query(Query),
    /// The check itself.
    Run(crate::helper::CheckInput),
}

impl Parse for CheckCall {
    fn parse(input: ParseStream) -> Result<Self> {
        // Both forms may start with `@announce_format "1"`: look behind it.
        let ahead = input.fork();
        parse_format(&ahead)?;
        if ahead.peek(Token![@]) {
            Ok(CheckCall::Query(input.parse()?))
        } else {
            Ok(CheckCall::Run(input.parse()?))
        }
    }
}

/// The attribute names of a META `attrs(..)` element, if it is one.
pub(crate) fn owned_from_meta(meta: &crate::helper::Meta) -> Option<OwnedAttrs> {
    if !meta.path.is_ident(META_NAME) {
        return None;
    }
    OwnedAttrs::parse_names(meta.args.clone().unwrap_or_default()).ok()
}

/// All the owned names that the META of an entry lists, merged.
pub(crate) fn owned_from_metas(metas: &[crate::helper::Meta]) -> OwnedAttrs {
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
        // The head is the check that is called; the answer drops it and calls the
        // next element with the list from there.
        let q: Query = syn::parse_quote! {
            @__announce_attrs [ a::check!, b::check!, ::announce::__private::finish! ] { [1u64; attr (x) { struct S; }] }
        };
        assert_eq!(q.list.0.len(), 3);
        let answered = q.answer(&OwnedAttrs::from_strs(&["a", "c"]));
        assert_eq!(
            s(&answered),
            "b::check!{@announce_format\"1\"@__announce_attrs[b::check!,::announce::__private::finish!]{[1u64;attr(x){structS;}]attrs(a,c)}}"
        );
        // Owning nothing: the state is passed on as it is.
        assert_eq!(
            s(q.answer(&OwnedAttrs::new())),
            "b::check!{@announce_format\"1\"@__announce_attrs[b::check!,::announce::__private::finish!]{[1u64;attr(x){structS;}]}}"
        );
        // The answer is a query again, headed by the macro it calls: the next check
        // answers it in the same way, and the last call is `finish!` with `[finish!]`.
        let q2: Query = syn::parse2(syn::parse2::<syn::Macro>(answered).unwrap().tokens).unwrap();
        assert_eq!(s(&q2.list), "[b::check!,::announce::__private::finish!]");
        assert_eq!(
            s(q2.answer(&OwnedAttrs::from_strs(&["b"]))),
            "::announce::__private::finish!{@announce_format\"1\"@__announce_attrs[::announce::__private::finish!]{[1u64;attr(x){structS;}]attrs(a,c)attrs(b)}}"
        );
        // Every path form of the head and of the next element is kept as written.
        let q: Query = syn::parse_quote! {
            @__announce_attrs [ crate::a!, ::x::b!, self::d!, e!, f::finish! ] { st }
        };
        assert_eq!(
            s(q.answer(&OwnedAttrs::new())),
            "::x::b!{@announce_format\"1\"@__announce_attrs[::x::b!,self::d!,e!,f::finish!]{st}}"
        );
        // The same check twice is a list like any other.
        let q: Query = syn::parse_quote! { @__announce_attrs [ a!, a!, f::finish! ] { st } };
        assert_eq!(
            s(q.answer(&OwnedAttrs::new())),
            "a!{@announce_format\"1\"@__announce_attrs[a!,f::finish!]{st}}"
        );
        // The check before `finish!`.
        let q: Query = syn::parse_quote! { @__announce_attrs [ b!, f::finish! ] { st } };
        assert_eq!(
            s(q.answer(&OwnedAttrs::from_strs(&["a"]))),
            "f::finish!{@announce_format\"1\"@__announce_attrs[f::finish!]{stattrs(a)}}"
        );
    }

    #[test]
    fn a_list_that_does_not_fit_is_an_error_not_a_panic() {
        // Nothing at all: a check before this one dropped the whole list.
        let q: Query = syn::parse_quote! { @__announce_attrs [] { st } };
        let out = s(q.answer(&OwnedAttrs::new()));
        assert!(
            out.contains("compile_error") && out.contains("muststartwiththemacrothatiscalled"),
            "{}",
            out
        );
        // Only the head: nothing to call after the check.
        let q: Query = syn::parse_quote! { @__announce_attrs [ b::check! ] { st } };
        let out = s(q.answer(&OwnedAttrs::new()));
        assert!(
            out.contains("compile_error") && out.contains("nomacrotocallafterthischeck"),
            "{}",
            out
        );
        // An attribute continuation is not a macro that can be called.
        let q: Query = syn::parse_quote! { @__announce_attrs [ #[derive(A)] ] { st } };
        assert!(s(q.answer(&OwnedAttrs::new())).contains("compile_error"));
        let q: Query = syn::parse_quote! { @__announce_attrs [ b!, #[derive(A)] ] { st } };
        assert!(s(q.answer(&OwnedAttrs::new())).contains("compile_error"));
        // A reserved name is reported where the check answers.
        let q: Query = syn::parse_quote! { @__announce_attrs [ b!, f::finish! ] { st } };
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
        let q: CheckCall = syn::parse_quote! { @__announce_attrs [ c!, f! ] { } };
        assert!(matches!(q, CheckCall::Query(_)));
        let r: CheckCall = syn::parse_quote! { (a, b), struct S; };
        match r {
            CheckCall::Run(i) => assert_eq!(s(i.args), "a,b"),
            _ => panic!(),
        }
    }
}
