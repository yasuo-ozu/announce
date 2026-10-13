//! Helpers for proc-macro crates that use [`announce`](https://docs.rs/announce)
//! trampolines.
//!
//! - Receive: [`callback!`] defines a callback that parses its [`Input`].
//!   [`Input::forward`] continues the chain. [`Input::gather`] asks for more
//!   definitions first.
//! - Start: [`ContList::invoke`] calls a trampoline from a proc macro.
//! - Redirect: [`redirect`] forwards a derive or attribute to a renamed trampoline.
//! - Transform: [`transformer!`] defines a list element that rewrites the newest entry.
//! - Check: [`check!`] defines a definition-side check (a `#[proc_macro]` that
//!   answers `#[announce]`'s attribute query, see [`OwnedAttrs`], and parses
//!   [`CheckInput`]). [`check_type`] and [`check_trait`] test for
//!   `announce::Announce`.
//! - Attributes of your own inside an announced item: a check owns their names
//!   ([`OwnedAttrs`]); [`Entry::owned_attrs`] and
//!   [`Entry::item_without_owned_attrs`] read them in a callback.
//! - Read META: [`Entry::find_meta`] and [`KvArgs`] parse `key = value` lists.
//! - Resolve relative types (feature `type-leak`, on by default):
//!   [`Entry::type_leak`] reads the [`TypeLeak`] table of a
//!   definition announced with `repeater = ..`, and [`TypeLeak::apply`] rewrites
//!   an item so that its types resolve in any crate. The table holds a
//!   `type_leak::Referrer` (re-exported as [`type_leak`]), so the arguments of the
//!   rewritten types are the ones type-leak gives for the use site ([`TypeLeak::entry_args`]).

pub mod attrs;
mod kv;
#[cfg(feature = "type-leak")]
pub mod leak;
mod transformer;
#[cfg(feature = "type-leak")]
mod typeleak;

pub use attrs::{remove_attributes, CheckCall, OwnedAttrs, Query};
pub use kv::KvArgs;
pub use transformer::signature_only;
/// The version of `type-leak` that [`TypeLeak`] uses: name it through this crate
/// so there is never a second, incompatible copy. (Feature `type-leak`.)
#[cfg(feature = "type-leak")]
pub use type_leak;
#[cfg(feature = "type-leak")]
pub use typeleak::TypeLeak;

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
                // Macro paths cannot carry generic arguments (AUDIT #13).
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
/// continuation list) puts the chain in expression position: its **first**
/// token is the identifier `expr`, as in `(expr)` or `(expr, other)`. That is
/// the rule of the generated trampolines (the `(expr $($rest:tt)*)` pattern of
/// their error arm), so `(other, expr)` does not select it in either. Every
/// other option is a namespace for helpers, forwarded unchanged.
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

    /// The `mock(path)` META, if the item is a copy of a std trait.
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
    /// Unknown keys are ignored. Needs the feature `type-leak` (on by default).
    #[cfg(feature = "type-leak")]
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

    /// The attribute names that the definition's checks own: the `attrs(..)`
    /// META element (empty without one). The definer removed these attributes
    /// from the item it emitted, but they are still in [`item`](Self::item), where
    /// the callback can read them ([`OwnedAttrs::find`]).
    ///
    /// A callback that emits the item itself (rather than reading it) must remove
    /// them first: rustc rejects an attribute that no macro declares. Use
    /// [`item_without_owned_attrs`](Self::item_without_owned_attrs).
    pub fn owned_attrs(&self) -> OwnedAttrs {
        attrs::owned_from_metas(&self.meta)
    }

    /// The payload parsed as an item, without the owned attributes: the item as
    /// the definer emitted it. The same as [`item`](Self::item) when the
    /// definition has no owned attributes.
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

/// What a callback receives: `(OPTIONS)? [list] ENTRY* REST`.
#[derive(Clone, Debug)]
pub struct Input {
    /// The global options group `(..)`. [`forward`](Self::forward) passes it on
    /// unchanged; [`fetch`](Self::fetch) and [`gather`](Self::gather) add
    /// `__announce_fetched(..)` to it (what they fetched, to detect a
    /// continuation that is not a trampoline) and pass the rest on.
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
                    // REST, AUDIT #14.)
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
    /// `new[0]` with the list `[new.., <this list>]`; the new entries arrive in
    /// front of the old ones.
    ///
    /// The paths are recorded in the options group (`__announce_fetched(..)`),
    /// as [`gather`](Self::gather) does. If this callback is called again with a
    /// path in that record but no entry for it, the path is not an `#[announce]`
    /// trampoline (it returned nothing it was asked for), and `fetch` emits a
    /// `compile_error!` saying so instead of fetching until the recursion limit.
    /// A callback that fetches conditionally (only what is still missing, as
    /// `gather` does) never meets that error for a trampoline.
    pub fn fetch(&self, new: impl IntoIterator<Item = Cont>) -> TokenStream {
        self.fetch_conts(new.into_iter().collect())
    }

