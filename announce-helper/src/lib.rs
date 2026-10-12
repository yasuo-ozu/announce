//! Helpers for proc-macro crates that use [`announce`](https://docs.rs/announce)
//! trampolines.
//!
//! - Receive: [`callback!`] defines a callback that parses its [`Input`].
//!   [`Input::forward`] continues the chain. [`Input::gather`] asks for more
//!   definitions first.
//! - Start: [`ContList::invoke`] calls a trampoline from a proc macro.
//! - Redirect: [`redirect`] forwards a derive or attribute to a renamed trampoline.
//! - Transform: [`transformer!`] defines a list element that rewrites the newest entry.
//! - Check: [`CheckInput`] parses a definition-side check. [`check_type`] and
//!   [`check_trait`] test for `announce::Announce`.
//! - Read META: [`Entry::find_meta`] and [`KvArgs`] parse `key = value` lists.

mod kv;
#[cfg(feature = "type-leak")]
pub mod leak;
#[doc(hidden)]
pub mod scope;
mod transformer;

pub use kv::KvArgs;
pub use transformer::signature_only;

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
    pub fn invoke_with_options(
        &self,
        options: Option<&TokenStream>,
        body: TokenStream,
    ) -> TokenStream {
        let opts = options.map(|o| quote! { ( #o ) });
        let in_expr = options.map_or(false, |o| {
            o.clone()
                .into_iter()
                .any(|tt| matches!(tt, proc_macro2::TokenTree::Ident(i) if i == "expr"))
        });
        let mut list = self.clone();
        list.name_dummies();
        match list.0.first() {
            None => quote! { compile_error! { "announce: empty continuation list" } },
            Some(Cont::Attr { .. }) if in_expr => quote! {
                compile_error! { "announce: the derive/attribute route emits items and cannot be used in expression position (`(expr)`)" }
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

/// Type table of an entry announced with `#[announce(repeater = ..)]`, from
/// [`Entry::type_leak`].
///
/// The payload is the original item. Type `types[I]` resolves from any crate as
/// `<() as REPEATER<ID, I, G>>::Type`, where `G` is `generics[I]`.
/// [`apply`](Self::apply) rewrites an item that way.
#[derive(Clone, Debug)]
pub struct TypeLeak {
    /// The repeater trait path as tokens, e.g. `$crate::Repeater`. Tokens, not a
    /// `syn::Path`, because `$crate` is not a path segment for syn.
    pub repeater: TokenStream,
    /// The definition's table id.
    pub id: syn::LitInt,
    /// `generics[i]`: the generic parameters that `types[i]` mentions, in order
    /// of first appearance, as a tuple type. Type parameters appear as
    /// themselves, lifetimes as `&'a ()`, consts as `[(); N]`. `()` if none.
    pub generics: Vec<syn::Type>,
    /// `types[i]`: the original type behind index `i`, as the definer wrote it.
    /// Relative paths in it do not resolve at the caller.
    pub types: Vec<syn::Type>,
}

impl TypeLeak {
    /// The index of `ty` in [`types`](Self::types), if it was interned.
    pub fn lookup(&self, ty: &syn::Type) -> Option<usize> {
        self.types.iter().position(|t| t == ty)
    }

    /// Rewrites every type of `types` in `item` (usually the entry's payload) to
    /// its projection, so the item resolves from any crate. A fn generic
    /// parameter named like a listed type shadows it.
    pub fn apply(&self, item: &mut syn::Item) {
        use syn::visit_mut::VisitMut;
        struct V<'a> {
            tl: &'a TypeLeak,
            shadowed: Vec<syn::Type>,
        }
        impl V<'_> {
            fn shadow(&mut self, g: &syn::Generics, f: impl FnOnce(&mut Self)) {
                let saved = self.shadowed.len();
                for p in &g.params {
                    if let syn::GenericParam::Type(t) = p {
                        let i = &t.ident;
                        self.shadowed.push(syn::parse_quote!(#i));
                    }
                }
                f(self);
                self.shadowed.truncate(saved);
            }
        }
        impl VisitMut for V<'_> {
            fn visit_type_mut(&mut self, ty: &mut syn::Type) {
                if !self.shadowed.contains(ty) {
                    if let Some(i) = self.tl.lookup(ty) {
                        // `projection` returns tokens (the repeater may be `$crate`).
                        let g = self
                            .tl
                            .generics
                            .get(i)
                            .cloned()
                            .unwrap_or_else(|| syn::parse_quote!(()));
                        *ty = syn::Type::Verbatim(self.tl.projection(i, &g));
                        return;
                    }
                }
                syn::visit_mut::visit_type_mut(self, ty)
            }
            fn visit_trait_item_fn_mut(&mut self, f: &mut syn::TraitItemFn) {
                let g = f.sig.generics.clone();
                self.shadow(&g, |v| syn::visit_mut::visit_trait_item_fn_mut(v, f));
            }
            fn visit_impl_item_fn_mut(&mut self, f: &mut syn::ImplItemFn) {
                let g = f.sig.generics.clone();
                self.shadow(&g, |v| syn::visit_mut::visit_impl_item_fn_mut(v, f));
            }
            fn visit_item_fn_mut(&mut self, f: &mut syn::ItemFn) {
                let g = f.sig.generics.clone();
                self.shadow(&g, |v| syn::visit_mut::visit_item_fn_mut(v, f));
            }
        }
        V {
            tl: self,
            shadowed: Vec::new(),
        }
        .visit_item_mut(item);
    }

    /// `<() as REPEATER<ID, index, generics>>::Type`, as tokens to emit.
    pub fn projection(&self, index: usize, generics: &syn::Type) -> TokenStream {
        let (a, id) = (&self.repeater, &self.id);
        quote! { <() as #a<#id, #index, #generics>>::Type }
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
    /// Unknown keys are ignored.
    pub fn type_leak(&self) -> Result<Option<TypeLeak>> {
        let m = match self.find_meta("type_leak") {
            Some(m) => m,
            None => return Ok(None),
        };
        let mut kv = m.kv()?;
        let t = TypeLeak {
            repeater: kv
                .tokens("repeater")
                .ok_or_else(|| syn::Error::new(Span::call_site(), "missing `repeater`"))?,
            id: kv.require("id")?,
            generics: kv.list("generics")?.unwrap_or_default(),
            types: kv.list("types")?.unwrap_or_default(),
        };
        Ok(Some(t))
    }

    /// Whether META lists a check whose last path segment is `name`. Only the
    /// last segment is compared, since the path is as the definer wrote it.
    pub fn has_check(&self, name: &str) -> bool {
        self.meta
            .iter()
            .any(|m| m.path.segments.last().map_or(false, |s| s.ident == name))
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
    /// The global options group `(..)`. [`forward`](Self::forward),
    /// [`fetch`](Self::fetch) and [`gather`](Self::gather) pass it on unchanged.
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
        // A `@__announce_dollar { $ }` prefix is meant for trampolines; skip
        // it if a callback receives it.
        if input.peek(Token![@]) {
            let fork = input.fork();
            let _: Token![@] = fork.parse()?;
            if matches!(fork.parse::<Ident>(), Ok(i) if i == "__announce_dollar") {
                let _g: proc_macro2::Group = fork.parse()?;
                syn::parse::discouraged::Speculative::advance_to(input, &fork);
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
    pub fn fetch(&self, new: impl IntoIterator<Item = Cont>) -> TokenStream {
        let list: ContList = new.into_iter().chain(self.list.0.iter().cloned()).collect();
        let (entries, rest) = (&self.entries, &self.rest);
        list.invoke_with_options(self.options.as_ref(), quote! { #(#entries)* #rest })
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
        // Already fetched once, but no entry came back: not a trampoline
        // (AUDIT 3 #9). The bookkeeping travels in the options group.
        let fetched = self.fetched_paths();
        if let Some(c) = missing
            .iter()
            .find(|c| fetched.contains(&path_key(&c.path())))
        {
            let msg = format!(
                "announce: `{}` was fetched but did not provide a definition; is it an `#[announce]` trampoline?",
                c.to_token_stream()
            );
            return Gather::Fetch(quote! { compile_error! { #msg } });
        }
        let mut options: Vec<TokenStream> = self.option_items();
        let paths = missing.iter().map(|c| c.path());
        options.push(quote! { __announce_fetched(#(#paths),*) });
        let list: ContList = missing
            .into_iter()
            .chain(self.list.0.iter().cloned())
            .collect();
        let (entries, rest) = (&self.entries, &self.rest);
        let opts = quote! { #(#options),* };
        Gather::Fetch(list.invoke_with_options(Some(&opts), quote! { #(#entries)* #rest }))
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
/// a trampoline dummy, returns `target! { payload }`.
///
/// `target` is the absolute path of the `#[announce(macro_name = ..)]` macro.
/// Pick a path that is valid at the user's site.
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
    // ignore it. The derive route cannot re-invoke the trampoline itself, so
    // the prefix is always added here.
    let dollar = proc_macro2::Punct::new('$', proc_macro2::Spacing::Alone);
    Some(quote! { #target ! { @__announce_dollar { #dollar } #body } })
}

/// What a definition-side check receives: `(ARGS), ITEM`.
pub struct CheckInput {
    /// The check's arguments, `(ARGS)`.
    pub args: TokenStream,
    /// The cfg-evaluated item.
    pub item: syn::Item,
}

impl Parse for CheckInput {
    fn parse(input: ParseStream) -> Result<Self> {
        let content;
        syn::parenthesized!(content in input);
        let args = content.parse()?;
        input.parse::<Token![,]>()?;
        Ok(CheckInput {
            args,
            item: input.parse()?,
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
    fn fetch_prepends_and_returns() {
        let input: Input = syn::parse_quote! { [me!, next!] a::F! => [] { struct S; } (rest) };
        let out = s(input.fetch([Cont::bang(syn::parse_quote!(g::G))]));
        assert_eq!(out, "g::G!{[g::G!,me!,next!]a::F!=>[]{structS;}(rest)}");
        assert_eq!(Cont::bang(syn::parse_quote!(a::F)), input.entries[0].key);
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

#[cfg(test)]
mod options_tests {
    use super::*;

    #[test]
    fn options_are_parsed_and_forwarded() {
        let input: Input = syn::parse_quote! {
            (type_leak) [me!, next!]
            a::F! => [type_leak(repeater = $crate::A, id = 7u64, generics = [(), (T,)], types = [Local, Vec<T>])] { struct F<T>(T); }
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
            fetch.starts_with("g::G!{(type_leak)[g::G!,me!,next!]"),
            "{}",
            fetch
        );
        let tl = input.entries[0].type_leak().unwrap().unwrap();
        assert_eq!(tl.types.len(), 2);
        assert_eq!(tl.id.base10_digits(), "7");
        let proj = tl.projection(1, &syn::parse_quote!((u8,)));
        assert_eq!(
            proj.to_string().replace(' ', ""),
            "<()as$crate::A<7u64,1usize,(u8,)>>::Type"
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
        assert!(out.contains("cannotbeusedinexpressionposition"), "{}", out);
        assert!(out.contains("(`(expr)`)"), "{}", out);
        assert!(!out.contains("__AnnouncePayload"), "{}", out);
        // The same with `expr` leading other options. (Only a *leading* `expr` is
        // the position option for the generated trampolines; this helper looks
        // for `expr` anywhere. That difference is AUDIT Part 4 #11, not decided
        // here, so no test pins how a later `expr` is read.)
        let opts = template_quote::quote!(expr, type_leak);
        let out = s(list.invoke_with_options(Some(&opts), TokenStream::new()));
        assert!(out.contains("expressionposition"), "{}", out);
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
            out_s.starts_with("::x::Y!{@__announce_dollar{$}[#[derive(p::D)]__AnnounceT_"),
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

    fn leak() -> TypeLeak {
        TypeLeak {
            repeater: template_quote::quote!(::my::R),
            id: syn::parse_quote!(7u64),
            generics: vec![syn::parse_quote!(()), syn::parse_quote!((T,))],
            types: vec![syn::parse_quote!(Local), syn::parse_quote!(Wrap<T>)],
        }
    }

    #[test]
    fn type_leak_lookup_and_projection() {
        let tl = leak();
        assert_eq!(tl.lookup(&syn::parse_quote!(Local)), Some(0));
        assert_eq!(tl.lookup(&syn::parse_quote!(Wrap<T>)), Some(1));
        assert_eq!(tl.lookup(&syn::parse_quote!(Wrap<U>)), None);
        assert_eq!(tl.lookup(&syn::parse_quote!(T)), None);
        assert_eq!(
            s(tl.projection(1, &syn::parse_quote!((u8,)))),
            "<()as::my::R<7u64,1usize,(u8,)>>::Type"
        );
        assert_eq!(
            s(tl.projection(0, &syn::parse_quote!(()))),
            "<()as::my::R<7u64,0usize,()>>::Type"
        );
    }

    #[test]
    fn type_leak_apply_rewrites_listed_types_only() {
        let tl = leak();
        let mut it: syn::Item = syn::parse_quote! {
            struct S<T> { a: Local, b: Wrap<T>, c: Vec<Local>, d: u8, e: T }
        };
        tl.apply(&mut it);
        let out = s(&it);
        let local = "<()as::my::R<7u64,0usize,()>>::Type";
        let wrap = "<()as::my::R<7u64,1usize,(T,)>>::Type";
        assert!(out.contains(&format!("a:{}", local)), "{}", out);
        assert!(out.contains(&format!("b:{}", wrap)), "{}", out);
        // An unlisted outer type keeps its shape; listed types inside it are rewritten.
        assert!(out.contains(&format!("c:Vec<{}>", local)), "{}", out);
        assert!(out.contains("d:u8") && out.contains("e:T}"), "{}", out);
    }

    #[test]
    fn type_leak_apply_with_missing_generics_uses_unit() {
        let mut tl = leak();
        tl.generics.clear();
        let mut it: syn::Item = syn::parse_quote! { struct S { a: Wrap<T> } };
        tl.apply(&mut it);
        assert!(
            s(&it).contains("<()as::my::R<7u64,1usize,()>>::Type"),
            "{}",
            s(&it)
        );
    }

    #[test]
    fn type_leak_apply_respects_shadowing_generics() {
        let tl = TypeLeak {
            repeater: template_quote::quote!(::my::R),
            id: syn::parse_quote!(7u64),
            generics: vec![syn::parse_quote!(())],
            types: vec![syn::parse_quote!(T)],
        };
        let proj = "<()as::my::R<7u64,0usize,()>>::Type";

        // Free fn: its own `T` shadows the interned `T`.
        let mut it: syn::Item = syn::parse_quote! { fn f<T>(x: T) -> T { let y: T = x; y } };
        tl.apply(&mut it);
        let out = s(&it);
        assert!(!out.contains(proj), "{}", out);
        assert_eq!(out, "fnf<T>(x:T)->T{lety:T=x;y}");

        // Trait fn: only the generic method is shadowed, and only inside it.
        let mut it: syn::Item = syn::parse_quote! {
            trait Tr { fn m<T>(x: T) -> T; fn n(x: T) -> T; }
        };
        tl.apply(&mut it);
        let out = s(&it);
        assert!(out.contains("fnm<T>(x:T)->T;"), "{}", out);
        assert!(
            out.contains(&format!("fnn(x:{})->{};", proj, proj)),
            "{}",
            out
        );

        // Impl fn.
        let mut it: syn::Item = syn::parse_quote! {
            impl Tr for S { fn m<T>(x: T) -> T { x } fn n(x: T) -> T { x } }
        };
        tl.apply(&mut it);
        let out = s(&it);
        assert!(out.contains("fnm<T>(x:T)->T{x}"), "{}", out);
        assert!(
            out.contains(&format!("fnn(x:{})->{}{{x}}", proj, proj)),
            "{}",
            out
        );

        // A lifetime or const parameter of the same text does not shadow a type.
        let mut it: syn::Item = syn::parse_quote! { fn f<'a>(x: T) {} };
        tl.apply(&mut it);
        assert!(s(&it).contains(&format!("x:{}", proj)), "{}", s(&it));

        // The shadow ends with the fn: the next item sees `T` again.
        let mut it: syn::Item = syn::parse_quote! {
            mod m { fn f<T>(x: T) {} struct S { a: T } }
        };
        tl.apply(&mut it);
        let out = s(&it);
        assert!(out.contains("fnf<T>(x:T){}"), "{}", out);
        assert!(out.contains(&format!("structS{{a:{}}}", proj)), "{}", out);
    }
}
