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

/// A check of the library that owns the attribute `field_attr` and rejects
/// `#[field_attr(bad)]`. It is used by `#[tracked(..)]` and by `#[announce(..)]`: the
/// machinery sends the same query as `#[announce]`.
#[proc_macro]
pub fn owner(input: TokenStream) -> TokenStream {
    announce::helper::check(input.into(), &["field_attr"], |ci| {
        for attr in ci.owned_attributes() {
            if matches!(attr.parse_args::<syn::Ident>(), Ok(i) if i == "bad") {
                return Err(syn::Error::new_spanned(
                    &attr,
                    "`#[field_attr(bad)]` is not allowed",
                ));
            }
        }
        Ok(TokenStream2::new())
    })
    .into()
}
