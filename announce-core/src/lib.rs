//! The implementation behind `#[announce]` and `#[derive(Announce)]`, as plain
//! functions, and the helpers for callbacks and checks. Do not depend on this
//! crate: go through [`announce`](https://docs.rs/announce), which re-exports
//! the public API as `announce::machinery` and `announce::helper`.
//!
//! A proc-macro crate cannot export functions, so the code behind the macros
//! lives here, in an ordinary library. `announce-macros` is a thin wrapper
//! around it. The crate name and layout are not a stable interface.
//!
//! The API of the generation is [`announce()`], [`derive_announce()`] and
//! [`Options`] (`announce::machinery`). See their docs, and
//! `announce::machinery` for how a library uses them. [`helper`] is what
//! callbacks and checks use (`announce::helper`): the parser of the tokens a
//! trampoline sends, and the macros [`callback!`] and [`transformer!`].

mod cratepath;
mod fastpath;
mod finish;
mod generate;
pub mod helper;
mod leak;
mod options;
mod scan;
mod timing;

use crate::helper::OwnedAttrs;
use proc_macro2::{Ident, Span, TokenStream};
use std::sync::atomic::{AtomicU64, Ordering};
use template_quote::{quote, ToTokens};

pub use finish::{finish, try_finish};
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
/// trampoline `macro_rules!` and the other generated items. The item that the
/// trampoline holds, and the checks get, has its `#[doc = ..]` attributes
/// removed unless `docs = true`; the emitted item keeps them.
///
/// With checks (`my_lib::check`) the result is not the item yet: it is the
/// call of the first check, with the query that asks every check which
/// attributes it owns. The last element of the query is the hidden macro
/// `finish!` of the `announce` crate (`<crate path>::__private::finish!`, see
/// [`finish()`]), which expands to what is described above, without the
/// attributes the checks own. See [`helper::attrs`] for the protocol.
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
///
/// The item is not parsed here: only its kind and name are read (`scan`), to
/// refuse what is not supported at once. Its syntax is validated by the parse of
/// the `Capture` derive that the result contains.
pub fn try_announce(attr: TokenStream, item: TokenStream) -> syn::Result<TokenStream> {
    let mut t = timing::Timer::new("attribute");
    let opts: Options = syn::parse2(attr.clone())?;
    t.step("options");
    let head = scan::head(&item)?;
    t.subject(|| format!("{:?} {}", head.kind, head.ident));
    t.step("scan");
    let random = random_u64();
    let out = if opts.checks.is_empty() {
        capture_output(&opts, &attr, item, random, &OwnedAttrs::new())
    } else {
        finish::query(&opts, finish::Mode::Attribute, &attr, &item, random)
    };
    t.step("emit");
    Ok(out)
}

