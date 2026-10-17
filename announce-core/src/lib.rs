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
/// item with the hidden attribute macro `#[<crate path>::__private::_actually_run_announce(..)]`
/// (the *runner*) as its last attribute, which holds the options and generates the
/// trampoline when it expands, after every other attribute macro on the item (and drops
/// the item, with `mock` or `emit_item = false`). If the item has the runner already
/// (another announcement was expanded first), the options are merged into it. An item
/// with no other attribute that could expand (documentation and `repr` cannot) is
/// announced at once. See [Several announcements on one
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
///
/// The bare `#[announce(..)]` attributes that are left in the item (the helper attributes
/// of a derive) are taken over: they are removed and their options are merged into
/// these. See the docs of `announce::announce` ("Several announcements on one item").
pub fn try_announce(attr: TokenStream, item: TokenStream) -> syn::Result<TokenStream> {
    let mut t = timing::Timer::new("attribute");
    let opts: Options = syn::parse2(attr)?;
    t.step("options");
    let head = scan::head(&item)?;
    t.subject(|| format!("{:?} {}", head.kind, head.ident));
    t.step("scan");
    // The helper attributes of a derive join this announcement, and its one query.
    let (opts, item) = match runner::take_over(opts, &head.ident, item) {
        Ok(taken) => taken,
        Err(error) => return Ok(error),
    };
    t.step("takeover");
    let attr = opts.to_token_stream();
    let random = random_u64();
    // The query of the checks, which ends with the announcement (`finish`), or the
    // announcement.
    let out = if opts.checks.is_empty() {
        announce_now(&opts, &attr, item, random, &OwnedAttrs::new(), &head)
    } else {
        finish::query(
            &opts,
            finish::Mode::Attribute,
            &attr,
            &item,
            random,
            &opts.checks,
            &OwnedAttrs::new(),
        )
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
        return capture_output(opts, attr, item, random, owned);
    }
    runner::announce(opts, &head.ident, item, owned)
}

/// What an announcement expands to when it is made (see `runner`), once the attribute
/// names that its checks own are known (`owned`; empty without checks): the item and the
/// capture dummy.
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

/// What the runner `#[<crate path>::__private::_actually_run_announce(args)]` does to
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

/// Does what `#[derive(Announce)]` does to its input, for a derive macro of your own
/// (the `#[derive(Announce)]` itself goes a little further, see below).
///
/// `input` is the struct, enum or union, including its attributes. Options come
/// from its `#[announce(..)]` helper attributes (several are merged), which are
/// removed from the announced copy; `mock` and `emit_item` are errors. The result
/// is what the derive emits: the trampoline and the `Announce` impl. The item itself
/// is not part of it, because a derive does not emit its input.
///
/// If the input has an announcement left to expand below the derive, the result is
/// empty: a derive expands before the attribute macros below it, and the one below
/// announces the item, with the options of the helper attribute that is left (it takes it
/// over). An announcement is the hidden attribute `_actually_run_announce` that
/// `#[announce]` puts last among the attributes, an `#[x::announce]` attribute, or another
/// `#[derive(Announce)]`. This is the one place where a path tells an announcement.
///
/// A library whose own derive macro calls this must list `announce` among the
/// derive's helper attributes (`#[proc_macro_derive(Mine, attributes(announce))]`)
/// if its users are to write options, or use [`derive_announce_with`]. Unlike
/// `#[derive(Announce)]`, this does not warn that `#[announce]` should be written.
///
/// Errors come back as a `compile_error!` invocation; see [`announce()`] and
/// [`try_derive_announce`].
pub fn derive_announce(input: TokenStream) -> TokenStream {
    try_derive_announce(input).unwrap_or_else(|e| e.to_compile_error())
}

/// [`derive_announce()`] with the error returned as a [`syn::Error`].
pub fn try_derive_announce(input: TokenStream) -> syn::Result<TokenStream> {
    derive_run(input).1
}

/// The body of `#[derive(Announce)]`. Not part of the API: it is
/// [`derive_announce()`] and a warning that `#[announce]` should be written.
#[doc(hidden)]
pub fn derive_entry(input: TokenStream) -> TokenStream {
    let (at, result) = derive_run(input);
    let mut out = result.unwrap_or_else(|e| e.to_compile_error());
    out.extend(deprecation_warning(at));
    out
}

