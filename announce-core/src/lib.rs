//! The implementation behind `#[announce]` and `#[derive(Announce)]`, as plain
//! functions. Do not depend on this crate: go through `announce::machinery`
//! (of [`announce`](https://docs.rs/announce)), which re-exports its public API.
//!
//! A proc-macro crate cannot export functions, so the code behind the macros
//! lives here, in an ordinary library. `announce-macros` is a thin wrapper
//! around it. The crate name and layout are not a stable interface.
//!
//! The API is [`announce()`], [`derive_announce()`] and [`Options`]. See their
//! docs, and `announce::machinery` for how a library uses them.

mod cratepath;
mod generate;
mod leak;
mod options;

use proc_macro2::{Ident, Span, TokenStream};
use std::sync::atomic::{AtomicU64, Ordering};
use template_quote::quote;

pub use generate::SINGLE_COPY_THRESHOLD;
pub use options::{Check, Layout, Options};

fn random_u64() -> u64 {
    use std::hash::{BuildHasher, Hasher};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let mut h = std::collections::hash_map::RandomState::new().build_hasher();
    h.write_u64(COUNTER.fetch_add(1, Ordering::Relaxed));
    h.finish()
}

/// Does what `#[announce(attr)]` does to `item`.
///
/// `attr` holds the tokens inside the parentheses of the attribute
/// (`macro_name = Name, my_lib::check`), and `item` the item the attribute is
/// on. The result replaces the item. It keeps the item as written (unless
/// `emit_item = false` or `mock`) and adds the code that emits the
/// trampoline `macro_rules!` and the other generated items.
///
/// A returned error is a syntax or type-context error of the same kind as
/// `#[announce]` reports. Here it comes back as a `compile_error!` invocation
/// in the returned tokens, so a proc macro can return the result as it is.
/// Use [`try_announce`] to get the [`syn::Error`] instead.
///
/// The generated code names the `announce` crate. See [`Options::krate()`] and
/// the `crate = ..` option.
pub fn announce(attr: TokenStream, item: TokenStream) -> TokenStream {
    try_announce(attr, item).unwrap_or_else(|e| e.to_compile_error())
}

/// [`announce()`] with the error returned as a [`syn::Error`].
pub fn try_announce(attr: TokenStream, item: TokenStream) -> syn::Result<TokenStream> {
    let opts: Options = syn::parse2(attr.clone())?;
    let parsed: syn::Item = syn::parse2(item.clone())?;
    generate::ident_vis(&parsed)?;
    let krate = opts.krate();
    let random = random_u64();
    let cap = Ident::new(
        &format!("__AnnounceCap_{}", generate::hex(random)),
        Span::call_site(),
    );
    let random_lit = syn::LitInt::new(&format!("{}u64", random), Span::call_site());
    let emit_item = opts.mock.is_none() && opts.emit_item;
    // DESIGN.md section 4.2: the derive sees the copy with every nested cfg
    // evaluated in this crate, then `remove` deletes the dummy.
    Ok(quote! {
        #(if emit_item) { #item }
        #[derive(#krate::__private::Capture)]
        #[__announce(#random_lit; #attr)]
        #[#krate::__private::remove]
        #[allow(dead_code, non_camel_case_types)]
        enum #cap { V = { #item 0 } }
    })
}

/// Does what `define_repeater!($vis $ident)` does: the definition of the
/// repeater trait, as type-leak writes it (`type_leak::repeater_definition`).
///
/// ```
/// let out = announce_core::define_repeater("pub Repeater".parse().unwrap());
/// let out = out.to_string();
/// assert!(out.contains("pub trait Repeater"), "{}", out);
/// ```
///
/// `$vis` may be empty (a private trait), `pub`, `pub(crate)`, .. The output
/// is the trait, whose const parameters with defaults need Rust 1.59 in the
/// crate that expands it. An error comes back as a `compile_error!`
/// invocation; see [`try_define_repeater`] for a [`syn::Error`].
pub fn define_repeater(input: TokenStream) -> TokenStream {
    try_define_repeater(input).unwrap_or_else(|e| e.to_compile_error())
}

