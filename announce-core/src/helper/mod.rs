//! Helpers for proc-macro crates that use [`announce`](https://docs.rs/announce)
//! trampolines: `announce::helper`.
//!
//! - Receive definitions: [`callback()`] runs a callback that parses its [`Input`].
//!   [`Input::gather`] asks for more definitions. [`Input::forward`] continues the
//!   chain.
//! - Start a chain: [`ContList::invoke`] calls a trampoline from a proc macro.
//! - Redirect: [`redirect`] forwards a derive or attribute to a renamed trampoline.
//! - Transform: [`transformer()`] runs a list element that rewrites the newest
//!   entry.
//! - Check a definition: [`check`] runs a check: it answers the query and parses its
//!   [`CheckInput`].
//!   [`attrs`] has the attributes that a check can own. [`check_type`] and
//!   [`check_trait`] test for `announce::Announce`.
//! - Read META: [`Entry::find_meta`] finds an element. [`Meta::parse_args_with`] parses its args.
//!   [`Entry::owned_attrs`] and [`Entry::item_without_owned_attrs`] read owned
//!   attributes.
//! - Resolve relative types:
//!   [`Entry::type_leak`] reads the [`TypeLeak`] table of a definition announced with
//!   `repeater = ..`. [`TypeLeak::apply`] rewrites an item so that its types resolve
//!   in any crate.
//! - Use relative types as written: [`Entry::item_resolved`] gives the item, and with a
//!   `strict = false` record ([`Entry::leak_failure`], [`LeakFailure`]) the check that fails
//!   outside its module (`announce::same_context!`, [`same_context()`]).
//!
//! - Detect announce: every call that announce makes starts with the prefix
//!   `@announce_format "1"` ([`format_prefix`], [`ANNOUNCE_FORMAT`], [`Input::format`]).
//!
//! The shape of the tokens that a callback receives is described at [`Input`].
//!
//! As `announce::helper`, this module needs the feature `proc-macro-api` of
//! `announce`, which is on by default (`default-features = false` leaves it out, for
//! `no_std` definitions).

pub mod attrs;
mod keys;
pub mod leak;
mod leakfail;
mod transformer;
mod typeleak;

pub use attrs::{remove_attributes, CheckCall, OwnedAttrs, Query};
pub(crate) use leakfail::{
    cause_path, ALIAS, NOT_PRIMITIVE, REFUSED, TRAIT_PATH, VALUE_PATH, WITHOUT_REPEATER,
};
pub use leakfail::{same_context, try_same_context, LeakError, LeakFailure};
pub use transformer::{signature_only, transformer};
/// The version of `type-leak` that [`TypeLeak`] uses. Name it through this crate,
/// so there is never a second, incompatible copy.
pub use type_leak;
pub use typeleak::TypeLeak;

// The macros `callback!` and `transformer!` (kept for compatibility; the functions of
// the same names are the API) are `#[macro_export]`ed, so they live at the crate root
// of `announce-core`; this makes them `announce::helper::callback!` and
// `announce::helper::transformer!`. A macro and a function of one name do not clash:
// they are in different namespaces. The macros are thin wrappers that call the
// functions through their `$crate` paths, which point at this module.
#[doc(hidden)]
pub use crate::{callback, transformer};

use proc_macro2::{Ident, Span, TokenStream};
use std::sync::atomic::{AtomicU64, Ordering};
use syn::ext::IdentExt;
use syn::parse::{Parse, ParseStream};
use syn::punctuated::Punctuated;
use syn::{Attribute, Path, Result, Token};
use template_quote::{quote, ToTokens};

/// The version of the wire format, as written in the prefix `@announce_format "1"`
/// that starts every call announce makes ([`format_prefix`]).
///
/// Bump it when the wire format changes in a way that a parser of the old version
/// cannot read ([the wire format](Input#the-wire-format)). A parser of this crate
/// accepts a call without the prefix (written by hand), and a call with this version.
/// Any other version is an error that names both versions.
pub const ANNOUNCE_FORMAT: &str = "1";

/// The word after the `@` of the prefix: `@announce_format "1"`.
const FORMAT_TAG: &str = "announce_format";

/// The start of every error about a version that is not [`ANNOUNCE_FORMAT`].
const FORMAT_ERROR: &str = "announce: incompatible wire format";

