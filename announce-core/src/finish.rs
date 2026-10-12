//! The attribute query round trip of `#[announce(check, ..)]` (DESIGN.md section
//! 4.7): the query that `#[announce]` sends through the checks, and `finish`, the
//! macro that ends it.
//!
//! The protocol that the checks see is documented in `announce_helper::attrs`.
//! This is the part around it: the state that travels in `{ STATE }` and what
//! `finish` does with it.
//!
//! ```text
//! STATE := '[' RANDOM 'u64' ';' MODE '(' OPTIONS ')' '{' ITEM '}' ']'  ('attrs' '(' IDENT,* ')')*
//! MODE  := 'attr' | 'derive'
//! ```
//!
//! The first group is written by `#[announce]` (or by the derive) and is only
//! read by `finish`. The `attrs(..)` groups are what the checks add.

use crate::capture_output;
use crate::options::Options;
use announce_helper::{OwnedAttrs, Query};
use proc_macro2::{Ident, Span, TokenStream};
use syn::parse::{Parse, ParseStream};
use syn::{Result, Token};
use template_quote::quote;

/// Which expansion started the query.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Mode {
    /// `#[announce(..)]`: `finish` emits the item.
    Attribute,
    /// `#[derive(Announce)]`: the user's item is already there.
    Derive,
}

impl Mode {
    fn word(self) -> &'static str {
        match self {
            Mode::Attribute => "attr",
            Mode::Derive => "derive",
        }
    }
}

const STATE_ERROR: &str = "announce: the state of the attribute query was changed by a check. \
     A check must forward `{ STATE }` unchanged and may only append `attrs(name, ..)`; see the \
     `announce_helper::attrs` docs";

/// The call of the first check, with the query: the expansion of
/// `#[announce(a, b, ..)]` when there are checks. `opts.checks` is not empty.
pub(crate) fn query(
    opts: &Options,
    mode: Mode,
    attr: &TokenStream,
    item: &TokenStream,
    random: u64,
) -> TokenStream {
    let krate = opts.krate();
    let lit = syn::LitInt::new(&format!("{}u64", random), Span::call_site());
    let mode = Ident::new(mode.word(), Span::call_site());
    let mut paths = opts.checks.iter().map(|c| &c.path);
    let first = paths.next().expect("a query needs a check");
    let rest = paths.map(|p| quote! { #p ! , }).collect::<Vec<_>>();
    quote! {
        #first ! {
            @__announce_attrs
            [ #(#rest)* #krate::__private::finish! ]
            { [ #lit ; #mode ( #attr ) { #item } ] }
        }
    }
}

struct State {
    random: u64,
    mode: Mode,
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
        let mode: Ident = core.parse()?;
        let mode = match mode.to_string().as_str() {
            "attr" => Mode::Attribute,
            "derive" => Mode::Derive,
            _ => return Err(syn::Error::new(mode.span(), STATE_ERROR)),
        };
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
            mode,
            attr,
            item,
            owned,
        })
    }
}

/// What the hidden macro `finish!` of the `announce` crate does: the end of the
/// attribute query that `#[announce(check, ..)]` and the derives send through
/// their checks (see `announce_helper::attrs` for the protocol).
///
/// `input` is the call's input, `@__announce_attrs [] { STATE }`, where `STATE`
/// is what `#[announce]` wrote and the checks added their `attrs(..)` to. The
/// result is what `#[announce]` expands to without checks: the item and the
/// capture dummy, or, for a derive, the trampoline and the other generated items.
/// The item the compiler sees has no attribute that a check owns; the payload and
/// the checks do.
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
    if !query.list.0.is_empty() {
        return Err(syn::Error::new(
            Span::call_site(),
            "announce: the attribute query reached `finish!` with elements left in its list; \
             a check must forward the list it received without its first element",
        ));
    }
    let state: State = syn::parse2(query.state)?;
    state.owned.check_names()?;
    let opts: Options = syn::parse2(state.attr.clone())?;
    // `#[announce]` checked the item before it started the query; a check could
    // have damaged it since.
    crate::generate::ident_vis(&syn::parse2(state.item.clone())?)?;
    match state.mode {
        Mode::Attribute => Ok(capture_output(
            &opts,
            &state.attr,
            state.item,
            state.random,
            &state.owned,
        )),
        Mode::Derive => {
            let item: syn::Item = syn::parse2(state.item)?;
            crate::finish_derive_now(item, opts, state.random, &state.owned)
        }
    }
}
