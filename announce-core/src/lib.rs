//! The implementation behind `#[announce]`, as plain functions, and the helpers for
//! callbacks and checks. Do not depend on this
//! crate: go through [`announce`](https://docs.rs/announce), which re-exports
//! the public API as `announce::machinery` and `announce::helper`.
//!
//! A proc-macro crate cannot export functions, so the code behind the macros
//! lives here, in an ordinary library. `announce-macros` is a thin wrapper
//! around it. The crate name and layout are not a stable interface.
//!
//! The API of the generation is [`announce()`] and [`Options`] (`announce::machinery`). See their docs, and
//! `announce::machinery` for how a library uses them. [`helper`] is what
//! callbacks and checks use (`announce::helper`): the parser of the tokens a
//! trampoline sends, and the functions [`helper::callback()`] and
//! [`helper::transformer()`].

mod cratepath;
mod fastpath;
mod finish;
mod generate;
pub mod helper;
mod leak;
mod options;
mod runner;
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
/// [`finish()`]), which expands to what is described above. See [`helper::attrs`]
/// for the protocol.
///
/// A returned error is a syntax or type-context error of the same kind as
/// `#[announce]` reports. Here it comes back as a `compile_error!` invocation
/// in the returned tokens, so a proc macro can return the result as it is.
/// Use [`try_announce`] to get the [`syn::Error`] instead.
///
/// The generated code names the `announce` crate. See [`Options::krate()`] and
/// the `crate = ..` option.
///
/// If the item has other outer attributes, the result is not the trampoline yet: it is the
/// item with the hidden attribute macro `#[<crate path>::__private::__announce_runner(..)]`
/// (the *runner*) as its last attribute, which holds the options and generates the
/// trampoline when it expands, after every other attribute macro on the item (and drops
/// the item, with `mock` or `emit_item = false`), and, after the item, a *sentinel* that
/// fails at the definition if the runner never runs (a macro below that drops the attributes
/// of the item). If the item has the runner already (another announcement was expanded
/// first), the options are merged into it, with no sentinel. An item with no other attribute
/// that could expand (documentation and `repr` cannot) is announced at once, with no runner
/// and no sentinel. See [Several announcements on one
/// item](https://docs.rs/announce/latest/announce/attr.announce.html#several-announcements-on-one-item).
pub fn announce(attr: TokenStream, item: TokenStream) -> TokenStream {
    try_announce(attr, item).unwrap_or_else(|e| e.to_compile_error())
}

/// [`announce()`] with the error returned as a [`syn::Error`].
///
/// The item is not parsed here: only its kind and name are read (`scan`), to
/// refuse what is not supported at once. Its syntax is validated once, by the parse of
/// the `Capture` derive that the runner (or this attribute, when it announces at once)
/// emits.
pub fn try_announce(attr: TokenStream, item: TokenStream) -> syn::Result<TokenStream> {
    let mut t = timing::Timer::new("attribute");
    let opts: Options = syn::parse2(attr)?;
    t.step("options");
    let head = scan::head(&item)?;
    t.subject(|| format!("{:?} {}", head.kind, head.ident));
    t.step("scan");
    let attr = opts.to_token_stream();
    let random = random_u64();
    // The query of the checks, which ends with the announcement (`finish`), or the
    // announcement.
    let out = if opts.checks.is_empty() {
        announce_now(&opts, &attr, item, random, &OwnedAttrs::new(), &head)
    } else {
        finish::query(&opts, &attr, &item, random, &opts.checks)
    };
    t.step("emit");
    Ok(out)
}