/// The prefix `@announce_format "1"`, with the version [`ANNOUNCE_FORMAT`].
///
/// Every call that announce makes to the next element of a chain starts with these
/// tokens, before everything else (`@__announce_self { .. }`, `(OPTIONS)`, `[LIST]`).
/// A macro of your own can match them to know that it was called by announce
/// ([the wire format](Input#the-wire-format)). A proc macro that builds such a call
/// by hand starts it with this.
pub fn format_prefix() -> TokenStream {
    quote! { @announce_format #ANNOUNCE_FORMAT }
}

/// The error for a call with `@announce_format "found"`.
fn format_error(span: Span, found: &str) -> syn::Error {
    syn::Error::new(
        span,
        format!(
            "{}: found `@announce_format {:?}`, expected `@announce_format {:?}`. The item was \
             announced, or the macro that reads it was built, with another version of \
             `announce`, whose wire format this version cannot read: use one version of \
             `announce` in the whole chain",
            FORMAT_ERROR, found, ANNOUNCE_FORMAT
        ),
    )
}

/// Whether `e` is the error of [`format_error`].
pub(crate) fn is_format_error(e: &syn::Error) -> bool {
    e.to_string().starts_with(FORMAT_ERROR)
}

/// Reads the prefix `@announce_format "VERSION"` if the input starts with it, and
/// returns the version. A version other than [`ANNOUNCE_FORMAT`] is an error. Nothing
/// is consumed when the input starts with anything else (a call written by hand).
pub(crate) fn parse_format(input: ParseStream) -> Result<Option<String>> {
    if !input.peek(Token![@]) {
        return Ok(None);
    }
    let fork = input.fork();
    let _: Token![@] = fork.parse()?;
    match fork.parse::<Ident>() {
        Ok(i) if i == FORMAT_TAG => {}
        _ => return Ok(None),
    }
    let lit: syn::LitStr = fork.parse().map_err(|e| {
        syn::Error::new(
            e.span(),
            "announce: `@announce_format` must be followed by a string literal, like \
             `@announce_format \"1\"`",
        )
    })?;
    syn::parse::discouraged::Speculative::advance_to(input, &fork);
    let found = lit.value();
    if found != ANNOUNCE_FORMAT {
        return Err(format_error(lit.span(), &found));
    }
    Ok(Some(found))
}

/// Splits the prefix `@announce_format "VERSION"` off the start of `ts`, whatever the
/// version is. The prefix is `None` when `ts` does not start with it.
fn split_format(ts: TokenStream) -> (Option<TokenStream>, TokenStream) {
    use proc_macro2::TokenTree;
    let mut it = ts.into_iter();
    let first: Vec<TokenTree> = it.by_ref().take(3).collect();
    let is_prefix = matches!(
        first.as_slice(),
        [TokenTree::Punct(p), TokenTree::Ident(i), TokenTree::Literal(_)]
            if p.as_char() == '@' && i == FORMAT_TAG
    );
    if is_prefix {
        (Some(first.into_iter().collect()), it.collect())
    } else {
        (None, first.into_iter().chain(it).collect())
    }
}

/// A random identifier `<prefix><16 hex digits>`, unique within the process.
pub fn gensym(prefix: &str) -> Ident {
    use std::hash::{BuildHasher, Hasher};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let mut h = std::collections::hash_map::RandomState::new().build_hasher();
    h.write_u64(COUNTER.fetch_add(1, Ordering::Relaxed));
    Ident::new(&format!("{}{:016x}", prefix, h.finish()), Span::call_site())
}

/// One element of a continuation list (also the `KEY` of an entry).
#[derive(Clone, Debug)]
#[allow(clippy::large_enum_variant)]
pub enum Cont {
    /// `path::to::mac!`
    Bang(Path),
    /// `#[derive(path)] Name` or `#[attr(..)] Name`. `Name` is the dummy enum
    /// that carries the payload; [`ContList::invoke`] fills it in.
    Attr {
        attr: Attribute,
        name: Option<Ident>,
    },
}

impl Cont {
    /// `path!`. Generic arguments are dropped: `Enum<A>` becomes `Enum!`.
    pub fn bang(mut path: Path) -> Self {
        for seg in &mut path.segments {
            seg.arguments = syn::PathArguments::None;
        }
        Cont::Bang(path)
    }

    /// `#[derive(path)]`
    pub fn derive(path: Path) -> Self {
        Cont::Attr {
            attr: syn::parse_quote!(#[derive(#path)]),
            name: None,
        }
    }

    /// The macro path: `path` of `path!`, the first path of `#[derive(path)]`,
    /// or the attribute path.
    pub fn path(&self) -> Path {
        match self {
            Cont::Bang(p) => p.clone(),
            Cont::Attr { attr, .. } => {
                if attr.path().is_ident("derive") {
                    let paths =
                        attr.parse_args_with(Punctuated::<Path, Token![,]>::parse_terminated);
                    if let Some(p) = paths.ok().and_then(|p| p.into_iter().next()) {
                        return p;
                    }
                }
                attr.path().clone()
            }
        }
    }
}

fn parse_macro_path(input: ParseStream) -> Result<Path> {
    let leading_colon: Option<Token![::]> = input.parse()?;
    let mut segments = Punctuated::new();
    loop {
        let ident = Ident::parse_any(input)?;
        segments.push_value(syn::PathSegment::from(ident));
        if input.peek(Token![::]) {
            segments.push_punct(input.parse()?);
        } else {
            break;
        }
    }
    Ok(Path {
        leading_colon,
        segments,
    })
}

impl Parse for Cont {
    fn parse(input: ParseStream) -> Result<Self> {
        if input.peek(Token![#]) {
            let pound_token = input.parse()?;
            let content;
            let bracket_token = syn::bracketed!(content in input);
            let attr = Attribute {
                pound_token,
                style: syn::AttrStyle::Outer,
                bracket_token,
                meta: content.parse()?,
            };
            let name = if input.peek(syn::Ident) {
                Some(input.parse()?)
            } else {
                None
            };
            Ok(Cont::Attr { attr, name })
        } else {
            let path = parse_macro_path(input)?;
            input.parse::<Token![!]>()?;
            Ok(Cont::Bang(path))
        }
    }
}

impl Cont {
    /// The comparison key: the tokens without the dummy name of an attribute
    /// continuation.
    fn eq_key(&self) -> String {
        match self {
            Cont::Bang(_) => self.to_token_stream().to_string(),
            Cont::Attr { attr, .. } => attr.to_token_stream().to_string(),
        }
    }
}

/// Continuations compare by tokens, ignoring spans and the dummy name.
impl PartialEq for Cont {
    fn eq(&self, other: &Self) -> bool {
        self.eq_key() == other.eq_key()
    }
}
impl Eq for Cont {}
impl std::hash::Hash for Cont {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.eq_key().hash(state)
    }
}

impl ToTokens for Cont {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        match self {
            Cont::Bang(p) => {
                // Macro paths cannot carry generic arguments.
                let mut p = p.clone();
                for seg in &mut p.segments {
                    seg.arguments = syn::PathArguments::None;
                }
                tokens.extend(quote! { #p ! })
            }
            Cont::Attr { attr, name } => tokens.extend(quote! { #attr #name }),
        }
    }
}

/// A continuation list, head first.
#[derive(Clone, Debug, Default)]
pub struct ContList(pub Vec<Cont>);

impl Parse for ContList {
    /// Parses the contents of the brackets: `a!, #[derive(B)], c!`.
    fn parse(input: ParseStream) -> Result<Self> {
        let p: Punctuated<Cont, Token![,]> = Punctuated::parse_terminated(input)?;
        Ok(ContList(p.into_iter().collect()))
    }
}

impl ToTokens for ContList {
    /// Emits `[a!, b!]` including the brackets.
    fn to_tokens(&self, tokens: &mut TokenStream) {
        let items = &self.0;
        tokens.extend(quote! { [ #(#items),* ] });
    }
}

impl FromIterator<Cont> for ContList {
    fn from_iter<I: IntoIterator<Item = Cont>>(iter: I) -> Self {
        ContList(iter.into_iter().collect())
    }
}

/// Whether a global options group (the tokens inside `( .. )` before the
/// continuation list) puts the chain in expression position. It does when its
/// **first** token is the identifier `expr`, as in `(expr)` or `(expr, other)`.
/// The generated trampolines use the same rule, so `(other, expr)` does not select
/// it in either. Every other option is a namespace for helpers, forwarded
/// unchanged.
pub fn is_expression_position(options: &TokenStream) -> bool {
    matches!(
        options.clone().into_iter().next(),
        Some(proc_macro2::TokenTree::Ident(i)) if i == "expr"
    )
}

impl ContList {
    /// Gives every attribute continuation a random dummy name, so two
    /// trampolines in one module never collide.
    pub fn name_dummies(&mut self) {
        for c in &mut self.0 {
            if let Cont::Attr { name, .. } = c {
                if name.is_none() {
                    *name = Some(gensym("__AnnounceT_"));
                }
            }
        }
    }

    /// Invokes the head with `@announce_format "1" [self] body`, where `body` is
    /// `ENTRY* REST` (a derive or attribute head gets the same tokens in the dummy
    /// enum).
    pub fn invoke(&self, body: TokenStream) -> TokenStream {
        self.invoke_with_options(None, body)
    }

    /// Like [`invoke`](Self::invoke), with a global options group
    /// `@announce_format "1" (options) [..] ..`.
    /// A derive or attribute head is refused (`compile_error!`) when the group
    /// starts with `expr` ([`is_expression_position`]).
    pub fn invoke_with_options(
        &self,
        options: Option<&TokenStream>,
        body: TokenStream,
    ) -> TokenStream {
        let opts = options.map(|o| quote! { ( #o ) });
        let in_expr = options.map_or(false, is_expression_position);
        let mut list = self.clone();
        list.name_dummies();
        let format = format_prefix();
        match list.0.first() {
            None => quote! { compile_error! { "announce: empty continuation list" } },
            Some(Cont::Attr { .. }) if in_expr => quote! {
                compile_error! { "announce: a derive/attribute continuation emits items, so it cannot be used with `(expr)`" }
            },
            Some(Cont::Bang(p)) => quote! { #p ! { #format #opts #list #body } },
            Some(Cont::Attr { attr, name }) => quote! {
                #attr
                #[allow(dead_code, non_camel_case_types)]
                enum #name { __AnnouncePayload = (stringify! { #format #opts #list #body }, 0).1 }
            },
        }
    }
}

/// One `META` element: `path` or `path(args)`.
#[derive(Clone, Debug)]
pub struct Meta {
    /// `path` of `path(args)`.
    pub path: Path,
    /// The tokens inside the parentheses, if any.
    pub args: Option<TokenStream>,
}

impl Meta {
    /// Parses the args with `parser`.
    pub fn parse_args_with<P: syn::parse::Parser>(&self, parser: P) -> Result<P::Output> {
        parser.parse2(self.args.clone().unwrap_or_default())
    }

    /// The args as `key = value` pairs. Values must be expressions; use
    /// [`parse_args_with`](Self::parse_args_with) for types and lists.
    pub fn name_values(&self) -> Result<Vec<syn::MetaNameValue>> {
        let p: Punctuated<syn::MetaNameValue, Token![,]> =
            self.parse_args_with(Punctuated::parse_terminated)?;
        Ok(p.into_iter().collect())
    }
}

impl Parse for Meta {
    fn parse(input: ParseStream) -> Result<Self> {
        let path = Path::parse_mod_style(input)?;
        let args = if input.peek(syn::token::Paren) {
            let content;
            syn::parenthesized!(content in input);
            Some(content.parse()?)
        } else {
            None
        };
        Ok(Meta { path, args })
    }
}

impl ToTokens for Meta {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        let p = &self.path;
        match &self.args {
            Some(a) => tokens.extend(quote! { #p(#a) }),
            None => tokens.extend(quote! { #p }),
        }
    }
}

/// `KEY => [META] { ITEM }`
#[derive(Clone, Debug)]
pub struct Entry {
    /// The continuation that produced this entry, as written in the list.
    pub key: Cont,
    /// The META elements: checks, `meta =`, `mock(..)`, `type_leak(..)`, `type_leak_error(..)`.
    pub meta: Vec<Meta>,
    /// The raw item tokens.
    pub item: TokenStream,
}

impl Entry {
    /// The payload parsed as an item.
    pub fn item(&self) -> Result<syn::Item> {
        syn::parse2(self.item.clone())
    }

    /// The path of the `mock(path)` META element, if there is one: the item stands
    /// in for the item at that path.
    pub fn mock(&self) -> Option<Path> {
        self.meta
            .iter()
            .find(|m| m.path.is_ident("mock"))
            .and_then(|m| syn::parse2(m.args.clone()?).ok())
    }

    /// The first META element whose last path segment is `name`. Use it to read
    /// a `meta = lib::info(..)` or a check.
    pub fn find_meta(&self, name: &str) -> Option<&Meta> {
        self.meta
            .iter()
            .find(|m| m.path.segments.last().map_or(false, |s| s.ident == name))
    }

    /// The META element that announce writes as `name(..)`: only the bare `name`, so a user
    /// element `lib::name(..)` is not taken for it.
    fn reserved_meta(&self, name: &str) -> Option<&Meta> {
        self.meta.iter().find(|m| m.path.is_ident(name))
    }

    /// The `type_leak(..)` META of an entry announced with `repeater = ..`.
    /// Unknown keys are ignored.
    /// A recorded failure ([`leak_failure`](Self::leak_failure)) is an error
    /// ([`LeakFailure::to_error`]); [`item_resolved`](Self::item_resolved) uses the item as written.
    pub fn type_leak(&self) -> Result<Option<TypeLeak>> {
        if let Some(failure) = self.leak_failure()? {
            return Err(failure.to_error());
        }
        let m = match self.reserved_meta("type_leak") {
            Some(m) => m,
            None => return Ok(None),
        };
        syn::parse2(m.to_token_stream()).map(Some)
    }

    /// The `type_leak_error(..)` META of an entry announced with `strict = false` whose types do
    /// not resolve outside its module. The record's path is the key of the entry.
    pub fn leak_failure(&self) -> Result<Option<LeakFailure>> {
        let m = match self.reserved_meta("type_leak_error") {
            Some(m) => m,
            None => return Ok(None),
        };
        let mut failure: LeakFailure = syn::parse2(m.to_token_stream())?;
        failure.path = self.key.path();
        Ok(Some(failure))
    }

    /// The item as a callback emits it, and what goes next to it: with a table
    /// ([`type_leak`](Self::type_leak)) the item rewritten by [`TypeLeak::apply`]; with a
    /// recorded failure ([`leak_failure`](Self::leak_failure)) the item as written and
    /// [`LeakFailure::assume_success`]; otherwise the item and no tokens. `krate` is the path of
    /// the `announce` crate where the tokens are emitted (`::announce`). The check is a
    /// `const _` item: put it beside an `impl` or `trait` block, not in it.
    ///
    /// ```
    /// # use announce_core as announce;
    /// # use announce::helper::Input;
    /// # use template_quote::quote;
    /// # let input: Input = syn::parse_quote! { [cb!] a::S! => [] { pub struct S { pub a: u8 } } };
    /// # let krate: syn::Path = syn::parse_quote!(::announce);
    /// # let mut out = proc_macro2::TokenStream::new();
    /// for entry in input.entries_in_order() {
    ///     let (item, check) = entry.item_resolved(&krate)?;
    ///     out.extend(quote! { #item #check });
    /// }
    /// # // The item has no record: it comes as written, with no check.
    /// # assert_eq!(out.to_string(), "pub struct S { pub a : u8 }");
    /// # Ok::<(), syn::Error>(())
    /// ```
    pub fn item_resolved(&self, krate: &Path) -> Result<(syn::Item, TokenStream)> {
        let mut item = self.item()?;
        if let Some(failure) = self.leak_failure()? {
            return Ok((item, failure.assume_success(krate)));
        }
        if let Some(table) = self.type_leak()? {
            table.try_apply(&mut item)?;
        }
        Ok((item, TokenStream::new()))
    }

    /// Whether META lists a check whose last path segment is `name`. Only the
    /// last segment is compared, since the path is as the definer wrote it.
    pub fn has_check(&self, name: &str) -> bool {
        self.meta
            .iter()
            .any(|m| m.path.segments.last().map_or(false, |s| s.ident == name))
    }

    /// The attribute names that the definition's checks own: the `attrs(..)` META
    /// element (empty without one). The definer removed these attributes from the
    /// item it emitted, but they are still in [`item`](Self::item), where a
    /// callback can read them ([`OwnedAttrs::find`]).
    ///
    /// A callback that emits the item itself must remove them first, because rustc
    /// rejects an attribute that no macro declares. Use
    /// [`item_without_owned_attrs`](Self::item_without_owned_attrs).
    pub fn owned_attrs(&self) -> OwnedAttrs {
        attrs::owned_from_metas(&self.meta)
    }

    /// The payload parsed as an item, without the owned attributes: the item as the
    /// definer emitted it. The same as [`item`](Self::item) when the definition has
    /// no owned attributes.
    pub fn item_without_owned_attrs(&self) -> Result<syn::Item> {
        let mut item = self.item()?;
        self.owned_attrs().strip(&mut item)?;
        Ok(item)
    }
}

impl Parse for Entry {
    fn parse(input: ParseStream) -> Result<Self> {
        let key = input.parse()?;
        input.parse::<Token![=>]>()?;
        let meta_content;
        syn::bracketed!(meta_content in input);
        let meta: Punctuated<Meta, Token![,]> = Punctuated::parse_terminated(&meta_content)?;
        let item_content;
        syn::braced!(item_content in input);
        Ok(Entry {
            key,
            meta: meta.into_iter().collect(),
            item: item_content.parse()?,
        })
    }
}

impl ToTokens for Entry {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        let (key, meta, item) = (&self.key, &self.meta, &self.item);
        tokens.extend(quote! { #key => [ #(#meta),* ] { #item } });
    }
}

/// What a callback receives: `@announce_format "1" (OPTIONS)? [LIST] ENTRY* REST`.
///
/// # The wire format
///
/// A callback receives these tokens:
///
/// ```text
/// @announce_format "1" (OPTIONS)? [LIST] ENTRY* REST
/// ```
///
/// - `@announce_format "1"`: the *format prefix*. Every call that announce makes to
///   the next element of a chain starts with it, whatever the element is: a
///   trampoline, a transformer, a callback, a check. `"1"` is the version of the wire
///   format ([`ANNOUNCE_FORMAT`]); it changes when this format changes in a way an old
///   reader cannot read. A call written by hand does not need the prefix, and every
///   parser of announce reads a call with or without it ([`Input::format`] is the
///   version found). A call with another version is one compile error that names both
///   versions, because the item and the macro that reads it were built with
///   incompatible versions of announce. [How a macro of your own detects the
///   prefix](#detecting-a-call-from-announce).
///   It starts every call that announce makes, also the call of the sentinel of the runner,
///   `NAME! { @announce_format "1" @__announce_exists }`.
///   (The *runner* is the hidden attribute that `#[announce]` puts on an item that has other
///   attributes. Its arguments have their own marker, `__announce_format_version = "1"`: it
///   versions the encoding of the arguments of an attribute, which the runners of two
///   versions of announce compare when they meet on one item. The runner is not a call of a
///   chain, and has no `@announce_format`; the calls of a chain are not attributes, and have
///   no `__announce_format_version`.)
/// - `OPTIONS`: global options, forwarded unchanged by every hop. Only a leading
///   `expr` has a meaning: it puts the chain in expression position, where derive
///   and attribute elements are refused ([`is_expression_position`]). The rest is
///   free for helpers ([`Input::has_option`]).
/// - `LIST`: the *continuation list*, the calls still to make, head first. The head
///   is the macro that is called. An element ([`Cont`]) is `path!`,
///   `#[derive(path)] Name` or `#[attr] Name`. The last two are the derive and
///   attribute route: `Name` is a dummy enum that carries the tokens, and a derive or
///   attribute macro hands them on with [`redirect`].
/// - `ENTRY`: `KEY => [META] { ITEM }`, one per definition ([`Entry`]). `KEY` is the
///   list element that produced the entry, as written.
/// - `REST`: tokens that the protocol never touches. A parser reads entries greedily,
///   so wrap a `REST` that starts like an entry.
///
/// How a chain runs:
///
/// - Each macro drops its own element (the head) from the list, adds an entry, and
///   calls the next element with the rest of the list. The last element is the
///   callback: it runs once, with one entry per trampoline.
/// - New entries go in front, so they are newest first.
///   [`Input::entries_in_order`] gives oldest first.
/// - `[META]` is always written, even when empty (`[]`).
/// - A trampoline can get the prefixes `@__announce_self { PATH }` and
///   `@__announce_dollar { $ }` after the format prefix, before the rest of this input:
///   `@announce_format "1" @__announce_self { PATH } @__announce_dollar { $ } ..`. Only
///   trampolines and [`redirect`] use them. Parsing an [`Input`] skips them.
/// - The calls that announce makes to a check have the prefix too: the query
///   `@announce_format "1" @__announce_attrs [..] { STATE }` and the check itself,
///   `@announce_format "1" (ARGS), ITEM` ([`attrs`]).
///
/// `META` holds these elements, as the definer wrote them:
///
/// - `path` or `path(args)`: a check (`#[announce(path(args))]`);
/// - `path(args)`: a `meta = path(args)` entry, never called ([`Entry::find_meta`]);
/// - `mock(path)`: the item stands in for the item at `path` ([`Entry::mock`]);
/// - `attrs(a, b)`: the attribute names that checks own ([`Entry::owned_attrs`]);
/// - `type_leak(..)`: the type table of a `repeater` ([`Entry::type_leak`]);
/// - `type_leak_error(id = R, item = Name, errors = [(TOKENS) "reason" { .. }, ..], krate = "..")`:
///   instead of a table, what a definer announced with `strict = false` could not carry
///   ([`Entry::leak_failure`]; [`LeakFailure`] has the syntax). `ANNOUNCE_FORMAT` is still `"1"`.
///
/// Ignore elements you do not know.
///
/// ```
/// # use announce_core as announce;
/// use announce::helper::Input;
///
/// let input: Input = syn::parse_quote! {
///     @announce_format "1"
///     [cb!]
///     a::F! => [my_lib::check, mock(::core::iter::Iterator)] { pub trait F {} }
///     REST tokens
/// };
/// assert_eq!(input.format.as_deref(), Some("1"));
/// let entry = &input.entries[0];
/// assert!(entry.has_check("check"));
/// assert!(entry.mock().is_some());
/// assert_eq!(input.rest.to_string(), "REST tokens");
///
/// // A call by hand has no prefix, and parses as well.
/// let by_hand: Input = syn::parse_quote! { [cb!] a::F! => [] { pub trait F {} } };
/// assert_eq!(by_hand.format, None);
///
/// // Another version is one error, which names the version found and the one read.
/// let err = syn::parse2::<Input>(syn::parse_quote! { @announce_format "2" [cb!] }).unwrap_err();
/// assert!(err.to_string().contains("found `@announce_format \"2\"`"));
/// assert!(err.to_string().contains("expected `@announce_format \"1\"`"));
/// ```
///
/// # Detecting a call from announce
///
/// A macro of your own, a `macro_rules!` callback, check or transformer, can match
/// the prefix to know that announce called it, and which version of the wire format
/// the call has. Put the arm for the prefix first. The arm for the call by hand
/// (without it) stays, if you want one:
///
/// ```
/// macro_rules! who_calls {
///     // Called by announce: the prefix, then `[LIST] ENTRY* REST`.
///     (@announce_format "1" [$($list:tt)*] $($rest:tt)*) => {
///         const CALLER: &str = "announce";
///     };
///     // Called by hand, or with a version that this macro does not know.
///     ($($rest:tt)*) => {
///         const CALLER: &str = "not announce, or another format";
///     };
/// }
///
/// who_calls! { @announce_format "1" [who_calls!] }
/// assert_eq!(CALLER, "announce");
/// ```
///
/// A macro that forwards the call to the next element starts the forwarded call with
/// the prefix again; [`Input::forward`] does it, and [`format_prefix`] gives the
/// tokens to a proc macro that builds the call itself. A `macro_rules!` callback
/// that is written for one version only says so in its pattern, as above: a call
/// with another version matches no arm, and rustc reports the call.
///
/// # A real example
///
/// This is what a callback gets from a real chain, captured by the test
/// `tests/wire.rs`. The definitions, in `defs::wire` of `tests/src/lib.rs`
/// (`Meters` is private to the defining crate, so `Length` has a `repeater`):
///
/// ```ignore
/// ::announce::define_repeater!(pub Repeater);
///
/// /// A point.
/// #[announce(announce_test_derive::lib_check, meta = crate::info(1))]
/// pub struct Point {
///     /// The x coordinate.
///     #[my_lib(skip)]
///     pub x: u8,
///     pub y: u8,
/// }
///
/// #[announce(repeater = crate::wire::Repeater)]
/// pub struct Length(pub Meters);
/// ```
///
/// The chain, in another crate. `show!` is the callback; it prints its whole input:
///
/// ```ignore
/// use announce_tests::defs::wire as defs;
///
/// macro_rules! show {
///     ($($t:tt)*) => { stringify!($($t)*) };
/// }
///
/// let wire: &str = defs::Point! { (expr) [defs::Point!, defs::Length!, show!] some rest };
/// ```
///
/// `wire` is this text. The tokens are the ones received, in order. Only the
/// layout is added by hand, and the random number of `id` (a different one for each
/// build) is replaced by `ID`. The test compares this block with what the chain
/// gives, so it cannot drift:
///
/// <!-- wire-example -->
/// ```text
/// @announce_format "1"
/// (expr)
/// [show!]
/// defs::Length! => [
///     type_leak(repeater = $crate::wire::Repeater, id = ID,
///               context = 0, referrer = ["type-leak-format-14" (Meters)])
/// ] {
///     pub struct Length(pub Meters);
/// }
/// defs::Point! => [
///     announce_test_derive::lib_check,
///     $crate::info(1),
///     attrs(my_lib)
/// ] {
///     pub struct Point {
///         #[my_lib(skip)]
///         pub x: u8,
///         pub y: u8,
///     }
/// }
/// some rest
/// ```
///
/// How the parts map to the fields of `Input`:
///
/// | Tokens | Field | Notes |
/// |---|---|---|
/// | `@announce_format "1"` | [`format`](Input::format) | The prefix, written by both hops. |
/// | `(expr)` | [`options`](Input::options) | From the call, forwarded by both hops. |
/// | `[show!]` | [`list`](Input::list) | Only the callback is left: `Point!` and `Length!` each dropped themselves. |
/// | `defs::Length! => [..] { .. }` | `entries[0]` | The newest entry, the last hop. Its `key` is as written in the call. |
/// | `defs::Point! => [..] { .. }` | `entries[1]` | The oldest entry. |
/// | `some rest` | [`rest`](Input::rest) | The caller's tokens, untouched. |
///
/// What is in the entries:
///
/// - `Length`: `type_leak(..)` is the table of the `repeater` ([`Entry::type_leak`]).
///   The item still says `Meters`, which only the defining crate can resolve;
///   the table is what a callback uses to write the type from anywhere
///   ([`TypeLeak::apply`]).
/// - `Point`: `announce_test_derive::lib_check` is the check, `$crate::info(1)` is
///   `meta = crate::info(1)` (`crate` became `$crate`, so that the path resolves where
///   the callback runs), and `attrs(my_lib)` lists the attribute names that the
///   check owns.
/// - The doc comments of `Point` are gone from the item, because the payload has
///   no docs unless `docs = true`. `#[my_lib(skip)]` is still there: the definer
///   removed it from the item it emitted, but the callback can read it.
///
/// # `self` and hygiene
///
/// `self` is hygienic. The `self` in the receiver of a method that the callback
/// receives (`&self`) belongs to the trampoline's expansion; a `self` that the
/// callback writes itself, in its own `quote!`, is another one to rustc. A callback
/// that mixes the received tokens with its own output, for example the received
/// signature with a body it writes, must use the `self` token of the payload in that
/// body. Otherwise rustc reports `E0424` ("cannot find value `self` in this
/// scope"). Take the token from the receiver of the parsed signature:
///
/// ```
/// # use announce_core as announce;
/// use announce::helper::Input;
/// use template_quote::quote;
///
/// // For `trait Get { fn get(&self) -> u8; }`: an impl that forwards `get` to a field.
/// fn forward(input: &Input) -> syn::Result<proc_macro2::TokenStream> {
///     let item = match input.entries[0].item()? {
///         syn::Item::Trait(t) => t,
///         _ => return Err(syn::Error::new(proc_macro2::Span::call_site(), "expected a trait")),
///     };
///     let sig = match &item.items[0] {
///         syn::TraitItem::Fn(f) => &f.sig,
///         _ => return Err(syn::Error::new(proc_macro2::Span::call_site(), "expected a method")),
///     };
///     let (trait_name, name) = (&item.ident, &sig.ident);
///     // The `self` of the payload: a `quote! { self.inner }` here would not match `#sig`.
///     let this = &sig.receiver().expect("a method with a receiver").self_token;
///     Ok(quote! {
///         impl #trait_name for Wrapper { #sig { #this.inner.#name() } }
///     })
/// }
///
/// let input: Input = syn::parse_quote! { [cb!] a::Get! => [] { trait Get { fn get(&self) -> u8; } } };
/// let out = forward(&input).unwrap().to_string();
/// assert!(out.contains("fn get (& self) -> u8 { self . inner . get () }"), "{}", out);
/// ```
#[derive(Clone, Debug)]
pub struct Input {
    /// The version of the prefix `@announce_format "VERSION"` that the call started
    /// with ([`ANNOUNCE_FORMAT`] when there is one), or `None` for a call without it
    /// (written by hand). A call with another version does not parse.
    ///
    /// [`forward`](Self::forward), [`fetch`](Self::fetch) and [`gather`](Self::gather)
    /// always write the prefix with the version of this crate, whatever this field is.
    pub format: Option<String>,
    /// The global options group `(..)`. [`forward`](Self::forward) passes it on
    /// unchanged. [`fetch`](Self::fetch) and [`gather`](Self::gather) add
    /// `__announce_fetched(..)` to it, which records what they fetched (to detect
    /// a continuation that is not a trampoline).
    pub options: Option<TokenStream>,
    /// The continuation list; its head is the callback itself.
    pub list: ContList,
    /// Newest first.
    pub entries: Vec<Entry>,
    /// REST: tokens after the entries, untouched by the protocol.
    pub rest: TokenStream,
}

impl Parse for Input {
    fn parse(input: ParseStream) -> Result<Self> {
        // `@announce_format "1"` comes first, before everything else.
        let format = parse_format(input)?;
        // The `@__announce_self { path }` and `@__announce_dollar { $ }`
        // prefixes are meant for trampolines; skip them if a callback
        // receives them.
        while input.peek(Token![@]) {
            let fork = input.fork();
            let _: Token![@] = fork.parse()?;
            match fork.parse::<Ident>() {
                Ok(i) if i == "__announce_dollar" || i == "__announce_self" => {
                    let _g: proc_macro2::Group = fork.parse()?;
                    syn::parse::discouraged::Speculative::advance_to(input, &fork);
                }
                _ => break,
            }
        }
        let options = if input.peek(syn::token::Paren) {
            let o;
            syn::parenthesized!(o in input);
            Some(o.parse()?)
        } else {
            None
        };
        let content;
        syn::bracketed!(content in input);
        let list = content.parse()?;
        let mut entries = Vec::new();
        loop {
            let fork = input.fork();
            match fork.parse::<Entry>() {
                Ok(e) => {
                    entries.push(e);
                    syn::parse::discouraged::Speculative::advance_to(input, &fork);
                }
                Err(e) => {
                    // `KEY =>` means an entry was intended: report it instead
                    // of silently treating it as REST.
                    // (`KEY => [` or `KEY => {` only; `foo! => bar` is valid
                    // REST.)
                    let probe = input.fork();
                    if probe.parse::<Cont>().is_ok()
                        && probe.parse::<Token![=>]>().is_ok()
                        && (probe.peek(syn::token::Bracket) || probe.peek(syn::token::Brace))
                    {
                        return Err(syn::Error::new(
                            e.span(),
                            format!("announce: malformed entry: {}", e),
                        ));
                    }
                    break;
                }
            }
        }
        Ok(Input {
            format,
            options,
            list,
            entries,
            rest: input.parse()?,
        })
    }
}

impl Input {
    /// Entries in the order their definitions were visited (oldest first).
    pub fn entries_in_order(&self) -> impl Iterator<Item = &Entry> {
        self.entries.iter().rev()
    }

    /// Fetches more definitions, then calls this callback again. It invokes
    /// `new[0]` with the list `[new.., <this list>]`, so the new entries arrive in
    /// front of the old ones.
    ///
    /// The paths are recorded in the options group as `__announce_fetched(..)`. If
    /// this callback is called again with a recorded path but no entry for it, that
    /// path is not an `#[announce]` trampoline. Then `fetch` emits a
    /// `compile_error!` instead of fetching until the recursion limit. A callback
    /// that fetches only what is still missing (as [`gather`](Self::gather) does)
    /// never meets that error for a trampoline.
    pub fn fetch(&self, new: impl IntoIterator<Item = Cont>) -> TokenStream {
        self.fetch_conts(new.into_iter().collect())
    }

    /// The bookkeeping of [`fetch`](Self::fetch) and [`gather`](Self::gather):
    /// refuses a continuation that was fetched before and left no entry,
    /// records the new ones in the options group, and invokes `[new.., <list>]`.
    fn fetch_conts(&self, new: Vec<Cont>) -> TokenStream {
        // Already fetched once, but no entry came back: not a trampoline. The
        // bookkeeping travels in the options group.
        let fetched = self.fetched_paths();
        if let Some(c) = new
            .iter()
            .find(|c| fetched.contains(&path_key(&c.path())) && self.entry_for(c).is_none())
        {
            let msg = format!(
                "announce: `{}` was fetched but did not provide a definition; is it an `#[announce]` trampoline?",
                c.to_token_stream()
            );
            return quote! { compile_error! { #msg } };
        }
        let mut options: Vec<TokenStream> = self.option_items();
        if !new.is_empty() {
            let paths = new.iter().map(|c| c.path());
            options.push(quote! { __announce_fetched(#(#paths),*) });
        }
        let opts = if options.is_empty() {
            None
        } else {
            Some(quote! { #(#options),* })
        };
        let list: ContList = new.into_iter().chain(self.list.0.iter().cloned()).collect();
        let (entries, rest) = (&self.entries, &self.rest);
        list.invoke_with_options(opts.as_ref(), quote! { #(#entries)* #rest })
    }

    /// Whether the global options group contains the ident `name`.
    pub fn has_option(&self, name: &str) -> bool {
        self.options.as_ref().map_or(false, |o| {
            o.clone()
                .into_iter()
                .any(|tt| matches!(tt, proc_macro2::TokenTree::Ident(i) if i == name))
        })
    }

    /// The comma-separated elements of the options group.
    pub fn option_items(&self) -> Vec<TokenStream> {
        let mut items = Vec::new();
        let mut cur = TokenStream::new();
        for tt in self.options.clone().unwrap_or_default() {
            if matches!(&tt, proc_macro2::TokenTree::Punct(p) if p.as_char() == ',') {
                if !cur.is_empty() {
                    items.push(std::mem::take(&mut cur));
                }
            } else {
                cur.extend(Some(tt));
            }
        }
        if !cur.is_empty() {
            items.push(cur);
        }
        items
    }

    fn fetched_paths(&self) -> Vec<String> {
        let mut out = Vec::new();
        for item in self.option_items() {
            let parsed: Result<Meta> = syn::parse2(item);
            if let Ok(m) = parsed {
                if m.path.is_ident("__announce_fetched") {
                    let paths: Result<Punctuated<Path, Token![,]>> =
                        m.parse_args_with(Punctuated::parse_terminated);
                    if let Ok(paths) = paths {
                        out.extend(paths.iter().map(path_key));
                    }
                }
            }
        }
        out
    }

    /// Parses REST as `T`.
    pub fn rest_as<T: Parse>(&self) -> Result<T> {
        syn::parse2(self.rest.clone())
    }

    /// The entry whose key names `cont` (compared by macro path, with generic
    /// arguments ignored), newest first.
    pub fn entry_for(&self, cont: &Cont) -> Option<&Entry> {
        let want = path_key(&cont.path());
        self.entries
            .iter()
            .find(|e| path_key(&e.key.path()) == want)
    }

    /// Asks for definitions: "give me these, then call me again".
    ///
    /// Returns [`Gather::Ready`] with one entry per requested continuation, in
    /// order, when all are present. Otherwise it returns [`Gather::Fetch`]: tokens
    /// that fetch the missing ones and call this callback again.
    ///
    /// ```
    /// # use announce_core as announce;
    /// # use announce::helper::{Cont, Gather, Input};
    /// # use proc_macro2::TokenStream;
    /// # fn run(input: Input, needed: Vec<Cont>) -> syn::Result<TokenStream> {
    /// let entries = match input.gather(needed) {
    ///     Gather::Ready(e) => e,
    ///     Gather::Fetch(ts) => return Ok(ts),
    /// };
    /// # Ok(entries.iter().map(|e| e.item.clone()).collect())
    /// # }
    /// #
    /// # let needed = || vec![Cont::bang(syn::parse_quote!(a::S))];
    /// # // `a::S!` is there: `gather` is ready.
    /// # let input: Input = syn::parse_quote! { [cb!] a::S! => [] { struct S; } };
    /// # assert_eq!(run(input, needed())?.to_string(), "struct S ;");
    /// # // It is missing: the tokens fetch it and call the callback again.
    /// # let out = run(syn::parse_quote! { [cb!] }, needed())?.to_string();
    /// # assert!(out.starts_with("a :: S !") && out.contains("cb !"), "{}", out);
    /// # Ok::<(), syn::Error>(())
    /// ```
    ///
    /// ## Pairing a fetch with the `Announce` check
    ///
    /// A path without `#[announce]` is reported by rustc alone: ``error[E0433]: cannot find
    /// `Plain` in `m` ``, with a "try to remove `!`" hint that does not help, and only for the
    /// first missing item. A callback that knows its paths name structs, enums or unions can
    /// emit [`check_type`] for each next to the fetch. That adds the error "`Plain` is not
    /// announced" for every missing item at once (rustc's error still comes first). For a simple
    /// dyn-compatible trait use [`check_trait`].
    ///
    /// ```
    /// # use announce_core as announce;
    /// use announce::helper::{check_type, Cont, Gather, Input};
    /// use proc_macro2::TokenStream;
    /// use template_quote::quote;
    ///
    /// // `paths` are structs, enums or unions: each has an `Announce` impl.
    /// fn run(input: Input, paths: Vec<syn::Path>) -> syn::Result<TokenStream> {
    ///     let entries = match input.gather(paths.iter().cloned().map(Cont::bang)) {
    ///         Gather::Ready(e) => e,
    ///         Gather::Fetch(ts) => {
    ///             let krate = syn::parse_quote!(::announce);
    ///             let checks = paths.iter().map(|p| check_type(&krate, &syn::parse_quote!(#p)));
    ///             return Ok(quote! { #(#checks)* #ts });
    ///         }
    ///     };
    ///     let n = entries.len();
    ///     Ok(quote!(const COUNT: usize = #n;))
    /// }
    /// #
    /// # // `a::S` is not there yet: the check goes in front of the fetch.
    /// # let input: Input = syn::parse_quote! { [cb!] };
    /// # let out = run(input, vec![syn::parse_quote!(a::S)])?.to_string();
    /// # assert!(out.starts_with("const _ : () ="), "{}", out);
    /// # assert!(out.contains("announce :: Announce") && out.contains("a :: S !"), "{}", out);
    /// # Ok::<(), syn::Error>(())
    /// ```
    ///
    /// The check is right only for a path that has an `Announce` impl. An announced `mod`, a
    /// type alias, a trait that is not dyn-compatible, and an item with `emit_item = false` or
    /// `mock` have none, so the check would reject a valid call. It is a `const _` item, so it
    /// does not go inside an `impl` or `trait` block. A generic type needs its arguments in the
    /// path given to `check_type` (`m::G<u8>`, since the bare `m::G` is rustc's `E0107`);
    /// [`Cont::bang`] drops them for the fetch. In expression position
    /// ([`is_expression_position`]) the output needs a block, `{ #(#checks)* #ts }`, or the
    /// callback skips the check there.
    pub fn gather(&self, needed: impl IntoIterator<Item = Cont>) -> Gather<'_> {
        let needed: Vec<Cont> = needed.into_iter().collect();
        // A callback asking for itself (or for its own head path) would
        // re-fetch forever.
        if let Some(head) = self.list.0.first() {
            if let Some(c) = needed
                .iter()
                .find(|c| path_key(&c.path()) == path_key(&head.path()))
            {
                let msg = format!(
                    "announce: `gather` asked for `{}`, which is the callback itself; \
                     request the definitions' trampolines instead",
                    c.to_token_stream()
                );
                return Gather::Fetch(quote! { compile_error! { #msg } });
            }
        }
        let mut missing: Vec<Cont> = Vec::new();
        for c in &needed {
            if self.entry_for(c).is_none()
                && !missing
                    .iter()
                    .any(|m| path_key(&m.path()) == path_key(&c.path()))
            {
                missing.push(c.clone());
            }
        }
        if missing.is_empty() {
            return Gather::Ready(
                needed
                    .iter()
                    .map(|c| self.entry_for(c).expect("checked above"))
                    .collect(),
            );
        }
        Gather::Fetch(self.fetch_conts(missing))
    }

    /// Continues the chain: drops the head and invokes the next element. Returns
    /// `None` if the head was the last element.
    pub fn forward(&self) -> Option<TokenStream> {
        if self.list.0.len() < 2 {
            return None;
        }
        let tail: ContList = self.list.0[1..].iter().cloned().collect();
        let (entries, rest) = (&self.entries, &self.rest);
        Some(tail.invoke_with_options(self.options.as_ref(), quote! { #(#entries)* #rest }))
    }
}

/// Result of [`Input::gather`].
pub enum Gather<'a> {
    /// All requested definitions are present, in the requested order.
    Ready(Vec<&'a Entry>),
    /// Emit these tokens: they fetch the missing definitions and call back.
    Fetch(TokenStream),
}

fn path_key(p: &Path) -> String {
    let mut p = p.clone();
    for seg in &mut p.segments {
        seg.arguments = syn::PathArguments::None;
    }
    p.to_token_stream().to_string()
}

#[doc(hidden)]
pub mod __private {
    pub use proc_macro2;
    pub use syn;
}

/// Runs a callback: the body of a `#[proc_macro]` that receives definitions from
/// trampolines.
///
/// - `input` is the input of the proc macro, the tokens a trampoline sends. It is
///   parsed as an [`Input`]; see there for the shape.
/// - `f` is the callback itself. It gets the parsed [`Input`] by value and returns the
///   tokens to emit, or a `syn::Error`.
///
/// A `syn::Error` (the input does not parse as an [`Input`], or `f` fails) becomes a
/// `compile_error!` in the result, so the result can be returned from the proc macro
/// as it is.
///
/// In a proc-macro crate this is a `#[proc_macro]` function. The example is run as a
/// doctest, which cannot define a proc macro: it has no attribute, and `proc_macro`
/// stands for `proc_macro2`.
///
/// ```
/// # use announce_core as announce;
/// # use announce::helper::{Cont, Entry, Input};
/// # use proc_macro2 as proc_macro;
/// # fn needed(_: &Input) -> syn::Result<Vec<Cont>> {
/// #     Ok(vec![Cont::bang(syn::parse_quote!(a::Get))])
/// # }
/// # fn generate(entries: Vec<&Entry>) -> syn::Result<proc_macro::TokenStream> {
/// #     let n = entries.len();
/// #     Ok(template_quote::quote!(const COUNT: usize = #n;))
/// # }
/// /// Receives trait definitions.
/// pub fn my_callback(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
///     announce::helper::callback(input.into(), |input| {
///         // `input` is an `announce::helper::Input`.
///         let entries = match input.gather(needed(&input)?) {
///             announce::helper::Gather::Ready(e) => e,
///             announce::helper::Gather::Fetch(ts) => return Ok(ts),
///         };
///         generate(entries)
///     })
///     .into()
/// }
/// #
/// # // `a::Get!` is there: `generate` runs.
/// # let out = my_callback(syn::parse_quote! { [cb!] a::Get! => [] { trait Get {} } });
/// # assert_eq!(out.to_string(), "const COUNT : usize = 1usize ;");
/// # // It is missing: the callback fetches it and is called again.
/// # let out = my_callback(syn::parse_quote! { [cb!] }).to_string();
/// # assert!(out.starts_with("a :: Get !") && out.contains("cb !"), "{}", out);
/// ```
///
/// The function can be tested with `proc_macro2` streams, because it does not use the
/// `proc_macro` crate:
///
/// ```
/// # use announce_core as announce;
/// use announce::helper::callback;
/// use proc_macro2::TokenStream;
/// use template_quote::quote;
///
/// let run = |input: TokenStream| {
///     callback(input, |input| {
///         let name: syn::Ident = input.rest_as()?;
///         let n = input.entries.len();
///         Ok(quote!(const #name: usize = #n;))
///     })
///     .to_string()
/// };
/// // One definition, then the caller's tokens.
/// let out = run(syn::parse_quote! { [cb!] a::S! => [] { struct S; } COUNT });
/// assert_eq!(out, "const COUNT : usize = 1usize ;");
/// // Input that is not a callback input, and an error of the callback.
/// assert!(run(syn::parse_quote! { not a callback input }).contains("compile_error"));
/// assert!(run(syn::parse_quote! { [cb!] 1 + 1 }).contains("compile_error"));
/// ```
///
/// The macro `callback!` (`announce::helper::callback! { pub fn name(input) { .. } }`)
/// does the same and defines the `#[proc_macro]` function too. It is kept for
/// compatibility; write the function above instead.
pub fn callback(input: TokenStream, f: impl FnOnce(Input) -> Result<TokenStream>) -> TokenStream {
    syn::parse2::<Input>(input)
        .and_then(f)
        .unwrap_or_else(|e| e.to_compile_error())
}

/// Defines a `#[proc_macro]` callback: the same as the function [`callback()`], in
/// the form of a macro.
///
/// **Kept for compatibility; prefer the function [`callback()`]**, called from a
/// `#[proc_macro]` function you write yourself. This macro is a thin wrapper that
/// defines that function: the body returns `syn::Result<proc_macro2::TokenStream>`
/// and gets the [`Input`] by value, as `input`.
///
/// ```ignore
/// announce::helper::callback! {
///     /// Receives trait definitions.
///     pub fn my_callback(input) {
///         let entries = match input.gather(needed(&input)?) {
///             announce::helper::Gather::Ready(e) => e,
///             announce::helper::Gather::Fetch(ts) => return Ok(ts),
///         };
///         generate(entries)
///     }
/// }
/// ```
///
/// is the same as
///
/// ```
/// # use announce_core as announce;
/// # use announce::helper::{Cont, Entry, Input};
/// # use proc_macro2 as proc_macro; // a doctest cannot define a `#[proc_macro]`
/// # fn needed(_: &Input) -> syn::Result<Vec<Cont>> {
/// #     Ok(vec![Cont::bang(syn::parse_quote!(a::Get))])
/// # }
/// # fn generate(entries: Vec<&Entry>) -> syn::Result<proc_macro::TokenStream> {
/// #     let n = entries.len();
/// #     Ok(template_quote::quote!(const COUNT: usize = #n;))
/// # }
/// /// Receives trait definitions.
/// pub fn my_callback(ts: proc_macro::TokenStream) -> proc_macro::TokenStream {
///     announce::helper::callback(ts.into(), |input| {
///         let entries = match input.gather(needed(&input)?) {
///             announce::helper::Gather::Ready(e) => e,
///             announce::helper::Gather::Fetch(ts) => return Ok(ts),
///         };
///         generate(entries)
///     })
///     .into()
/// }
/// #
/// # let out = my_callback(syn::parse_quote! { [cb!] a::Get! => [] { trait Get {} } });
/// # assert_eq!(out.to_string(), "const COUNT : usize = 1usize ;");
/// ```
///
/// The facade crate must re-export the callback (e.g. under `__private`) so
/// that generated code can name it.
#[doc(hidden)]
#[macro_export]
macro_rules! callback {
    ($(#[$m:meta])* $vis:vis fn $name:ident ($input:ident) $body:block) => {
        $(#[$m])*
        #[proc_macro]
        $vis fn $name(__announce_ts: ::proc_macro::TokenStream) -> ::proc_macro::TokenStream {
            ::core::convert::Into::into($crate::helper::callback(
                ::core::convert::Into::into(__announce_ts),
                |$input: $crate::helper::Input|
                 -> $crate::helper::__private::syn::Result<$crate::helper::__private::proc_macro2::TokenStream> {
                    $body
                },
            ))
        }
    };
}

/// Runs a check: the body of a `#[proc_macro]` that runs where an item is announced,
/// and that answers `#[announce]`'s attribute query.
///
/// - `input` is the input of the proc macro: either the attribute query
///   `@__announce_attrs [..] { .. }` or the check's own input, `(ARGS), ITEM`
///   ([`CheckCall`]), each after the prefix `@announce_format "1"` that announce writes.
/// - `attributes` lists the attribute names that the check owns:
///   `#[announce(my_lib::my_check)]` then lets the item carry `#[my_lib]`,
///   `#[my_lib(..)]` and `#[my_lib::..]` attributes. It may be empty, for a check that
///   owns none: the check still answers the query. See [`attrs`] for the query and
///   for what a name matches.
/// - `f` is the check itself. It gets the parsed [`CheckInput`] (`args`, `item` with
///   the owned attributes, and `owned`, which is `attributes`) and returns the tokens
///   to emit, or a `syn::Error`. A check usually returns nothing, or an `Err`.
///
/// The query is answered with `attributes` ([`Query::answer`]) and `f` is not called.
/// For the check's own input, the result is what `f` returns. A `syn::Error` (the
/// input does not parse, a name in `attributes` is not an identifier or is reserved
/// ([`OwnedAttrs::check_names`]), or `f` fails) becomes a `compile_error!`, like in
/// [`callback()`]. So the result can be returned from the proc macro as it is.
///
/// In a proc-macro crate this is a `#[proc_macro]` function. The example is run as a
/// doctest, which cannot define a proc macro: it has no attribute, and `proc_macro`
/// stands for `proc_macro2`.
///
/// ```
/// # use announce_core as announce;
/// # use proc_macro2 as proc_macro;
/// /// Rejects `#[my_lib(bad)]`.
/// pub fn my_check(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
///     announce::helper::check(input.into(), &["my_lib", "other"], |c| {
///         // `c` is an `announce::helper::CheckInput`.
///         for attr in c.owned_attributes() {
///             if attr.path().is_ident("my_lib") && attr.parse_args::<syn::Ident>().map_or(false, |i| i == "bad") {
///                 return Err(syn::Error::new_spanned(&attr, "`bad` is not allowed"));
///             }
///         }
///         Ok(proc_macro2::TokenStream::new())
///     })
///     .into()
/// }
/// #
/// # // The attribute is owned by the check, and allowed.
/// # let ok = my_check(syn::parse_quote! { (), struct S { #[my_lib(skip)] a: u8 } });
/// # assert!(ok.is_empty(), "{}", ok);
/// # // `bad` is an error at the attribute.
/// # let err = my_check(syn::parse_quote! { (), struct S { #[my_lib(bad)] a: u8 } }).to_string();
/// # assert!(err.contains("compile_error") && err.contains("`bad` is not allowed"), "{}", err);
/// ```
///
/// The function can be tested with `proc_macro2` streams, because it does not use the
/// `proc_macro` crate:
///
/// ```
/// # use announce_core as announce;
/// use announce::helper::check;
/// use proc_macro2::TokenStream;
///
/// let run = |input: TokenStream| {
///     check(input, &["my_lib"], |c| {
///         match &c.item {
///             syn::Item::Struct(s) if matches!(s.fields, syn::Fields::Unnamed(_)) => {
///                 Err(syn::Error::new_spanned(&s.fields, "tuple structs are not supported"))
///             }
///             _ => Ok(TokenStream::new()),
///         }
///     })
///     .to_string()
/// };
/// // The check itself.
/// assert_eq!(run(syn::parse_quote! { (), struct S { a: u8 } }), "");
/// assert!(run(syn::parse_quote! { (), struct S(u8); }).contains("compile_error"));
/// // The query is answered with the names, and the check is not run.
/// let out = run(syn::parse_quote! { @__announce_attrs [ me!, next! ] { st } });
/// assert!(out.contains("next !") && out.contains("attrs (my_lib)"), "{}", out);
/// ```
pub fn check(
    input: TokenStream,
    attributes: &[&str],
    f: impl FnOnce(&mut CheckInput) -> Result<TokenStream>,
) -> TokenStream {
    let run = || -> Result<TokenStream> {
        for name in attributes {
            if !is_attribute_name(name) {
                return Err(syn::Error::new(
                    Span::call_site(),
                    format!(
                        "announce: `{}` is not an identifier: it cannot be an owned attribute name",
                        name
                    ),
                ));
            }
        }
        let owned = OwnedAttrs::from_strs(attributes);
        match syn::parse2::<CheckCall>(input)? {
            // `answer` reports a reserved name where the check answers.
            CheckCall::Query(q) => Ok(q.answer(&owned)),
            CheckCall::Run(c) => {
                owned.check_names()?;
                f(&mut c.with_owned(owned))
            }
        }
    };
    run().unwrap_or_else(|e| e.to_compile_error())
}

/// Whether `name` (or `r#name`) can be turned into an `Ident` without a panic.
fn is_attribute_name(name: &str) -> bool {
    let name = name.strip_prefix("r#").unwrap_or(name);
    let mut chars = name.chars();
    matches!(chars.next(), Some(c) if c == '_' || c.is_alphabetic())
        && chars.all(|c| c == '_' || c.is_alphanumeric())
        && name != "_"
}

/// Extracts the trampoline payload from the dummy enum that a derive or
/// attribute continuation receives. Spans, hygiene and `$crate` survive.
pub fn payload(item: TokenStream) -> Option<TokenStream> {
    let di: syn::DeriveInput = syn::parse2(item).ok()?;
    let e = match di.data {
        syn::Data::Enum(e) => e,
        _ => return None,
    };
    let v = e.variants.first()?;
    if v.ident != "__AnnouncePayload" || e.variants.len() != 1 {
        return None;
    }
    let f = match &v.discriminant.as_ref()?.1 {
        syn::Expr::Field(f) => f,
        _ => return None,
    };
    let t = match &*f.base {
        syn::Expr::Tuple(t) => t,
        _ => return None,
    };
    let m = match t.elems.first()? {
        syn::Expr::Macro(m) => m,
        _ => return None,
    };
    if !m.mac.path.is_ident("stringify") {
        return None;
    }
    Some(m.mac.tokens.clone())
}

/// For a derive or attribute whose name clashes with a trampoline: if `item` is a
/// trampoline dummy, returns `target! { @announce_format "1" @__announce_self {
/// target } @__announce_dollar { $ } payload }`. Otherwise returns `None`.
///
/// `target` is the absolute path of the `#[announce(macro_name = ..)]` macro. Pick
/// a path that is valid at the user's site. The prefixes are for the trampoline (see
/// [the wire format](Input#the-wire-format)). `@announce_format "1"` is the version of
/// the format; the payload of the dummy starts with it, and it is moved to the front
/// of the call. The trampoline needs its own path to call itself again, when it holds
/// a large item once, and a real `$` to un-escape an item that has `$` in it.
///
/// In a proc-macro crate this is a `#[proc_macro_derive(Parse)]` function. The example is run
/// as a doctest, which cannot define a proc macro: it has no attribute, and `proc_macro`
/// stands for `proc_macro2`.
///
/// ```
/// # use announce_core as announce;
/// # use announce::helper::{Cont, ContList};
/// # use proc_macro2 as proc_macro;
/// # use proc_macro::TokenStream;
/// pub fn derive_parse(input: TokenStream) -> TokenStream {
///     if let Some(out) = announce::helper::redirect(input.clone().into(), &syn::parse_quote!(::pars::ParseAnnounce)) {
///         return out.into();
///     }
///     // normal derive
/// #   TokenStream::new()
/// }
/// #
/// # // The dummy of a trampoline call goes to `::pars::ParseAnnounce!`, other items do not.
/// # let dummy = ContList(vec![Cont::derive(syn::parse_quote!(Parse))]).invoke(TokenStream::new());
/// # let out = derive_parse(dummy).to_string();
/// # assert!(out.starts_with(":: pars :: ParseAnnounce !"), "{}", out);
/// # assert!(derive_parse(syn::parse_quote! { struct S; }).is_empty());
/// ```
pub fn redirect(item: TokenStream, target: &Path) -> Option<TokenStream> {
    // The payload starts with `@announce_format "VERSION"`, written by whoever
    // made the dummy. A call starts with it, before the two prefixes below, and
    // keeps the version it was made with, so that a trampoline of another version
    // reports the mismatch. A payload without it (a hand-made dummy) gets the
    // version of this crate.
    let (format, body) = split_format(payload(item)?);
    let format = format.unwrap_or_else(format_prefix);
    // A trampoline whose payload contains `$` needs a real `$` from its
    // caller (`@__announce_dollar { $ }`); the others ignore it. A trampoline
    // that holds its item once calls itself again and needs its own path
    // (`@__announce_self { path }`); the others ignore that too. The derive route cannot re-invoke the
    // trampoline itself, so both prefixes are always added here.
    let dollar = proc_macro2::Punct::new('$', proc_macro2::Spacing::Alone);
    Some(quote! {
        #target ! { #format @__announce_self { #target } @__announce_dollar { #dollar } #body }
    })
}

/// What a check receives: `(ARGS), ITEM`, after the prefix `@announce_format "1"` when
/// announce calls the check (a call by hand has no prefix, and parses as well).
///
/// The item has its `#[cfg]` evaluated, and it still has the attributes that the
/// checks own (see [`OwnedAttrs`]), so the check can validate them. [`check`] parses
/// this and also answers the attribute query. Parsing a `CheckInput` by hand fails
/// with an error that says so when `#[announce]` sends the query.
pub struct CheckInput {
    /// The check's arguments, `(ARGS)`.
    pub args: TokenStream,
    /// The cfg-evaluated item, with the owned attributes.
    pub item: syn::Item,
    /// The attribute names that the check declared with `attributes` in
    /// [`check`]. Empty when the input was parsed by hand.
    pub owned: OwnedAttrs,
}

impl CheckInput {
    /// The same input with the declared attribute names.
    pub fn with_owned(mut self, owned: OwnedAttrs) -> Self {
        self.owned = owned;
        self
    }

    /// The owned attributes in the item, in source order: what the check
    /// validates ([`OwnedAttrs::find`]).
    pub fn owned_attributes(&self) -> Vec<Attribute> {
        self.owned.find(&self.item)
    }
}

impl Parse for CheckInput {
    fn parse(input: ParseStream) -> Result<Self> {
        // The call starts with `@announce_format "1"`, unless it was written by hand.
        parse_format(input)?;
        if input.peek(Token![@]) {
            return Err(input.error(
                "announce: this check received the attribute query `@__announce_attrs` that \
                 `#[announce]` sends first, and does not answer it. Run the check with \
                 `announce::helper::check`, or parse `announce::helper::CheckCall` and answer \
                 `Query` (a `macro_rules!` check needs one more arm); every check must \
                 answer. See the `announce::helper::attrs` docs",
            ));
        }
        let content;
        syn::parenthesized!(content in input);
        let args = content.parse()?;
        input.parse::<Token![,]>()?;
        Ok(CheckInput {
            args,
            item: input.parse()?,
            owned: OwnedAttrs::new(),
        })
    }
}

/// Best-effort check that `ty` is announced (`ty: Announce`). `krate` is the
/// consumer's path to the `announce` crate.
pub fn check_type(krate: &Path, ty: &syn::Type) -> TokenStream {
    quote! {
        const _: () = {
            fn __announce_chk<__AnnounceT: ?::core::marker::Sized + #krate::Announce>() {}
            let _ = __announce_chk::<#ty>;
        };
    }
}

/// Best-effort check that the trait at `path` is announced (`dyn path: Announce`).
/// Only meaningful for simple dyn-compatible traits.
pub fn check_trait(krate: &Path, path: &Path) -> TokenStream {
    check_type(krate, &syn::parse_quote!(dyn #path))
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

    #[test]
    fn bang_drops_generic_arguments() {
        assert_eq!(s(Cont::bang(syn::parse_quote!(a::Enum<A, B>))), "a::Enum!");
    }

    #[test]
    fn input_roundtrip_and_meta() {
        let input: Input = syn::parse_quote! {
            [cb!, #[derive(p::D)] N]
            a::F! => [x::check(1), mock(::core::iter::Iterator)] { pub trait F {} }
            #[derive(p::D)] M => [] { pub struct S; }
            (rest)
        };
        assert_eq!(input.list.0.len(), 2);
        assert_eq!(input.entries.len(), 2);
        assert_eq!(s(&input.entries[0].meta[0]), "x::check(1)");
        assert_eq!(
            s(input.entries[0].find_meta("check").unwrap().args.clone()),
            "1"
        );
        assert!(input.entries[0].has_check("check"));
        assert_eq!(
            s(input.entries[0].mock().unwrap()),
            "::core::iter::Iterator"
        );
        assert_eq!(s(input.entries[1].key.path()), "p::D");
        assert_eq!(s(&input.rest), "(rest)");
        let order: Vec<String> = input.entries_in_order().map(|e| s(e.key.path())).collect();
        assert_eq!(order, ["p::D", "a::F"]);
        let fwd = input.forward().unwrap().to_string();
        assert!(fwd.contains("__AnnouncePayload"), "{}", fwd);
    }

    #[test]
    fn entries_know_the_attributes_their_checks_own() {
        let input: Input = syn::parse_quote! {
            [cb!]
            a::F! => [x::check, attrs(my_lib, other), m::info(1), attrs(other, third)] {
                #[my_lib(on_item)]
                pub struct F { #[other] pub a: u8, #[third(x)] #[doc = "d"] pub b: u8 }
            }
            a::G! => [] { pub struct G; }
        };
        let owned = input.entries[0].owned_attrs();
        let names: Vec<String> = owned.names().iter().map(|n| n.to_string()).collect();
        assert_eq!(names, ["my_lib", "other", "third"]);
        assert_eq!(owned.find(&input.entries[0].item().unwrap()).len(), 3);
        let stripped = input.entries[0].item_without_owned_attrs().unwrap();
        assert_eq!(s(&stripped), "pubstructF{puba:u8,#[doc=\"d\"]pubb:u8}");
        // Without `attrs(..)` there is nothing to strip.
        assert!(input.entries[1].owned_attrs().is_empty());
        assert_eq!(
            s(input.entries[1].item_without_owned_attrs().unwrap()),
            "pubstructG;"
        );
        // `attrs` is found by its exact name, not as the last segment of a path.
        let input: Input = syn::parse_quote! { [cb!] a::F! => [lib::attrs(x)] { struct F; } };
        assert!(input.entries[0].owned_attrs().is_empty());
    }

    #[test]
    fn check_input_reports_the_query_it_does_not_answer() {
        let e = syn::parse2::<CheckInput>(template_quote::quote! {
            @__announce_attrs [ c!, f! ] { st }
        })
        .err()
        .unwrap()
        .to_string();
        assert!(
            e.contains("does not answer it") && e.contains("announce::helper::check`"),
            "{}",
            e
        );
        let ok: CheckInput = syn::parse_quote! { (x, y), struct S; };
        assert!(ok.owned.is_empty() && ok.owned_attributes().is_empty());
        let ok = ok.with_owned(OwnedAttrs::from_strs(&["a"]));
        assert!(ok.owned.contains("a"));
    }

    #[test]
    fn check_answers_the_query_without_running_the_closure() {
        let q = || -> TokenStream {
            syn::parse_quote! { @__announce_attrs [ a::check!, b::check!, f::finish! ] { st } }
        };
        let mut ran = false;
        let out = check(q(), &["my_lib", "other"], |_| {
            ran = true;
            Ok(TokenStream::new())
        });
        assert!(!ran);
        assert_eq!(
            s(out),
            "b::check!{@announce_format\"1\"@__announce_attrs[b::check!,f::finish!]{stattrs(my_lib,other)}}"
        );
        // No names: the state is passed on as it is, and the closure is not run.
        let out = check(q(), &[], |_| panic!("the query is not a call"));
        assert_eq!(
            s(out),
            "b::check!{@announce_format\"1\"@__announce_attrs[b::check!,f::finish!]{st}}"
        );
        // Names are written as text: a raw identifier is its plain name.
        let out = check(q(), &["r#type", "type"], |_| unreachable!());
        assert_eq!(
            s(out),
            "b::check!{@announce_format\"1\"@__announce_attrs[b::check!,f::finish!]{stattrs(type)}}"
        );
        // The answer of `Query` is kept as is: a list that does not fit is a compile error.
        let out = s(check(
            syn::parse_quote! { @__announce_attrs [ a::check! ] { st } },
            &[],
            |_| unreachable!(),
        ));
        assert!(out.contains("compile_error") && out.contains("nomacrotocallafterthischeck"));
    }

    #[test]
    fn check_runs_the_closure_with_the_owned_names() {
        let mut seen = None;
        let out = check(
            syn::parse_quote! { (x, y), #[my_lib(a)] struct S { #[other] a: u8, #[kept] b: u8 } },
            &["my_lib", "other"],
            |c| {
                seen = Some((
                    s(&c.args),
                    s(&c.item),
                    c.owned
                        .names()
                        .iter()
                        .map(|n| n.to_string())
                        .collect::<Vec<_>>(),
                    c.owned_attributes().len(),
                ));
                Ok(quote! { pub const SEEN: u8 = 1; })
            },
        );
        // Whatever the closure returns is the result.
        assert_eq!(s(out), "pubconstSEEN:u8=1;");
        let (args, item, owned, found) = seen.unwrap();
        assert_eq!(args, "x,y");
        assert!(
            item.starts_with("#[my_lib(a)]structS{#[other]a:u8"),
            "{}",
            item
        );
        assert_eq!(owned, ["my_lib", "other"]);
        assert_eq!(found, 2);
        // The closure can change the input (`&mut`) and has no names to own.
        let out = check(syn::parse_quote! { (), struct S; }, &[], |c| {
            assert!(c.owned.is_empty());
            c.args = quote! { changed };
            Ok(c.args.clone())
        });
        assert_eq!(s(out), "changed");
    }

    #[test]
    fn check_turns_errors_into_compile_errors() {
        // An error of the closure.
        let out = s(check(
            syn::parse_quote! { (), struct S(u8); },
            &[],
            |c| match &c.item {
                syn::Item::Struct(it) => Err(syn::Error::new_spanned(&it.fields, "no tuples")),
                _ => Ok(TokenStream::new()),
            },
        ));
        assert_eq!(out, "::core::compile_error!{\"notuples\"}");
        // An input that is neither the query nor `(ARGS), ITEM`.
        let mut ran = false;
        let out = s(check(syn::parse_quote! { not a check call }, &[], |_| {
            ran = true;
            Ok(TokenStream::new())
        }));
        assert!(!ran && out.contains("compile_error"), "{}", out);
        // A reserved name, on the query and on the call.
        for input in [
            syn::parse_quote! { @__announce_attrs [ a!, f::finish! ] { st } },
            syn::parse_quote! { (), struct S; },
        ] {
            let out = s(check(input, &["my_lib", "doc"], |_| unreachable!()));
            assert!(
                out.contains("compile_error") && out.contains("`doc`cannotbeanownedattributename"),
                "{}",
                out
            );
        }
        // A name that is not an identifier is an error, not a panic.
        for bad in ["", "my-lib", "1st", "_", "a b", "r#"] {
            let out = s(check(
                syn::parse_quote! { (), struct S; },
                &[bad],
                |_| unreachable!(),
            ));
            assert!(
                out.contains("compile_error") && out.contains("isnotanidentifier"),
                "{:?}: {}",
                bad,
                out
            );
        }
    }

    #[test]
    fn fetch_prepends_and_returns() {
        let input: Input = syn::parse_quote! { [me!, next!] a::F! => [] { struct S; } (rest) };
        let out = s(input.fetch([Cont::bang(syn::parse_quote!(g::G))]));
        // The fetch is recorded in the options group.
        assert_eq!(
            out,
            "g::G!{@announce_format\"1\"(__announce_fetched(g::G))[g::G!,me!,next!]a::F!=>[]{structS;}(rest)}"
        );
        assert_eq!(Cont::bang(syn::parse_quote!(a::F)), input.entries[0].key);
        // Nothing to fetch: nothing to record.
        let out = s(input.fetch([]));
        assert_eq!(
            out,
            "me!{@announce_format\"1\"[me!,next!]a::F!=>[]{structS;}(rest)}"
        );
    }

    #[test]
    fn fetch_of_a_path_that_left_no_entry_is_an_error() {
        // The bookkeeping of `gather`, shared. `g::G`
        // was fetched, the callback is called again and has no entry for it.
        // (`g::G` forwarded to `me` without adding an entry.)
        let again: Input = syn::parse_quote! {
            (__announce_fetched(g::G)) [me!] a::F! => [] { struct S; }
        };
        let out = s(again.fetch([Cont::bang(syn::parse_quote!(g::G))]));
        assert_eq!(
            out,
            "compile_error!{\"announce:`g::G!`wasfetchedbutdidnotprovideadefinition;isitan`#[announce]`trampoline?\"}"
        );
        // The same message as `gather`.
        let from_gather = match again.gather([Cont::bang(syn::parse_quote!(g::G))]) {
            Gather::Fetch(ts) => s(ts),
            Gather::Ready(_) => panic!(),
        };
        assert_eq!(out, from_gather);
        // Generic arguments do not hide the path.
        let out = s(again.fetch([Cont::bang(syn::parse_quote!(g::G<u8>))]));
        assert!(out.contains("wasfetchedbutdidnotprovide"), "{}", out);
        // A path that did answer (an entry is there) may be fetched again, and
        // other paths are fetched as usual; the record grows.
        let answered: Input = syn::parse_quote! {
            (mine, __announce_fetched(g::G)) [me!] g::G! => [] { struct G; }
        };
        let out = s(answered.fetch([Cont::bang(syn::parse_quote!(g::G))]));
        assert!(
            out.starts_with("g::G!{@announce_format\"1\"(mine,__announce_fetched(g::G),__announce_fetched(g::G))"),
            "{}",
            out
        );
        let out = s(answered.fetch([Cont::bang(syn::parse_quote!(h::H))]));
        assert!(
            out.starts_with("h::H!{@announce_format\"1\"(mine,__announce_fetched(g::G),__announce_fetched(h::H))"),
            "{}",
            out
        );
        // One of several missing paths is enough for the error.
        let out = s(again.fetch([
            Cont::bang(syn::parse_quote!(h::H)),
            Cont::bang(syn::parse_quote!(g::G)),
        ]));
        assert!(out.starts_with("compile_error!"), "{}", out);
    }

    #[test]
    fn fetch_and_gather_share_the_record() {
        let input: Input = syn::parse_quote! { [me!] };
        // `gather` fetches `g::G` ...
        let first = match input.gather([Cont::bang(syn::parse_quote!(g::G))]) {
            Gather::Fetch(ts) => ts,
            Gather::Ready(_) => panic!(),
        };
        assert_eq!(
            s(&first),
            "g::G!{@announce_format\"1\"(__announce_fetched(g::G))[g::G!,me!]}"
        );
        // ... and the callback, called again by a non-trampoline, fetches it
        // raw: the error is the same whichever one is used.
        let body: Input =
            syn::parse2(template_quote::quote! { (__announce_fetched(g::G)) [me!] }).unwrap();
        assert!(s(body.fetch([Cont::bang(syn::parse_quote!(g::G))]))
            .contains("wasfetchedbutdidnotprovide"));
        // And the other way round.
        let raw = input.fetch([Cont::bang(syn::parse_quote!(g::G))]);
        assert_eq!(
            s(&raw),
            "g::G!{@announce_format\"1\"(__announce_fetched(g::G))[g::G!,me!]}"
        );
        match body.gather([Cont::bang(syn::parse_quote!(g::G))]) {
            Gather::Fetch(ts) => assert!(s(ts).contains("wasfetchedbutdidnotprovide")),
            Gather::Ready(_) => panic!(),
        }
    }

    #[test]
    fn malformed_entry_is_an_error() {
        let r = syn::parse2::<Input>(template_quote::quote! { [cb!] a::F! => { struct S; } });
        assert!(r.err().unwrap().to_string().contains("malformed entry"));
    }

    #[test]
    fn invoke_names_attribute_dummies() {
        let list: ContList = [
            Cont::derive(syn::parse_quote!(p::D)),
            Cont::bang(syn::parse_quote!(cb)),
        ]
        .into_iter()
        .collect();
        let out = s(list.invoke(TokenStream::new()));
        assert!(
            out.starts_with(
                "#[derive(p::D)]#[allow(dead_code,non_camel_case_types)]enum__AnnounceT_"
            ),
            "{}",
            out
        );
        let body = payload(list.invoke(TokenStream::new())).map(s).unwrap();
        assert!(
            body.starts_with("@announce_format\"1\"[#[derive(p::D)]__AnnounceT_"),
            "{}",
            body
        );
        assert!(redirect(list.invoke(TokenStream::new()), &syn::parse_quote!(::x::Y)).is_some());
    }
}

#[cfg(test)]
mod stable_referrer_tests {
    use super::*;

    #[test]
    fn stable_referrer_is_deterministic() {
        let item: syn::ItemStruct = syn::parse_quote! {
            pub struct S { a: Foo, b: Bar, c: Baz, d: Qux, e: Quux, f: Corge }
        };
        let analyse = || {
            let mut l = type_leak::Leaker::new();
            let context = l.intern_struct(&item, true).unwrap();
            (
                crate::helper::leak::StableReferrer::new(l.finish()),
                context,
            )
        };
        let order = || {
            let (r, _) = analyse();
            r.types()
                .iter()
                .map(|t| t.to_token_stream().to_string())
                .collect::<Vec<_>>()
        };
        let first = order();
        assert_eq!(first.len(), 6);
        let mut sorted = first.clone();
        sorted.sort();
        assert_eq!(first, sorted);
        for _ in 0..20 {
            assert_eq!(order(), first);
        }
        let (r, context) = analyse();
        let mut it: syn::Item = syn::Item::Struct(item.clone());
        r.visit_item_mut(context, &mut it, |_, i| {
            syn::parse_str(&format!("T{}", i)).unwrap()
        });
        let s = it.to_token_stream().to_string();
        assert!(
            s.contains("a : T3") && s.contains("b : T0") && s.contains("f : T2"),
            "{}",
            s
        );
        let ty: syn::Type = syn::parse_quote!(Qux);
        assert_eq!(r.index_of(&ty), Some(5));
        let ty = r.expand(context, ty, |_, i| {
            syn::parse_str(&format!("T{}", i)).unwrap()
        });
        assert_eq!(ty.to_token_stream().to_string(), "T5");
    }

    #[test]
    fn stable_referrer_numbers_required_entries_only() {
        // `::other::X` names another crate: an optional entry, left as written.
        let item: syn::ItemStruct = syn::parse_quote! {
            pub struct S { a: Foo, b: ::other::X, c: Bar, d: ::std::string::String }
        };
        let mut l = type_leak::Leaker::new();
        let context = l.intern_struct(&item, true).unwrap();
        let r = crate::helper::leak::StableReferrer::new(l.finish());
        let types: Vec<String> = r
            .types()
            .iter()
            .map(|t| t.to_token_stream().to_string())
            .collect();
        assert_eq!(types, ["Bar", "Foo"]);
        let mut it = syn::Item::Struct(item);
        r.visit_item_mut(context, &mut it, |_, i| {
            syn::parse_str(&format!("T{}", i)).unwrap()
        });
        let s = it.to_token_stream().to_string();
        assert!(
            s.contains("a : T1") && s.contains("b : :: other :: X") && s.contains("c : T0"),
            "{}",
            s
        );
    }
}

#[cfg(test)]
mod options_tests {
    use super::*;

    #[test]
    fn options_are_parsed_and_forwarded() {
        let mut leaker = type_leak::Leaker::new();
        let context = leaker
            .intern_struct(
                &syn::parse_quote!(
                    struct F<T> {
                        a: Local,
                        b: Vec<T>,
                    }
                ),
                false,
            )
            .unwrap();
        let tl = TypeLeak::new(
            syn::parse_quote!(::my::A),
            syn::parse_quote!(7u64),
            leaker.finish(),
            context,
        );
        let meta = tl.meta();
        let input: Input = syn::parse_quote! {
            (type_leak) [me!, next!]
            a::F! => [#meta] { struct F<T>(T); }
            (rest)
        };
        assert!(input.has_option("type_leak"));
        let fwd = input.forward().unwrap().to_string().replace(' ', "");
        assert!(
            fwd.starts_with("next!{@announce_format\"1\"(type_leak)[next!]a::F!=>"),
            "{}",
            fwd
        );
        let fetch = input
            .fetch([Cont::bang(syn::parse_quote!(g::G))])
            .to_string()
            .replace(' ', "");
        assert!(
            fetch.starts_with(
                "g::G!{@announce_format\"1\"(type_leak,__announce_fetched(g::G))[g::G!,me!,next!]"
            ),
            "{}",
            fetch
        );
        let read = input.entries[0].type_leak().unwrap().unwrap();
        assert_eq!(read.types().count(), 2);
        assert_eq!(read.id.base10_digits(), "7");
        assert_eq!(read.referrer, tl.referrer);
        let proj = read.projection(1, &syn::parse_quote!((u8,)));
        assert_eq!(
            proj.to_string().replace(' ', ""),
            "<()as::my::A<(u8,),7u64,1usize>>::Type"
        );
        let plain: Input = syn::parse_quote! { [me!] a::F! => [] { struct F; } };
        assert!(!plain.has_option("type_leak"));
        assert!(plain.entries[0].type_leak().unwrap().is_none());
    }
}

#[cfg(test)]
mod low_audit_tests {
    use super::*;

    #[test]
    fn cont_equality_ignores_dummy_name_and_generics() {
        let a: Cont = syn::parse_quote!(
            #[derive(p::D)]
            N1
        );
        let b: Cont = syn::parse_quote!(
            #[derive(p::D)]
            N2
        );
        assert_eq!(a, b);
        let c = Cont::Bang(syn::parse_quote!(a::E<T>));
        assert_eq!(c.to_token_stream().to_string().replace(' ', ""), "a::E!");
        let d: Cont = syn::parse_quote!(#[derive(x::A, y::B)]);
        assert_eq!(
            d.path().to_token_stream().to_string().replace(' ', ""),
            "x::A"
        );
    }

    #[test]
    fn rest_like_an_entry_without_meta_is_rest() {
        let i: Input = syn::parse_quote! { [cb!] foo! => bar };
        assert!(i.entries.is_empty());
        assert_eq!(i.rest.to_string().replace(' ', ""), "foo!=>bar");
    }

    #[test]
    fn gather_refuses_self_request() {
        let i: Input = syn::parse_quote! { [me!] };
        match i.gather([Cont::bang(syn::parse_quote!(me))]) {
            Gather::Fetch(ts) => assert!(ts.to_string().contains("which is the callback itself")),
            Gather::Ready(_) => panic!(),
        }
    }
}

#[cfg(test)]
mod gap_tests {
    use super::*;
    use proc_macro2::{Punct, Spacing};

    fn s(ts: impl ToTokens) -> String {
        ts.to_token_stream()
            .to_string()
            .split_whitespace()
            .collect()
    }

    fn mk_dollar() -> Punct {
        Punct::new('$', Spacing::Alone)
    }

    #[test]
    fn check_trait_asserts_the_dyn_trait_is_announced() {
        let out = s(check_trait(
            &syn::parse_quote!(::announce),
            &syn::parse_quote!(my::Tr),
        ));
        assert!(out.contains("dynmy::Tr"), "{}", out);
        assert!(out.contains("::announce::Announce"), "{}", out);
        assert!(out.contains("?::core::marker::Sized"), "{}", out);
        assert!(out.starts_with("const_:()="), "{}", out);
        // `check_trait` is `check_type` of the `dyn` type.
        let direct = s(check_type(
            &syn::parse_quote!(::announce),
            &syn::parse_quote!(dyn my::Tr),
        ));
        assert_eq!(out, direct);
    }

    #[test]
    fn check_type_uses_the_given_krate_path() {
        let out = s(check_type(
            &syn::parse_quote!(::renamed),
            &syn::parse_quote!(Vec<u8>),
        ));
        assert!(out.contains("::renamed::Announce"), "{}", out);
        assert!(out.contains("__announce_chk::<Vec<u8>>"), "{}", out);
    }

    #[test]
    fn meta_name_values_parse_expression_values() {
        let m: Meta = syn::parse_quote!(lib::info(a = 1, b = "x", c = 1 + 2));
        let nv = m.name_values().unwrap();
        assert_eq!(nv.len(), 3);
        assert_eq!(s(&nv[0].path), "a");
        assert_eq!(s(&nv[1].value), "\"x\"");
        assert_eq!(s(&nv[2].value), "1+2");
        // No args: empty list.
        let m: Meta = syn::parse_quote!(lib::info);
        assert!(m.name_values().unwrap().is_empty());
        // A flag is an error.
        let m: Meta = syn::parse_quote!(lib::info(a = 1, on));
        assert!(m.name_values().is_err());
        // Array expressions are expressions; types are not.
        let m: Meta = syn::parse_quote!(lib::info(xs = [A, B]));
        assert_eq!(m.name_values().unwrap().len(), 1);
        let m: Meta = syn::parse_quote!(lib::info(ty = Vec<u8, A>));
        assert!(m.name_values().is_err());
    }

    #[test]
    fn meta_args_default_to_empty() {
        let m: Meta = syn::parse_quote!(lib::info);
        assert!(m.args.is_none());
        assert_eq!(s(&m), "lib::info");
        let m: Meta = syn::parse_quote!(lib::info(1, 2));
        assert_eq!(s(&m), "lib::info(1,2)");
    }

    fn hash_of<T: std::hash::Hash>(t: &T) -> u64 {
        use std::hash::Hasher;
        let mut h = std::collections::hash_map::DefaultHasher::new();
        t.hash(&mut h);
        h.finish()
    }

    #[test]
    fn cont_hash_agrees_with_eq() {
        let a: Cont = syn::parse_quote!(
            #[derive(p::D)]
            N1
        );
        let b: Cont = syn::parse_quote!(
            #[derive(p::D)]
            N2
        );
        let c: Cont = syn::parse_quote!(#[derive(p::D)]);
        assert_eq!(a, b);
        assert_eq!(hash_of(&a), hash_of(&b));
        assert_eq!(hash_of(&a), hash_of(&c));
        let bang = Cont::bang(syn::parse_quote!(p::D));
        let bang_generic = Cont::Bang(syn::parse_quote!(p::D<T>));
        assert_eq!(bang, bang_generic);
        assert_eq!(hash_of(&bang), hash_of(&bang_generic));
        // Different continuations differ, and a derive is not the bang of the same path.
        let other: Cont = syn::parse_quote!(
            #[derive(p::E)]
            N1
        );
        assert_ne!(a, other);
        assert_ne!(a, bang);
        let set: std::collections::HashSet<Cont> = [a.clone(), b, c, other].into_iter().collect();
        assert_eq!(set.len(), 2);
        assert!(set.contains(&a));
    }

    #[test]
    fn cont_path_of_attribute_and_derive() {
        let c: Cont = syn::parse_quote!(
            #[my::attr(x, y)]
            N
        );
        assert_eq!(s(c.path()), "my::attr");
        let d = Cont::derive(syn::parse_quote!(::p::D));
        assert_eq!(s(d.path()), "::p::D");
        assert_eq!(s(&d), "#[derive(::p::D)]");
        // A `derive` without a usable path falls back to the attribute path.
        let e: Cont = syn::parse_quote!(
            #[derive()]
            N
        );
        assert_eq!(s(e.path()), "derive");
    }

    #[test]
    fn input_skips_a_leading_dollar_prefix() {
        let dollar = mk_dollar();
        let ts = template_quote::quote! {
            @ __announce_dollar { #dollar }
            (opt) [cb!, n!]
            a::F! => [x::check(1)] { struct S; }
            (rest)
        };
        let input: Input = syn::parse2(ts).unwrap();
        assert_eq!(s(input.options.as_ref().unwrap()), "opt");
        assert!(input.has_option("opt"));
        assert_eq!(input.list.0.len(), 2);
        assert_eq!(s(&input.list), "[cb!,n!]");
        assert_eq!(input.entries.len(), 1);
        assert_eq!(s(input.entries[0].key.path()), "a::F");
        assert_eq!(s(&input.entries[0].meta[0]), "x::check(1)");
        assert_eq!(s(&input.rest), "(rest)");
        // The prefix is dropped on `forward` and `fetch`.
        let out = s(input.forward().unwrap());
        assert!(!out.contains("__announce_dollar"), "{}", out);
    }

    #[test]
    fn input_dollar_prefix_without_options() {
        let dollar = mk_dollar();
        let ts = template_quote::quote! { @ __announce_dollar { #dollar } [cb!] };
        let input: Input = syn::parse2(ts).unwrap();
        assert!(input.options.is_none());
        assert_eq!(input.list.0.len(), 1);
        assert!(input.entries.is_empty());
        assert!(input.rest.is_empty());
    }

    #[test]
    fn input_skips_the_self_and_dollar_prefixes() {
        let dollar = mk_dollar();
        let ts = template_quote::quote! {
            @ __announce_self { ::a::F }
            @ __announce_dollar { #dollar }
            [cb!, n!] REST
        };
        let input: Input = syn::parse2(ts).unwrap();
        assert!(input.options.is_none());
        assert_eq!(s(&input.list), "[cb!,n!]");
        assert_eq!(s(&input.rest), "REST");
        // Either one alone, and the self prefix before the options group.
        let ts = template_quote::quote! { @ __announce_self { F } (o) [cb!] };
        let input: Input = syn::parse2(ts).unwrap();
        assert!(input.has_option("o"));
        assert_eq!(input.list.0.len(), 1);
        // The prefixes are dropped on `forward`.
        let ts = template_quote::quote! { @ __announce_self { F } [cb!, n!] };
        let out = s(syn::parse2::<Input>(ts).unwrap().forward().unwrap());
        assert!(!out.contains("__announce_self"), "{}", out);
    }

    #[test]
    fn input_rejects_other_prefixes() {
        assert!(syn::parse2::<Input>(template_quote::quote! { @ other { x } [cb!] }).is_err());
        assert!(syn::parse2::<Input>(template_quote::quote! { @ [cb!] }).is_err());
        // The prefix alone is not an input.
        let dollar = mk_dollar();
        let ts = template_quote::quote! { @ __announce_dollar { #dollar } };
        assert!(syn::parse2::<Input>(ts).is_err());
    }

    #[test]
    fn invoke_with_an_empty_list_is_a_compile_error() {
        let out = s(ContList::default().invoke(template_quote::quote!(x y)));
        assert_eq!(out, "compile_error!{\"announce:emptycontinuationlist\"}");
        let opts = template_quote::quote!(expr);
        let out = s(ContList::default().invoke_with_options(Some(&opts), TokenStream::new()));
        assert!(out.contains("emptycontinuationlist"), "{}", out);
    }

    #[test]
    fn invoke_in_expression_position_refuses_the_derive_route() {
        let list: ContList = [
            Cont::derive(syn::parse_quote!(p::D)),
            Cont::bang(syn::parse_quote!(cb)),
        ]
        .into_iter()
        .collect();
        let opts = template_quote::quote!(expr);
        let out = s(list.invoke_with_options(Some(&opts), template_quote::quote!(body)));
        assert!(out.starts_with("compile_error!{"), "{}", out);
        assert!(out.contains("cannotbeusedwith`(expr)`"), "{}", out);
        assert!(!out.contains("__AnnouncePayload"), "{}", out);
        // The same with `expr` leading other options.
        let opts = template_quote::quote!(expr, type_leak);
        let out = s(list.invoke_with_options(Some(&opts), TokenStream::new()));
        assert!(out.contains("cannotbeusedwith`(expr)`"), "{}", out);
    }

    #[test]
    fn only_a_leading_expr_selects_expression_position() {
        // The rule of the trampolines, shared.
        let yes = |ts: TokenStream| is_expression_position(&ts);
        assert!(yes(template_quote::quote!(expr)));
        assert!(yes(template_quote::quote!(expr, type_leak)));
        assert!(yes(template_quote::quote!(expr __announce_fetched(a))));
        assert!(!yes(template_quote::quote!(type_leak, expr)));
        assert!(!yes(template_quote::quote!(__announce_fetched(expr))));
        assert!(!yes(template_quote::quote!((expr))));
        assert!(!yes(template_quote::quote!(expression)));
        assert!(!yes(TokenStream::new()));
        // A derive continuation is refused only for a leading `expr`; with `expr`
        // later the options are a namespace, and the dummy enum is emitted.
        let list: ContList = [
            Cont::derive(syn::parse_quote!(p::D)),
            Cont::bang(syn::parse_quote!(cb)),
        ]
        .into_iter()
        .collect();
        let opts = template_quote::quote!(type_leak, expr);
        let out = s(list.invoke_with_options(Some(&opts), template_quote::quote!(body)));
        assert!(out.contains("__AnnouncePayload"), "{}", out);
        assert!(!out.contains("cannotbeusedwith"), "{}", out);
        let opts = template_quote::quote!(__announce_fetched(expr));
        let out = s(list.invoke_with_options(Some(&opts), template_quote::quote!(body)));
        assert!(out.contains("__AnnouncePayload"), "{}", out);
        // A bang head forwards the group as it is, whatever it holds.
        let bang: ContList = [Cont::bang(syn::parse_quote!(p::cb))].into_iter().collect();
        let opts = template_quote::quote!(type_leak, expr);
        let out = s(bang.invoke_with_options(Some(&opts), template_quote::quote!(body)));
        assert_eq!(
            out,
            "p::cb!{@announce_format\"1\"(type_leak,expr)[p::cb!]body}"
        );
    }

    #[test]
    fn expression_option_is_fine_for_a_bang_head_and_other_options_for_derives() {
        let bang: ContList = [Cont::bang(syn::parse_quote!(p::cb))].into_iter().collect();
        let opts = template_quote::quote!(expr);
        let out = s(bang.invoke_with_options(Some(&opts), template_quote::quote!(body)));
        assert_eq!(out, "p::cb!{@announce_format\"1\"(expr)[p::cb!]body}");
        let derive: ContList = [Cont::derive(syn::parse_quote!(p::D))]
            .into_iter()
            .collect();
        let opts = template_quote::quote!(type_leak);
        let out = s(derive.invoke_with_options(Some(&opts), template_quote::quote!(body)));
        assert!(out.contains("__AnnouncePayload"), "{}", out);
        assert!(
            out.contains("stringify!{@announce_format\"1\"(type_leak)[#[derive(p::D)]__AnnounceT_"),
            "{}",
            out
        );
    }

    #[test]
    fn name_dummies_keeps_existing_names_and_gives_distinct_new_ones() {
        let mut list: ContList = [
            Cont::derive(syn::parse_quote!(p::A)),
            Cont::derive(syn::parse_quote!(p::B)),
            syn::parse_quote!(
                #[derive(p::C)]
                Fixed
            ),
            Cont::bang(syn::parse_quote!(cb)),
        ]
        .into_iter()
        .collect();
        list.name_dummies();
        let names: Vec<String> = list
            .0
            .iter()
            .filter_map(|c| match c {
                Cont::Attr { name: Some(n), .. } => Some(n.to_string()),
                _ => None,
            })
            .collect();
        assert_eq!(names.len(), 3);
        assert_eq!(names[2], "Fixed");
        assert!(names[0].starts_with("__AnnounceT_") && names[1].starts_with("__AnnounceT_"));
        assert_ne!(names[0], names[1]);
        // `name_dummies` is idempotent.
        let before = s(&list);
        list.name_dummies();
        assert_eq!(s(&list), before);
    }

    #[test]
    fn redirect_ignores_ordinary_items() {
        let target: Path = syn::parse_quote!(::x::Y);
        for item in [
            template_quote::quote! { struct S; },
            template_quote::quote! { enum E { A, B } },
            template_quote::quote! { enum E { __AnnouncePayload } },
            template_quote::quote! { enum E { __AnnouncePayload = 1 } },
            template_quote::quote! { enum E { __AnnouncePayload = (1, 0).1 } },
            template_quote::quote! { enum E { __AnnouncePayload = (other!{ x }, 0).1 } },
            template_quote::quote! { enum E { __AnnouncePayload = (stringify!{ x }, 0).1, B } },
            template_quote::quote! { enum E { Other = (stringify!{ x }, 0).1 } },
            template_quote::quote! { fn f() {} },
        ] {
            assert!(payload(item.clone()).is_none(), "{}", item);
            assert!(redirect(item.clone(), &target).is_none(), "{}", item);
        }
    }

    #[test]
    fn redirect_wraps_the_payload_with_the_dollar_prefix() {
        let list: ContList = [
            Cont::derive(syn::parse_quote!(p::D)),
            Cont::bang(syn::parse_quote!(cb)),
        ]
        .into_iter()
        .collect();
        let dummy = list.invoke(template_quote::quote!(body));
        let out = redirect(dummy.clone(), &syn::parse_quote!(::x::Y)).unwrap();
        let out_s = s(&out);
        assert!(
            out_s.starts_with(
                "::x::Y!{@announce_format\"1\"@__announce_self{::x::Y}@__announce_dollar{$}[#[derive(p::D)]__AnnounceT_"
            ),
            "{}",
            out_s
        );
        assert!(out_s.ends_with("cb!]body}"), "{}", out_s);
        // The prefix of the payload is moved to the front, not copied.
        assert_eq!(out_s.matches("@announce_format").count(), 1, "{}", out_s);
        // The `$` is a real punct, not a string.
        let has_dollar = out.into_iter().any(|tt| match tt {
            proc_macro2::TokenTree::Group(g) => g
                .stream()
                .into_iter()
                .any(|t| matches!(t, proc_macro2::TokenTree::Group(g2) if g2.stream().into_iter().any(|x| matches!(x, proc_macro2::TokenTree::Punct(p) if p.as_char() == '$')))),
            _ => false,
        });
        assert!(has_dollar);
        // What `redirect` produces parses as an `Input` (the prefix is skipped).
        let input: Input = syn::parse2(
            match redirect(dummy, &syn::parse_quote!(::x::Y))
                .unwrap()
                .into_iter()
                .last()
            {
                Some(proc_macro2::TokenTree::Group(g)) => g.stream(),
                other => panic!("{:?}", other),
            },
        )
        .unwrap();
        assert_eq!(input.list.0.len(), 2);
        assert_eq!(s(input.rest), "body");
    }
}

/// The format prefix `@announce_format "1"` of every call.
#[cfg(test)]
mod format_tests {
    use super::*;
    use proc_macro2::{Punct, Spacing};

    fn s(ts: impl ToTokens) -> String {
        ts.to_token_stream()
            .to_string()
            .split_whitespace()
            .collect()
    }

    /// What every call of announce starts with, without whitespace.
    const PREFIX: &str = "@announce_format\"1\"";

    #[test]
    fn the_version_is_written_once_and_the_prefix_is_made_of_it() {
        assert_eq!(ANNOUNCE_FORMAT, "1");
        assert_eq!(s(format_prefix()), PREFIX);
        assert_eq!(
            s(format_prefix()),
            format!("@announce_format{:?}", ANNOUNCE_FORMAT)
        );
    }

    #[test]
    fn input_reads_a_call_with_the_prefix_and_one_without() {
        let with: Input = syn::parse_quote! { @announce_format "1" (expr) [cb!] REST };
        assert_eq!(with.format.as_deref(), Some(ANNOUNCE_FORMAT));
        assert!(with.has_option("expr"));
        assert_eq!(s(&with.rest), "REST");
        let without: Input = syn::parse_quote! { (expr) [cb!] REST };
        assert_eq!(without.format, None);
        assert_eq!(s(&without.rest), "REST");
        // The rest of the input is read the same way.
        assert_eq!(s(&with.list), s(&without.list));
        // The prefix is first; the prefixes of the trampolines follow it.
        let all: Input = syn::parse_quote! {
            @announce_format "1" @__announce_self { a::b } @__announce_dollar { $ } (x) [cb!] REST
        };
        assert_eq!(all.format.as_deref(), Some("1"));
        assert!(all.has_option("x"));
        assert_eq!(s(&all.rest), "REST");
        // Entries and a REST that looks like an entry do not hide the prefix.
        let e: Input = syn::parse_quote! {
            @announce_format "1" [cb!] a::F! => [] { struct F; } b::G! => [] { struct G; }
        };
        assert_eq!(e.entries.len(), 2);
    }

    #[test]
    fn the_prefix_after_the_other_prefixes_is_not_a_prefix() {
        let late = syn::parse2::<Input>(template_quote::quote! {
            @__announce_self { a::b } @announce_format "1" [cb!]
        });
        assert!(late.is_err());
        // Two of them are not one either.
        let twice = syn::parse2::<Input>(template_quote::quote! {
            @announce_format "1" @announce_format "1" [cb!]
        });
        assert!(twice.is_err());
    }

    #[test]
    fn another_version_is_one_error_that_names_both() {
        let e = syn::parse2::<Input>(template_quote::quote! { @announce_format "2" [cb!] })
            .unwrap_err()
            .to_string();
        assert_eq!(
            e.matches("announce: incompatible wire format").count(),
            1,
            "{}",
            e
        );
        assert!(
            e.contains("found `@announce_format \"2\"`")
                && e.contains("expected `@announce_format \"1\"`"),
            "{}",
            e
        );
        // The same error from every parser.
        let query = template_quote::quote! { @announce_format "7" @__announce_attrs [ f! ] { } };
        let call = template_quote::quote! { @announce_format "7" (), struct S; };
        let errors = vec![
            syn::parse2::<CheckCall>(query.clone()).err().unwrap(),
            syn::parse2::<CheckCall>(call.clone()).err().unwrap(),
            syn::parse2::<Query>(query).err().unwrap(),
            syn::parse2::<CheckInput>(call).err().unwrap(),
        ];
        for e in errors {
            let e = e.to_string();
            assert!(
                e.starts_with("announce: incompatible wire format")
                    && e.contains("found `@announce_format \"7\"`")
                    && e.contains("expected `@announce_format \"1\"`"),
                "{}",
                e
            );
        }
    }

    #[test]
    fn a_prefix_without_a_string_is_an_error() {
        for bad in [
            template_quote::quote! { @announce_format [cb!] },
            template_quote::quote! { @announce_format 1 [cb!] },
            template_quote::quote! { @announce_format },
        ] {
            let e = syn::parse2::<Input>(bad).unwrap_err().to_string();
            assert!(e.contains("followed by a string literal"), "{}", e);
        }
        // `@` and another word is not the prefix: the input is read as it is.
        let e = syn::parse2::<Input>(template_quote::quote! { @other [cb!] })
            .unwrap_err()
            .to_string();
        assert!(!e.contains("wire format"), "{}", e);
    }

    #[test]
    fn checks_take_a_call_with_the_prefix_and_one_without() {
        for tokens in [
            template_quote::quote! { @announce_format "1" (a, b), struct S; },
            template_quote::quote! { (a, b), struct S; },
        ] {
            let i: CheckInput = syn::parse2(tokens.clone()).unwrap();
            assert_eq!(s(&i.args), "a,b");
            match syn::parse2::<CheckCall>(tokens).unwrap() {
                CheckCall::Run(i) => assert_eq!(s(&i.args), "a,b"),
                CheckCall::Query(_) => panic!("the check itself"),
            }
        }
        for tokens in [
            template_quote::quote! { @announce_format "1" @__announce_attrs [ me!, f! ] { st } },
            template_quote::quote! { @__announce_attrs [ me!, f! ] { st } },
        ] {
            let q: Query = syn::parse2(tokens.clone()).unwrap();
            assert_eq!(q.list.0.len(), 2);
            assert!(matches!(
                syn::parse2::<CheckCall>(tokens).unwrap(),
                CheckCall::Query(_)
            ));
            // The answer has the prefix, whatever the query had.
            assert!(
                s(q.answer(&OwnedAttrs::new())).starts_with(&format!("f!{{{}", PREFIX)),
                "{}",
                s(q.answer(&OwnedAttrs::new()))
            );
        }
        // `check` runs the closure for both forms of the check's own input.
        for tokens in [
            template_quote::quote! { @announce_format "1" (), struct S; },
            template_quote::quote! { (), struct S; },
        ] {
            let mut ran = false;
            let out = check(tokens, &[], |_| {
                ran = true;
                Ok(TokenStream::new())
            });
            assert!(ran && out.is_empty());
        }
        // The query of the other version is a `compile_error!`, and the check is not run.
        let out = s(check(
            template_quote::quote! { @announce_format "3" @__announce_attrs [ me!, f! ] { st } },
            &[],
            |_| unreachable!(),
        ));
        assert!(
            out.starts_with("::core::compile_error!") && out.contains("incompatiblewireformat"),
            "{}",
            out
        );
    }

    #[test]
    fn every_call_that_a_helper_makes_starts_with_the_prefix() {
        let starts = |ts: TokenStream| {
            let out = s(&ts);
            // `path!{` + prefix, or the prefix in the payload of a dummy.
            assert!(
                out.contains(&format!("!{{{}", PREFIX))
                    || out.contains(&format!("stringify!{{{}", PREFIX)),
                "{}",
                out
            );
            // And it is first: nothing but the macro path comes before it.
            let at = out.find(PREFIX).unwrap();
            assert!(
                out[..at].ends_with("!{") || out[..at].ends_with("stringify!{"),
                "{}",
                out
            );
            assert_eq!(out.matches(PREFIX).count(), 1, "{}", out);
        };
        let input: Input = syn::parse_quote! { [me!, next!] a::F! => [] { struct F; } (rest) };
        // `forward`, `fetch` and `gather`.
        starts(input.forward().unwrap());
        starts(input.fetch([Cont::bang(syn::parse_quote!(g::G))]));
        match input.gather([Cont::bang(syn::parse_quote!(g::G))]) {
            Gather::Fetch(ts) => starts(ts),
            Gather::Ready(_) => panic!("g::G is not there"),
        }
        // `forward_transformed`.
        starts(input.forward_transformed(input.entries[0].clone()).unwrap());
        // `ContList::invoke` and `invoke_with_options`, for a macro head and for a
        // derive or attribute head (the prefix is first in the payload of the dummy).
        let bang: ContList = [Cont::bang(syn::parse_quote!(p::cb))].into_iter().collect();
        starts(bang.invoke(TokenStream::new()));
        starts(bang.invoke_with_options(Some(&template_quote::quote!(expr)), TokenStream::new()));
        let derive: ContList = [
            Cont::derive(syn::parse_quote!(p::D)),
            Cont::bang(syn::parse_quote!(cb)),
        ]
        .into_iter()
        .collect();
        let dummy = derive.invoke(template_quote::quote!(body));
        starts(dummy.clone());
        assert!(s(payload(dummy.clone()).unwrap()).starts_with(&format!("{}[", PREFIX)));
        // `redirect`: the prefix is first, before the two prefixes of the trampoline.
        let out = redirect(dummy, &syn::parse_quote!(::x::Y)).unwrap();
        starts(out.clone());
        assert!(s(out).starts_with(&format!(
            "::x::Y!{{{}@__announce_self{{::x::Y}}@__announce_dollar{{$}}",
            PREFIX
        )));
        // The query of a check, and the answer of `check`.
        let q: Query = syn::parse_quote! { @__announce_attrs [ me!, f! ] { st } };
        starts(q.answer(&OwnedAttrs::new()));
        starts(check(
            template_quote::quote! { @__announce_attrs [ me!, f! ] { st } },
            &["a"],
            |_| unreachable!(),
        ));
    }

    #[test]
    fn the_callback_and_transformer_functions_take_and_write_the_prefix() {
        let call = |prefix: TokenStream| {
            template_quote::quote! { #prefix [me!, next!] a::S! => [] { struct S; } NAME }
        };
        let with = call(format_prefix());
        let without = call(TokenStream::new());
        // `callback` reads a call with the prefix and one without.
        for input in [with.clone(), without.clone()] {
            let out = callback(input, |i| {
                let name: syn::Ident = i.rest_as()?;
                Ok(template_quote::quote! { const #name: usize = 1; })
            });
            assert_eq!(s(out), "constNAME:usize=1;");
        }
        // What it forwards has the prefix, whatever it got.
        for input in [with.clone(), without.clone()] {
            let out = s(callback(input, |i| Ok(i.forward().unwrap())));
            assert!(out.starts_with(&format!("next!{{{}", PREFIX)), "{}", out);
        }
        // `transformer` forwards the rewritten entry with the prefix, too.
        for input in [with.clone(), without] {
            let out = s(transformer(input, |mut entry| {
                entry.meta.push(syn::parse_quote!(seen));
                Ok(entry)
            }));
            assert!(
                out.starts_with(&format!("next!{{{}[next!]a::S!=>[seen]", PREFIX)),
                "{}",
                out
            );
        }
        // Another version is one error from both, naming both versions.
        let other = template_quote::quote! { @announce_format "5" [me!, next!] a::S! => [] { struct S; } NAME };
        for out in [
            s(callback(other.clone(), |_| unreachable!())),
            s(transformer(other, |_| unreachable!())),
        ] {
            assert!(
                out.starts_with("::core::compile_error!")
                    && out.contains(
                        "found`@announce_format\\\"5\\\"`,expected`@announce_format\\\"1\\\"`"
                    ),
                "{}",
                out
            );
        }
    }

    #[test]
    fn a_call_without_the_prefix_is_never_checked_for_a_version() {
        // Nothing given, nothing validated: every parser of announce reads the call as it
        // is, with no version and no error about one. (A given version that is not ours is
        // the error: `another_version_is_one_error_that_names_both`.)
        let no_error = |what: &str, out: &str| {
            assert!(
                !out.contains("wire format") && !out.contains("compile_error"),
                "{}: {}",
                what,
                out
            );
        };
        let dollar = Punct::new('$', Spacing::Alone);
        // `Input`: with the options, the prefixes of a trampoline, and an entry.
        let calls = [
            template_quote::quote! { [cb!] },
            template_quote::quote! { (expr) [cb!] a::F! => [] { struct F; } REST },
            template_quote::quote! { @__announce_dollar { #dollar } [cb!] REST },
            template_quote::quote! {
                @__announce_self { ::a::F } @__announce_dollar { #dollar } (o) [cb!] REST
            },
        ];
        for call in calls {
            let input: Input = syn::parse2(call.clone()).unwrap();
            assert_eq!(input.format, None, "{}", s(&call));
            // What it fetches has the prefix of this version.
            let fetched = s(input.fetch([Cont::bang(syn::parse_quote!(g::G))]));
            assert!(fetched.contains(&format!("!{{{}", PREFIX)), "{}", fetched);
            // And the callback function reads it.
            let out = s(callback(call.clone(), |_| Ok(TokenStream::new())));
            assert_eq!(out, "", "{}", s(&call));
        }
        // `Query` and `CheckCall`, `CheckInput`, `check`.
        let query = template_quote::quote! { @__announce_attrs [ me!, f! ] { st } };
        let own = template_quote::quote! { (a), struct S; };
        let q: Query = syn::parse2(query.clone()).unwrap();
        no_error(
            "Query::answer",
            &s(q.answer(&OwnedAttrs::from_strs(&["x"]))),
        );
        assert!(matches!(
            syn::parse2::<CheckCall>(query.clone()).unwrap(),
            CheckCall::Query(_)
        ));
        assert!(matches!(
            syn::parse2::<CheckCall>(own.clone()).unwrap(),
            CheckCall::Run(_)
        ));
        syn::parse2::<CheckInput>(own.clone()).unwrap();
        let answered = s(check(query, &["x"], |_| unreachable!()));
        no_error("check (query)", &answered);
        assert!(
            answered.starts_with(&format!("f!{{{}", PREFIX)),
            "{}",
            answered
        );
        let mut ran = false;
        let out = check(own, &["x"], |_| {
            ran = true;
            Ok(TokenStream::new())
        });
        assert!(ran && out.is_empty());
        // `callback` and `transformer` on a call by hand: no error, whatever they emit.
        let call = template_quote::quote! { [me!, next!] a::S! => [] { struct S; } NAME };
        no_error(
            "callback",
            &s(callback(call.clone(), |i| Ok(i.forward().unwrap()))),
        );
        no_error("transformer", &s(transformer(call, Ok)));
        // `redirect` on a dummy whose payload has no prefix: it writes ours.
        let dummy = template_quote::quote! {
            #[derive(p::D)]
            enum N { __AnnouncePayload = (stringify!{ [cb!] rest }, 0).1 }
        };
        let out = redirect(dummy, &syn::parse_quote!(::x::Y)).unwrap();
        no_error("redirect", &s(&out));
        assert!(s(out).starts_with(&format!("::x::Y!{{{}", PREFIX)));
    }

    #[test]
    fn check_answers_a_query_with_the_prefix_and_without_it() {
        let answer = |tokens: TokenStream| s(check(tokens, &["my_lib"], |_| unreachable!()));
        let with = answer(template_quote::quote! {
            @announce_format "1" @__announce_attrs [ me!, f! ] { st }
        });
        let without = answer(template_quote::quote! { @__announce_attrs [ me!, f! ] { st } });
        assert_eq!(with, without);
        assert_eq!(
            with,
            format!("f!{{{}@__announce_attrs[f!]{{stattrs(my_lib)}}}}", PREFIX)
        );
    }

    #[test]
    fn redirect_moves_the_prefix_of_the_payload_to_the_front_and_keeps_its_version() {
        let dollar = Punct::new('$', Spacing::Alone);
        let dummy = |payload: TokenStream| {
            template_quote::quote! {
                #[derive(p::D)]
                enum N { __AnnouncePayload = (stringify!{ #payload }, 0).1 }
            }
        };
        let target: Path = syn::parse_quote!(::x::Y);
        // A payload with the prefix: moved, not copied.
        let out = redirect(
            dummy(template_quote::quote!(@announce_format "1" [cb!] rest)),
            &target,
        )
        .unwrap();
        assert_eq!(
            s(out),
            format!(
                "::x::Y!{{{}@__announce_self{{::x::Y}}@__announce_dollar{{{}}}[cb!]rest}}",
                PREFIX,
                dollar.as_char()
            )
        );
        // The version of the maker is kept, so that the trampoline reports the mismatch.
        let out = redirect(
            dummy(template_quote::quote!(@announce_format "9" [cb!] rest)),
            &target,
        )
        .unwrap();
        assert!(s(out).starts_with("::x::Y!{@announce_format\"9\"@__announce_self"));
        // A dummy that was made by hand, without the prefix, gets this version.
        let out = redirect(dummy(template_quote::quote!([cb!] rest)), &target).unwrap();
        assert!(s(out).starts_with(&format!("::x::Y!{{{}@__announce_self", PREFIX)));
        // A payload that is shorter than the prefix is left alone.
        let out = redirect(dummy(template_quote::quote!(@announce_format)), &target).unwrap();
        assert!(s(out).ends_with("@__announce_dollar{$}@announce_format}"));
    }
}
