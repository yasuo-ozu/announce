//! The attribute query round trip of `#[announce(check, ..)]`: the query that
//! `#[announce]` sends through the checks, and `finish`, the macro that ends it.
//!
//! The protocol that the checks see is documented in [`crate::helper::attrs`].
//! This is the part around it: the state that travels in `{ STATE }` and what
//! `finish` does with it.
//!
//! The list in the query is headed by the macro that is called, so `finish!`
//! always receives `[ finish! ]`:
//!
//! ```text
//! a::check! { @__announce_attrs [ a::check!, b::check!, KRATE::__private::finish! ] { STATE } }
//! b::check! { @__announce_attrs [ b::check!, KRATE::__private::finish! ] { STATE attrs(..) } }
//! KRATE::__private::finish! { @__announce_attrs [ KRATE::__private::finish! ] { STATE attrs(..) } }
//! ```
//!
//! ```text
//! STATE := '[' RANDOM 'u64' ';' 'attr' '(' OPTIONS ')' '{' ITEM '}' ']'  ('attrs' '(' IDENT,* ')')*
//! ```
//!
//! The first group is written by `#[announce]` and is only read by `finish`. The
//! `attrs(..)` groups are what the checks add.

use crate::helper::{OwnedAttrs, Query};
use crate::options::{Check, Options};
use proc_macro2::{Ident, Span, TokenStream};
use syn::parse::{Parse, ParseStream};
use syn::{Result, Token};
use template_quote::quote;

/// The word that tags the state of the query that `#[announce]` starts.
const TAG: &str = "attr";

const STATE_ERROR: &str = "announce: the state of the attribute query was changed by a check. \
     A check must forward `{ STATE }` unchanged and may only append `attrs(name, ..)`; see the \
     `announce::helper::attrs` docs";

/// The call of the first check, with the query: the expansion of
/// `#[announce(a, b, ..)]` when there are checks. `ask`, the checks to ask, is not empty:
/// it is `opts.checks`.
///
/// The list is headed by the macro that is called, like a continuation list:
/// `a! { @__announce_attrs [ a!, b!, KRATE::__private::finish! ] { STATE } }`.
pub(crate) fn query(
    opts: &Options,
    attr: &TokenStream,
    item: &TokenStream,
    random: u64,
    ask: &[Check],
) -> TokenStream {
    let krate = opts.krate();
    let lit = syn::LitInt::new(&format!("{}u64", random), Span::call_site());
    let tag = Ident::new(TAG, Span::call_site());
    let paths: Vec<&syn::Path> = ask.iter().map(|c| &c.path).collect();
    // `ask` is not empty: with nothing to ask there is no query (`try_announce`).
    let first = paths.first().expect("a query needs a check");
    quote! {
        #first ! {
            @__announce_attrs
            [ #(#paths ! ,)* #krate::__private::finish! ]
            { [ #lit ; #tag ( #attr ) { #item } ] }
        }
    }
}

struct State {
    random: u64,
    attr: TokenStream,
    item: TokenStream,
    owned: OwnedAttrs,
}

impl Parse for State {
    fn parse(input: ParseStream) -> Result<Self> {
        let core;
        syn::bracketed!(core in input);
        let lit: syn::LitInt = core.parse()?;
        core.parse::<Token![;]>()?;
        let tag: Ident = core.parse()?;
        if tag != TAG {
            return Err(syn::Error::new(tag.span(), STATE_ERROR));
        }
        let attr;
        syn::parenthesized!(attr in core);
        let attr: TokenStream = attr.parse()?;
        let item;
        syn::braced!(item in core);
        let item: TokenStream = item.parse()?;
        if !core.is_empty() {
            return Err(core.error(STATE_ERROR));
        }
        let mut owned = OwnedAttrs::new();
        while !input.is_empty() {
            let key: Ident = input
                .parse()
                .map_err(|e| syn::Error::new(e.span(), STATE_ERROR))?;
            if key != "attrs" || !input.peek(syn::token::Paren) {
                return Err(syn::Error::new(key.span(), STATE_ERROR));
            }
            let names;
            syn::parenthesized!(names in input);
            let names = OwnedAttrs::parse_names(names.parse()?)?;
            owned.extend(names.names().iter().cloned());
        }
        Ok(State {
            random: lit.base10_parse()?,
            attr,
            item,
            owned,
        })
    }
}

/// What the hidden macro `finish!` of the `announce` crate does: the end of the
/// attribute query that `#[announce(check, ..)]` sends through its checks (see
/// [`crate::helper::attrs`] for the protocol).
///
/// `input` is the call's input, `@__announce_attrs [ finish! ] { STATE }` (the
/// list is headed by the macro that is called, `finish!` itself, and has nothing
/// else), where `STATE` is what `#[announce]` wrote and the checks added their
/// `attrs(..)` to. The result is what `#[announce]` expands to without checks: the
/// item with the runner (see `announce::announce`, "Several announcements on one item"),
/// or the item and the capture dummy. The item the compiler sees in the end has no
/// attribute that a check owns; the payload and the checks do.
///
/// A library does not call this: its query ends at the macro of the `announce`
/// crate (`<crate path>::__private::finish!`, reached through the `crate = ..`
/// option), which calls it. It is public with the rest of the machinery, to
/// test or to wrap that macro. An error comes back as a `compile_error!`
/// invocation; see [`try_finish`] for a [`syn::Error`].
pub fn finish(input: TokenStream) -> TokenStream {
    try_finish(input).unwrap_or_else(|e| e.to_compile_error())
}

/// [`finish()`] with the error returned as a [`syn::Error`].
pub fn try_finish(input: TokenStream) -> Result<TokenStream> {
    let query: Query = syn::parse2(input).map_err(|e| {
        syn::Error::new(
            e.span(),
            format!(
                "announce: `finish!` is the end of the attribute query that `#[announce]` \
                 sends through its checks, and cannot be called by hand ({})",
                e
            ),
        )
    })?;
    // The list is headed by the macro that is called, and `finish!` is the last
    // element: exactly `[ finish! ]`.
    match query.list.0.as_slice() {
        [crate::helper::Cont::Bang(_)] => {}
        [] => {
            return Err(syn::Error::new(
                Span::call_site(),
                "announce: the attribute query reached `finish!` with an empty list; the list \
                 starts with the macro that is called, so a check must forward its list \
                 without its own first element, and keep the rest (`[ finish! ]` at the end)",
            ))
        }
        _ => {
            return Err(syn::Error::new(
                Span::call_site(),
                "announce: the attribute query reached `finish!` with elements left in its list \
                 (it must be exactly `[ finish! ]`); a check must forward the list it received \
                 without its own first element",
            ))
        }
    }
    let mut t = crate::timing::Timer::new("finish");
    let state: State = syn::parse2(query.state)?;
    state.owned.check_names()?;
    let opts: Options = syn::parse2(state.attr.clone())?;
    t.step("state");
    // `#[announce]` checked the kind of the item before it started the query; a check
    // could have damaged it since. Its syntax is checked by the capture derive.
    let head = crate::scan::head(&state.item)?;
    t.step("scan");
    // The query is answered, so what the checks own is known: this is where the
    // announcement is made (see `announce_now`).
    Ok(crate::announce_now(
        &opts,
        &state.attr,
        state.item,
        state.random,
        &state.owned,
        &head,
    ))
}