    /// The bookkeeping of [`fetch`](Self::fetch) and [`gather`](Self::gather):
    /// refuses a continuation that was fetched before and left no entry,
    /// records the new ones in the options group, and invokes `[new.., <list>]`.
    fn fetch_conts(&self, new: Vec<Cont>) -> TokenStream {
        // Already fetched once, but no entry came back: not a trampoline
        // (AUDIT 3 #9). The bookkeeping travels in the options group.
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

    /// "Give me these definitions, then call me again."
    ///
    /// Returns [`Gather::Ready`] with one entry per requested continuation, in
    /// order, when all are present. Otherwise it returns [`Gather::Fetch`]:
    /// tokens that fetch the missing ones and call this callback again.
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
        // re-fetch forever (AUDIT #15).
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

    /// Continues the chain: drops the head and invokes the next element. `None`
    /// if the head was the last element.
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

/// Defines a `#[proc_macro]` callback that parses its [`Input`] and turns
/// `syn::Error`s into compile errors. The body returns
/// `syn::Result<proc_macro2::TokenStream>`.
///
/// ```ignore
/// announce_helper::callback! {
///     /// Receives trait definitions.
///     pub fn my_callback(input) {
///         let entries = match input.gather(needed(&input)?) {
///             announce_helper::Gather::Ready(e) => e,
///             announce_helper::Gather::Fetch(ts) => return Ok(ts),
///         };
///         generate(entries)
///     }
/// }
/// ```
///
/// The facade crate must re-export the callback (e.g. under `__private`) so
/// that generated code can name it.
#[macro_export]
macro_rules! callback {
    ($(#[$m:meta])* $vis:vis fn $name:ident ($input:ident) $body:block) => {
        $(#[$m])*
        #[proc_macro]
        $vis fn $name(__announce_ts: ::proc_macro::TokenStream) -> ::proc_macro::TokenStream {
            let __announce_run = |__announce_ts: ::proc_macro::TokenStream|
             -> $crate::__private::syn::Result<$crate::__private::proc_macro2::TokenStream> {
                let $input: $crate::Input = $crate::__private::syn::parse(__announce_ts)?;
                $body
            };
            match __announce_run(__announce_ts) {
                ::core::result::Result::Ok(ts) => ts.into(),
                ::core::result::Result::Err(e) => e.to_compile_error().into(),
            }
        }
    };
}

/// Defines a definition-side check: a `#[proc_macro]` that runs at the place where
/// an item is announced, and answers `#[announce]`'s attribute query.
///
/// ```ignore
/// announce_helper::check! {
///     /// Rejects `#[my_lib(bad)]` and tuple structs.
///     pub fn my_check(input) attributes(my_lib, other) {
///         // `input` is an `announce_helper::CheckInput`: `args`, `item` (with the
///         // owned attributes) and `owned`.
///         for attr in input.owned_attributes() {
///             if attr.path().is_ident("my_lib") && attr.parse_args::<syn::Ident>().map_or(false, |i| i == "bad") {
///                 return Err(syn::Error::new_spanned(attr, "`bad` is not allowed"));
///             }
///         }
///         Ok(proc_macro2::TokenStream::new())
///     }
/// }
/// ```
///
/// `attributes(..)` is optional. It lists the attribute names the check owns:
/// `#[announce(my_lib::my_check)]` lets the item carry `#[my_lib]`,
/// `#[my_lib(..)]`, `#[my_lib::..]` attributes, which `#[announce]` removes from
/// the item it emits. Leave it out for a check that owns none; the check still
/// answers the query. The body returns `syn::Result<proc_macro2::TokenStream>`,
/// like [`callback!`]; a check usually returns nothing, or a `compile_error!` as
/// an `Err`. A name that rustc or a standard tool uses (`doc`, `cfg`, ..) is an
/// error where `#[announce]` is used. See [`OwnedAttrs`] for what a name matches.
#[macro_export]
macro_rules! check {
    (
        $(#[$m:meta])* $vis:vis fn $name:ident ($input:ident)
        $(attributes($($attr:ident),* $(,)?))?
        $body:block
    ) => {
        $(#[$m])*
        #[proc_macro]
        $vis fn $name(__announce_ts: ::proc_macro::TokenStream) -> ::proc_macro::TokenStream {
            let __announce_owned = $crate::OwnedAttrs::from_strs(&[$($(::core::stringify!($attr)),*)?]);
            let __announce_run = |__announce_ts: ::proc_macro::TokenStream|
             -> $crate::__private::syn::Result<$crate::__private::proc_macro2::TokenStream> {
                match $crate::__private::syn::parse::<$crate::CheckCall>(__announce_ts)? {
                    $crate::CheckCall::Query(__announce_q) => {
                        ::core::result::Result::Ok(__announce_q.answer(&__announce_owned))
                    }
                    $crate::CheckCall::Run(__announce_i) => {
                        let $input = __announce_i.with_owned(__announce_owned.clone());
                        $body
                    }
                }
            };
            match __announce_run(__announce_ts) {
                ::core::result::Result::Ok(ts) => ts.into(),
                ::core::result::Result::Err(e) => e.to_compile_error().into(),
            }
        }
    };
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

/// For a derive or attribute whose name clashes with a trampoline: if `item` is
/// a trampoline dummy, returns `target! { @__announce_self { target }
/// @__announce_dollar { $ } payload }`.
///
/// `target` is the absolute path of the `#[announce(macro_name = ..)]` macro.
/// Pick a path that is valid at the user's site. The two prefixes are for the
/// trampoline (see DESIGN.md sections 4 and 4.1): it needs its own path to call
/// itself again, when it holds a large item once, and a real `$` to
/// un-escape an item that has `$` in it. `Input::parse` skips them if a
/// callback receives them.
///
/// ```ignore
/// #[proc_macro_derive(Parse)]
/// pub fn derive_parse(input: TokenStream) -> TokenStream {
///     if let Some(out) = announce_helper::redirect(input.clone().into(), &syn::parse_quote!(::pars::ParseAnnounce)) {
///         return out.into();
///     }
///     // normal derive
/// }
/// ```
pub fn redirect(item: TokenStream, target: &Path) -> Option<TokenStream> {
    let body = payload(item)?;
    // A trampoline whose payload contains `$` needs a real `$` from its
    // caller (`@__announce_dollar { $ }`, DESIGN.md section 4); the others
    // ignore it. A trampoline that holds its item once calls itself again
    // and needs its own path (`@__announce_self { path }`, section 4.1); the
    // others ignore that too. The derive route cannot re-invoke the
    // trampoline itself, so both prefixes are always added here.
    let dollar = proc_macro2::Punct::new('$', proc_macro2::Spacing::Alone);
    Some(quote! { #target ! { @__announce_self { #target } @__announce_dollar { #dollar } #body } })
}

/// What a definition-side check receives: `(ARGS), ITEM`.
///
/// The item has its `#[cfg]` evaluated, and it still has the attributes that the
/// checks own (see [`OwnedAttrs`]): the check is where they are validated.
/// [`check!`] defines a check that can use this, and it also answers the
/// attribute query; parsing a `CheckInput` by hand fails with an error that says
/// so when `#[announce]` sends the query.
pub struct CheckInput {
    /// The check's arguments, `(ARGS)`.
    pub args: TokenStream,
    /// The cfg-evaluated item, with the owned attributes.
    pub item: syn::Item,
    /// The attribute names the check declared with `attributes(..)` in
    /// [`check!`]; empty when the input was parsed by hand.
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
                 `#[announce]` sends first, and does not answer it. Define the check with \
                 `announce_helper::check!`, or parse `announce_helper::CheckCall` and answer \
                 `Query` (a `macro_rules!` check needs one more arm); every check must \
                 answer. See the `announce_helper::attrs` docs",
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
            @__announce_attrs [ f! ] { st }
        })
        .err()
        .unwrap()
        .to_string();
        assert!(
            e.contains("does not answer it") && e.contains("announce_helper::check!"),
            "{}",
            e
        );
        let ok: CheckInput = syn::parse_quote! { (x, y), struct S; };
        assert!(ok.owned.is_empty() && ok.owned_attributes().is_empty());
        let ok = ok.with_owned(OwnedAttrs::from_strs(&["a"]));
        assert!(ok.owned.contains("a"));
    }

    #[test]
    fn fetch_prepends_and_returns() {
        let input: Input = syn::parse_quote! { [me!, next!] a::F! => [] { struct S; } (rest) };
        let out = s(input.fetch([Cont::bang(syn::parse_quote!(g::G))]));
        // The fetch is recorded in the options group (ALGORITHM_FINDINGS #13).
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
        // ALGORITHM_FINDINGS #13: the bookkeeping of `gather`, shared. `g::G`
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

    #[cfg(feature = "type-leak")]
    #[test]
    fn stable_referrer_is_deterministic() {
        let item: syn::ItemStruct = syn::parse_quote! {
            pub struct S { a: Foo, b: Bar, c: Baz, d: Qux, e: Quux, f: Corge }
        };
        let analyse = || {
            let mut l = type_leak::Leaker::new();
            let context = l.intern_struct(&item, true).unwrap();
            (crate::leak::StableReferrer::new(l.finish()), context)
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

    #[cfg(feature = "type-leak")]
    #[test]
    fn stable_referrer_numbers_required_entries_only() {
        // `::other::X` names another crate: an optional entry, left as written.
        let item: syn::ItemStruct = syn::parse_quote! {
            pub struct S { a: Foo, b: ::other::X, c: Bar, d: ::std::string::String }
        };
        let mut l = type_leak::Leaker::new();
        let context = l.intern_struct(&item, true).unwrap();
        let r = crate::leak::StableReferrer::new(l.finish());
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

#[cfg(all(test, feature = "type-leak"))]
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
        // ALGORITHM_FINDINGS #11: the rule of the trampolines, shared.
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