/// [`define_repeater()`] with the error returned as a [`syn::Error`].
pub fn try_define_repeater(input: TokenStream) -> syn::Result<TokenStream> {
    use syn::parse::Parser;
    const USAGE: &str = "announce: expected `define_repeater!(<visibility> <Name>)`, \
                         e.g. `define_repeater!(pub Repeater)`";
    let (vis, ident) = (|input: syn::parse::ParseStream| {
        let vis: syn::Visibility = input.parse()?;
        let ident: Ident = input
            .parse()
            .map_err(|e: syn::Error| syn::Error::new(e.span(), USAGE))?;
        if !input.is_empty() {
            return Err(input.error(USAGE));
        }
        Ok((vis, ident))
    })
    .parse2(input)?;
    let mut definition = type_leak::repeater_definition(&vis, &ident);
    definition.attrs.push(syn::parse_quote! {
        #[doc = "The repeater of `#[announce(repeater = ..)]`: it carries the types of \
                 announced items, which do not resolve in other crates, to the macros \
                 that copy them. Defined by `announce::define_repeater!`; do not \
                 implement it or write its paths yourself."]
    });
    Ok(quote! { #definition })
}

/// Does what `#[derive(Announce)]` does to its input.
///
/// `input` is the struct, enum or union, including its attributes. Options come
/// from its `#[announce(..)]` helper attribute, which is removed from the
/// announced copy; `mock` and `emit_item` are errors. The result is what the
/// derive emits: the trampoline and the `Announce` impl. The item itself is not
/// part of it, because a derive does not emit its input.
///
/// A library whose own derive macro calls this must list `announce` among the
/// derive's helper attributes (`#[proc_macro_derive(Mine, attributes(announce))]`)
/// if its users are to write options, or use [`derive_announce_with`].
///
/// Errors come back as a `compile_error!` invocation; see [`announce()`] and
/// [`try_derive_announce`].
pub fn derive_announce(input: TokenStream) -> TokenStream {
    try_derive_announce(input).unwrap_or_else(|e| e.to_compile_error())
}

/// [`derive_announce()`] with the error returned as a [`syn::Error`].
pub fn try_derive_announce(input: TokenStream) -> syn::Result<TokenStream> {
    let mut item: syn::Item = syn::parse2(input)?;
    let attrs = derive_attrs(&mut item)?;
    let mut opts = Options::default();
    let mut seen = false;
    for a in attrs.iter().filter(|a| a.path().is_ident("announce")) {
        if seen {
            return Err(syn::Error::new_spanned(
                a,
                "announce: only one `#[announce(..)]` helper attribute is allowed",
            ));
        }
        seen = true;
        if let syn::Meta::List(l) = &a.meta {
            opts = syn::parse2(l.tokens.clone())?;
        }
    }
    for key in ["mock", "emit_item"] {
        if opts.given.iter().any(|g| g == key) {
            return Err(derive_option_error(key));
        }
    }
    finish_derive(item, opts)
}

/// Like [`derive_announce()`], with the options given in code instead of a
/// helper attribute. Any `#[announce(..)]` attribute on the input is removed
/// and ignored.
///
/// This is for a derive macro of your own that decides the options itself, for
/// example `crate` (see [`Options::krate()`]). `opts.mock` must be `None`;
/// `emit_item` is ignored, because the derive input is already emitted by the
/// user.
pub fn derive_announce_with(opts: Options, input: TokenStream) -> syn::Result<TokenStream> {
    let mut item: syn::Item = syn::parse2(input)?;
    derive_attrs(&mut item)?;
    if opts.mock.is_some() {
        return Err(derive_option_error("mock"));
    }
    finish_derive(item, opts)
}

/// The attributes of a struct, enum or union; error for any other item.
fn derive_attrs(item: &mut syn::Item) -> syn::Result<&mut Vec<syn::Attribute>> {
    match item {
        syn::Item::Struct(s) => Ok(&mut s.attrs),
        syn::Item::Enum(e) => Ok(&mut e.attrs),
        syn::Item::Union(u) => Ok(&mut u.attrs),
        other => Err(syn::Error::new_spanned(
            other,
            "`#[derive(Announce)]` supports struct, enum and union items",
        )),
    }
}

fn derive_option_error(key: &str) -> syn::Error {
    syn::Error::new(
        Span::call_site(),
        format!(
            "`{}` does not apply to `#[derive(Announce)]`: the item is emitted as written",
            key
        ),
    )
}

fn finish_derive(mut item: syn::Item, mut opts: Options) -> syn::Result<TokenStream> {
    derive_attrs(&mut item)?.retain(|a| !a.path().is_ident("announce"));
    // The derive input is already cfg-evaluated by rustc.
    opts.emit_item = false;
    opts.derived = true;
    generate::generate(item, &opts, random_u64())
}

/// The body of the hidden `Capture` derive that `#[announce]` expands to.
/// Not part of the API.
#[doc(hidden)]
pub fn capture(input: TokenStream) -> syn::Result<TokenStream> {
    let di: syn::DeriveInput = syn::parse2(input)?;
    let attr = di
        .attrs
        .iter()
        .find(|a| a.path().is_ident("__announce"))
        .ok_or_else(|| {
            syn::Error::new_spanned(
                &di.ident,
                "announce: missing #[__announce] on capture dummy",
            )
        })?;
    let args: options::CaptureArgs = attr.parse_args()?;
    let expr = match &di.data {
        syn::Data::Enum(e) => e
            .variants
            .first()
            .and_then(|v| v.discriminant.as_ref())
            .map(|d| &d.1),
        _ => None,
    };
    let stmts = match expr {
        Some(syn::Expr::Block(b)) => &b.block.stmts,
        _ => {
            return Err(syn::Error::new_spanned(
                &di.ident,
                "announce: malformed capture dummy",
            ))
        }
    };
    // The copy is gone when the item itself was `#[cfg]`'d out.
    let item = stmts.iter().find_map(|s| match s {
        syn::Stmt::Item(i) => Some(i.clone()),
        _ => None,
    });
    match item {
        Some(item) => generate::generate(item, &args.opts, args.random),
        None => Ok(TokenStream::new()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use template_quote::ToTokens;

    fn norm(ts: &TokenStream) -> String {
        ts.to_string().split_whitespace().collect()
    }

    fn ts(s: &str) -> TokenStream {
        s.parse().unwrap()
    }

    #[test]
    fn options_round_trip() {
        let src = "crate = ::l::announce, macro_name = N, mock = a::B, repeater = crate::R, \
                   allow_primitive = false, allow_std_prelude = true, emit_item = false, \
                   layout = single, meta = m::info(1, 2), meta = m::bare, c::check(x), c::bare";
        let opts: Options = syn::parse2(ts(src)).unwrap();
        let printed = opts.to_token_stream();
        let again: Options = syn::parse2(printed.clone()).unwrap();
        assert_eq!(norm(&printed), norm(&again.to_token_stream()));
        assert_eq!(
            norm(&printed),
            "crate=::l::announce,macro_name=N,mock=a::B,repeater=crate::R,\
             allow_primitive=false,allow_std_prelude=true,emit_item=false,layout=single,\
             meta=m::info(1,2),meta=m::bare,c::check(x),c::bare"
        );
        for (src, want) in [("layout = auto", ""), ("layout = direct", "layout=direct")] {
            let o: Options = syn::parse2(ts(src)).unwrap();
            assert_eq!(norm(&o.to_token_stream()), want);
        }
        let e = syn::parse2::<Options>(ts("layout = fast")).err().unwrap();
        assert!(
            e.to_string().contains("`auto`, `single` or `direct`"),
            "{}",
            e
        );
    }

    #[test]
    fn define_repeater_is_type_leaks_definition() {
        // The tokens are `type_leak::repeater_definition()`'s, with the docs on top.
        for (input, vis) in [
            ("pub Repeater", "pub"),
            ("pub(crate) Repeater", "pub(crate)"),
            ("Repeater", ""),
        ] {
            let out = try_define_repeater(ts(input)).unwrap();
            let def = type_leak::repeater_definition(
                &syn::parse_str::<syn::Visibility>(vis).unwrap(),
                &Ident::new("Repeater", Span::call_site()),
            );
            let expected = norm(&def.to_token_stream());
            let got = norm(&out);
            assert!(got.ends_with(&expected), "{}\n{}", got, expected);
            assert!(got.starts_with("#[doc="), "{}", got);
        }
    }

    #[test]
    fn define_repeater_reports_a_bad_input() {
        for bad in ["", "pub", "pub Repeater extra", "pub 1", "Rep, pub"] {
            let e = try_define_repeater(ts(bad)).err();
            assert!(e.is_some(), "{:?} was accepted", bad);
        }
        let out = norm(&define_repeater(ts("pub")));
        assert!(
            out.starts_with("::core::compile_error!") && out.contains("expected`define_repeater!(<visibility><Name>)`"),
            "{}",
            out
        );
    }

    #[test]
    fn default_options_print_nothing() {
        assert!(Options::default().to_token_stream().is_empty());
    }

    #[test]
    fn announce_names_the_given_crate() {
        let out = announce(
            ts("crate = ::l::announce"),
            ts("pub struct S { pub a: u8 }"),
        );
        let out = norm(&out);
        assert!(out.contains("pubstructS{puba:u8}"), "{}", out);
        assert!(
            out.contains("derive(::l::announce::__private::Capture)"),
            "{}",
            out
        );
        assert!(
            out.contains("#[::l::announce::__private::remove]"),
            "{}",
            out
        );
    }

    #[test]
    fn announce_through_options_equals_the_attribute() {
        let mut opts: Options = syn::parse2(ts("macro_name = N")).unwrap();
        opts.krate = Some(syn::parse_quote!(::l::announce));
        let item = ts("pub struct S;");
        let a = norm(&announce(opts.to_token_stream(), item.clone()));
        let b = norm(&announce(ts("crate = ::l::announce, macro_name = N"), item));
        // Only the random numbers differ; the attribute arguments are the same.
        let args = |s: &str| {
            s.split("u64;")
                .nth(1)
                .unwrap()
                .split(")]")
                .next()
                .unwrap()
                .to_string()
        };
        assert_eq!(args(&a), "crate=::l::announce,macro_name=N");
        assert_eq!(args(&a), args(&b));
    }

    #[test]
    fn errors_are_compile_error_tokens_or_syn_errors() {
        let bad = ts("fn f() {}");
        let out = announce(TokenStream::new(), bad.clone());
        assert!(out.to_string().contains("compile_error"), "{}", out);
        let e = try_announce(TokenStream::new(), bad).err().unwrap();
        assert!(e.to_string().contains("supports struct, enum"), "{}", e);
        let e = try_announce(ts("nope = 1"), ts("struct S;")).err().unwrap();
        assert!(e.to_string().contains("unknown announce option"), "{}", e);
    }

    #[test]
    fn derive_announce_uses_the_helper_attribute() {
        let out = derive_announce(ts(
            "#[announce(crate = ::l::announce, macro_name = N)] pub struct S(pub u8);",
        ));
        let out = norm(&out);
        assert!(out.contains("macro_rules!"), "{}", out);
        assert!(out.contains("::l::announce::Announce"), "{}", out);
        // The helper attribute is not part of the announced copy.
        assert!(!out.contains("#[announce"), "{}", out);
    }

    #[test]
    fn layout_is_an_option_of_the_attribute_and_of_the_derive() {
        let item = "pub struct S { pub a: u8 }";
        let derive = |attr: &str| {
            norm(&derive_announce(ts(&format!(
                "#[announce({})] {}",
                attr, item
            ))))
        };
        assert!(!derive("").contains("__announce_emit"));
        assert!(!derive("layout = direct").contains("__announce_emit"));
        assert!(derive("layout = single").contains("__announce_emit"));
        // The attribute hands the option to the capture derive.
        let out = norm(&announce(ts("layout = single"), ts(item)));
        assert!(out.contains("#[__announce("), "{}", out);
        assert!(out.contains("layout=single"), "{}", out);
        // From code.
        let opts = Options {
            layout: Layout::Single,
            ..Options::default()
        };
        let out = derive_announce_with(opts, ts(item)).unwrap();
        assert!(norm(&out).contains("__announce_emit"));
    }

    #[test]
    fn derive_rejects_mock_and_emit_item() {
        for src in [
            "#[announce(mock = a::B)] struct S;",
            "#[announce(emit_item = true)] struct S;",
        ] {
            let e = try_derive_announce(ts(src)).err().unwrap();
            assert!(e.to_string().contains("does not apply"), "{}", e);
        }
        let opts = Options {
            mock: Some(syn::parse_quote!(a::B)),
            ..Options::default()
        };
        let e = derive_announce_with(opts, ts("struct S;")).err().unwrap();
        assert!(e.to_string().contains("`mock` does not apply"), "{}", e);
        let e = try_derive_announce(ts("fn f() {}")).err().unwrap();
        assert!(
            e.to_string().contains("supports struct, enum and union"),
            "{}",
            e
        );
    }

    #[test]
    fn derive_with_options_ignores_the_helper_attribute() {
        let opts = Options {
            krate: Some(syn::parse_quote!(::l::announce)),
            ..Options::default()
        };
        let out =
            derive_announce_with(opts, ts("#[announce(macro_name = Other)] struct S;")).unwrap();
        let out = norm(&out);
        assert!(out.contains("::l::announce::Announce"), "{}", out);
        assert!(!out.contains("Other"), "{}", out);
    }
}
