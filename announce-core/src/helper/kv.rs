//! Typed `key = value` argument lists for META and REST.

use proc_macro2::{Ident, Span, TokenStream, TokenTree};
use syn::ext::IdentExt;
use syn::parse::{Parse, ParseStream, Parser};
use syn::punctuated::Punctuated;
use syn::{Error, Result, Token};

/// A parsed `key = value, flag, key = [a, b], ..` list. Values are kept as tokens
/// and parsed on demand into any [`syn::parse::Parse`] type (types, paths,
/// literals, tuples, bracketed lists, ..).
///
/// A value runs to the next top-level `,`. Commas inside `<..>` (generic
/// arguments) and inside groups do not end a value. A value with an unclosed `<`
/// (like the comparison `x < y`) is an error: put such expressions in parentheses.
/// A key without `=` is a flag.
///
/// ```
/// # use announce_core as announce;
/// use announce::helper::KvArgs;
/// let mut kv: KvArgs = syn::parse_quote!(ty = Vec<u8, A>, paths = [a::b, c], n = 3, fast);
/// let ty: syn::Type = kv.require("ty").unwrap();
/// let paths: Vec<syn::Path> = kv.list("paths").unwrap().unwrap();
/// let n: syn::LitInt = kv.require("n").unwrap();
/// assert!(kv.flag("fast").unwrap());
/// kv.finish().unwrap();
/// # let _ = (ty, paths, n);
/// ```
#[derive(Clone, Debug, Default)]
pub struct KvArgs {
    entries: Vec<(Ident, Option<TokenStream>)>,
    span: Option<Span>,
}

impl Parse for KvArgs {
    fn parse(input: ParseStream) -> Result<Self> {
        let span = input.span();
        let mut entries = Vec::new();
        while !input.is_empty() {
            let key = Ident::parse_any(input)?;
            let value = if input.peek(Token![=]) && !input.peek(Token![==]) {
                input.parse::<Token![=]>()?;
                let mut ts = Vec::new();
                let mut depth = 0usize;
                let mut prev_minus = false;
                while !input.is_empty() {
                    if depth == 0 && input.peek(Token![,]) {
                        break;
                    }
                    let tt: TokenTree = input.parse()?;
                    if let TokenTree::Punct(p) = &tt {
                        match p.as_char() {
                            '<' => depth += 1,
                            '>' if !prev_minus => depth = depth.saturating_sub(1),
                            _ => {}
                        }
                        prev_minus = p.as_char() == '-';
                    } else {
                        prev_minus = false;
                    }
                    ts.push(tt);
                }
                if ts.is_empty() {
                    return Err(Error::new(
                        key.span(),
                        format!("missing value for `{}`", key),
                    ));
                }
                if depth != 0 {
                    // A comparison or shift (`x < y`, `1 << 2`) looks like an
                    // unclosed `<..>` and would swallow the following keys.
                    return Err(Error::new(
                        key.span(),
                        format!(
                            "unbalanced `<` in the value of `{}`; put expressions containing `<` in parentheses",
                            key
                        ),
                    ));
                }
                Some(ts.into_iter().collect())
            } else {
                None
            };
            if entries.iter().any(|(k, _): &(Ident, _)| *k == key) {
                return Err(Error::new(key.span(), format!("duplicate key `{}`", key)));
            }
            entries.push((key, value));
            if input.is_empty() {
                break;
            }
            input.parse::<Token![,]>()?;
        }
        Ok(KvArgs {
            entries,
            span: Some(span),
        })
    }
}

impl KvArgs {
    fn remove(&mut self, key: &str) -> Option<(Ident, Option<TokenStream>)> {
        let i = self.entries.iter().position(|(k, _)| k == key)?;
        Some(self.entries.remove(i))
    }

    fn span(&self) -> Span {
        self.span.unwrap_or_else(Span::call_site)
    }

    /// Takes `key = VALUE` and parses VALUE as `T`. `Ok(None)` if absent.
    pub fn get<T: Parse>(&mut self, key: &str) -> Result<Option<T>> {
        match self.remove(key) {
            None => Ok(None),
            Some((k, None)) => Err(Error::new(k.span(), format!("`{}` needs a value", k))),
            Some((_, Some(v))) => syn::parse2(v).map(Some),
        }
    }

    /// Like [`get`](Self::get), but a missing key is an error.
    pub fn require<T: Parse>(&mut self, key: &str) -> Result<T> {
        let span = self.span();
        self.get(key)?
            .ok_or_else(|| Error::new(span, format!("missing `{}`", key)))
    }

    /// Takes `key = [a, b, ..]` and parses each element as `T`. It returns a `Vec`.
    /// For a `Punctuated`, write the element type out and collect, e.g.
    /// `kv.list::<syn::Path>("k")?` then `.into_iter().collect::<Punctuated<_, Token![,]>>()`.
    pub fn list<T: Parse>(&mut self, key: &str) -> Result<Option<Vec<T>>> {
        match self.remove(key) {
            None => Ok(None),
            Some((k, None)) => Err(Error::new(
                k.span(),
                format!("`{}` needs a `[..]` value", k),
            )),
            Some((_, Some(v))) => {
                let parser = |input: ParseStream| {
                    let content;
                    syn::bracketed!(content in input);
                    Punctuated::<T, Token![,]>::parse_terminated(&content)
                };
                Ok(Some(parser.parse2(v)?.into_iter().collect()))
            }
        }
    }

    /// Takes the raw tokens of `key = VALUE`.
    pub fn tokens(&mut self, key: &str) -> Option<TokenStream> {
        self.remove(key).and_then(|(_, v)| v)
    }

    /// Takes a bare `key` (a flag). `key = ..` is an error.
    pub fn flag(&mut self, key: &str) -> Result<bool> {
        match self.remove(key) {
            None => Ok(false),
            Some((_, None)) => Ok(true),
            Some((k, Some(_))) => Err(Error::new(k.span(), format!("`{}` takes no value", k))),
        }
    }

    /// Errors on any key that was not taken.
    pub fn finish(self) -> Result<()> {
        match self.entries.first() {
            None => Ok(()),
            Some((k, _)) => Err(Error::new(k.span(), format!("unknown key `{}`", k))),
        }
    }
}
