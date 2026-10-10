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
    quote! { pub const #name: &[(&str, &str, &str)] = &[ #(#rows),* ]; }.into()
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

/// Definition-side check: rejects tuple structs. `no_tuple! { (ARGS), ITEM }`.
#[proc_macro]
pub fn no_tuple(input: TokenStream) -> TokenStream {
    let ci: announce_helper::CheckInput = match syn::parse(input) {
        Ok(i) => i,
        Err(e) => return e.to_compile_error().into(),
    };
    match &ci.item {
        syn::Item::Struct(s) if matches!(s.fields, syn::Fields::Unnamed(_)) => {
            let msg = format!("this macro does not support tuple structs ({})", ci.args);
            syn::Error::new_spanned(&s.fields, msg)
                .to_compile_error()
                .into()
        }
        _ => TokenStream::new(),
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
    quote! { #[::announce::announce(emit_item = false)] #input }.into()
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
        let paths = syn::punctuated::Punctuated::<syn::Path, syn::Token![,]>::parse_terminated(&content)?;
        Ok(GatherRest { paths: paths.into_iter().collect(), name: input.parse()? })
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
