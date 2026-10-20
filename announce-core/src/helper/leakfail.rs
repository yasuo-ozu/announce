//! What a definition found that does not resolve outside its module: one error for each
//! offending type or path, at its tokens in the item. `#[announce]` reports them at the
//! definition ([`LeakFailure::to_definition_error`]).

use proc_macro2::{Span, TokenStream};
use template_quote::ToTokens;

/// The reason of a type that a `repeater` could carry, in a definition that has none.
/// [`LeakFailure`] recognizes it, to tell the definer's fix from a path to write
/// absolutely.
pub(crate) const WITHOUT_REPEATER: &str = "without `repeater`";

/// The reason of a relative trait path.
pub(crate) const TRAIT_PATH: &str = "with a relative trait path in a bound";
/// The reason of a relative value path.
pub(crate) const VALUE_PATH: &str = "with a relative value path";
/// The reason of a relative path on the right-hand side of a type alias.
pub(crate) const ALIAS: &str = "with a relative path in a type alias";
/// The reason of a type that type-leak refuses to carry (`Wrap<'_>`, `Local<Self>`, ..).
pub(crate) const REFUSED: &str = "with a type that cannot be carried";

/// The tokens of an error as text, as the definer reads them.
fn type_text(e: &LeakError) -> String {
    crate::leak::tidy(&e.tokens.to_string()).replace("$crate", "crate")
}

/// What an error is about, for a message: the tokens in backticks, or "a type of the item"
/// when there are none.
fn what(e: &LeakError) -> String {
    let ty = type_text(e);
    if ty.is_empty() {
        "a type of the item".to_string()
    } else {
        format!("`{}`", ty)
    }
}

/// What the definer can do about a cause, for the message of the error at the definition.
/// `attr` names the attribute of the item: ``the `#[announce]` ``.
fn fix_for(reason: &str, attr: &str) -> String {
    if reason == WITHOUT_REPEATER {
        format!(
            "add `repeater = ..` to {}, or write the path absolutely",
            attr
        )
    } else if reason == NOT_PRIMITIVE {
        "use a primitive type for the const parameter".to_string()
    } else {
        "write the path absolutely".to_string()
    }
}

/// The reason of a const generic whose type is not a primitive one. No path fixes it: the
/// parameter needs a primitive type.
pub(crate) const NOT_PRIMITIVE: &str = "with a const generic that is not of a primitive type";

/// What the definer could not carry: tokens of the definition, and a short reason.
#[derive(Clone, Debug)]
pub struct LeakError {
    /// The offending tokens, as written in the definition, with their spans.
    pub tokens: TokenStream,
    /// A short phrase (``without `repeater` ``, `with a relative trait path in a bound`).
    pub reason: String,
}

impl LeakError {
    /// An error at `tokens` with the phrase `reason`.
    pub fn new(tokens: impl ToTokens, reason: impl Into<String>) -> Self {
        LeakError {
            tokens: tokens.to_token_stream(),
            reason: reason.into(),
        }
    }
}

/// What a definition has that does not resolve outside its module.
#[derive(Clone, Debug)]
pub struct LeakFailure {
    /// What does not resolve.
    pub errors: Vec<LeakError>,
}

impl LeakFailure {
    /// A failure of `errors`.
    pub fn new(errors: Vec<LeakError>) -> Self {
        LeakFailure { errors }
    }

    /// The failure as a compile error at the definition: one error for each thing that does
    /// not resolve outside the module, at its tokens in the item. No call is known yet, so the
    /// message says what the definer can do:
    ///
    /// ```text
    /// `Local` cannot be named outside its module: add `repeater = ..` to the `#[announce]`, or write the path absolutely
    /// `Base` cannot be named outside its module (a relative trait path in a bound): write the path absolutely
    /// `Idx` cannot be named outside its module (a const generic that is not of a primitive type): use a primitive type for the const parameter
    /// ```
    pub(crate) fn to_definition_error(&self) -> syn::Error {
        let mut out: Option<syn::Error> = None;
        for e in &self.errors {
            let why = if e.reason == WITHOUT_REPEATER {
                String::new()
            } else {
                format!(" ({})", e.reason.strip_prefix("with ").unwrap_or(&e.reason))
            };
            let message = format!(
                "{} cannot be named outside its module{}: {}",
                what(e),
                why,
                fix_for(&e.reason, "the `#[announce]`")
            );
            let err = syn::Error::new_spanned(&e.tokens, message);
            match &mut out {
                Some(o) => o.combine(err),
                None => out = Some(err),
            }
        }
        out.unwrap_or_else(|| {
            syn::Error::new(
                Span::call_site(),
                "the item cannot be named outside its module",
            )
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn failure(errors: &[(&str, &str)]) -> LeakFailure {
        LeakFailure::new(
            errors
                .iter()
                .map(|(t, r)| LeakError::new(t.parse::<TokenStream>().unwrap(), *r))
                .collect(),
        )
    }

    #[test]
    fn the_definition_error_says_what_the_definer_can_do() {
        let f = failure(&[
            ("A", WITHOUT_REPEATER),
            ("B < T >", "with a relative trait path in a bound"),
            ("", "with a type that cannot be encoded (x)"),
        ]);
        let errors: Vec<String> = f
            .to_definition_error()
            .into_iter()
            .map(|e| e.to_string())
            .collect();
        assert_eq!(
            errors,
            [
                "`A` cannot be named outside its module: add `repeater = ..` to the `#[announce]`, \
                 or write the path absolutely",
                "`B<T>` cannot be named outside its module (a relative trait path in a bound): \
                 write the path absolutely",
                "a type of the item cannot be named outside its module \
                 (a type that cannot be encoded (x)): write the path absolutely",
            ]
        );
        assert_eq!(failure(&[]).to_definition_error().into_iter().count(), 1);
        // A const generic of a type that is not primitive: no path helps.
        let f = failure(&[("crate :: Idx", NOT_PRIMITIVE)]);
        assert_eq!(
            f.to_definition_error().to_string(),
            "`crate::Idx` cannot be named outside its module \
             (a const generic that is not of a primitive type): \
             use a primitive type for the const parameter"
        );
    }
}
