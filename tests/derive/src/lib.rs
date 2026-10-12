//! Proc macros used by the announce tests: an announce-aware derive, a proc-macro
//! callback, a trampoline starter and a definition-side check.

use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use template_quote::quote;

/// announce-aware derive: redirects trampolines to `::announce_test_defs::conflict::ParseAnnounce!`.
#[proc_macro_derive(Parse)]
pub fn derive_parse(input: TokenStream) -> TokenStream {
    let input: TokenStream2 = input.into();
    if let Some(out) = announce_helper::redirect(
        input,
        &syn::parse_quote!(::announce_test_defs::conflict::ParseAnnounce),
    ) {
        return out.into();
    }
    TokenStream::new()
}

/// announce-aware attribute: same redirect; consumes the dummy.
#[proc_macro_attribute]
pub fn parse_attr(_: TokenStream, input: TokenStream) -> TokenStream {
    let input: TokenStream2 = input.into();
    match announce_helper::redirect(
        input.clone(),
        &syn::parse_quote!(::announce_test_defs::conflict::ParseAnnounce),
    ) {
        Some(out) => out.into(),
        None => input.into(),
    }
}

/// Derive whose trampoline holds its item once (a large trait, `layout = single`
/// route): redirects to `::announce_test_defs::large::BigParseAnnounce!`.
#[proc_macro_derive(BigParse)]
pub fn derive_big_parse(input: TokenStream) -> TokenStream {
    let input: TokenStream2 = input.into();
    match announce_helper::redirect(
        input,
        &syn::parse_quote!(::announce_test_defs::large::BigParseAnnounce),
    ) {
        Some(out) => out.into(),
        None => TokenStream::new(),
    }
}

/// The attribute form of [`derive_big_parse`]; consumes the dummy.
#[proc_macro_attribute]
pub fn big_parse_attr(_: TokenStream, input: TokenStream) -> TokenStream {
    let input: TokenStream2 = input.into();
    match announce_helper::redirect(
        input.clone(),
        &syn::parse_quote!(::announce_test_defs::large::BigParseAnnounce),
    ) {
        Some(out) => out.into(),
        None => input.into(),
    }
}

/// Derive that redirects to a small trampoline held once
/// (`::announce_test_defs::large::SmallSingleParseAnnounce!`).
#[proc_macro_derive(SmallSingleParse)]
pub fn derive_small_single_parse(input: TokenStream) -> TokenStream {
    let input: TokenStream2 = input.into();
    match announce_helper::redirect(
        input,
        &syn::parse_quote!(::announce_test_defs::large::SmallSingleParseAnnounce),
    ) {
        Some(out) => out.into(),
        None => TokenStream::new(),
    }
}