/// The warning that a derive written by the user is deprecated. Stable Rust has no
/// warnings for proc macros, so a deprecated item is used.
///
/// The tokens have the span `at`, the name of the item: rustc does not report the use of a
/// deprecated item in a span that comes from the expansion of a derive (a derive on a
/// deprecated type would warn), so `Span::call_site()` would show nothing. The warning
/// is at the name of the item, and a module (or a function) can `#[allow(deprecated)]`.
fn deprecation_warning(at: Span) -> TokenStream {
    const NOTE: &str =
        "write `#[announce]` on the item instead of `#[derive(Announce)]`, with the \
         options of the helper attribute `#[announce(..)]` as its arguments (several announcements \
         of one item are merged either way)";
    fn respan(ts: TokenStream, at: Span) -> TokenStream {
        use proc_macro2::{Group, TokenTree};
        ts.into_iter()
            .map(|t| match t {
                TokenTree::Group(g) => {
                    let mut group = Group::new(g.delimiter(), respan(g.stream(), at));
                    group.set_span(at);
                    TokenTree::Group(group)
                }
                mut t => {
                    t.set_span(at);
                    t
                }
            })
            .collect()
    }
    respan(
        quote! {
            const _: () = {
                #[deprecated(note = #NOTE)]
                struct DeriveAnnounce;
                let _ = DeriveAnnounce;
            };
        },
        at,
    )
}

/// [`derive_announce()`], with the span of the name of the item (for the warning;
/// `Span::call_site()` if there is none).
fn derive_run(input: TokenStream) -> (Span, syn::Result<TokenStream>) {
    let mut t = timing::Timer::new("derive");
    let item: syn::Item = match syn::parse2(input) {
        Ok(item) => item,
        Err(e) => return (Span::call_site(), Err(e)),
    };
    t.step("parse");
    // Refuses what is not a struct, enum or union, before the name is read.
    if !matches!(
        item,
        syn::Item::Struct(_) | syn::Item::Enum(_) | syn::Item::Union(_)
    ) {
        return (Span::call_site(), Err(unsupported_derive_item(&item)));
    }
    let name = match generate::ident_vis(&item) {
        Ok((name, _)) => name,
        Err(e) => return (Span::call_site(), Err(e)),
    };
    (name.span(), user_derive(item, name, t))
}

