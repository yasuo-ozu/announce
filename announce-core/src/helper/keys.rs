//! Readers for the `name(key = value, ..)` META elements that announce writes
//! ([`TypeLeak`](crate::helper::TypeLeak), [`LeakFailure`](crate::helper::LeakFailure)). A
//! reader skips the keys it does not know, so a later version can add keys with any value.

use proc_macro2::{Ident, TokenTree};
use syn::ext::IdentExt;
use syn::parse::{Parse, ParseStream};
use syn::punctuated::Punctuated;
use syn::{Error, Result, Token};

/// Reads `name(key = value, ..)`. `value` reads the value of a key and tells whether it knows
/// the key; a key it does not know, with or without a value, is skipped.
pub(crate) fn read_element(
    input: ParseStream,
    name: &str,
    value: impl FnMut(&str, ParseStream) -> Result<bool>,
) -> Result<Ident> {
    let ident: Ident = input.parse()?;
    if ident != name {
        let found = format!("expected `{}(..)`, found `{}`", name, ident);
        return Err(Error::new(ident.span(), found));
    }
    let args;
    syn::parenthesized!(args in input);
    read_keys(&args, value)?;
    Ok(ident)
}

/// Reads `key = value, ..` up to the end of `input`, as [`read_element`] does.
pub(crate) fn read_keys(
    input: ParseStream,
    mut value: impl FnMut(&str, ParseStream) -> Result<bool>,
) -> Result<()> {
    while !input.is_empty() {
        let key = Ident::parse_any(input)?.to_string();
        if input.parse::<Option<Token![=]>>()?.is_none() || !value(&key, input)? {
            skip_to_comma(input)?;
        }
        if !input.is_empty() {
            input.parse::<Token![,]>()?;
        }
    }
    Ok(())
}

/// The value of `key`, or the error for a missing one.
pub(crate) fn need<T>(value: Option<T>, element: &Ident, key: &str) -> Result<T> {
    value.ok_or_else(|| {
        let missing = format!("missing `{}` in the `{}(..)` META", key, element);
        Error::new(element.span(), missing)
    })
}

/// A bracketed list of `T`.
pub(crate) fn bracketed_list<T: Parse>(input: ParseStream) -> Result<Vec<T>> {
    let content;
    syn::bracketed!(content in input);
    let list = Punctuated::<T, Token![,]>::parse_terminated(&content)?;
    Ok(list.into_iter().collect())
}

/// Skips up to the next `,` outside groups and `<..>`.
pub(crate) fn skip_to_comma(input: ParseStream) -> Result<()> {
    let (mut depth, mut arrow) = (0usize, false);
    while !input.is_empty() && !(depth == 0 && input.peek(Token![,])) {
        let c = match input.parse::<TokenTree>()? {
            TokenTree::Punct(p) => Some(p.as_char()),
            _ => None,
        };
        match c {
            Some('<') => depth += 1,
            Some('>') if !arrow => depth = depth.saturating_sub(1),
            _ => {}
        }
        arrow = c == Some('-');
    }
    Ok(())
}