/// Callback: `summary! { [summary!] ENTRY* NAME }` emits
/// `pub const NAME: &[(&str, &str, &str)] = &[(key, meta, item-kind+ident), ..]`
/// in visiting order, and forwards if more continuations remain.
#[proc_macro]
pub fn summary(input: TokenStream) -> TokenStream {
    let input: announce_helper::Input = match syn::parse(input) {
        Ok(i) => i,
        Err(e) => return e.to_compile_error().into(),
    };
    if let Some(fwd) = input.forward() {
        return fwd.into();
    }
    let name: syn::Ident = match syn::parse2(input.rest.clone()) {
        Ok(n) => n,
        Err(e) => return e.to_compile_error().into(),
    };
    let rows = input.entries_in_order().map(|e| {
        let key = e.key.path().segments.last().unwrap().ident.to_string();
        let meta = e
            .meta
            .iter()
            .map(|m| template_quote::ToTokens::to_token_stream(m).to_string())
            .collect::<Vec<_>>()
            .join(", ");
        let item = match e.item() {
            Ok(syn::Item::Struct(s)) => format!("struct {}", s.ident),
            Ok(syn::Item::Enum(s)) => format!("enum {}", s.ident),
            Ok(syn::Item::Union(s)) => format!("union {}", s.ident),
            Ok(syn::Item::Trait(s)) => format!("trait {} ({} items)", s.ident, s.items.len()),
            Ok(syn::Item::Type(s)) => format!("type {}", s.ident),
            Ok(syn::Item::Mod(s)) => format!("mod {}", s.ident),
            Ok(_) => "other".to_string(),
            Err(e) => format!("error: {}", e),
        };
        quote! { (#key, #meta, #item) }
    });
    TokenStream::from(quote! { pub const #name: &[(&str, &str, &str)] = &[ #(#rows),* ]; })
}

/// Starts a trampoline from a proc macro: `start!([a!, #[derive(B)], ..] REST)`.
/// Attribute continuations get random dummy names.
#[proc_macro]
pub fn start(input: TokenStream) -> TokenStream {
    struct Start(announce_helper::ContList, TokenStream2);
    impl syn::parse::Parse for Start {
        fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
            let content;
            syn::bracketed!(content in input);
            Ok(Start(content.parse()?, input.parse()?))
        }
    }
    match syn::parse::<Start>(input) {
        Ok(Start(list, rest)) => list.invoke(rest).into(),
        Err(e) => e.to_compile_error().into(),
    }
}

announce_helper::check! {
    /// Definition-side check: rejects tuple structs. `no_tuple! { (ARGS), ITEM }`.
    /// It owns no attributes, and answers the attribute query all the same.
    pub fn no_tuple(ci) {
        match &ci.item {
            syn::Item::Struct(s) if matches!(s.fields, syn::Fields::Unnamed(_)) => {
                let msg = format!("this macro does not support tuple structs ({})", ci.args);
                Err(syn::Error::new_spanned(&s.fields, msg))
            }
            _ => Ok(TokenStream2::new()),
        }
    }
}

announce_helper::check! {
    /// Owns the attribute `my_lib`, and rejects `#[my_lib(bad)]` anywhere in the item.
    pub fn lib_check(ci) attributes(my_lib) {
        for attr in ci.owned_attributes() {
            if matches!(attr.parse_args::<syn::Ident>(), Ok(i) if i == "bad") {
                return Err(syn::Error::new_spanned(&attr, "`#[my_lib(bad)]` is not allowed"));
            }
        }
        Ok(TokenStream2::new())
    }
}

announce_helper::check! {
    /// Owns the attributes `other` and `tag`, and rejects `#[other(forbidden)]`.
    pub fn other_check(ci) attributes(other, tag) {
        for attr in ci.owned_attributes() {
            if attr.path().is_ident("other") && attr.parse_args::<syn::Ident>().map_or(false, |i| i == "forbidden") {
                return Err(syn::Error::new_spanned(&attr, "`#[other(forbidden)]` is not allowed"));
            }
        }
        Ok(TokenStream2::new())
    }
}

announce_helper::check! {
    /// Owns `my_lib` and records what it saw: `pub const SEEN_<ITEM>: &[&str]`, one
    /// string for each owned attribute (without spaces), in source order.
    pub fn record_attrs(ci) attributes(my_lib) {
        let name = match &ci.item {
            syn::Item::Struct(s) => s.ident.to_string(),
            syn::Item::Enum(s) => s.ident.to_string(),
            syn::Item::Trait(s) => s.ident.to_string(),
            syn::Item::Mod(s) => s.ident.to_string(),
            other => return Err(syn::Error::new_spanned(other, "record_attrs: unsupported item")),
        };
        let seen: Vec<String> = ci
            .owned_attributes()
            .iter()
            .map(|a| template_quote::ToTokens::to_token_stream(&a.meta).to_string().split_whitespace().collect())
            .collect();
        let id = syn::Ident::new(&format!("SEEN_{}", name.to_uppercase()), proc_macro2::Span::call_site());
        Ok(quote! { pub const #id: &[&str] = &[ #(#seen),* ]; })
    }
}

announce_helper::check! {
    /// Owns an attribute that rustc uses: always an error where it is used.
    pub fn doc_owner(_ci) attributes(doc) {
        Ok(TokenStream2::new())
    }
}

/// Declares the attribute `rules` and does nothing else. A `#[derive(Announce)]`
/// cannot remove attributes from its item, so an item that has `#[rules(..)]` in it
/// needs a derive that declares them (the check is `announce_test_defs::rules_check`).
#[proc_macro_derive(Rules, attributes(rules))]
pub fn derive_rules(_: TokenStream) -> TokenStream {
    TokenStream::new()
}

/// A check written the way checks were before the attribute query existed: it
/// parses `CheckInput` and does not answer the query.
#[proc_macro]
pub fn old_style_check(input: TokenStream) -> TokenStream {
    match syn::parse::<announce_helper::CheckInput>(input) {
        Ok(_) => TokenStream::new(),
        Err(e) => e.to_compile_error().into(),
    }
}

/// `check_marker!(path)`: best-effort announce::Announce check for a type.
#[proc_macro]
pub fn check_marker(input: TokenStream) -> TokenStream {
    let ty: syn::Type = match syn::parse(input) {
        Ok(t) => t,
        Err(e) => return e.to_compile_error().into(),
    };
    announce_helper::check_type(&syn::parse_quote!(::announce), &ty).into()
}

/// A derive that announces its own input via `#[announce(emit_item = false)]`.
#[proc_macro_derive(Announced)]
pub fn derive_announced(input: TokenStream) -> TokenStream {
    let input: TokenStream2 = input.into();
    TokenStream::from(quote! { #[::announce::announce(emit_item = false)] #input })
}

announce_helper::transformer! {
    /// Transformer: keeps only `pub` fields in the newest entry's struct and
    /// lists their names in META as `fields(..)`.
    pub fn public_fields(entry) {
        let mut s: syn::ItemStruct = syn::parse2(entry.item.clone())?;
        let mut names = Vec::new();
        if let syn::Fields::Named(f) = &mut s.fields {
            f.named = f
                .named
                .clone()
                .into_iter()
                .filter(|f| matches!(f.vis, syn::Visibility::Public(_)))
                .collect();
            names = f.named.iter().map(|f| f.ident.clone().unwrap()).collect();
        }
        entry.item = quote! { #s };
        entry.meta.push(syn::parse_quote!(fields(#(#names),*)));
        Ok(entry)
    }
}

struct GatherRest {
    paths: Vec<syn::Path>,
    name: syn::Ident,
}
impl syn::parse::Parse for GatherRest {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        let content;
        syn::parenthesized!(content in input);
        let paths =
            syn::punctuated::Punctuated::<syn::Path, syn::Token![,]>::parse_terminated(&content)?;
        Ok(GatherRest {
            paths: paths.into_iter().collect(),
            name: input.parse()?,
        })
    }
}

announce_helper::callback! {
    /// `gathered! { [gathered!] (a::A, b::B) NAME }` fetches the listed
    /// definitions (in one go) and emits `NAME: &[&str]` with their item
    /// idents in the requested order.
    pub fn gathered(input) {
        let rest: GatherRest = input.rest_as()?;
        let needed = rest.paths.iter().cloned().map(announce_helper::Cont::bang);
        let entries = match input.gather(needed) {
            announce_helper::Gather::Ready(e) => e,
            announce_helper::Gather::Fetch(ts) => return Ok(ts),
        };
        let mut idents = Vec::new();
        for e in entries {
            let ident = match e.item()? {
                syn::Item::Struct(s) => s.ident,
                syn::Item::Enum(s) => s.ident,
                syn::Item::Trait(s) => s.ident,
                syn::Item::Type(s) => s.ident,
                other => return Err(syn::Error::new_spanned(other, "unexpected item")),
            };
            idents.push(ident.to_string());
        }
        let name = &rest.name;
        Ok(quote! { pub const #name: &[&str] = &[ #(#idents),* ]; })
    }
}

announce_helper::callback! {
    /// Records, for every entry in visiting order, `pub const <REST>: &[(&str, usize, usize, String)]`
    /// is not possible in a const, so the rows are strings: `"names|in item|in stripped"`:
    /// the owned attribute names of the entry (`Entry::owned_attrs`, comma separated), the
    /// number of owned attributes in `Entry::item` and in `Entry::item_without_owned_attrs`.
    pub fn owned_report(input) {
        let name: syn::Ident = input.rest_as()?;
        let mut rows = Vec::new();
        for e in input.entries_in_order() {
            let owned = e.owned_attrs();
            let names: Vec<String> = owned.names().iter().map(|n| n.to_string()).collect();
            let item = e.item()?;
            let stripped = e.item_without_owned_attrs()?;
            rows.push(format!("{}|{}|{}", names.join(","), owned.find(&item).len(), owned.find(&stripped).len()));
        }
        Ok(quote! { pub const #name: &[&str] = &[ #(#rows),* ]; })
    }
}

announce_helper::callback! {
    /// Reports what a callback receives next to the entries, for the tests that
    /// REST is the caller's tokens and nothing else. Emits, with fixed names:
    /// `REST: &str` (`input.rest`, printed), `ENTRIES: usize`, `OWNED: &[&str]` (the
    /// names of `Entry::owned_attrs`, comma separated, one row for each entry in
    /// visiting order) and `META: &[&str]` (the META of each entry). It forwards
    /// when more continuations remain.
    pub fn rest_probe(input) {
        if let Some(fwd) = input.forward() {
            return Ok(fwd);
        }
        let rest = input.rest.to_string();
        let entries = input.entries.len();
        let mut owned = Vec::new();
        let mut metas = Vec::new();
        for e in input.entries_in_order() {
            let names: Vec<String> = e.owned_attrs().names().iter().map(|n| n.to_string()).collect();
            owned.push(names.join(","));
            metas.push(e.meta.iter().map(|m| template_quote::ToTokens::to_token_stream(m).to_string()).collect::<Vec<_>>().join(", "));
        }
        Ok(quote! {
            pub const REST: &str = #rest;
            pub const ENTRIES: usize = #entries;
            pub const OWNED: &[&str] = &[ #(#owned),* ];
            pub const META: &[&str] = &[ #(#metas),* ];
        })
    }
}

announce_helper::callback! {
    /// Re-emits every entry's item, rewritten with its type-leak information
    /// (`TypeLeak::apply`) when the definition has an repeater, and records the
    /// META as `pub const <REST>: &[&str]`.
    pub fn apply_leak(input) {
        let name: syn::Ident = input.rest_as()?;
        let mut out = proc_macro2::TokenStream::new();
        let mut metas = Vec::new();
        for e in input.entries_in_order() {
            let mut item = e.item()?;
            if let Some(tl) = e.type_leak()? {
                tl.apply(&mut item);
            }
            metas.push(e.meta.iter().map(|m| template_quote::ToTokens::to_token_stream(m).to_string()).collect::<Vec<_>>().join(", "));
            out.extend(quote! { #item });
        }
        Ok(quote! { #out pub const #name: &[&str] = &[ #(#metas),* ]; })
    }
}

announce_helper::callback! {
    /// Reads the `type_leak(..)` META through `Entry::type_leak` and records,
    /// for every entry in visiting order, `pub const <REST>: &[&str]` with
    /// `"-"` when the entry has no table, else `"[Type@Args,..]"`: each
    /// replaced type (type-leak's required entries) and the first argument of
    /// its repeater at the scope of the item (types and arguments without
    /// spaces, sorted by text, so the order of the table does not matter).
    pub fn leak_types(input) {
        let name: syn::Ident = input.rest_as()?;
        let squash = |t: &dyn template_quote::ToTokens| {
            template_quote::ToTokens::to_token_stream(t).to_string().split_whitespace().collect::<String>()
        };
        let mut rows = Vec::new();
        for e in input.entries_in_order() {
            rows.push(match e.type_leak()? {
                None => "-".to_string(),
                Some(tl) => {
                    let mut pairs = Vec::new();
                    for (ix, ty) in tl.types() {
                        let args = tl.entry_args(ix, &announce_helper::type_leak::UseSite::identity())?;
                        pairs.push(format!("{}@{}", squash(ty), squash(&args)));
                    }
                    pairs.sort();
                    format!("[{}]", pairs.join(","))
                }
            });
        }
        Ok(quote! { pub const #name: &[&str] = &[ #(#rows),* ]; })
    }
}
