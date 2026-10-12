//! A stand-in for the proc-macro crate of a library that builds or rewrites
//! items and announces them by calling `announce::machinery`, instead of
//! emitting `#[announce]`.
//!
//! The library's users do not depend on `announce`; the library re-exports it
//! (here `announce_test_machinery_lib::announce`), so the macros pass
//! `crate = ::announce_test_machinery_lib::announce`.

use announce::machinery::{self, Options};
use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use template_quote::{quote, ToTokens};

fn facade() -> syn::Path {
    syn::parse_quote!(::announce_test_machinery_lib::announce)
}

/// `#[tracked(OPTIONS)]` on a struct with named fields: adds a field
/// `pub tracked: bool` and announces the result. `OPTIONS` are announce options.
#[proc_macro_attribute]
pub fn tracked(attr: TokenStream, item: TokenStream) -> TokenStream {
    let mut opts: Options = match syn::parse(attr) {
        Ok(o) => o,
        Err(e) => return e.to_compile_error().into(),
    };
    opts.krate.get_or_insert_with(facade);
    let mut s: syn::ItemStruct = match syn::parse(item) {
        Ok(s) => s,
        Err(e) => return e.to_compile_error().into(),
    };
    if let syn::Fields::Named(f) = &mut s.fields {
        f.named.push(syn::parse_quote!(pub tracked: bool));
    }
    machinery::announce(opts.to_token_stream(), quote! { #s }).into()
}

/// `pair!(Left, Right)`: generates two structs and announces both. The
/// attribute is written as tokens.
#[proc_macro]
pub fn pair(input: TokenStream) -> TokenStream {
    struct Names(syn::Ident, syn::Ident);
    impl syn::parse::Parse for Names {
        fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
            let l = input.parse()?;
            input.parse::<syn::Token![,]>()?;
            Ok(Names(l, input.parse()?))
        }
    }
    let Names(l, r) = match syn::parse(input) {
        Ok(n) => n,
        Err(e) => return e.to_compile_error().into(),
    };
    let krate = facade();
    let attr = |n: &syn::Ident| quote! { crate = #krate, meta = test_lib::pair(#n) };
    let mut out = TokenStream2::new();
    out.extend(machinery::announce(
        attr(&l),
        quote! { pub struct #l { pub value: u8 } },
    ));
    out.extend(machinery::announce(
        attr(&r),
        quote! { pub struct #r { pub twin: u8 } },
    ));
    out.into()
}

/// `#[derive(Tracked)]` with `#[tracked(OPTIONS)]`: announces the struct, enum or
/// union, with options built in code (`derive_announce_with`).
#[proc_macro_derive(Tracked, attributes(tracked))]
pub fn derive_tracked(input: TokenStream) -> TokenStream {
    let mut item: syn::DeriveInput = match syn::parse(input) {
        Ok(i) => i,
        Err(e) => return e.to_compile_error().into(),
    };
    let mut opts = Options::default();
    for a in item.attrs.iter().filter(|a| a.path().is_ident("tracked")) {
        if let syn::Meta::List(l) = &a.meta {
            opts = match syn::parse2(l.tokens.clone()) {
                Ok(o) => o,
                Err(e) => return e.to_compile_error().into(),
            };
        }
    }
    item.attrs.retain(|a| !a.path().is_ident("tracked"));
    opts.krate.get_or_insert_with(facade);
    match machinery::derive_announce_with(opts, quote! { #item }) {
        Ok(ts) => ts.into(),
        Err(e) => e.to_compile_error().into(),
    }
}

/// `#[derive(Plain)]` with the `#[announce(..)]` helper attribute of
/// `#[derive(Announce)]` (`derive_announce`). The options must hold `crate = ..`,
/// as the library's users do not depend on `announce`.
#[proc_macro_derive(Plain, attributes(announce))]
pub fn derive_plain(input: TokenStream) -> TokenStream {
    machinery::derive_announce(input.into()).into()
}