/// A `#[derive(Announce)]`.
fn user_derive(mut item: syn::Item, name: Ident, mut t: timing::Timer) -> syn::Result<TokenStream> {
    let attrs = derive_attrs(&mut item)?;
    // The helper attributes are one announcement (a macro that announces what it
    // produces can add one to the user's). They must agree on the trampoline name.
    // Checked before the derive stands aside: whatever happens to the item, the options
    // that cannot be given to a derive are an error.
    let mut opts: Option<Options> = None;
    for a in attrs.iter().filter(|a| a.path().is_ident("announce")) {
        let next: Options = match &a.meta {
            syn::Meta::List(l) => syn::parse2(l.tokens.clone())?,
            _ => Options::default(),
        };
        for key in ["mock", "emit_item"] {
            if next.given.iter().any(|g| g == key) {
                return Err(derive_option_error(key));
            }
        }
        opts = Some(match opts {
            None => next,
            Some(prev) => prev
                .merge(&next, &name)
                .map_err(|c| syn::Error::new_spanned(a, c.message()))?,
        });
    }
    // Another announcement below this derive makes the trampoline (it expands
    // after the derive, and takes the helper attributes that are left over).
    if runner::derive_defers(attrs) {
        return Ok(TokenStream::new());
    }
    t.step("options");
    finish_derive(item, opts.unwrap_or_default(), t)
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

/// The error for an item that a derive is not put on.
fn unsupported_derive_item(item: &syn::Item) -> syn::Error {
    syn::Error::new_spanned(
        item,
        "`#[derive(Announce)]` supports struct, enum and union items",
    )
}

/// The attributes of a struct, enum or union; error for any other item.
fn derive_attrs(item: &mut syn::Item) -> syn::Result<&mut Vec<syn::Attribute>> {
    match item {
        syn::Item::Struct(s) => Ok(&mut s.attrs),
        syn::Item::Enum(e) => Ok(&mut e.attrs),
        syn::Item::Union(u) => Ok(&mut u.attrs),
        other => Err(unsupported_derive_item(other)),
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
        &opts.checks,
        &OwnedAttrs::new(),
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
            assert!(!out.contains("_actually_run_announce"), "{}", out);
        }
        // An item with another attribute has the runner as its last attribute, in the
        // given crate, and nothing else.
        let out = norm(&announce(
            ts("crate = ::l::announce"),
            ts("#[derive(Debug)] pub struct S { pub a: u8 }"),
        ));
        assert_eq!(
            out,
            "#[derive(Debug)]#[::l::announce::__private::_actually_run_announce(_announce_format_version=\"1\",crate=::l::announce)]pubstructS{puba:u8}"
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
        // The attribute hands the option to the capture derive, and to the runner.
        let out = norm(&announce(ts("layout = single"), ts(item)));
        assert!(out.contains("#[__announce("), "{}", out);
        assert!(out.contains("layout=single"), "{}", out);
        let out = norm(&announce(
            ts("layout = single"),
            ts(&format!("#[x] {}", item)),
        ));
        assert!(
            out.contains("_actually_run_announce(_announce_format_version=\"1\",layout=single)"),
            "{}",
            out
        );
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
            out.contains(";attr(crate=::l::announce,meta=m::info,a::check,b::check(x)){pubstructS{puba:u8}}]}}"),
            "{}",
            out
        );
        // Nothing else yet: no item, no runner, no capture dummy.
        assert!(
            !out.contains("Capture")
                && !out.contains("_actually_run_announce")
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
        // Another attribute: the runner, which carries the options.
        let out = norm(&finish(finish_input(
            "attr",
            "crate=::l::announce,a::check",
            &format!("#[x] {}", item),
            "",
        )));
        assert_eq!(
            out,
            "#[x]#[::l::announce::__private::_actually_run_announce(_announce_format_version=\"1\",crate=::l::announce,a::check)]pubstructS{puba:u8}"
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
        // With another attribute there is the runner, which has the names. The attribute
        // that is owned follows it.
        let out = norm(&finish(finish_input(
            "attr",
            "crate=::l::announce,a::check",
            &format!("#[my_lib(on_item)] #[x] {}", item),
            "attrs(my_lib) attrs(other, my_lib)",
        )));
        assert_eq!(
            out,
            "#[x]#[::l::announce::__private::_actually_run_announce(_announce_format_version=\"1\",_announce_owned(my_lib,other),crate=::l::announce,a::check)]\
             #[my_lib(on_item)]pubstructS{#[my_lib(skip)]puba:u8}"
        );
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
                            .map_or(false, |l| l == "_actually_run_announce")
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
                .map_or(false, |l| l == "_actually_run_announce");
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
            first.starts_with("a::check!{@__announce_attrs"),
            "{}",
            first
        );
        let runner = norm(&finish(finish_input(
            "attr",
            "crate=::l::announce,a::check",
            "#[other] #[my_lib(x)] pub struct S;",
            "attrs(my_lib)",
        )));
        assert_eq!(
            runner,
            "#[other]#[::l::announce::__private::_actually_run_announce(_announce_format_version=\"1\",_announce_owned(my_lib),crate=::l::announce,a::check)]#[my_lib(x)]pubstructS;"
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
        assert!(!end.contains("_actually_run_announce"), "{}", end);
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
    fn the_runner_asks_only_the_checks_nobody_asked() {
        // The runner has asked `a::check` and knows `my_lib`. A helper attribute of a
        // derive has a check of its own that nobody has asked: only that one gets the
        // query, and the answer of `a::check` is in the state already.
        let args = "_announce_format_version = \"1\", _announce_owned(my_lib), crate = ::l::announce, a::check";
        let out = norm(&run_announce(
            ts(args),
            ts("#[announce(b::check)] pub struct S;"),
        ));
        assert!(
            out.starts_with(
                "b::check!{@__announce_attrs[b::check!,::l::announce::__private::finish!]{["
            ),
            "{}",
            out
        );
        assert_eq!(out.matches("@__announce_attrs").count(), 1, "{}", out);
        assert!(out.ends_with("}]attrs(my_lib)}}"), "{}", out);
        // The helper is gone from the item, and both checks are in the state.
        assert!(
            out.contains(";run(crate=::l::announce,a::check,b::check){pubstructS;}]"),
            "{}",
            out
        );
        // The end of the query: the trampoline.
        let end = norm(&finish(finish_input(
            "run",
            "crate=::l::announce,a::check,b::check",
            "pub struct S;",
            "attrs(my_lib) attrs(other)",
        )));
        assert!(
            end.starts_with("pubstructS;#[derive(")
                || end.starts_with("#[derive(::l::announce::__private::Capture)]"),
            "{}",
            end
        );
        assert!(
            end.contains("[my_lib,other];crate=::l::announce,a::check,b::check)]"),
            "{}",
            end
        );
        // A helper with a check that was asked already asks nothing.
        let out = norm(&run_announce(
            ts(args),
            ts("#[announce(a::check)] pub struct S;"),
        ));
        assert!(
            !out.contains("__announce_attrs") && out.contains("Capture"),
            "{}",
            out
        );
        assert!(
            out.contains("[my_lib];crate=::l::announce,a::check)]"),
            "{}",
            out
        );
    }

    #[test]
    fn an_item_that_is_not_emitted_goes_through_the_runner_as_well() {
        for opt in ["emit_item = false", "mock = a::B"] {
            let attr = format!("crate = ::l::announce, {}", opt);
            // Another attribute: the runner, which holds the option and drops the item
            // when it expands; the macros below see the item until then.
            let out = norm(&announce(ts(&attr), ts("#[derive(Debug)] pub struct S;")));
            let want = format!(
                "#[derive(Debug)]#[::l::announce::__private::_actually_run_announce(_announce_format_version=\"1\",crate=::l::announce,{})]pubstructS;",
                opt.replace(' ', "")
            );
            assert_eq!(out, want);
            // No other attribute: nothing to wait for, the dummy alone (the item is not
            // emitted).
            let out = norm(&announce(ts(&attr), ts("pub struct S;")));
            assert!(
                out.starts_with("#[derive(::l::announce::__private::Capture)]")
                    && !out.contains("_actually_run_announce"),
                "{}",
                out
            );
            assert_eq!(out.matches("pubstructS").count(), 1, "{}", out);
            // The runner makes the trampoline and drops the item.
            let args = format!("_announce_format_version = \"1\", {}", attr);
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
            assert!(out.starts_with("a::check!{@__announce_attrs"), "{}", out);
            let end = norm(&finish(finish_input(
                "attr",
                &format!("crate=::l::announce,{},a::check", opt.replace(' ', "")),
                "#[derive(Debug)] pub struct S;",
                "attrs(my_lib)",
            )));
            assert!(
                end.ends_with("pubstructS;")
                    && end.contains("_actually_run_announce(_announce_format_version=\"1\",_announce_owned(my_lib),"),
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
                "#[::l::announce::__private::_actually_run_announce(_announce_format_version = \"1\"{})]",
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
        let args = "_announce_format_version = \"1\"";
        // Unset is the name of the item: `Parse!` and `ParseAlt!` are two names.
        let item = "#[announce(macro_name = ParseAlt)] pub trait Parse {}";
        let out = norm(&run_announce(ts(args), ts(item)));
        // The helper is taken over by the runner, whose name is the name of the item.
        assert!(out.starts_with("::core::compile_error!"), "{}", out);
        assert_eq!(out.matches("compile_error").count(), 1, "{}", out);
        assert!(out.contains("(`ParseAlt`and`Parse`)"), "{}", out);
        assert!(out.contains("onetrampoline"), "{}", out);
        assert!(out.ends_with(&norm(&ts(item))), "{}", out);
        // The same name, set in one of them or in both, is one trampoline.
        for (args, item) in [
            ("macro_name = Parse", "#[announce] pub trait Parse {}"),
            ("", "#[announce(macro_name = Parse)] pub trait Parse {}"),
            (
                "macro_name = ParseAlt",
                "#[announce(macro_name = ParseAlt)] pub trait Parse {}",
            ),
        ] {
            let mut src = "_announce_format_version = \"1\", crate = ::l::announce".to_string();
            if !args.is_empty() {
                src = format!("{}, {}", src, args);
            }
            let out = norm(&run_announce(ts(&src), ts(item)));
            assert!(!out.contains("compile_error"), "{}: {}", item, out);
            assert_eq!(out.matches("Capture").count(), 1, "{}", out);
        }
        // An announcement merged into the runner has the same rule.
        let out = norm(&announce(
            ts("macro_name = Other"),
            ts("#[x] #[::l::announce::__private::_actually_run_announce(_announce_format_version = \"1\")] pub trait Parse {}"),
        ));
        assert!(out.starts_with("::core::compile_error!"), "{}", out);
        assert!(out.contains("(`Other`and`Parse`)"), "{}", out);
    }

    #[test]
    fn derive_merges_its_helper_attributes() {
        let out = norm(&derive_announce(ts(
            "#[announce(crate = ::l::announce, meta = a::x)] #[announce(meta = b::x)] pub struct S(pub u8);",
        )));
        assert!(
            out.contains("[a::x,b::x]") || out.contains("a::x,b::x"),
            "{}",
            out
        );
        assert_eq!(
            out.matches("macro_rules!__announce_S_").count(),
            1,
            "{}",
            out
        );
        let e = try_derive_announce(ts(
            "#[announce(macro_name = A)] #[announce(macro_name = A, meta = m::x)] #[announce(macro_name = B)] struct S;",
        ))
        .err()
        .unwrap();
        assert!(
            e.to_string()
                .contains("announced twice with different trampoline names (`B` and `A`)"),
            "{}",
            e
        );
    }

    #[test]
    fn derive_helpers_must_agree_on_the_trampoline_name() {
        // Unset is the name of the item.
        for src in [
            "#[announce(macro_name = Alt)] #[announce(meta = m::x)] struct S;",
            "#[announce] #[announce(macro_name = Alt)] struct S;",
            "#[announce(macro_name = Alt)] #[announce] struct S;",
        ] {
            let e = try_derive_announce(ts(src)).err().unwrap();
            let msg = e.to_string();
            assert!(
                msg.contains("different trampoline names") && msg.contains("`Alt`"),
                "{}: {}",
                src,
                msg
            );
        }
        // Equal effective names merge, and the explicit one is kept.
        for src in [
            "#[announce(macro_name = S)] #[announce(meta = m::x)] struct S;",
            "#[announce(meta = m::x)] #[announce(macro_name = S)] struct S;",
            "#[announce(macro_name = Alt)] #[announce(macro_name = Alt)] struct S;",
        ] {
            let out = norm(&try_derive_announce(ts(src)).unwrap());
            assert_eq!(out.matches("macro_rules!").count(), 1, "{}: {}", src, out);
        }
        let out = norm(&derive_announce(ts(
            "#[announce(meta = m::x)] #[announce(macro_name = Alt)] struct S;",
        )));
        assert!(out.contains("compile_error"), "{}", out);
    }

    #[test]
    fn derive_leaves_the_announcement_to_the_attribute_below() {
        for below in [
            // The runner, whatever its path.
            "#[::announce::__private::_actually_run_announce(_announce_format_version = \"1\")]",
            "#[x::y::_actually_run_announce]",
            // An attribute that has not expanded yet is seen by its path.
            "#[x::announce(meta = b::x)]",
            "#[::announce::announce]",
            "#[derive(Debug, Announce)]",
        ] {
            let src = format!("#[announce(meta = a::x)] {} pub struct S(pub u8);", below);
            assert!(derive_announce(ts(&src)).is_empty(), "{}", src);
        }
        // Its own helper alone does not make it wait.
        assert!(!derive_announce(ts("#[announce(meta = a::x)] struct S;")).is_empty());
        // An attribute of another crate that is named like it, but not qualified, is the
        // helper of the derive, as before; a name that only contains `announce` or
        // `_actually_run_announce` is another attribute.
        assert!(!derive_announce(ts("#[announce(meta = a::x)] #[announced] struct S;")).is_empty());
        assert!(!derive_announce(ts(
            "#[x::run_announce] #[_actually_run_announced] struct S;"
        ))
        .is_empty());
        // The renamed import is not seen.
        assert!(!derive_announce(ts("#[ann] struct S;")).is_empty());
        // A derive of the library API does not wait: the options are its own.
        let opts = Options {
            krate: Some(syn::parse_quote!(::l::announce)),
            ..Options::default()
        };
        let out = derive_announce_with(opts, ts("#[x::announce] struct S;")).unwrap();
        assert!(!out.is_empty());
    }

    fn entry(src: &str) -> String {
        norm(&derive_entry(ts(src)))
    }

    #[test]
    fn only_the_derive_macro_warns() {
        let src = "pub struct S(pub u8);";
        let out = entry(src);
        assert!(out.contains("macro_rules!"), "{}", out);
        assert!(
            out.contains("deprecated(note=") && out.contains("DeriveAnnounce"),
            "{}",
            out
        );
        // The functions of the library do not warn.
        assert!(!norm(&derive_announce(ts(src))).contains("deprecated"));
        assert!(!norm(&try_derive_announce(ts(src)).unwrap()).contains("deprecated"));
        let opts = Options::default();
        assert!(!norm(&derive_announce_with(opts, ts(src)).unwrap()).contains("deprecated"));
    }

    #[test]
    fn a_derive_that_stands_aside_still_warns() {
        for below in [
            "#[::announce::announce]",
            "#[x::announce]",
            "#[derive(Announce)]",
        ] {
            let out = entry(&format!("{} struct S;", below));
            assert!(!out.contains("macro_rules"), "{}", out);
            assert!(out.contains("DeriveAnnounce"), "{}", out);
        }
    }

    #[test]
    fn the_warning_has_the_span_of_the_name_of_the_item() {
        // A span from a derive's own expansion would silence rustc's `deprecated` lint;
        // the name of the item comes from the user's code.
        let input = ts("pub struct S(pub u8);");
        let name_span = match input.clone().into_iter().nth(2) {
            Some(proc_macro2::TokenTree::Ident(i)) => i.span(),
            other => panic!("{:?}", other),
        };
        let out = derive_entry(input);
        let warning: Vec<_> = out
            .into_iter()
            .skip_while(|t| !matches!(t, proc_macro2::TokenTree::Ident(i) if i == "const"))
            .collect();
        assert!(!warning.is_empty());
        for t in warning {
            // Outside a proc macro the spans compare equal for all of them.
            assert_eq!(format!("{:?}", t.span()), format!("{:?}", name_span));
        }
    }

    #[test]
    fn a_user_derive_is_checked_before_it_stands_aside() {
        // An announcement below it will make the trampoline, but the options that a
        // derive cannot take are an error all the same (the item was lost before).
        for below in [
            "#[::announce::announce]",
            "#[x::announce]",
            "#[derive(Announce)]",
        ] {
            let src = format!("#[announce(emit_item = false)] {} struct S;", below);
            let e = try_derive_announce(ts(&src)).err().unwrap();
            assert!(
                e.to_string().contains("`emit_item` does not apply"),
                "{}: {}",
                src,
                e
            );
            let e = try_derive_announce(ts(&src.replace("emit_item = false", "mock = a::B")))
                .err()
                .unwrap();
            assert!(e.to_string().contains("`mock` does not apply"), "{}", e);
        }
        // Options that are not ours are an error as well, instead of nothing.
        let e = try_derive_announce(ts(
            "#[announce(nope = 1)] #[::announce::announce] struct S;",
        ))
        .err()
        .unwrap();
        assert!(e.to_string().contains("unknown announce option"), "{}", e);
        // And the warning is there with the error.
        let out = entry("#[announce(mock = a::B)] #[::announce::announce] struct S;");
        assert!(
            out.starts_with("::core::compile_error!") && out.contains("DeriveAnnounce"),
            "{}",
            out
        );
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
        for item in ["pub struct S { pub a: }", "pub trait T { fn }"] {
            let out = norm(&announce(TokenStream::new(), ts(item)));
            assert!(
                (out.contains("Capture") || out.contains("_actually_run_announce"))
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
