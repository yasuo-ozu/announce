//! Helpers for proc-macro crates that start, receive or redirect
//! [`announce`](https://docs.rs/announce) trampolines.
//!
//! - [`ContList::invoke`] starts a trampoline from a proc macro.
//! - [`Input`] parses what a callback receives
//!   (`[list] ENTRY* REST`), and [`Input::forward`] continues the chain.
//! - [`redirect`] lets an announce-aware derive or attribute forward a trampoline
//!   routed through it to the renamed `#[announce(macro_name = ..)]` macro.
//! - [`transformer!`] / [`Input::forward_transformed`] write transformers: list
//!   elements that rewrite the newest entry (e.g. [`signature_only`]).
//! - [`CheckInput`] parses what a definition-side check receives.
//! - [`check_type`] / [`check_trait`] emit the best-effort `announce::Marker` check.

mod kv;
#[doc(hidden)]
pub mod scope;
#[cfg(feature = "type-leak")]
pub mod leak;
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
    /// `#[derive(path)] Name` or `#[attr(..)] Name`. The name is the dummy
    /// enum that carries the payload; [`ContList::invoke`] fills it in.
    Attr {
        attr: Attribute,
        name: Option<Ident>,
    },
}

impl Cont {
    /// `path!`. Generic arguments are dropped (`Enum<A>` becomes `Enum!`),
    /// since macro paths cannot carry them.
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

