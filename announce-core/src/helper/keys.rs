//! Readers for the `key = value` lists that the definer writes in META: the records
//! [`LeakFailure`](crate::helper::LeakFailure) and [`TypeLeak`](crate::helper::TypeLeak) parse.
//! A reader takes the keys it knows and skips the rest, so a later version can add keys with any
//! value.

use proc_macro2::{Ident, TokenTree};
use syn::ext::IdentExt;
use syn::parse::{Parse, ParseStream};
use syn::punctuated::Punctuated;
use syn::{Error, Result, Token};

/// Reads `key = value, ..` (a trailing comma is fine) up to the end of `input`. `value` reads
/// the value of a key in `known`; the value of any other key is skipped ([`skip_to_comma`]), and
/// so is a bare key (a flag), because a later version may add keys with any value.
pub(crate) fn read_keys(
    input: ParseStream,
    known: &[&str],
    mut value: impl FnMut(&Ident, ParseStream) -> Result<()>,
) -> Result<()> {
    while !input.is_empty() {
        let key = Ident::parse_any(input)?;
        let is_known = known.iter().any(|k| key == *k);
        if input.peek(Token![=]) && !input.peek(Token![==]) {
            input.parse::<Token![=]>()?;
            if is_known {
                value(&key, input)?;
            } else {
                skip_to_comma(input)?;
            }
        } else if is_known {
            return Err(Error::new(key.span(), format!("`{}` needs a value", key)));
        }
        if input.is_empty() {
            break;
        }
        input.parse::<Token![,]>()?;
    }
    Ok(())
}

/// Sets `slot` to the value of `key`, which may be given once.
pub(crate) fn once<T>(slot: &mut Option<T>, key: &Ident, value: T) -> Result<()> {
    match slot.replace(value) {
        None => Ok(()),
        Some(_) => Err(Error::new(key.span(), format!("duplicate key `{}`", key))),
    }
}

/// A bracketed list of `T`.
pub(crate) fn bracketed_list<T: Parse>(input: ParseStream) -> Result<Vec<T>> {
    let content;
    syn::bracketed!(content in input);
    Ok(Punctuated::<T, Token![,]>::parse_terminated(&content)?
        .into_iter()
        .collect())
}

/// Skips what a later version adds (the value of a key, tokens after an error), up to the next `,` outside groups and `<..>`.
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