/// What `#[announce]` expands to once the attribute names that its checks own
/// are known (`owned`; empty without checks): the item and the capture dummy.
///
/// The derive sees the copy with every nested cfg
/// evaluated in this crate, then `remove` deletes the dummy. Without owned
/// attributes the item is emitted here, as written. With owned attributes it
/// is the derive that emits it, from its cfg-evaluated copy and without them.
pub(crate) fn capture_output(
    opts: &Options,
    attr: &TokenStream,
    item: TokenStream,
    random: u64,
    owned: &OwnedAttrs,
) -> TokenStream {
    let krate = opts.krate();
    let cap = Ident::new(
        &format!("__AnnounceCap_{}", generate::hex(random)),
        Span::call_site(),
    );
    let random_lit = syn::LitInt::new(&format!("{}u64", random), Span::call_site());
    let emit_item = opts.mock.is_none() && opts.emit_item && owned.is_empty();
    let names = owned.names();
    let owned_arg = (!owned.is_empty()).then(|| quote! { [ #(#names),* ] ; });
    quote! {
        #(if emit_item) { #item }
        #[derive(#krate::__private::Capture)]
        #[__announce(#random_lit; #owned_arg #attr)]
        #[#krate::__private::remove]
        #[allow(dead_code, non_camel_case_types)]
        enum #cap { V = { #item 0 } }
    }
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
    let mut t = timing::Timer::new("derive");
    let mut item: syn::Item = syn::parse2(input)?;
    t.step("parse");
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
    t.step("options");
    finish_derive(item, opts, t)
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
    finish_derive(item, opts, timing::Timer::new("derive"))
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

fn finish_derive(
    mut item: syn::Item,
    mut opts: Options,
    mut t: timing::Timer,
) -> syn::Result<TokenStream> {
    derive_attrs(&mut item)?.retain(|a| !a.path().is_ident("announce"));
    // The derive input is already cfg-evaluated by rustc.
    opts.emit_item = false;
    opts.derived = true;
    if opts.checks.is_empty() {
        t.subject(|| {
            generate::ident_vis(&item).map_or_else(|_| String::new(), |(i, _)| i.to_string())
        });
        return generate::generate_timed(item, &opts, random_u64(), &OwnedAttrs::new(), &mut t);
    }
    // The same query as `#[announce]` sends. A derive cannot remove the
    // attributes from its input, so a library that owns attributes declares them
    // on its own derive (`attributes(..)`); the query only lists them in META.
    let (attr, item) = (opts.to_token_stream(), item.to_token_stream());
    let random = random_u64();
    Ok(finish::query(
        &opts,
        finish::Mode::Derive,
        &attr,
        &item,
        random,
    ))
}

/// The end of the derive's query: [`finish_derive`] with the owned names known.
pub(crate) fn finish_derive_now(
    item: syn::Item,
    mut opts: Options,
    random: u64,
    owned: &OwnedAttrs,
    t: &mut timing::Timer,
) -> syn::Result<TokenStream> {
    opts.emit_item = false;
    opts.derived = true;
    generate::generate_timed(item, &opts, random, owned, t)
}

/// What the `Capture` derive reads from its input, the capture dummy:
/// the arguments of `#[__announce(..)]` and the tokens of the item, if it is
/// still there (it is gone when it was `#[cfg]`'d out).
struct Dummy {
    args: options::CaptureArgs,
    item: Option<TokenStream>,
}

/// Reads the dummy `#[__announce(ARGS)] .. enum Cap { V = { ITEM 0 } }` from its
/// top-level tokens, without parsing it. `None` if it has another shape.
fn scan_dummy(input: TokenStream) -> Option<(TokenStream, Option<TokenStream>)> {
    use proc_macro2::{Delimiter, TokenTree};
    let mut it = input.into_iter().peekable();
    let mut args = None;
    while matches!(it.peek(), Some(TokenTree::Punct(p)) if p.as_char() == '#') {
        it.next();
        let group = match it.next()? {
            TokenTree::Group(g) if g.delimiter() == Delimiter::Bracket => g,
            _ => return None,
        };
        let mut inner = group.stream().into_iter();
        if matches!(inner.next(), Some(TokenTree::Ident(i)) if i == "__announce") {
            match inner.next() {
                Some(TokenTree::Group(a))
                    if a.delimiter() == Delimiter::Parenthesis && inner.next().is_none() =>
                {
                    args = Some(a.stream())
                }
                _ => return None,
            }
        }
    }
    if !matches!(it.next()?, TokenTree::Ident(i) if i == "enum") {
        return None;
    }
    if !matches!(it.next()?, TokenTree::Ident(_)) {
        return None;
    }
    let body = match it.next()? {
        TokenTree::Group(g) if g.delimiter() == Delimiter::Brace => g,
        _ => return None,
    };
    if it.next().is_some() {
        return None;
    }
    let mut variant = body.stream().into_iter();
    if !matches!(variant.next()?, TokenTree::Ident(_)) {
        return None;
    }
    if !matches!(variant.next()?, TokenTree::Punct(p) if p.as_char() == '=') {
        return None;
    }
    let block = match variant.next()? {
        TokenTree::Group(g) if g.delimiter() == Delimiter::Brace => g,
        _ => return None,
    };
    if variant.next().is_some() {
        return None;
    }
    let mut tts: Vec<TokenTree> = block.stream().into_iter().collect();
    match tts.pop()? {
        TokenTree::Literal(l) if l.to_string() == "0" => {}
        _ => return None,
    }
    let item = if tts.is_empty() {
        None
    } else {
        Some(tts.into_iter().collect())
    };
    Some((args?, item))
}

/// The same with `syn`, for a dummy of another shape than [`scan_dummy`] knows.
fn read_dummy_with_syn(input: TokenStream) -> syn::Result<Dummy> {
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
        syn::Stmt::Item(i) => Some(i.to_token_stream()),
        _ => None,
    });
    Ok(Dummy { args, item })
}

fn read_dummy(input: TokenStream) -> syn::Result<Dummy> {
    match scan_dummy(input.clone()) {
        Some((args, item)) => Ok(Dummy {
            args: syn::parse2(args)?,
            item,
        }),
        None => read_dummy_with_syn(input),
    }
}

/// The body of the hidden `Capture` derive that `#[announce]` expands to.
/// Not part of the API.
///
/// The item is parsed here, once: everything else is done
/// on that tree and on the tokens it prints.
#[doc(hidden)]
pub fn capture(input: TokenStream) -> syn::Result<TokenStream> {
    let mut t = timing::Timer::new("capture");
    let dummy = read_dummy(input)?;
    t.step("read");
    let tokens = match dummy.item {
        Some(tokens) => tokens,
        None => return Ok(TokenStream::new()),
    };
    let args = dummy.args;
    let item: syn::Item = syn::parse2(tokens.clone())?;
    t.subject(|| generate::ident_vis(&item).map_or_else(|_| String::new(), |(i, _)| i.to_string()));
    t.step("parse");
    let prep = generate::prepare(item, Some(tokens), &args.opts, &args.owned);
    t.step("strip");
    Ok(
        match generate::emit(&prep, &args.opts, args.random, &args.owned, &mut t) {
            Ok(generated) => generated,
            Err(e) => {
                let mut out = e.to_compile_error();
                out.extend(generate::real_item(&prep, &args.opts, &args.owned));
                out
            }
        },
    )
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
                   layout = single, docs = true, meta = m::info(1, 2), meta = m::bare, \
                   c::check(x), c::bare";
        let opts: Options = syn::parse2(ts(src)).unwrap();
        let printed = opts.to_token_stream();
        let again: Options = syn::parse2(printed.clone()).unwrap();
        assert_eq!(norm(&printed), norm(&again.to_token_stream()));
        assert_eq!(
            norm(&printed),
            "crate=::l::announce,macro_name=N,mock=a::B,repeater=crate::R,\
             allow_primitive=false,allow_std_prelude=true,emit_item=false,layout=single,docs=true,\
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
            out.starts_with("::core::compile_error!")
                && out.contains("expected`define_repeater!(<visibility><Name>)`"),
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

    /// The state group of a query for `S`, as `query` writes it, to feed `finish`.
    fn finish_input(mode: &str, attr: &str, item: &str, answers: &str) -> TokenStream {
        ts(&format!(
            "@__announce_attrs [ ::l::announce::__private::finish! ] {{ [7u64; {} ({}) {{ {} }}] {} }}",
            mode, attr, item, answers
        ))
    }

    #[test]
    fn checks_send_the_query_before_anything_is_emitted() {
        let out = norm(&announce(
            ts("crate = ::l::announce, a::check, b::check(x), meta = m::info"),
            ts("pub struct S { pub a: u8 }"),
        ));
        // The first check is called, and heads the list: all the checks and
        // `finish!` follow, like a continuation list.
        assert!(
            out.starts_with(
                "a::check!{@__announce_attrs[a::check!,b::check!,::l::announce::__private::finish!]{[",
            ),
            "{}",
            out
        );
        // The state: a random number, the mode, the attribute and the item.
        assert!(
            out.contains(";attr(crate=::l::announce,a::check,b::check(x),meta=m::info){pubstructS{puba:u8}}]}}"),
            "{}",
            out
        );
        // Nothing else yet: no item, no capture dummy.
        assert!(
            !out.contains("Capture") && out.matches("pubstructS").count() == 1,
            "{}",
            out
        );
        // Without checks there is no query.
        let out = norm(&announce(
            ts("crate = ::l::announce, meta = m::info"),
            ts("pub struct S;"),
        ));
        assert!(
            !out.contains("__announce_attrs") && out.contains("Capture"),
            "{}",
            out
        );
    }

    #[test]
    fn finish_without_owned_attributes_emits_what_announce_emits_without_checks() {
        let item = "pub struct S { pub a: u8 }";
        let out = norm(&finish(finish_input(
            "attr",
            "crate=::l::announce,a::check",
            item,
            "",
        )));
        assert!(
            out.starts_with("pubstructS{puba:u8}#[derive(::l::announce::__private::Capture)]"),
            "{}",
            out
        );
        assert!(
            out.contains("#[__announce(7u64;crate=::l::announce,a::check)]"),
            "{}",
            out
        );
    }

    #[test]
    fn finish_with_owned_attributes_leaves_the_item_to_the_capture_derive() {
        let item = "pub struct S { #[my_lib(skip)] pub a: u8 }";
        let out = norm(&finish(finish_input(
            "attr",
            "crate=::l::announce,a::check",
            item,
            "attrs(my_lib) attrs(other, my_lib)",
        )));
        // The item is not emitted here (it would have an attribute no macro knows),
        // only in the dummy that the derive reads, with the owned names in front
        // of the options.
        assert!(
            out.starts_with("#[derive(::l::announce::__private::Capture)]"),
            "{}",
            out
        );
        assert!(
            out.contains("#[__announce(7u64;[my_lib,other];crate=::l::announce,a::check)]"),
            "{}",
            out
        );
        assert_eq!(out.matches("#[my_lib(skip)]").count(), 1, "{}", out);
    }

    #[test]
    fn finish_reports_a_bad_query() {
        let item = "struct S;";
        let attr = "a::check";
        let err = |ts: TokenStream| try_finish(ts).err().unwrap().to_string();
        // A name that rustc uses.
        let e = err(finish_input("attr", attr, item, "attrs(doc)"));
        assert!(
            e.contains("`doc` cannot be an owned attribute name"),
            "{}",
            e
        );
        // A check that changed the state.
        let e = err(finish_input("attr", attr, item, "other(x)"));
        assert!(e.contains("was changed by a check"), "{}", e);
        let e = err(finish_input("attr", attr, item, "attrs[x]"));
        assert!(e.contains("was changed by a check"), "{}", e);
        let e = err(finish_input("weird", attr, item, ""));
        assert!(e.contains("was changed by a check"), "{}", e);
        // A list that was not forwarded to the end: a check kept its own head ...
        let e = err(ts(
            "@__announce_attrs [ x!, f::finish! ] { [7u64; attr (a) { struct S; }] }",
        ));
        assert!(e.contains("elements left in its list"), "{}", e);
        // ... or dropped more than that.
        let e = err(ts(
            "@__announce_attrs [] { [7u64; attr (a) { struct S; }] }",
        ));
        assert!(e.contains("with an empty list"), "{}", e);
        let e = err(ts(
            "@__announce_attrs [ #[derive(A)] ] { [7u64; attr (a) { struct S; }] }",
        ));
        assert!(e.contains("elements left in its list"), "{}", e);
        // The list `[ finish! ]`, whatever the path of the macro, is the end.
        try_finish(ts(
            "@__announce_attrs [ x::y! ] { [7u64; attr () { struct S; }] }",
        ))
        .unwrap();
        // Called by hand.
        let e = err(ts("struct S;"));
        assert!(e.contains("cannot be called by hand"), "{}", e);
        // The item is parsed again, with its error.
        assert!(try_finish(finish_input("attr", attr, "fn f() {}", "")).is_err());
    }

    #[test]
    fn capture_arguments_read_the_owned_names() {
        let with: options::CaptureArgs =
            syn::parse2(ts("5u64; [a, b]; crate = ::l::announce, c::check")).unwrap();
        assert_eq!(with.random, 5);
        assert_eq!(
            with.owned
                .names()
                .iter()
                .map(|n| n.to_string())
                .collect::<Vec<_>>(),
            ["a", "b"]
        );
        assert_eq!(
            norm(&with.opts.to_token_stream()),
            "crate=::l::announce,c::check"
        );
        let without: options::CaptureArgs = syn::parse2(ts("5u64; c::check")).unwrap();
        assert!(without.owned.is_empty());
        assert_eq!(norm(&without.opts.to_token_stream()), "c::check");
    }

    #[test]
    fn derive_with_checks_sends_the_same_query() {
        let out = norm(&derive_announce(ts(
            "#[announce(crate = ::l::announce, macro_name = N, a::check)] pub struct S(#[my_lib] pub u8);",
        )));
        assert!(
            out.starts_with(
                "a::check!{@__announce_attrs[a::check!,::l::announce::__private::finish!]{["
            ) && out.contains(";derive(crate=::l::announce,macro_name=N,emit_item=false,a::check)"),
            "{}",
            out
        );
        // The helper attribute is not part of the item; the derive's own attributes are.
        assert!(
            !out.contains("#[announce") && out.contains("pubstructS(#[my_lib]pubu8);"),
            "{}",
            out
        );
        // The end of the query: the trampoline, the check and the `Announce` impl,
        // with the owned names in META; the item is not emitted again.
        let end = norm(&finish(finish_input(
            "derive",
            "crate=::l::announce,macro_name=N,emit_item=false,a::check",
            "pub struct S(#[my_lib] pub u8);",
            "attrs(my_lib)",
        )));
        assert!(end.contains("macro_rules!__announce_S_"), "{}", end);
        assert!(
            end.contains("a::check!{(),pubstructS(#[my_lib]pubu8);}"),
            "{}",
            end
        );
        assert!(
            end.contains("[a::check,attrs(my_lib)]{pubstructS(#[my_lib]pubu8);}"),
            "{}",
            end
        );
        assert!(end.contains("::l::announce::AnnounceforS{}"), "{}", end);
        assert!(!end.starts_with("pubstructS"), "{}", end);
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

    // ---- the capture dummy, read without a parse ----------------------------

    const DUMMY_ARGS: &str = "7u64; crate = ::l::announce";

    fn dummy(args: &str, body: &str) -> TokenStream {
        ts(&format!(
            "#[derive(::l::announce::__private::Capture)] #[__announce({})] \
             #[::l::announce::__private::remove] #[allow(dead_code, non_camel_case_types)] \
             enum __AnnounceCap_0 {{ V = {{ {} 0 }} }}",
            args, body
        ))
    }

    /// `#[derive(..)]` is not part of what a derive macro receives.
    fn derive_input(args: &str, body: &str) -> TokenStream {
        let mut it = dummy(args, body).into_iter();
        it.next();
        it.next();
        it.collect()
    }

    #[test]
    fn the_dummy_is_read_from_its_top_level_tokens() {
        let (args, item) =
            scan_dummy(derive_input(DUMMY_ARGS, "pub struct S { pub a: u8 }")).unwrap();
        assert_eq!(norm(&args), "7u64;crate=::l::announce");
        assert_eq!(norm(&item.unwrap()), "pubstructS{puba:u8}");
        // An item that was `#[cfg]`'d out leaves the block with the `0` only.
        let (_, item) = scan_dummy(derive_input(DUMMY_ARGS, "")).unwrap();
        assert!(item.is_none());
        // Other shapes are not read (the code then parses the dummy).
        for bad in [
            "struct S;",
            "enum E { V = 1 }",
            "enum E { V = { 1 } }",
            "enum E { V = { S } }",
            "#[__announce] enum E { V = { 0 } }",
            "#[__announce(1u64;)] enum E { V = { S 1 } }",
        ] {
            assert!(scan_dummy(ts(bad)).is_none(), "{}", bad);
        }
    }

    #[test]
    fn a_dummy_of_another_shape_is_read_with_syn() {
        // `pub`: the scan does not know it, `syn::DeriveInput` does.
        let input = ts(
            "#[__announce(7u64; crate = ::l::announce)] pub enum Cap { V = { pub struct S; 0 } }",
        );
        assert!(scan_dummy(input.clone()).is_none());
        let d = read_dummy(input).unwrap();
        assert_eq!(d.args.random, 7);
        assert_eq!(norm(&d.item.unwrap()), "pubstructS;");
        let e = read_dummy(ts("pub enum Cap { V = { 0 } }")).err().unwrap();
        assert!(e.to_string().contains("missing #[__announce]"), "{}", e);
    }

    #[test]
    fn capture_emits_what_the_derive_emits() {
        let out = norm(&capture(derive_input(DUMMY_ARGS, "pub struct S { pub a: u8 }")).unwrap());
        assert!(out.contains("macro_rules!__announce_S_"), "{}", out);
        assert!(out.contains("::l::announce::AnnounceforS{}"), "{}", out);
        // The item itself is not part of it: `#[announce]` emitted it.
        assert!(!out.starts_with("pubstruct"), "{}", out);
        // Gone with its `#[cfg]`.
        assert!(capture(derive_input(DUMMY_ARGS, "")).unwrap().is_empty());
        // Not a valid item: the error of the parse.
        assert!(capture(derive_input(DUMMY_ARGS, "pub struct S { pub a: }")).is_err());
    }

    #[test]
    fn capture_keeps_the_real_item_next_to_its_error() {
        // With owned attributes the derive emits the item, so an error in the type
        // context is followed by the item (without the owned attributes and the cfgs).
        let args = "7u64; [my_lib]; crate = ::l::announce";
        let out = norm(
            &capture(derive_input(
                args,
                "pub struct S { #[my_lib] #[cfg(a)] pub a: Local, pub b: u8 }",
            ))
            .unwrap(),
        );
        assert!(out.starts_with("::core::compile_error!"), "{}", out);
        assert!(out.ends_with("pubstructS{puba:Local,pubb:u8}"), "{}", out);
        // Without owned names `#[announce]` emitted it: the error only.
        let out =
            norm(&capture(derive_input(DUMMY_ARGS, "pub struct S { pub a: Local }")).unwrap());
        assert!(out.starts_with("::core::compile_error!"), "{}", out);
        assert!(!out.contains("pubstructS"), "{}", out);
    }

    #[test]
    fn the_attribute_reads_no_more_than_the_head_of_the_item() {
        // The syntax of the rest is the business of the derive: an item with an
        // error in it is passed on, to be refused there.
        let out = norm(&announce(TokenStream::new(), ts("pub struct S { pub a: }")));
        assert!(
            out.contains("Capture") && !out.contains("compile_error"),
            "{}",
            out
        );
        // But what is not a supported item is refused at once, with the same errors.
        for (src, msg) in [
            ("pub fn f() {}", "supports struct, enum"),
            ("mod m;", "only inline modules"),
            ("impl S {}", "supports struct, enum"),
        ] {
            let e = try_announce(TokenStream::new(), ts(src)).err().unwrap();
            assert!(e.to_string().contains(msg), "{}: {}", src, e);
        }
        // The kinds, with all their qualifiers.
        for src in [
            "struct S;",
            "pub(crate) enum E {}",
            "union U { a: u8 }",
            "pub unsafe trait T {}",
            "type A = u8;",
            "mod m { }",
        ] {
            assert!(try_announce(TokenStream::new(), ts(src)).is_ok(), "{}", src);
        }
    }
}
