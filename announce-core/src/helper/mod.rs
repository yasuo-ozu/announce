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
//! - Read META: [`Entry::find_meta`] and [`KvArgs`] parse `key = value` lists.
//!   [`Entry::owned_attrs`] and [`Entry::item_without_owned_attrs`] read owned
//!   attributes.
//! - Resolve relative types:
//!   [`Entry::type_leak`] reads the [`TypeLeak`] table of a definition announced with
//!   `repeater = ..`. [`TypeLeak::apply`] rewrites an item so that its types resolve
//!   in any crate.
//!
//! The shape of the tokens that a callback receives is described at [`Input`].
//!
//! As `announce::helper`, this module needs the feature `proc-macro-api` of
//! `announce`, which is on by default (`default-features = false` leaves it out, for
//! `no_std` definitions).

pub mod attrs;
mod kv;
pub mod leak;
mod transformer;
mod typeleak;

pub use attrs::{remove_attributes, CheckCall, OwnedAttrs, Query};
pub use kv::KvArgs;
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

    /// Invokes the head with `[self] body`, where `body` is `ENTRY* REST`.
    pub fn invoke(&self, body: TokenStream) -> TokenStream {
        self.invoke_with_options(None, body)
    }

    /// Like [`invoke`](Self::invoke), with a global options group `(options) [..] ..`.
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
        match list.0.first() {
            None => quote! { compile_error! { "announce: empty continuation list" } },
            Some(Cont::Attr { .. }) if in_expr => quote! {
                compile_error! { "announce: a derive/attribute continuation emits items, so it cannot be used with `(expr)`" }
            },
            Some(Cont::Bang(p)) => quote! { #p ! { #opts #list #body } },
            Some(Cont::Attr { attr, name }) => quote! {
                #attr
                #[allow(dead_code, non_camel_case_types)]
                enum #name { __AnnouncePayload = (stringify! { #opts #list #body }, 0).1 }
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

    /// The args as a [`KvArgs`] list. Absent args give an empty list.
    pub fn kv(&self) -> Result<KvArgs> {
        self.parse_args_with(KvArgs::parse)
    }

    /// The args as `key = value` pairs. Values must be expressions; use
    /// [`kv`](Self::kv) for types and lists.
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
    /// The META elements: checks, `meta =`, `mock(..)`, `type_leak(..)`.
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

    /// The `type_leak(..)` META of an entry announced with `repeater = ..`.
    /// Unknown keys are ignored.
    pub fn type_leak(&self) -> Result<Option<TypeLeak>> {
        let m = match self.find_meta("type_leak") {
            Some(m) => m,
            None => return Ok(None),
        };
        TypeLeak::from_kv(&mut m.kv()?).map(Some)
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

/// What a callback receives: `(OPTIONS)? [LIST] ENTRY* REST`.
///
/// # The wire format
///
/// A callback receives these tokens:
///
/// ```text
/// (OPTIONS)? [LIST] ENTRY* REST
/// ```
///
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
///   `@__announce_dollar { $ }` before this input. Only trampolines and [`redirect`]
///   use them. Parsing an [`Input`] skips them.
///
/// `META` holds these elements, as the definer wrote them:
///
/// - `path` or `path(args)`: a check (`#[announce(path(args))]`);
/// - `path(args)`: a `meta = path(args)` entry, never called ([`Entry::find_meta`]);
/// - `mock(path)`: the item stands in for the item at `path` ([`Entry::mock`]);
/// - `attrs(a, b)`: the attribute names that checks own ([`Entry::owned_attrs`]);
/// - `type_leak(..)`: the type table of a `repeater` ([`Entry::type_leak`]).
///
/// Ignore elements you do not know.
///
/// ```
/// # use announce_core as announce;
/// use announce::helper::Input;
///
/// let input: Input = syn::parse_quote! {
///     [cb!]
///     a::F! => [my_lib::check, mock(::core::iter::Iterator)] { pub trait F {} }
///     REST tokens
/// };
/// let entry = &input.entries[0];
/// assert!(entry.has_check("check"));
/// assert!(entry.mock().is_some());
/// assert_eq!(input.rest.to_string(), "REST tokens");
/// ```
///
/// # A real example
///
/// This is what a callback gets from a real chain, captured by the test
/// `tests/user/tests/wire.rs`. The definitions, in `tests/defs/src/wire.rs`
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
/// use announce_test_defs::wire as defs;
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
    /// ```ignore
    /// let entries = match input.gather(needed) {
    ///     Gather::Ready(e) => e,
    ///     Gather::Fetch(ts) => return Ok(ts),
    /// };
    /// ```
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
///   ([`CheckCall`]).
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
/// ```ignore
/// /// Rejects `#[my_lib(bad)]` and tuple structs.
/// #[proc_macro]
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
/// trampoline dummy, returns `target! { @__announce_self { target }
/// @__announce_dollar { $ } payload }`. Otherwise returns `None`.
///
/// `target` is the absolute path of the `#[announce(macro_name = ..)]` macro. Pick
/// a path that is valid at the user's site. The two prefixes are for the
/// trampoline (see [the wire format](Input#the-wire-format)): it needs its own path
/// to call itself again, when it holds a large item once, and a real `$` to
/// un-escape an item that has `$` in it.
///
/// ```ignore
/// #[proc_macro_derive(Parse)]
/// pub fn derive_parse(input: TokenStream) -> TokenStream {
///     if let Some(out) = announce::helper::redirect(input.clone().into(), &syn::parse_quote!(::pars::ParseAnnounce)) {
///         return out.into();
///     }
///     // normal derive
/// }
/// ```
pub fn redirect(item: TokenStream, target: &Path) -> Option<TokenStream> {
    let body = payload(item)?;
    // A trampoline whose payload contains `$` needs a real `$` from its
    // caller (`@__announce_dollar { $ }`); the others ignore it. A trampoline
    // that holds its item once calls itself again and needs its own path
    // (`@__announce_self { path }`); the others ignore that too. The derive route cannot re-invoke the
    // trampoline itself, so both prefixes are always added here.
    let dollar = proc_macro2::Punct::new('$', proc_macro2::Spacing::Alone);
    Some(quote! { #target ! { @__announce_self { #target } @__announce_dollar { #dollar } #body } })
}

/// What a check receives: `(ARGS), ITEM`.
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
            "b::check!{@__announce_attrs[b::check!,f::finish!]{stattrs(my_lib,other)}}"
        );
        // No names: the state is passed on as it is, and the closure is not run.
        let out = check(q(), &[], |_| panic!("the query is not a call"));
        assert_eq!(
            s(out),
            "b::check!{@__announce_attrs[b::check!,f::finish!]{st}}"
        );
        // Names are written as text: a raw identifier is its plain name.
        let out = check(q(), &["r#type", "type"], |_| unreachable!());
        assert_eq!(
            s(out),
            "b::check!{@__announce_attrs[b::check!,f::finish!]{stattrs(type)}}"
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
            "g::G!{(__announce_fetched(g::G))[g::G!,me!,next!]a::F!=>[]{structS;}(rest)}"
        );
        assert_eq!(Cont::bang(syn::parse_quote!(a::F)), input.entries[0].key);
        // Nothing to fetch: nothing to record.
        let out = s(input.fetch([]));
        assert_eq!(out, "me!{[me!,next!]a::F!=>[]{structS;}(rest)}");
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
            out.starts_with("g::G!{(mine,__announce_fetched(g::G),__announce_fetched(g::G))"),
            "{}",
            out
        );
        let out = s(answered.fetch([Cont::bang(syn::parse_quote!(h::H))]));
        assert!(
            out.starts_with("h::H!{(mine,__announce_fetched(g::G),__announce_fetched(h::H))"),
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
        assert_eq!(s(&first), "g::G!{(__announce_fetched(g::G))[g::G!,me!]}");
        // ... and the callback, called again by a non-trampoline, fetches it
        // raw: the error is the same whichever one is used.
        let body: Input =
            syn::parse2(template_quote::quote! { (__announce_fetched(g::G)) [me!] }).unwrap();
        assert!(s(body.fetch([Cont::bang(syn::parse_quote!(g::G))]))
            .contains("wasfetchedbutdidnotprovide"));
        // And the other way round.
        let raw = input.fetch([Cont::bang(syn::parse_quote!(g::G))]);
        assert_eq!(s(&raw), "g::G!{(__announce_fetched(g::G))[g::G!,me!]}");
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
        assert!(body.starts_with("[#[derive(p::D)]__AnnounceT_"), "{}", body);
        assert!(redirect(list.invoke(TokenStream::new()), &syn::parse_quote!(::x::Y)).is_some());
    }
}

#[cfg(test)]
mod kv_tests {
    use super::*;

    #[test]
    fn kv_types_lists_flags_and_errors() {
        let m: Meta = syn::parse_quote!(lib::info(ty = ::std::collections::HashMap<u8, Vec<u8>>, out = fn() -> u8, xs = [A, b::C], on));
        let mut kv = m.kv().unwrap();
        let ty: syn::Type = kv.require("ty").unwrap();
        assert_eq!(
            ty.to_token_stream().to_string().replace(' ', ""),
            "::std::collections::HashMap<u8,Vec<u8>>"
        );
        let out: syn::Type = kv.require("out").unwrap();
        assert_eq!(
            out.to_token_stream().to_string().replace(' ', ""),
            "fn()->u8"
        );
        let xs: Vec<Path> = kv.list("xs").unwrap().unwrap();
        assert_eq!(xs.len(), 2);
        assert!(kv.flag("on").unwrap());
        assert!(kv.get::<syn::Type>("absent").unwrap().is_none());
        kv.finish().unwrap();

        let mut kv: KvArgs = syn::parse_quote!(a = 1, b = 2);
        let _: syn::LitInt = kv.require("a").unwrap();
        assert!(kv
            .finish()
            .unwrap_err()
            .to_string()
            .contains("unknown key `b`"));
        assert!(syn::parse2::<KvArgs>(template_quote::quote!(a = 1, a = 2)).is_err());
        let mut kv: KvArgs = syn::parse_quote!();
        assert!(kv
            .require::<syn::Type>("x")
            .unwrap_err()
            .to_string()
            .contains("missing `x`"));
    }

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
            fwd.starts_with("next!{(type_leak)[next!]a::F!=>"),
            "{}",
            fwd
        );
        let fetch = input
            .fetch([Cont::bang(syn::parse_quote!(g::G))])
            .to_string()
            .replace(' ', "");
        assert!(
            fetch.starts_with("g::G!{(type_leak,__announce_fetched(g::G))[g::G!,me!,next!]"),
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
mod kv_balance_tests {
    use super::*;

    #[test]
    fn unbalanced_angle_is_an_error() {
        let e = syn::parse2::<KvArgs>(template_quote::quote!(a = x < y, b = 1)).unwrap_err();
        assert!(
            e.to_string().contains("unbalanced `<` in the value of `a`"),
            "{}",
            e
        );
        assert!(syn::parse2::<KvArgs>(template_quote::quote!(a = 1 << 2, b = 3)).is_err());
        let mut kv: KvArgs = syn::parse_quote!(a = (x < y), b = 1, f = fn() -> u8);
        assert!(kv.tokens("a").is_some() && kv.tokens("b").is_some() && kv.tokens("f").is_some());
        kv.finish().unwrap();
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
mod kv_error_tests {
    use super::*;

    fn parse(ts: TokenStream) -> Result<KvArgs> {
        syn::parse2(ts)
    }

    #[test]
    fn missing_value_after_equals_is_an_error() {
        let e = parse(template_quote::quote!(a =)).unwrap_err().to_string();
        assert_eq!(e, "missing value for `a`");
        let e = parse(template_quote::quote!(a = , b = 1))
            .unwrap_err()
            .to_string();
        assert_eq!(e, "missing value for `a`");
        let e = parse(template_quote::quote!(ok = 1, b =))
            .unwrap_err()
            .to_string();
        assert_eq!(e, "missing value for `b`");
    }

    #[test]
    fn get_on_a_flag_key_needs_a_value() {
        let mut kv: KvArgs = syn::parse_quote!(f, a = 1);
        let e = kv.get::<syn::Type>("f").unwrap_err().to_string();
        assert_eq!(e, "`f` needs a value");
        // `require` reports the same error, not "missing".
        let mut kv: KvArgs = syn::parse_quote!(f);
        assert_eq!(
            kv.require::<syn::Type>("f").unwrap_err().to_string(),
            "`f` needs a value"
        );
    }

    #[test]
    fn list_on_a_flag_key_needs_a_bracketed_value() {
        let mut kv: KvArgs = syn::parse_quote!(f, a = 1);
        let e = kv.list::<syn::Path>("f").unwrap_err().to_string();
        assert_eq!(e, "`f` needs a `[..]` value");
    }

    #[test]
    fn flag_on_a_key_with_a_value_takes_no_value() {
        let mut kv: KvArgs = syn::parse_quote!(a = 1);
        let e = kv.flag("a").unwrap_err().to_string();
        assert_eq!(e, "`a` takes no value");
        // An absent flag is false and does not consume anything else.
        let mut kv: KvArgs = syn::parse_quote!(b = 1);
        assert!(!kv.flag("a").unwrap());
        assert!(kv.tokens("b").is_some());
    }

    #[test]
    fn list_on_a_non_bracketed_value_is_an_error() {
        let mut kv: KvArgs = syn::parse_quote!(a = 1);
        let e = kv.list::<syn::LitInt>("a").unwrap_err().to_string();
        assert!(e.contains("expected square brackets"), "{}", e);
        let mut kv: KvArgs = syn::parse_quote!(a = (x, y));
        let e = kv.list::<syn::Path>("a").unwrap_err().to_string();
        assert!(e.contains("expected square brackets"), "{}", e);
        let mut kv: KvArgs = syn::parse_quote!(a = [1, 2] extra);
        assert!(kv.list::<syn::LitInt>("a").is_err());
    }

    #[test]
    fn list_element_and_value_parse_errors_surface() {
        let mut kv: KvArgs = syn::parse_quote!(a = [x, 1]);
        assert!(kv.list::<syn::Path>("a").is_err());
        let mut kv: KvArgs = syn::parse_quote!(a = 1);
        assert!(kv.get::<syn::Path>("a").is_err());
    }

    #[test]
    fn list_accepts_empty_and_trailing_comma() {
        let mut kv: KvArgs = syn::parse_quote!(a = [], b = [x::Y, z,]);
        assert!(kv.list::<Path>("a").unwrap().unwrap().is_empty());
        assert_eq!(kv.list::<Path>("b").unwrap().unwrap().len(), 2);
        kv.finish().unwrap();
    }

    #[test]
    fn taken_keys_are_gone_and_finish_names_the_first_leftover() {
        let mut kv: KvArgs = syn::parse_quote!(a = 1, b, c = 2);
        assert!(kv.tokens("a").is_some());
        assert!(kv.tokens("a").is_none());
        assert_eq!(kv.finish().unwrap_err().to_string(), "unknown key `b`");
    }

    #[test]
    fn duplicate_keys_and_keywords() {
        let e = parse(template_quote::quote!(a, a)).unwrap_err().to_string();
        assert_eq!(e, "duplicate key `a`");
        // Keywords are fine as keys.
        let mut kv: KvArgs = syn::parse_quote!(type = u8, crate = a::b, fn);
        assert!(kv.get::<syn::Type>("type").unwrap().is_some());
        assert!(kv.get::<Path>("crate").unwrap().is_some());
        assert!(kv.flag("fn").unwrap());
        kv.finish().unwrap();
    }

    #[test]
    fn values_run_to_the_next_top_level_comma() {
        // `1 b = 2` is all one value; two flags need a comma between them.
        let mut kv = parse(template_quote::quote!(a = 1 b = 2)).unwrap();
        assert_eq!(kv.tokens("a").unwrap().to_string().replace(' ', ""), "1b=2");
        assert!(parse(template_quote::quote!(a b)).is_err());
    }

    #[test]
    fn equality_comparison_is_a_flag_followed_by_junk() {
        // `a == b` is not `a = (= b)`: `==` ends neither value nor key.
        assert!(parse(template_quote::quote!(a == b)).is_err());
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
        // A flag, or a non-expression value, is an error; `kv` accepts them.
        let m: Meta = syn::parse_quote!(lib::info(a = 1, on));
        assert!(m.name_values().is_err());
        let m: Meta = syn::parse_quote!(lib::info(a = 1, on));
        assert!(m.kv().is_ok());
        // Array expressions are expressions; types are not.
        let m: Meta = syn::parse_quote!(lib::info(xs = [A, B]));
        assert_eq!(m.name_values().unwrap().len(), 1);
        let m: Meta = syn::parse_quote!(lib::info(ty = Vec<u8, A>));
        assert!(m.name_values().is_err());
        assert!(m.kv().is_ok());
    }

    #[test]
    fn meta_args_default_to_empty() {
        let m: Meta = syn::parse_quote!(lib::info);
        assert!(m.args.is_none());
        assert!(m.kv().unwrap().finish().is_ok());
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
        assert_eq!(out, "p::cb!{(type_leak,expr)[p::cb!]body}");
    }

    #[test]
    fn expression_option_is_fine_for_a_bang_head_and_other_options_for_derives() {
        let bang: ContList = [Cont::bang(syn::parse_quote!(p::cb))].into_iter().collect();
        let opts = template_quote::quote!(expr);
        let out = s(bang.invoke_with_options(Some(&opts), template_quote::quote!(body)));
        assert_eq!(out, "p::cb!{(expr)[p::cb!]body}");
        let derive: ContList = [Cont::derive(syn::parse_quote!(p::D))]
            .into_iter()
            .collect();
        let opts = template_quote::quote!(type_leak);
        let out = s(derive.invoke_with_options(Some(&opts), template_quote::quote!(body)));
        assert!(out.contains("__AnnouncePayload"), "{}", out);
        assert!(
            out.contains("stringify!{(type_leak)[#[derive(p::D)]__AnnounceT_"),
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
                "::x::Y!{@__announce_self{::x::Y}@__announce_dollar{$}[#[derive(p::D)]__AnnounceT_"
            ),
            "{}",
            out_s
        );
        assert!(out_s.ends_with("cb!]body}"), "{}", out_s);
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