/// The announcement, once the attribute names that its checks own are known (`owned`:
/// empty without checks): the trampoline at once if nothing can expand after it, or the
/// item with the runner (see `runner`). That is so for `mock` and `emit_item = false`
/// as well: the runner drops the item.
pub(crate) fn announce_now(
    opts: &Options,
    attr: &TokenStream,
    item: TokenStream,
    random: u64,
    owned: &OwnedAttrs,
    head: &scan::Head,
) -> TokenStream {
    // No other attribute that could expand (documentation, `repr`, .. and the attributes
    // that the checks own do not): there is nothing to wait for.
    if !runner::can_expand_after(&item, owned) {
        return capture_output(opts, attr, item, random, owned, None);
    }
    runner::announce(opts, head, item, owned)
}

/// What an announcement expands to when it is made (see `runner`), once the attribute
/// names that its checks own are known (`owned`; empty without checks): the item and the
/// capture dummy.
///
/// The derive sees the copy with every nested cfg
/// evaluated in this crate, then `remove` deletes the dummy. Without owned
/// attributes the item is emitted here, as written. With owned attributes it
/// is the derive that emits it, from its cfg-evaluated copy and without them.
///
/// `sentinel` is the kind of sentinel that the announcement emitted next to the item, if
/// it did: the derive then emits a stand-in for what the sentinel needs if it fails, so
/// that the error it reports is not followed by the one of the sentinel.
pub(crate) fn capture_output(
    opts: &Options,
    attr: &TokenStream,
    item: TokenStream,
    random: u64,
    owned: &OwnedAttrs,
    sentinel: Option<runner::Sentinel>,
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
    let sentinel_arg = sentinel.map(|s| {
        let word = Ident::new(s.capture_word(), Span::call_site());
        quote! { #word ; }
    });
    quote! {
        #(if emit_item) { #item }
        #[derive(#krate::__private::Capture)]
        #[__announce(#random_lit; #sentinel_arg #owned_arg #attr)]
        #[#krate::__private::remove]
        #[allow(dead_code, non_camel_case_types)]
        enum #cap { V = { #item 0 } }
    }
}

/// What the runner `#[<crate path>::__private::__announce_runner(args)]` does to
/// the item that follows it. Not part of the API: it is the body of the hidden attribute
/// macro that `#[announce]` puts last among the attributes of an item. See the docs of
/// `announce::announce` ("Several announcements on one item").
#[doc(hidden)]
pub fn run_announce(args: TokenStream, item: TokenStream) -> TokenStream {
    runner::run(args, item)
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
    let item: syn::Item = match syn::parse2(tokens.clone()) {
        Ok(item) => item,
        // The announcement has a sentinel: what it needs stands in for the trampoline.
        Err(e) => match (args.sentinel, scan::head(&tokens)) {
            (Some(sentinel), Ok(head)) => {
                let mut out = e.to_compile_error();
                out.extend(generate::sentinel_stub(
                    sentinel,
                    &head.ident,
                    &args.opts,
                    args.random,
                ));
                return Ok(out);
            }
            _ => return Err(e),
        },
    };
    let ident = generate::ident_vis(&item).ok().map(|(i, _)| i);
    t.subject(|| ident.as_ref().map_or_else(String::new, |i| i.to_string()));
    t.step("parse");
    let prep = generate::prepare(item, Some(tokens), &args.opts, &args.owned);
    t.step("strip");
    Ok(
        match generate::emit(&prep, &args.opts, args.random, &args.owned, &mut t) {
            Ok(generated) => generated,
            Err(e) => {
                let mut out = e.to_compile_error();
                out.extend(generate::real_item(&prep, &args.opts, &args.owned));
                if let (Some(sentinel), Some(ident)) = (args.sentinel, &ident) {
                    out.extend(generate::sentinel_stub(
                        sentinel,
                        ident,
                        &args.opts,
                        args.random,
                    ));
                }
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

    /// The sentinel of a struct `S` without generics (in the crate `::l::announce`), as
    /// printed: it stands after the item.
    const CHK: &str = "#[allow(deprecated)]const_:()={trait__AnnounceSentinel{}\
                       impl__AnnounceSentinelforSwhereS:::l::announce::Announce{}};";

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
        // An item with no other attribute is announced at once: the item and the capture
        // derive, which names the given crate.
        for item in ["pub struct S { pub a: u8 }", "pub trait T { fn f(&self); }"] {
            let out = norm(&announce(ts("crate = ::l::announce"), ts(item)));
            assert!(out.starts_with(&norm(&ts(item))), "{}", out);
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
            assert!(!out.contains("__announce_runner"), "{}", out);
        }
        // An item with another attribute has the runner as its last attribute, in the
        // given crate, and nothing else.
        let out = norm(&announce(
            ts("crate = ::l::announce"),
            ts("#[derive(Debug)] pub struct S { pub a: u8 }"),
        ));
        assert_eq!(
            out,
            [
                "#[derive(Debug)]#[::l::announce::__private::__announce_runner(__announce_format_version=\"1\",__announce_sentinel=\"impl\",crate=::l::announce)]pubstructS{puba:u8}",
                CHK
            ]
            .concat()
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
    fn layout_is_an_option_of_the_attribute() {
        let item = "pub struct S { pub a: u8 }";
        // The capture derive reads it, and makes the trampoline of that layout.
        let captured = |opts: &str| {
            norm(
                &capture(derive_input(
                    &format!("7u64; crate = ::l::announce{}", opts),
                    item,
                ))
                .unwrap(),
            )
        };
        assert!(!captured("").contains("__announce_emit"));
        assert!(!captured(", layout = direct").contains("__announce_emit"));
        assert!(captured(", layout = single").contains("__announce_emit"));
        // The attribute hands the option to the capture derive, and to the runner.
        let out = norm(&announce(ts("layout = single"), ts(item)));
        assert!(out.contains("#[__announce("), "{}", out);
        assert!(out.contains("layout=single"), "{}", out);
        let out = norm(&announce(
            ts("layout = single"),
            ts(&format!("#[x] {}", item)),
        ));
        assert!(
            out.contains(
                "__announce_runner(__announce_format_version=\"1\",__announce_sentinel=\"impl\",layout=single)"
            ),
            "{}",
            out
        );
    }

    /// The state group of a query for `S`, as `query` writes it, to feed `finish`.
    fn finish_input(attr: &str, item: &str, answers: &str) -> TokenStream {
        ts(&format!(
            "@__announce_attrs [ ::l::announce::__private::finish! ] {{ [7u64; attr ({}) {{ {} }}] {} }}",
            attr, item, answers
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
                "a::check!{@announce_format\"1\"@__announce_attrs[a::check!,b::check!,::l::announce::__private::finish!]{[",
            ),
            "{}",
            out
        );
        // The state: a random number, the mode, the attribute and the item.
        assert!(
            out.contains(";attr(crate=::l::announce,meta=m::info,a::check,b::check(x)){pubstructS{puba:u8}}]}}"),
            "{}",
            out
        );
        // Nothing else yet: no item, no runner, no capture dummy.
        assert!(
            !out.contains("Capture")
                && !out.contains("__announce_runner")
                && out.matches("pubstructS").count() == 1,
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
        // No other attribute: the item and the capture dummy, as without checks.
        let out = norm(&finish(finish_input(
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
        // Another attribute: the runner, which carries the options.
        let out = norm(&finish(finish_input(
            "crate=::l::announce,a::check",
            &format!("#[x] {}", item),
            "",
        )));
        assert_eq!(
            out,
            [
                "#[x]#[::l::announce::__private::__announce_runner(__announce_format_version=\"1\",__announce_sentinel=\"impl\",crate=::l::announce,a::check)]pubstructS{puba:u8}",
                CHK
            ]
            .concat()
        );
    }

    #[test]
    fn finish_with_owned_attributes_leaves_the_item_to_the_capture_derive() {
        let item = "pub struct S { #[my_lib(skip)] pub a: u8 }";
        let out = norm(&finish(finish_input(
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
        // With another attribute there is the runner, which has the names. The attribute
        // that is owned follows it.
        let out = norm(&finish(finish_input(
            "crate=::l::announce,a::check",
            &format!("#[my_lib(on_item)] #[x] {}", item),
            "attrs(my_lib) attrs(other, my_lib)",
        )));
        assert_eq!(
            out,
            [
                "#[x]#[::l::announce::__private::__announce_runner(__announce_format_version=\"1\",__announce_sentinel=\"impl\",__announce_owned(my_lib,other),crate=::l::announce,a::check)]\
                 #[my_lib(on_item)]pubstructS{#[my_lib(skip)]puba:u8}",
                CHK
            ]
            .concat()
        );
    }

    #[test]
    fn finish_reports_a_bad_query() {
        let item = "struct S;";
        let attr = "a::check";
        let err = |ts: TokenStream| try_finish(ts).err().unwrap().to_string();
        // A name that rustc uses.
        let e = err(finish_input(attr, item, "attrs(doc)"));
        assert!(
            e.contains("`doc` cannot be an owned attribute name"),
            "{}",
            e
        );
        // A check that changed the state.
        let e = err(finish_input(attr, item, "other(x)"));
        assert!(e.contains("was changed by a check"), "{}", e);
        let e = err(finish_input(attr, item, "attrs[x]"));
        assert!(e.contains("was changed by a check"), "{}", e);
        let e = err(ts(
            "@__announce_attrs [ f::finish! ] { [7u64; weird (a) { struct S; }] }",
        ));
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
        assert!(try_finish(finish_input(attr, "fn f() {}", "")).is_err());
    }

    #[test]
    fn finish_reads_the_query_with_the_prefix_and_without_it() {
        let state = |prefix: &str| {
            ts(&format!(
                "{} @__announce_attrs [ ::l::announce::__private::finish! ] {{ [7u64; attr (crate=::l::announce) {{ pub struct S; }}] attrs(a) }}",
                prefix
            ))
        };
        let without = norm(&finish(state("")));
        let with = norm(&finish(state("@announce_format \"1\"")));
        assert!(without.contains("Capture"), "{}", without);
        assert_eq!(without, with);
        // Another version is one error that names both versions; it is not the
        // error of a call by hand.
        let e = try_finish(state("@announce_format \"2\""))
            .err()
            .unwrap()
            .to_string();
        assert!(
            e.starts_with("announce: incompatible wire format: found `@announce_format \"2\"`, expected `@announce_format \"1\"`")
                && !e.contains("cannot be called by hand"),
            "{}",
            e
        );
        // `finish` turns it into a `compile_error!`.
        let out = norm(&finish(state("@announce_format \"2\"")));
        assert!(
            out.starts_with("::core::compile_error!") && out.contains("incompatiblewireformat"),
            "{}",
            out
        );
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

    // ---- several announcements of one item (see `runner`) --------------------------

    /// Follows `#[announce(..)]` on the item the way rustc does, with checks that own
    /// attributes (`owned`: the check and the names it owns), until the trampoline: the
    /// first attribute whose path ends in `announce` is expanded, then the runner; the
    /// rest of the attributes are inert here. Returns the terminal output and how many
    /// times the query was sent.
    fn drive(attr: &str, item: &str, owned: &[(&str, &[&str])]) -> (String, usize) {
        let (mut attr, mut item) = (ts(attr), ts(item));
        let mut queries = 0;
        let mut out = announce(attr.clone(), item.clone());
        for _ in 0..20 {
            if norm(&out).contains("@__announce_attrs") {
                // The call of the first check. The checks forward the query, each adds
                // its `attrs(..)`, `finish!` ends it.
                queries += 1;
                let group = match out.clone().into_iter().last() {
                    Some(proc_macro2::TokenTree::Group(g)) => g,
                    _ => panic!("{}", out),
                };
                let q: helper::Query = syn::parse2(group.stream()).unwrap();
                let mut names = OwnedAttrs::new();
                for c in &q.list.0 {
                    let text: String = norm(&c.to_token_stream()).replace('!', "");
                    for (check, own) in owned {
                        if *check == text {
                            names.extend(OwnedAttrs::from_strs(own).names().iter().cloned());
                        }
                    }
                }
                let (state, last) = (&q.state, q.list.0.last().unwrap());
                out = finish(quote! { @__announce_attrs [ #last ] { #state #names } });
                continue;
            }
            if norm(&out).contains("Capture") {
                return (norm(&out), queries);
            }
            // The next macro is the first attribute from the top that is an `announce`,
            // or the runner when none is left: its arguments, and the item without it.
            let o = scan::outer(&out);
            let at = o
                .attrs
                .iter()
                .position(|a| a.segments.last().map_or(false, |l| l == "announce"))
                .or_else(|| {
                    o.attrs.iter().position(|a| {
                        a.segments
                            .last()
                            .map_or(false, |l| l == "__announce_runner")
                    })
                })
                .unwrap_or_else(|| panic!("nothing left to expand: {}", norm(&out)));
            attr = o.attrs[at].args.clone().unwrap().stream();
            item = o
                .tts
                .iter()
                .enumerate()
                .filter(|(i, _)| *i != o.attrs[at].at && *i != o.attrs[at].at + 1)
                .map(|(_, t)| t.clone())
                .collect();
            let is_runner = o.attrs[at]
                .segments
                .last()
                .map_or(false, |l| l == "__announce_runner");
            out = if is_runner {
                run_announce(attr.clone(), item.clone())
            } else {
                announce(attr.clone(), item.clone())
            };
        }
        panic!("no end: {}", norm(&out));
    }

    #[test]
    fn the_query_is_sent_once_for_an_announcement() {
        let owned: &[(&str, &[&str])] = &[("a::check", &["my_lib"]), ("b::check", &["other"])];
        // The first expansion sends the query, `finish` puts the runner on the item with
        // the answer in its arguments, and the owned attribute after it. The runner makes
        // the trampoline without asking again.
        let first = norm(&announce(
            ts("crate = ::l::announce, a::check"),
            ts("#[other] #[my_lib(x)] pub struct S;"),
        ));
        assert!(
            first.starts_with("a::check!{@announce_format\"1\"@__announce_attrs"),
            "{}",
            first
        );
        let runner = norm(&finish(finish_input(
            "crate=::l::announce,a::check",
            "#[other] #[my_lib(x)] pub struct S;",
            "attrs(my_lib)",
        )));
        assert_eq!(
            runner,
            [
                "#[other]#[::l::announce::__private::__announce_runner(__announce_format_version=\"1\",__announce_sentinel=\"impl\",__announce_owned(my_lib),crate=::l::announce,a::check)]#[my_lib(x)]pubstructS;",
                CHK
            ]
            .concat()
        );
        // The same, driven all the way.
        let (end, queries) = drive(
            "crate = ::l::announce, a::check",
            "#[other] #[my_lib(x)] pub struct S;",
            owned,
        );
        assert_eq!(queries, 1, "{}", end);
        assert_eq!(end.matches("Capture").count(), 1, "{}", end);
        // The owned names go to the capture derive, which emits the item without them.
        assert!(
            end.contains("#[__announce(")
                && end.contains("[my_lib];crate=::l::announce,a::check)]"),
            "{}",
            end
        );
        // An answer without names is carried as well, and is no reason to ask again.
        let (end, queries) = drive("crate = ::l::announce, b::check", "#[x] pub struct S;", &[]);
        assert_eq!(queries, 1, "{}", end);
        // No other attribute, no runner: one query, and the trampoline.
        let (end, queries) = drive("crate = ::l::announce, a::check", "pub struct S;", owned);
        assert_eq!(queries, 1);
        assert!(!end.contains("__announce_runner"), "{}", end);
    }

    #[test]
    fn two_announcements_with_checks_ask_once_each() {
        // Two announcements with checks, merged into the runner: the query is sent for
        // each of them once, and the answers are united. The attribute of each stands
        // after the runner when it expands.
        let owned: &[(&str, &[&str])] = &[("a::check", &["my_lib"]), ("b::check", &["other"])];
        let item = "#[::l::announce::announce(b::check)] #[my_lib(x)] #[other(y)] pub struct S;";
        let (end, queries) = drive("crate = ::l::announce, a::check", item, owned);
        assert_eq!(queries, 2, "{}", end);
        assert_eq!(end.matches("Capture").count(), 1, "{}", end);
        assert!(
            end.contains("[my_lib,other];crate=::l::announce,a::check,b::check)]"),
            "{}",
            end
        );
        // The same checks twice: one query for each announcement, the check once in META.
        let item = "#[::l::announce::announce(a::check)] #[my_lib(x)] pub struct S;";
        let (end, queries) = drive("crate = ::l::announce, a::check", item, owned);
        assert_eq!(queries, 2, "{}", end);
        assert_eq!(end.matches("a::check").count(), 1, "{}", end);
        // The same on a trait.
        let item = "#[::l::announce::announce(b::check)] #[my_lib(x)] #[other(y)] pub trait T {}";
        let (end, queries) = drive("crate = ::l::announce, a::check", item, owned);
        assert_eq!(queries, 2, "{}", end);
        assert!(end.contains("[my_lib,other];"), "{}", end);
    }

    #[test]
    fn an_item_that_is_not_emitted_goes_through_the_runner_as_well() {
        for opt in ["emit_item = false", "mock = a::B"] {
            let attr = format!("crate = ::l::announce, {}", opt);
            // Another attribute: the runner, which holds the option and drops the item
            // when it expands; the macros below see the item until then.
            let out = norm(&announce(ts(&attr), ts("#[derive(Debug)] pub struct S;")));
            // The item is not emitted, so the sentinel is the call of the trampoline.
            let want = format!(
                "#[derive(Debug)]#[::l::announce::__private::__announce_runner(__announce_format_version=\"1\",__announce_sentinel=\"macro\",crate=::l::announce,{})]pubstructS;S!{{@announce_format\"1\"@__announce_exists}}",
                opt.replace(' ', "")
            );
            assert_eq!(out, want);
            // No other attribute: nothing to wait for, the dummy alone (the item is not
            // emitted).
            let out = norm(&announce(ts(&attr), ts("pub struct S;")));
            assert!(
                out.starts_with("#[derive(::l::announce::__private::Capture)]")
                    && !out.contains("__announce_runner"),
                "{}",
                out
            );
            assert_eq!(out.matches("pubstructS").count(), 1, "{}", out);
            // The runner makes the trampoline and drops the item.
            let args = format!("__announce_format_version = \"1\", {}", attr);
            let out = norm(&run_announce(ts(&args), ts("#[doc = \"d\"] pub struct S;")));
            assert!(
                out.starts_with("#[derive(::l::announce::__private::Capture)]"),
                "{}",
                out
            );
            assert_eq!(out.matches("pubstructS").count(), 1, "{}", out);
            // With a check, the query comes first, then the runner.
            let out = norm(&announce(
                ts(&format!("{}, a::check", attr)),
                ts("#[derive(Debug)] pub struct S;"),
            ));
            assert!(
                out.starts_with("a::check!{@announce_format\"1\"@__announce_attrs"),
                "{}",
                out
            );
            let end = norm(&finish(finish_input(
                &format!("crate=::l::announce,{},a::check", opt.replace(' ', "")),
                "#[derive(Debug)] pub struct S;",
                "attrs(my_lib)",
            )));
            assert!(
                end.contains("pubstructS;S!{@announce_format\"1\"@__announce_exists}")
                    && end.contains("__announce_runner(__announce_format_version=\"1\",__announce_sentinel=\"macro\",__announce_owned(my_lib),"),
                "{}",
                end
            );
        }
    }

    #[test]
    fn announcements_merge_with_one_that_is_not_emitted() {
        // The values that are set win over those that are not, in both orders; two
        // values that differ are the one merge error.
        let runner = |opt: &str| {
            format!(
                "#[::l::announce::__private::__announce_runner(__announce_format_version = \"1\"{})]",
                opt
            )
        };
        let out = norm(&announce(
            ts("meta = a::x"),
            ts(&format!(
                "#[x] {} pub struct S;",
                runner(", emit_item = false")
            )),
        ));
        assert!(out.contains("emit_item=false,meta=a::x)]"), "{}", out);
        let out = norm(&announce(
            ts("emit_item = false"),
            ts(&format!("#[x] {} pub struct S;", runner(", meta = a::x"))),
        ));
        assert!(out.contains("emit_item=false"), "{}", out);
        assert!(out.contains("meta=a::x"), "{}", out);
        let out = norm(&announce(
            ts("mock = a::B"),
            ts(&format!(
                "#[x] {} pub trait S {{}}",
                runner(", meta = a::x")
            )),
        ));
        assert!(
            out.contains("mock=a::B") && !out.contains("compile_error"),
            "{}",
            out
        );
        // Different values are a conflict, in one error with the item.
        let out = norm(&announce(
            ts("emit_item = false"),
            ts(&format!(
                "#[x] {} pub struct S;",
                runner(", emit_item = true")
            )),
        ));
        assert!(out.starts_with("::core::compile_error!"), "{}", out);
        assert!(out.contains("`emit_item=false`"), "{}", out);
        assert_eq!(out.matches("compile_error").count(), 1, "{}", out);
        let out = norm(&announce(
            ts("mock = a::B"),
            ts(&format!(
                "#[x] {} pub trait S {{}}",
                runner(", mock = a::C")
            )),
        ));
        assert!(
            out.starts_with("::core::compile_error!") && out.contains("mock="),
            "{}",
            out
        );
    }

    #[test]
    fn announcements_merge_only_under_one_trampoline_name() {
        let runner = |opts: &str| {
            format!(
                "#[::l::announce::__private::__announce_runner(__announce_format_version = \"1\"{})]",
                opts
            )
        };
        // Unset is the name of the item: `Parse!` and `ParseAlt!` are two names.
        let item = format!("#[x] {} pub trait Parse {{}}", runner(""));
        let out = norm(&announce(ts("macro_name = ParseAlt"), ts(&item)));
        assert!(out.starts_with("::core::compile_error!"), "{}", out);
        assert_eq!(out.matches("compile_error").count(), 1, "{}", out);
        assert!(out.contains("(`ParseAlt`and`Parse`)"), "{}", out);
        assert!(out.contains("onetrampoline"), "{}", out);
        assert!(out.ends_with(&norm(&ts(&item))), "{}", out);
        // The same when one runner expands with another on the item.
        let out = norm(&run_announce(
            ts("__announce_format_version = \"1\", macro_name = ParseAlt"),
            ts(&format!("{} pub trait Parse {{}}", runner(""))),
        ));
        assert!(out.starts_with("::core::compile_error!"), "{}", out);
        assert!(out.contains("(`ParseAlt`and`Parse`)"), "{}", out);
        // The same name, set in one of them or in both, is one trampoline.
        for (attr, set) in [
            ("macro_name = Parse", ""),
            ("", ", macro_name = Parse"),
            ("macro_name = ParseAlt", ", macro_name = ParseAlt"),
        ] {
            let item = format!("#[x] {} pub trait Parse {{}}", runner(set));
            let out = norm(&announce(ts(attr), ts(&item)));
            assert!(!out.contains("compile_error"), "{}: {}", item, out);
            assert_eq!(out.matches("__announce_runner").count(), 1, "{}", out);
        }
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
    fn the_capture_arguments_read_the_sentinel() {
        use runner::Sentinel;
        let with: options::CaptureArgs =
            syn::parse2(ts("5u64; sentinel_impl; [a]; crate = ::l::announce")).unwrap();
        assert_eq!(with.sentinel, Some(Sentinel::Impl));
        assert_eq!(with.owned.names().len(), 1);
        let with: options::CaptureArgs = syn::parse2(ts("5u64; sentinel_macro; c::check")).unwrap();
        assert_eq!(with.sentinel, Some(Sentinel::Macro));
        assert_eq!(norm(&with.opts.to_token_stream()), "c::check");
        let without: options::CaptureArgs = syn::parse2(ts("5u64; c::check")).unwrap();
        assert_eq!(without.sentinel, None);
        assert!(syn::parse2::<options::CaptureArgs>(ts("5u64; sentinel_cake; c::check")).is_err());
        // The announcement writes the word into the dummy, the capture reads it.
        let out = norm(&capture_output(
            &Options::default(),
            &TokenStream::new(),
            ts("pub struct S;"),
            5,
            &OwnedAttrs::new(),
            Some(runner::Sentinel::Macro),
        ));
        assert!(
            out.contains("#[__announce(5u64;sentinel_macro;)]"),
            "{}",
            out
        );
    }

    #[test]
    fn capture_leaves_what_the_sentinel_needs_next_to_its_error() {
        // The error is reported; the sentinel of the announcement is satisfied, so that it
        // does not add an error about the same item.
        let args = "7u64; sentinel_impl; crate = ::l::announce";
        for body in ["pub struct S { pub a: Local }", "pub struct S { pub a: }"] {
            let out = norm(&capture(derive_input(args, body)).unwrap());
            assert!(
                out.starts_with("::core::compile_error!"),
                "{}: {}",
                body,
                out
            );
            assert!(
                out.ends_with("impl::l::announce::AnnounceforS{}"),
                "{}: {}",
                body,
                out
            );
        }
        let args = "7u64; sentinel_macro; crate = ::l::announce, macro_name = N";
        for body in [
            "pub trait S { fn f(&self) -> Local; }",
            "pub trait S { fn }",
        ] {
            let out = norm(&capture(derive_input(args, body)).unwrap());
            assert!(
                out.starts_with("::core::compile_error!"),
                "{}: {}",
                body,
                out
            );
            assert!(
                out.contains(
                    "macro_rules!__announce_stub_0000000000000007{($(@announce_format\"1\")?@__announce_exists)=>{};}"
                ) && out.ends_with("use__announce_stub_0000000000000007asN;"),
                "{}: {}",
                body,
                out
            );
        }
        // Without a sentinel there is nothing of the kind.
        let out =
            norm(&capture(derive_input(DUMMY_ARGS, "pub struct S { pub a: Local }")).unwrap());
        assert!(
            !out.contains("Announce") && !out.contains("__announce_exists"),
            "{}",
            out
        );
        // The item is `cfg`'d out: nothing, as its sentinel.
        let args = "7u64; sentinel_macro; crate = ::l::announce";
        assert!(capture(derive_input(args, "")).unwrap().is_empty());
    }

    #[test]
    fn the_attribute_reads_no_more_than_the_head_of_the_item() {
        // The syntax of the rest is the business of the derive: an item with an
        // error in it is passed on, to be refused there.
        for item in ["pub struct S { pub a: }", "pub trait T { fn }"] {
            let out = norm(&announce(TokenStream::new(), ts(item)));
            assert!(
                (out.contains("Capture") || out.contains("__announce_runner"))
                    && !out.contains("compile_error"),
                "{}",
                out
            );
        }
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