    /// The macro path: `path` of `path!`, the derive path of
    /// `#[derive(path)]` (the first one of `#[derive(a, b)]`), or the
    /// attribute path.
    pub fn path(&self) -> Path {
        match self {
            Cont::Bang(p) => p.clone(),
            Cont::Attr { attr, .. } => {
                if attr.path().is_ident("derive") {
                    let paths = attr.parse_args_with(Punctuated::<Path, Token![,]>::parse_terminated);
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
    /// The comparison key: the tokens without the random dummy name of an
    /// attribute continuation (AUDIT #12), and with generic arguments of a
    /// bang path dropped.
    fn eq_key(&self) -> String {
        match self {
            Cont::Bang(_) => self.to_token_stream().to_string(),
            Cont::Attr { attr, .. } => attr.to_token_stream().to_string(),
        }
    }
}

/// Continuations compare by their tokens (spans and the dummy name of an
/// attribute continuation ignored), so a consumer can deduplicate keys.
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
    /// Gives every attribute continuation a random dummy name, so that two
    /// trampolines in one module never collide (DESIGN.md section 5).
    pub fn name_dummies(&mut self) {
        for c in &mut self.0 {
            if let Cont::Attr { name, .. } = c {
                if name.is_none() {
                    *name = Some(gensym("__AnnounceT_"));
                }
            }
        }
    }

    /// Invokes the head with `[self] body`. `body` is `ENTRY* REST`.
    pub fn invoke(&self, body: TokenStream) -> TokenStream {
        self.invoke_with_options(None, body)
    }

    /// Like [`invoke`](Self::invoke), with a global options group, e.g.
    /// `Some(quote!(expr))` for `(expr) [..] ..` (DESIGN.md §4.6).
    pub fn invoke_with_options(&self, options: Option<&TokenStream>, body: TokenStream) -> TokenStream {
        let opts = options.map(|o| quote! { ( #o ) });
        let in_expr = options.is_some_and(|o| {
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

/// One `META` element: a check `path` / `path(args)`, or `mock(path)`.
#[derive(Clone, Debug)]
pub struct Meta {
    /// `path` of `path(args)`.
    pub path: Path,
    /// The tokens inside the parentheses, if any.
    pub args: Option<TokenStream>,
}

impl Meta {
    /// Parses the parenthesised args, e.g. with a `Punctuated<syn::MetaNameValue, Token![,]>`
    /// parser for `key = value` lists: `meta.parse_args_with(Punctuated::parse_terminated)`.
    pub fn parse_args_with<P: syn::parse::Parser>(&self, parser: P) -> Result<P::Output> {
        parser.parse2(self.args.clone().unwrap_or_default())
    }

    /// The args as a typed [`KvArgs`] list (`key = <any tokens>`, flags,
    /// `key = [..]` lists). Absent args give an empty list.
    pub fn kv(&self) -> Result<KvArgs> {
        self.parse_args_with(KvArgs::parse)
    }

    /// `key = value, ..` args as name/value pairs (values must be expressions;
    /// use [`kv`](Self::kv) for types and lists).
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

/// Type-leak information of an entry announced with `#[announce(repeater = ..)]`
/// (see [`Entry::type_leak`]). The payload is the original item; `types[I]`
/// resolves from any crate as `<() as REPEATER<ID, I, GENERICS>>::Type`, where
/// GENERICS is the tuple of the item's generic parameters (`T`, `&'a ()`,
/// `[(); N]`) in declaration order. [`apply`](Self::apply) rewrites an item
/// accordingly.
#[derive(Clone, Debug)]
pub struct TypeLeak {
    /// The repeater trait path as tokens, e.g. `$crate::Repeater` (resolves in the
    /// defining crate). Kept as tokens because `$crate` is not a `syn::Path`
    /// segment outside a macro expansion.
    pub repeater: TokenStream,
    /// The definition's table id.
    pub id: syn::LitInt,
    /// `generics[i]`: the generic parameters that `types[i]` mentions (from
    /// any scope of the item: the item, a method, a generic associated type),
    /// in order of first appearance, encoded as a tuple type (`T`, `&'a ()`,
    /// `[(); N]`). `()` for a type that mentions none.
    pub generics: Vec<syn::Type>,
    /// `types[i]` is the original type behind projection index `i`, as the
    /// definer wrote it (for identification and diagnostics; relative paths
    /// in it do not resolve at the caller).
    pub types: Vec<syn::Type>,
}

impl TypeLeak {
    /// The index of `ty` in [`types`](Self::types), if it was interned.
    pub fn lookup(&self, ty: &syn::Type) -> Option<usize> {
        self.types.iter().position(|t| t == ty)
    }

    /// Rewrites every interned type in `item` (typically the entry's payload)
    /// to its projection `<() as REPEATER<ID, I, generics[I]>>::Type`, so the
    /// item resolves from any crate. Matching follows type-leak: a whole type
    /// equal to `types[I]` is replaced (otherwise its parts are visited), and
    /// inside a fn whose own generic parameter is named like an interned type,
    /// that type is not replaced. The parameters named in `generics[I]` are in
    /// scope wherever `types[I]` occurs, so the projection is valid there.
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
                        let g = self.tl.generics.get(i).cloned().unwrap_or_else(|| syn::parse_quote!(()));
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
        V { tl: self, shadowed: Vec::new() }.visit_item_mut(item);
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
    /// The META elements (checks, `meta =`, `mock(..)`, `type_leak(..)`).
    pub meta: Vec<Meta>,
    /// The raw item tokens.
    pub item: TokenStream,
}

impl Entry {
    /// The payload parsed as an item.
    pub fn item(&self) -> Result<syn::Item> {
        syn::parse2(self.item.clone())
    }

    /// The `mock(path)` META, when the item is an announce-std style copy.
    pub fn mock(&self) -> Option<Path> {
        self.meta
            .iter()
            .find(|m| m.path.is_ident("mock"))
            .and_then(|m| syn::parse2(m.args.clone()?).ok())
    }

    /// The first META element whose last path segment is `name`, e.g. to read
    /// the args of a `#[announce(meta = lib::info(..))]` or of a check.
    pub fn find_meta(&self, name: &str) -> Option<&Meta> {
        self.meta
            .iter()
            .find(|m| m.path.segments.last().is_some_and(|s| s.ident == name))
    }

    /// The `type_leak(repeater = .., id = .., generics = .., types = [..])` META
    /// of an entry announced with `repeater = ..`. Unknown keys are ignored, so
    /// newer announce versions can add information.
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

    /// Whether META lists a check whose last path segment is `name`. The path
    /// is copied as the definer wrote it, so only the last segment is
    /// compared (DESIGN.md section 6).
    pub fn has_check(&self, name: &str) -> bool {
        self.meta
            .iter()
            .any(|m| m.path.segments.last().is_some_and(|s| s.ident == name))
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
    /// The global options group, e.g. `expr` for `(expr) [..]`. Options other
    /// than a leading `expr` are forwarded unchanged by every hop.
    /// Forwarded unchanged by [`forward`](Self::forward), [`fetch`](Self::fetch)
    /// and [`gather`](Self::gather).
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

    /// Fetches more definitions and comes back: invokes `new[0]` with list
    /// `[new.., <this whole list, head included>]` and the same entries and
    /// rest. The callback is therefore invoked again with the new entries in
    /// front of the old ones.
    pub fn fetch(&self, new: impl IntoIterator<Item = Cont>) -> TokenStream {
        let list: ContList = new.into_iter().chain(self.list.0.iter().cloned()).collect();
        let (entries, rest) = (&self.entries, &self.rest);
        list.invoke_with_options(self.options.as_ref(), quote! { #(#entries)* #rest })
    }

    /// Whether the global options group contains `name` (e.g. `"type_leak"`).
    pub fn has_option(&self, name: &str) -> bool {
        self.options.as_ref().is_some_and(|o| {
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
                    let paths: Result<Punctuated<Path, Token![,]>> = m.parse_args_with(Punctuated::parse_terminated);
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
        self.entries.iter().find(|e| path_key(&e.key.path()) == want)
    }

    /// "Give me the definitions of these, then call me again": returns
    /// [`Gather::Ready`] with one entry per requested continuation (in the
    /// requested order) when all are present, and otherwise
    /// [`Gather::Fetch`] with the tokens that fetch the missing ones and
    /// re-invoke this callback.
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
            if let Some(c) = needed.iter().find(|c| path_key(&c.path()) == path_key(&head.path())) {
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
            if self.entry_for(c).is_none() && !missing.iter().any(|m| path_key(&m.path()) == path_key(&c.path())) {
                missing.push(c.clone());
            }
        }
        if missing.is_empty() {
            return Gather::Ready(needed.iter().map(|c| self.entry_for(c).expect("checked above")).collect());
        }
        // Already fetched once, but no entry came back: not a trampoline
        // (AUDIT 3 #9). The bookkeeping travels in the options group.
        let fetched = self.fetched_paths();
        if let Some(c) = missing.iter().find(|c| fetched.contains(&path_key(&c.path()))) {
            let msg = format!(
                "announce: `{}` was fetched but did not provide a definition; is it an `#[announce]` trampoline?",
                c.to_token_stream()
            );
            return Gather::Fetch(quote! { compile_error! { #msg } });
        }
        let mut options: Vec<TokenStream> = self.option_items();
        let paths = missing.iter().map(|c| c.path());
        options.push(quote! { __announce_fetched(#(#paths),*) });
        let list: ContList = missing.into_iter().chain(self.list.0.iter().cloned()).collect();
        let (entries, rest) = (&self.entries, &self.rest);
        let opts = quote! { #(#options),* };
        Gather::Fetch(list.invoke_with_options(Some(&opts), quote! { #(#entries)* #rest }))
    }

    /// Continues the chain: drops the head and invokes the next element with
    /// the same entries and rest. `None` when this was the last element.
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
/// The facade crate still has to re-export the callback (e.g. under
/// `__private`) so that generated code can name it.
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

/// Extracts the trampoline payload from the dummy enum an attribute
/// continuation receives: `enum N { __AnnouncePayload = (stringify!{..}, 0).1 }`.
/// The tokens are taken from the unexpanded `stringify!`, so spans, hygiene
/// and `$crate` survive.
pub fn payload(item: TokenStream) -> Option<TokenStream> {
    let di: syn::DeriveInput = syn::parse2(item).ok()?;
    let syn::Data::Enum(e) = di.data else {
        return None;
    };
    let v = e.variants.first()?;
    if v.ident != "__AnnouncePayload" || e.variants.len() != 1 {
        return None;
    }
    let syn::Expr::Field(f) = &v.discriminant.as_ref()?.1 else {
        return None;
    };
    let syn::Expr::Tuple(t) = &*f.base else {
        return None;
    };
    let syn::Expr::Macro(m) = t.elems.first()? else {
        return None;
    };
    if !m.mac.path.is_ident("stringify") {
        return None;
    }
    Some(m.mac.tokens.clone())
}

/// For an announce-aware derive or attribute: if `item` is a trampoline dummy,
/// returns `target! { payload }`, where `target` is the absolute path of the
/// `#[announce(macro_name = ..)]` macro. Choosing a path that is valid at the
/// user's site is the caller's responsibility (DESIGN.md section 5).
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
    Some(quote! { #target ! { #body } })
}

/// What a definition-side check macro receives: `(ARGS), ITEM`.
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

/// Best-effort check that `ty` has an `#[announce]` definition (`ty: Marker`).
/// `krate` is the consumer's path to the `announce` crate.
pub fn check_type(krate: &Path, ty: &syn::Type) -> TokenStream {
    quote! {
        const _: () = {
            fn __announce_chk<__AnnounceT: ?::core::marker::Sized + #krate::Marker>() {}
            let _ = __announce_chk::<#ty>;
        };
    }
}

/// Best-effort check that the trait at `path` has an `#[announce]` definition
/// (`dyn path: Marker`). Only meaningful for simple dyn-compatible traits.
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
        assert_eq!(ty.to_token_stream().to_string().replace(' ', ""), "::std::collections::HashMap<u8,Vec<u8>>");
        let out: syn::Type = kv.require("out").unwrap();
        assert_eq!(out.to_token_stream().to_string().replace(' ', ""), "fn()->u8");
        let xs: Vec<Path> = kv.list("xs").unwrap().unwrap();
        assert_eq!(xs.len(), 2);
        assert!(kv.flag("on").unwrap());
        assert!(kv.get::<syn::Type>("absent").unwrap().is_none());
        kv.finish().unwrap();

        let mut kv: KvArgs = syn::parse_quote!(a = 1, b = 2);
        let _: syn::LitInt = kv.require("a").unwrap();
        assert!(kv.finish().unwrap_err().to_string().contains("unknown key `b`"));
        assert!(syn::parse2::<KvArgs>(template_quote::quote!(a = 1, a = 2)).is_err());
        let mut kv: KvArgs = syn::parse_quote!();
        assert!(kv.require::<syn::Type>("x").unwrap_err().to_string().contains("missing `x`"));
    }

    #[cfg(feature = "type-leak")]
    #[test]
    fn stable_referrer_is_deterministic() {
        let item: syn::ItemStruct = syn::parse_quote! {
            pub struct S { a: Foo, b: Bar, c: Baz, d: Qux, e: Quux, f: Corge }
        };
        let order = || {
            let mut l = type_leak::Leaker::from_struct(&item).unwrap();
            l.reduce_roots();
            let r = crate::leak::StableReferrer::new(l.finish());
            r.types().iter().map(|t| t.to_token_stream().to_string()).collect::<Vec<_>>()
        };
        let first = order();
        assert_eq!(first.len(), 6);
        let mut sorted = first.clone();
        sorted.sort();
        assert_eq!(first, sorted);
        for _ in 0..20 {
            assert_eq!(order(), first);
        }
        let mut l = type_leak::Leaker::from_struct(&item).unwrap();
        l.reduce_roots();
        let r = crate::leak::StableReferrer::new(l.finish());
        let mut it: syn::Item = syn::Item::Struct(item.clone());
        r.visit_item_mut(&mut it, |_, i| syn::parse_str(&format!("T{}", i)).unwrap());
        let s = it.to_token_stream().to_string();
        assert!(s.contains("a : T3") && s.contains("b : T0") && s.contains("f : T2"), "{}", s);
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
        assert!(fwd.starts_with("next!{(type_leak)[next!]a::F!=>"), "{}", fwd);
        let fetch = input.fetch([Cont::bang(syn::parse_quote!(g::G))]).to_string().replace(' ', "");
        assert!(fetch.starts_with("g::G!{(type_leak)[g::G!,me!,next!]"), "{}", fetch);
        let tl = input.entries[0].type_leak().unwrap().unwrap();
        assert_eq!(tl.types.len(), 2);
        assert_eq!(tl.id.base10_digits(), "7");
        let proj = tl.projection(1, &syn::parse_quote!((u8,)));
        assert_eq!(proj.to_string().replace(' ', ""), "<()as$crate::A<7u64,1usize,(u8,)>>::Type");
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
        assert!(e.to_string().contains("unbalanced `<` in the value of `a`"), "{}", e);
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
        let a: Cont = syn::parse_quote!(#[derive(p::D)] N1);
        let b: Cont = syn::parse_quote!(#[derive(p::D)] N2);
        assert_eq!(a, b);
        let c = Cont::Bang(syn::parse_quote!(a::E<T>));
        assert_eq!(c.to_token_stream().to_string().replace(' ', ""), "a::E!");
        let d: Cont = syn::parse_quote!(#[derive(x::A, y::B)]);
        assert_eq!(d.path().to_token_stream().to_string().replace(' ', ""), "x::A");
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
