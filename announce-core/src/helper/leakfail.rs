//! The record of a definition whose types do not resolve outside its module: the
//! `type_leak_error(..)` META, and what a callback makes of it.
//!
//! By default `#[announce]` refuses such an item: it is an error at the definition
//! ([`LeakFailure::to_definition_error`]). A definer that writes `strict = false` does not
//! refuse it. It cannot know where the trampoline will be called, and a call in the item's
//! own module is fine: the item, as written, resolves there. So the definer records what it
//! found ([`LeakFailure`]) and leaves the decision to the call. A callback that emits the item
//! as written asks [`Entry::item_resolved`](crate::helper::Entry::item_resolved) and gets, with
//! the item, a check that fails to compile unless the call is in the item's module
//! ([`LeakFailure::assume_success`]); a caller that wants only the check puts
//! `announce::same_context!` in its chain ([`same_context()`]). A callback that needs a type
//! table gets [`LeakFailure::to_error`] from
//! [`Entry::type_leak`](crate::helper::Entry::type_leak): the item has none, and the message
//! says so wherever the callback is called.

use crate::helper::KvArgs;
use proc_macro2::{Delimiter, Group, Ident, Span, TokenStream, TokenTree};
use syn::ext::IdentExt;
use syn::parse::{ParseStream, Parser};
use syn::{Item, LitInt, LitStr, Path, Result, Token};
use template_quote::{quote, ToTokens};

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

/// Whether a reason is about a name that resolves in the module of the item only. The other
/// reasons are about what a type table can carry (a const generic that is not of a primitive
/// type, a type that cannot be encoded, ..), and the name resolves where it is.
fn is_scope(reason: &str) -> bool {
    matches!(reason, WITHOUT_REPEATER | TRAIT_PATH | VALUE_PATH | ALIAS)
}

/// The part of an error of the record in braces: `message = ".."`, `notes = [".."]` and
/// `help = ".."`, in any order. It is read as a [`KvArgs`] list and `finish` is not called, so a
/// key that a later version adds is skipped whatever its value is (`span = a::b`, `level = -1`,
/// `ty = Vec<u8>`), and so is a flag with no value.
fn read_texts(input: ParseStream, error: &mut LeakError) -> Result<()> {
    let mut kv: KvArgs = input.parse()?;
    error.message = kv.get::<LitStr>("message")?.map(|s| s.value());
    if let Some(notes) = kv.list::<LitStr>("notes")? {
        error.notes = notes.iter().map(LitStr::value).collect();
    }
    error.help = kv.get::<LitStr>("help")?.map(|s| s.value());
    Ok(())
}

/// Skips what a later version adds to an error, up to the next `,` that is not in a group or in
/// `<..>` (`Vec<u8, A>`, as in a [`KvArgs`] value), which is the next error.
fn skip_to_comma(input: ParseStream) -> Result<()> {
    let (mut depth, mut minus) = (0usize, false);
    while !input.is_empty() && !(depth == 0 && input.peek(Token![,])) {
        let tt: TokenTree = input.parse()?;
        if let TokenTree::Punct(p) = &tt {
            match p.as_char() {
                '<' => depth += 1,
                '>' if !minus => depth = depth.saturating_sub(1),
                _ => {}
            }
            minus = p.as_char() == '-';
        } else {
            minus = false;
        }
    }
    Ok(())
}

/// The tokens of an error as text, as the caller reads them.
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

/// What the definer can do about a cause, for the message of the error at the definition and of
/// the one at the call. `attr` names the attribute of the item: ``the `#[announce]` `` where the
/// error points at the item, ``its `#[announce]` `` at a call.
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

/// What the definer could not carry: tokens of the definition, a short reason, and what a call
/// says about it.
///
/// The record carries the texts that the call cannot derive: type-leak's message, notes and
/// help for what it refuses, and the texts of announce's own checks. A record without them (one
/// built by hand, or by a definer that does not write them) gets the texts that the reason
/// has by default.
#[derive(Clone, Debug)]
#[non_exhaustive]
pub struct LeakError {
    /// The offending tokens, as written in the definition, with their spans.
    pub tokens: TokenStream,
    /// A short phrase (``without `repeater` ``, `with a relative
    /// trait path in a bound`). It tells a name that resolves in the module of the item only
    /// from what no table can carry, and is what the short errors of
    /// [`to_error`](LeakFailure::to_error) and of the definition use.
    pub reason: String,
    /// The headline of the error of the check at a call, for example `use absolute path: 'Local'`.
    pub message: Option<String>,
    /// Why, one note for each, the most useful first.
    pub notes: Vec<String>,
    /// What to write instead.
    pub help: Option<String>,
}

impl LeakError {
    /// An error at `tokens` with the phrase `reason`, and no texts of its own.
    pub fn new(tokens: impl ToTokens, reason: impl Into<String>) -> Self {
        LeakError {
            tokens: tokens.to_token_stream(),
            reason: reason.into(),
            message: None,
            notes: Vec::new(),
            help: None,
        }
    }

    /// This error with the headline `message`.
    pub fn with_message(mut self, message: impl Into<String>) -> Self {
        self.message = Some(message.into());
        self
    }

    /// This error with the notes `notes`.
    pub fn with_notes(mut self, notes: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.notes = notes.into_iter().map(Into::into).collect();
        self
    }

    /// This error with the help `help`.
    pub fn with_help(mut self, help: impl Into<String>) -> Self {
        self.help = Some(help.into());
        self
    }

    /// The tokens as text, as the caller reads them; empty when there are none.
    fn text(&self) -> String {
        type_text(self)
    }

    /// This error with the texts that its reason has by default, where it has none of its own.
    /// `item` is the name of the item: a type that needs a `repeater` says to add it to the
    /// `#[announce]` of the item. The texts of a relative name are about the path in the tokens
    /// ([`cause_path`]): for `&'static Local` the message names `Local` and the help writes
    /// `Local` absolutely, not the reference.
    pub(crate) fn described(mut self, item: &str) -> Self {
        let text = self.text();
        // What is written as the name of a type that is relative: the path in the cause, without
        // arguments and without what stands before it (`UiShared` for `&'static UiShared`).
        let cause = cause_path(&self.tokens);
        let (name, written) = match &cause {
            Some(c) => (c.path.clone(), c.written.clone()),
            None => (text.clone(), text.clone()),
        };
        if self.message.is_none() {
            self.message = Some(if is_scope(&self.reason) {
                format!("use absolute path: '{}'", written)
            } else if self.reason == NOT_PRIMITIVE {
                format!("const parameter of a non-primitive type: '{}'", text)
            } else if text.is_empty() {
                "a type of the item cannot be carried".to_string()
            } else {
                format!("cannot be carried: '{}'", text)
            });
        }
        if self.notes.is_empty() {
            if self.reason == NOT_PRIMITIVE {
                self.notes.push(
                    "a const parameter of a type that is not primitive cannot be carried"
                        .to_string(),
                );
            } else if !is_scope(&self.reason) && self.reason != REFUSED {
                let why = self.reason.strip_prefix("with ").unwrap_or(&self.reason);
                self.notes.push(why.to_string());
            }
        }
        if self.help.is_none() {
            self.help = Some(
                if is_scope(&self.reason) && crate::leak::PRELUDE.contains(&name.as_str()) {
                    // A name of the std prelude: it resolves almost everywhere, but a caller may
                    // shadow it, and `crate::` is not where it is.
                    format!(
                    "write `{n}` with an absolute path (it is a name of the std prelude, which a \
                     caller may shadow), or add `allow_std_prelude = true` to the `#[announce]` \
                     of `{i}`",
                    n = name,
                    i = item
                )
                } else if self.reason == WITHOUT_REPEATER {
                    format!(
                    "write `{n}` with an absolute path (e.g. `crate::{n}`), or add `repeater = ..` \
                     to the `#[announce]` of `{i}`",
                    n = name,
                    i = item
                )
                } else if is_scope(&self.reason) {
                    format!(
                        "write `{n}` with an absolute path (e.g. `crate::{n}`)",
                        n = name
                    )
                } else if self.reason == NOT_PRIMITIVE {
                    "use a primitive type for the const parameter".to_string()
                } else {
                    "write the path absolutely".to_string()
                },
            );
        }
        self
    }
}

/// The record of a definition that has no type-leak table because something in it does
/// not resolve outside its module: the `type_leak_error(..)` META element of an entry
/// ([`Entry::leak_failure`](crate::helper::Entry::leak_failure)).
///
/// The META element is
///
/// ```text
/// type_leak_error(id = 42u64,
///                 errors = [(Local) "without `repeater`" { message = "use absolute path: 'Local'",
///                                                         help = "write `Local` with an absolute path (e.g. `crate::Local`), or add `repeater = ..` to the `#[announce]` of `Item`" },
///                           (Base) "with a relative trait path in a bound"])
/// ```
///
/// - `id`: the random number of the announcement. The definer emitted a private marker for it
///   in the module of the item, and [`assume_success`](Self::assume_success) tests for it.
/// - `krate = ".."` (optional, after `errors`): the name of the crate that the definer is
///   compiling, as Cargo names it (`CARGO_CRATE_NAME`), for the texts that name the call.
/// - `errors`: one `(TOKENS) "reason" { .. }` for each thing that does not resolve. `TOKENS`
///   are the tokens of the definition (their spans are the definition's, so an error can be
///   reported there), and `reason` is a short phrase. The part in braces is optional, and
///   has what a call says about the cause: `message = ".."`, `notes = ["..", ..]` and
///   `help = ".."`, each optional. A reader that does not find them uses the texts that the
///   reason has by default.
///
/// Unknown keys are ignored when reading, in the record and in the part in braces of an error
/// (a value runs to the next `,` at the top level, as in [`KvArgs`], and a key may stand alone),
/// and so are further tokens after an error's braces, up to the next error. An entry has either
/// `type_leak(..)` or this, never both.
///
/// The texts name the call as the caller wrote it (`m::Item`, and the module `m`). A call written
/// through `$crate` in a macro of the definer's library (`$crate::m::Item!`) is a path that a
/// proc macro cannot resolve, so the texts say `krate` for `$crate`: `lib::m::Item`, module
/// `lib::m`. It is the crate that the definer compiles, as Cargo names it, so a dependency that
/// the user renamed in their `Cargo.toml` shows its real name. If the `$crate` of the call
/// belongs to another crate than the definer (a re-export through the macro of a third crate),
/// the texts still name the definer's. A record without `krate` (the definer was not built by
/// Cargo, or wrote it before this key existed) drops the `$crate::`: `m::Item`, module `m`.
///
/// Next to the item, the definition emits a private marker for `id`: a trait with a
/// method `__announce_ctx` that is implemented for `announce::__private::Probe<id>`. Only
/// the module of the item, its blocks and the submodules that `use super::*` have the trait
/// in scope, and the check of [`assume_success`](Self::assume_success) calls the method on
/// that probe. Where the trait is in scope the call finds it; anywhere else it finds
/// the fallback of `announce`, whose type does not satisfy the check.
#[derive(Clone, Debug)]
#[non_exhaustive]
pub struct LeakFailure {
    /// The random number of the announcement, the `R` of the marker.
    pub id: LitInt,
    /// What does not resolve.
    pub errors: Vec<LeakError>,
    /// The path the caller wrote to call the trampoline, without the `!`. Its last segment
    /// carries the span of the call: [`to_error`](Self::to_error) points at it, and the check
    /// of [`assume_success`](Self::assume_success) has it in the note about the bound. The
    /// definer sets the name of the item.
    pub path: Path,
    /// The name of the item. The trait of the check is named after it, with its span.
    pub ident: Ident,
    /// The name of the crate that the definer compiled, as Cargo names it, when it is known.
    /// The texts use it for a `$crate` at the head of [`path`](Self::path).
    pub krate: Option<String>,
}

/// The segments of a path for a text: a `$crate` at the head is the crate `krate` when it is
/// known, and is left out when it is not (`$crate::m::Item` is `lib::m::Item`, or `m::Item`). A
/// proc macro cannot tell which crate a `$crate` token means, so the definer's is used. A path
/// that does not begin with `$crate` is as it is.
fn resolve_dollar_crate(mut segments: Vec<String>, krate: Option<&str>) -> Vec<String> {
    if segments.first().map_or(false, |s| s == "$crate") {
        match krate {
            Some(krate) => segments[0] = krate.to_string(),
            None => {
                segments.remove(0);
            }
        }
    }
    segments
}

/// The texts of the error that the check at a call gives for one cause.
struct CauseText {
    message: String,
    label: String,
    notes: Vec<String>,
}

impl LeakFailure {
    /// A record of `errors` for the item `ident`.
    pub fn new(ident: Ident, id: LitInt, errors: Vec<LeakError>) -> Self {
        LeakFailure {
            id,
            errors,
            path: Path::from(ident.clone()),
            ident,
            krate: None,
        }
    }

    /// This record with the name of the crate that the definer compiled. A call written through
    /// `$crate` names that crate in the texts.
    pub fn with_krate(mut self, krate: impl Into<String>) -> Self {
        self.krate = Some(krate.into());
        self
    }

    /// The `type_leak_error(..)` META element, as the definer writes it.
    pub fn meta(&self) -> TokenStream {
        let id = &self.id;
        let errors = self.errors.iter().map(|e| {
            let (tokens, reason) = (&e.tokens, LitStr::new(&e.reason, Span::call_site()));
            let lit = |s: &String| LitStr::new(s, Span::call_site());
            let mut texts: Vec<TokenStream> = Vec::new();
            if let Some(message) = &e.message {
                let message = lit(message);
                texts.push(quote! { message = #message });
            }
            if !e.notes.is_empty() {
                let notes = e.notes.iter().map(lit);
                texts.push(quote! { notes = [ #(#notes),* ] });
            }
            if let Some(help) = &e.help {
                let help = lit(help);
                texts.push(quote! { help = #help });
            }
            if texts.is_empty() {
                quote! { (#tokens) #reason }
            } else {
                quote! { (#tokens) #reason { #(#texts),* } }
            }
        });
        let krate = self.krate.as_ref().map(|k| {
            let k = LitStr::new(k, Span::call_site());
            quote! { , krate = #k }
        });
        quote! { type_leak_error(id = #id, errors = [ #(#errors),* ] #krate) }
    }

    /// Reads the arguments of the `type_leak_error(..)` META element
    /// ([`Meta::kv`](crate::helper::Meta::kv)). `path` is what the caller wrote to call the
    /// trampoline, and `ident` the name of the item. Unknown keys are ignored.
    pub fn from_kv(kv: &mut KvArgs, path: Path, ident: Ident) -> Result<Self> {
        let call_site = Span::call_site();
        let id: LitInt = kv.require("id")?;
        let krate = kv.get::<LitStr>("krate")?.map(|k| k.value());
        let errors = kv.tokens("errors").ok_or_else(|| {
            syn::Error::new(
                call_site,
                "missing `errors` in the `type_leak_error(..)` META",
            )
        })?;
        let errors = (|input: ParseStream| {
            let content;
            syn::bracketed!(content in input);
            let mut errors = Vec::new();
            while !content.is_empty() {
                let tokens;
                syn::parenthesized!(tokens in content);
                let tokens: TokenStream = tokens.parse()?;
                let reason: LitStr = content.parse()?;
                let mut error = LeakError::new(tokens, reason.value());
                if content.peek(syn::token::Brace) {
                    let texts;
                    syn::braced!(texts in content);
                    read_texts(&texts, &mut error)?;
                }
                skip_to_comma(&content)?;
                errors.push(error);
                if content.is_empty() {
                    break;
                }
                content.parse::<Token![,]>()?;
            }
            Ok(errors)
        })
        .parse2(errors)?;
        Ok(LeakFailure {
            id,
            errors,
            path,
            ident,
            krate,
        })
    }

    /// The segments of the path of the trampoline, for the texts. A `$crate` at the head is a
    /// path that a proc macro cannot resolve: it is the crate of the definer when the record
    /// has its name, and it is left out when it has not.
    fn text_segments(&self) -> Vec<String> {
        let segments = self.path.segments.iter().map(|s| s.ident.to_string());
        resolve_dollar_crate(segments.collect(), self.krate.as_deref())
    }

    /// The path of the trampoline as the caller wrote it, `a::b::Item`.
    fn path_text(&self) -> String {
        self.path_prefix(self.text_segments().len())
    }

    /// The first `n` segments of the path, `a::b` for `a::b::Item` and 2.
    fn path_prefix(&self, n: usize) -> String {
        let segments: Vec<String> = self.text_segments().into_iter().take(n).collect();
        let lead = if self.path.leading_colon.is_some() {
            "::"
        } else {
            ""
        };
        format!("{}{}", lead, segments.join("::"))
    }

    /// The failure as compile errors, for a callback that needs a type table and gets none
    /// ([`Entry::type_leak`](crate::helper::Entry::type_leak) returns them, so that a callback
    /// that reads the table with `?` fails). A callback cannot tell where it is called, so the
    /// message is true in the module of the item as well. There is one error for each cause, a
    /// plain line that ends with what the definer can do, like the error at the definition. They
    /// are spanned at the last segment of the path that the caller wrote ([`path`](Self::path)),
    /// where the error belongs, and not at the tokens of the definition, which may be in another
    /// crate:
    ///
    /// ```text
    /// this macro cannot use `m::Item`: `Local` cannot be named outside its module; add `repeater = ..` to its `#[announce]`, or write the path absolutely
    /// this macro cannot use `m::Item`: `Base` cannot be named outside its module (a relative trait path in a bound); write the path absolutely
    /// this macro cannot use `m::Item`: `crate::Idx` cannot be carried (a const generic that is not of a primitive type); use a primitive type for the const parameter
    /// ```
    ///
    /// A record with no cause is one error without a fix. A callback that can use the item as
    /// written in its module reads [`Entry::item_resolved`](crate::helper::Entry::item_resolved)
    /// instead.
    pub fn to_error(&self) -> syn::Error {
        let span = self
            .path
            .segments
            .last()
            .map_or_else(Span::call_site, |s| s.ident.span());
        let lead = format!("this macro cannot use `{}`", self.path_text());
        let mut messages: Vec<String> = Vec::new();
        for e in &self.errors {
            let what = what(e);
            let clause = if is_scope(&e.reason) {
                match e.reason.strip_prefix("with ") {
                    Some(why) if e.reason != WITHOUT_REPEATER => {
                        format!("{} cannot be named outside its module ({})", what, why)
                    }
                    _ => format!("{} cannot be named outside its module", what),
                }
            } else if e.reason == REFUSED {
                // The reason of a refusal says that already.
                format!("{} cannot be carried", what)
            } else {
                let why = e.reason.strip_prefix("with ").unwrap_or(&e.reason);
                format!("{} cannot be carried ({})", what, why)
            };
            let message = format!(
                "{}: {}; {}",
                lead,
                clause,
                fix_for(&e.reason, "its `#[announce]`")
            );
            if !messages.contains(&message) {
                messages.push(message);
            }
        }
        if messages.is_empty() {
            messages.push(format!(
                "{}: a type of the item cannot be named outside its module",
                lead
            ));
        }
        let mut out: Option<syn::Error> = None;
        for message in messages {
            let err = syn::Error::new(span, message);
            match &mut out {
                Some(o) => o.combine(err),
                None => out = Some(err),
            }
        }
        out.expect("at least one message")
    }

    /// The failure as a compile error at the definition, for an item that is not announced with
    /// `strict = false`: one error for each thing that does not resolve outside the module, at
    /// its tokens in the item. No call is known yet, so the message says what the definer can
    /// do:
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

    /// The texts of the error at the call for one cause: type-leak's layout, with the call in
    /// the note that rustc adds about the bound.
    fn cause_text(&self, e: &LeakError) -> CauseText {
        let e = e.clone().described(&self.ident.unraw().to_string());
        let path = self.path_text();
        let n = self.text_segments().len();
        let (used, module) = if n >= 2 {
            let module = self.path_prefix(n - 1);
            (
                format!("`{}` is used outside its module `{}`", path, module),
                format!("`{}`", module),
            )
        } else {
            (
                format!("`{}` is used outside its module", path),
                "its module".to_string(),
            )
        };
        let label = if is_scope(&e.reason) {
            let kind = match e.reason.as_str() {
                TRAIT_PATH => "relative trait path",
                VALUE_PATH => "relative value path",
                ALIAS => "relative path in a type alias",
                _ => "relative type",
            };
            format!("{}, not in scope where `{}!` is called", kind, path)
        } else {
            format!("cannot be carried, so `{}!` works only in {}", path, module)
        };
        // The way in, then why, then what to do, as plain notes: the help of rustc is not one of
        // a trait.
        let mut notes = vec![used];
        notes.extend(e.notes);
        notes.extend(e.help);
        CauseText {
            message: e.message.unwrap_or_default(),
            label,
            notes,
        }
    }

    /// The check that a callback emits next to an item that it uses as written, as the
    /// failure records it. It compiles where the item's own names resolve (in the module of
    /// the item, in its function bodies and its submodules that `use super::*`) and is a
    /// compile error elsewhere: one error for each cause (each thing that the record names
    /// once), at the tokens of that cause in the definition, with type-leak's wording. The error
    /// underlines the whole path that the cause names, from its first token to its last
    /// (`inner::Local`, `Wrap<'_>`; for `&'static Local` it is `Local`). The call is in the note
    /// that rustc adds about the bound (`required by a bound in
    /// `Check::used_outside_its_module``), which points at the last segment of
    /// [`path`](Self::path):
    ///
    /// ```text
    /// error[E0277]: use absolute path: 'Base'
    ///   --> deflib/src/lib.rs:14:18
    ///    |
    /// 14 |     pub trait Sub: Base {
    ///    |                    ^^^^ relative trait path, not in scope where `deflib::m::Item!` is called
    ///    |
    ///    = note: `deflib::m::Item` is used outside its module `deflib::m`
    ///    = note: write `Base` with an absolute path (e.g. `crate::Base`)
    /// note: required by a bound in `Check::used_outside_its_module`
    ///   --> user/src/main.rs:1:36
    /// ```
    ///
    /// The check for a cause is a block of its own, `const _: () = { .. };`: a trait named after
    /// the item (`InModuleOf_Item`) that only the answer of the module's marker satisfies, a
    /// function that needs it, and a call that asks the marker's probe (see [`LeakFailure`]).
    /// The texts are those of the trait, `#[diagnostic::on_unimplemented]`, where the compiler
    /// that builds this crate knows it (Rust 1.78), and `#[diagnostic::do_not_recommend]` keeps
    /// the error short (1.85). Earlier compilers show the plain error that a trait is not
    /// implemented, at the same place. The call has the location of the cause, and the name
    /// resolution of the caller: it is the span of the caller's path, located at the cause. rustc
    /// reports a call from its first token to its last, so its first tokens are located at the
    /// start of the cause and its last ones at the end of it, and the function that the bound is
    /// on is `Check::used_outside_its_module`, so that its path has the same range.
    ///
    /// The check is an item, or a statement in a block. It cannot stand among the associated
    /// items of an `impl` or a `trait` block, where Rust does not allow a `const _`, and a
    /// trait impl takes no other items. A callback that emits into an `impl` puts these tokens
    /// beside the impl, at the call's item position, and not inside it. They depend on the
    /// module of the call only, so they give the same answer there:
    ///
    /// ```text
    /// CHECK
    /// impl S { METHODS }
    /// ```
    ///
    /// `krate` is the path of the `announce` crate where the tokens are emitted
    /// (`::announce`, or `$crate`). It must be the `announce` that the definition used: the
    /// check compares types of that crate.
    pub fn assume_success(&self, krate: &Path) -> TokenStream {
        let span = self
            .path
            .segments
            .last()
            .map_or_else(Span::call_site, |s| s.ident.span());
        // One check for each distinct cause, in the order of the record.
        let mut causes: Vec<&LeakError> = Vec::new();
        for e in &self.errors {
            let seen = |c: &&LeakError| type_text(c) == type_text(e) && c.reason == e.reason;
            if !causes.iter().any(seen) {
                causes.push(e);
            }
        }
        if causes.is_empty() {
            let text = CauseText {
                message: "a type of the item cannot be named outside its module".to_string(),
                label: format!("`{}!` is used outside its module", self.path_text()),
                notes: Vec::new(),
            };
            return self.check(krate, span, text, None);
        }
        causes
            .into_iter()
            .map(|e| self.check(krate, span, self.cause_text(e), cause_range(&e.tokens)))
            .collect()
    }

    /// The check for one cause. `cause` is where the cause starts and ends in the definition
    /// ([`cause_range`]); without it the error is at the caller's path.
    fn check(
        &self,
        krate: &Path,
        span: Span,
        text: CauseText,
        cause: Option<(Span, Span)>,
    ) -> TokenStream {
        let tr = Ident::new(
            &format!("InModuleOf_{}", self.ident.unraw()),
            self.ident.span(),
        );
        let id = &self.id;
        // The texts are format strings of `on_unimplemented`: a brace is doubled.
        let lit =
            |s: &str| LitStr::new(&s.replace('{', "{{").replace('}', "}}"), Span::call_site());
        let (message, label) = (lit(&text.message), lit(&text.label));
        let notes: Vec<LitStr> = text.notes.iter().map(|n| lit(n)).collect();
        let texts = if cfg!(announce_diagnostic) {
            quote! {
                #[diagnostic::on_unimplemented(
                    message = #message,
                    label = #label,
                    #(note = #notes),*
                )]
            }
        } else {
            TokenStream::new()
        };
        let not_recommended = if cfg!(announce_do_not_recommend) {
            quote! { #[diagnostic::do_not_recommend] }
        } else {
            TokenStream::new()
        };
        // Everything has the span of the caller's path, so that the note about the bound that
        // comes with the error points there and not into the macro that wrote it. The call is
        // the exception: it is located at the cause, which is where rustc reports the error.
        // rustc reports an expression from its first token to its last, so the tokens at the
        // start of the call are located at the start of the cause and the ones at the end of the
        // call (the callee's name, and the groups of the arguments) at the end of it: the whole
        // cause is underlined, and not one token. Each piece keeps the hygiene of the caller, so
        // the names resolve as before.
        let respan = crate::runner::respan;
        let (first, last) = match cause {
            Some((start, end)) => (span.located_at(start), span.located_at(end)),
            None => (span, span),
        };
        let konst = respan(quote! { const _: () = }, span);
        let head = respan(
            quote! {
                #[allow(unused_imports)]
                use #krate::__private::ContextFallback as _;
                use #krate::__private::Probe as __AnnounceProbe;
                #texts
                #[allow(non_camel_case_types)]
                trait #tr {}
                #not_recommended
                impl #tr for #krate::__private::ThisCallInTheModule {}
                // An associated function, so that its path can span the cause: the note about
                // the bound reads `Check::used_outside_its_module`, and the call has no other
                // span of its own ("required by a bound introduced by this call").
                #[allow(dead_code)]
                struct Check;
                impl Check {
                    fn used_outside_its_module<T: #tr>(_: T) {}
                }
                #[allow(dead_code)]
                fn __check()
            },
            span,
        );
        // `Check::used_outside_its_module(__AnnounceProbe::<ID>.__announce_ctx())`.
        let callee = {
            let (check, name) = (
                respan(quote! { Check:: }, first),
                respan(quote! { used_outside_its_module }, last),
            );
            quote! { #check #name }
        };
        let probe = respan(quote! { __AnnounceProbe::<#id>.__announce_ctx }, first);
        let mut unit = Group::new(Delimiter::Parenthesis, TokenStream::new());
        unit.set_span(last);
        let mut args = Group::new(Delimiter::Parenthesis, quote! { #probe #unit });
        args.set_span(last);
        let semi = respan(quote! { ; }, span);
        quote! { #konst { #head { #callee #args #semi } } #semi }
    }
}

/// The keywords that only qualify a type, and are not the name of anything.
const QUALIFIERS: &[&str] = &[
    "dyn", "impl", "mut", "const", "for", "fn", "unsafe", "extern",
];

/// The tokens of the level of `ts` (a group is a level of its own) that holds the first
/// identifier that names something, and its index there. `quote` is whether the token before was
/// the `'` of a lifetime.
fn locate(ts: &TokenStream, quote: &mut bool) -> Option<(Vec<TokenTree>, usize)> {
    let level: Vec<TokenTree> = ts.clone().into_iter().collect();
    let mut hit = None;
    for (i, tt) in level.iter().enumerate() {
        match tt {
            TokenTree::Group(g) => {
                *quote = false;
                if let Some(found) = locate(&g.stream(), quote) {
                    return Some(found);
                }
            }
            TokenTree::Punct(p) => *quote = p.as_char() == '\'',
            TokenTree::Literal(_) => *quote = false,
            TokenTree::Ident(id) => {
                let skip = *quote || QUALIFIERS.contains(&id.to_string().as_str());
                *quote = false;
                if !skip {
                    hit = Some(i);
                    break;
                }
            }
        }
    }
    hit.map(|i| (level, i))
}

/// The first and the last token of a cause where its error is reported: of the path that it
/// names ([`cause_path`]), the leading `::` if there is one and else the first identifier that
/// names something, to its last segment or, when angle-bracketed arguments follow, their last `>`.
/// For `&'static Local` and `*const Local` that is `Local` and `Local`, for `Wrap<'a, Local>`
/// `Wrap` and `>`, for `inner::Local` `inner` and `Local`, for `(Local, u8)` `Local` and `Local`.
/// What stands before the path (a reference, a pointer, a keyword such as `dyn`) is not in it,
/// and neither is what stands around it in a tuple or an array. A cause without such a path is
/// whole, from its first to its last token. `None` when it has no tokens.
fn cause_extent(ts: &TokenStream) -> Option<(TokenTree, TokenTree)> {
    let (level, i) = match locate(ts, &mut false) {
        Some(found) => found,
        None => {
            let all: Vec<TokenTree> = ts.clone().into_iter().collect();
            return Some((all.first()?.clone(), all.last()?.clone()));
        }
    };
    // The same reading as `cause_path`: a leading `::`, the segments, the arguments in angle
    // brackets (after the path or after a `::`, `Vec::<T>`).
    let start = if i >= 2 && is_colon(level.get(i - 1)) && is_colon(level.get(i - 2)) {
        i - 2
    } else {
        i
    };
    let mut next = i + 1;
    while is_colon(level.get(next)) && is_colon(level.get(next + 1)) {
        match level.get(next + 2) {
            Some(TokenTree::Ident(_)) => next += 3,
            _ => break,
        }
    }
    let is = |tt: Option<&TokenTree>, c: char| matches!(tt, Some(TokenTree::Punct(p)) if p.as_char() == c);
    let open = if is_colon(level.get(next)) && is_colon(level.get(next + 1)) {
        next + 2
    } else {
        next
    };
    let mut end = next;
    if is(level.get(open), '<') {
        let mut depth = 0usize;
        end = open;
        while let Some(tt) = level.get(end) {
            end += 1;
            if is(Some(tt), '<') {
                depth += 1;
            } else if is(Some(tt), '>') && !is(level.get(end.wrapping_sub(2)), '-') {
                depth -= 1;
                if depth == 0 {
                    break;
                }
            }
        }
    }
    Some((level[start].clone(), level[end - 1].clone()))
}

/// The spans of [`cause_extent`], where the error of a cause starts and ends.
fn cause_range(ts: &TokenStream) -> Option<(Span, Span)> {
    cause_extent(ts).map(|(first, last)| (first.span(), last.span()))
}

/// Whether `tt` is the `:` of a `::`, which is spelled as two of them.
fn is_colon(tt: Option<&TokenTree>) -> bool {
    matches!(tt, Some(TokenTree::Punct(p)) if p.as_char() == ':')
}

/// The path that a cause names, as [`cause_path`] reads it.
pub(crate) struct CausePath {
    /// The path without generic arguments, the name to write absolutely: `Wrap`.
    pub path: String,
    /// The path with the generic arguments that follow it, as it is written: `Wrap<'a, Local>`.
    pub written: String,
    /// Whether generic arguments follow the path.
    pub generic: bool,
}

/// The path that a cause names: the first identifier that names something ([`locate`]: not a
/// lifetime or a keyword such as `dyn` or `mut`), with the segments written after it, and the angle-bracketed arguments that follow. `&inner::Local` names `inner::Local`,
/// `Wrap<'a, Local>` names `Wrap` (written `Wrap<'a, Local>`), `dyn ::core::fmt::Debug` names
/// `::core::fmt::Debug`. What stands before the path, a reference, a pointer or a keyword, is not
/// part of it, and neither is what stands around it in a tuple or an array. `None` when no
/// identifier is in the tokens. The path is as the caller writes it, so a `$crate` is `crate`.
pub(crate) fn cause_path(ts: &TokenStream) -> Option<CausePath> {
    let (level, i) = locate(ts, &mut false)?;
    let start = if i >= 2 && is_colon(level.get(i - 1)) && is_colon(level.get(i - 2)) {
        i - 2
    } else {
        i
    };
    let mut path = if start < i { "::" } else { "" }.to_string();
    path.push_str(&level[i].to_string());
    let mut next = i + 1;
    while is_colon(level.get(next)) && is_colon(level.get(next + 1)) {
        match level.get(next + 2) {
            Some(TokenTree::Ident(id)) => {
                path.push_str("::");
                path.push_str(&id.to_string());
                next += 3;
            }
            _ => break,
        }
    }
    // The arguments in angle brackets, after the path or after a `::` (`Vec::<T>`).
    let open = if is_colon(level.get(next)) && is_colon(level.get(next + 1)) {
        next + 2
    } else {
        next
    };
    let is = |tt: Option<&TokenTree>, c: char| matches!(tt, Some(TokenTree::Punct(p)) if p.as_char() == c);
    let mut end = next;
    if is(level.get(open), '<') {
        let mut depth = 0usize;
        end = open;
        while let Some(tt) = level.get(end) {
            end += 1;
            if is(Some(tt), '<') {
                depth += 1;
            } else if is(Some(tt), '>') && !is(level.get(end.wrapping_sub(2)), '-') {
                depth -= 1;
                if depth == 0 {
                    break;
                }
            }
        }
    }
    let text = |tokens: &[TokenTree]| {
        let ts: TokenStream = tokens.iter().cloned().collect();
        crate::leak::tidy(&ts.to_string()).replace("$crate", "crate")
    };
    Some(CausePath {
        path: path.replace("$crate", "crate"),
        written: text(&level[start..end]),
        generic: end > next,
    })
}

/// What the macro `announce::same_context!` does: a callback that a caller puts in the
/// chain to require that the call is in the type context of the items (their modules). See
/// the docs of that macro.
///
/// `input` is `KRATE, INPUT`: the path of the `announce` crate (`$crate`, which the
/// `macro_rules!` in `announce` writes), and the input of the callback
/// ([`Input`](crate::helper::Input)). For each entry with a recorded failure
/// ([`Entry::leak_failure`](crate::helper::Entry::leak_failure)) the result has its check
/// ([`LeakFailure::assume_success`]), and then the call of the next element of the list with
/// the input unchanged ([`Input::forward`](crate::helper::Input::forward)); nothing follows
/// the checks when it was the last element. In expression position (`(expr)`) the result is
/// a block. It is public with the rest of the machinery, to test or to wrap that macro. An
/// error comes back as a `compile_error!` invocation; see [`try_same_context`] for a
/// [`syn::Error`].
pub fn same_context(input: TokenStream) -> TokenStream {
    try_same_context(input).unwrap_or_else(|e| e.to_compile_error())
}

/// [`same_context()`] with the error returned as a [`syn::Error`].
pub fn try_same_context(input: TokenStream) -> Result<TokenStream> {
    use crate::helper::{is_expression_position, Input};
    let (krate, input) = (|input: ParseStream| -> Result<(Path, Input)> {
        let krate: Path = input.call(Path::parse_mod_style)?;
        input.parse::<Token![,]>()?;
        Ok((krate, input.parse()?))
    })
    .parse2(input)?;
    let mut out = TokenStream::new();
    for entry in input.entries_in_order() {
        if let Some(failure) = entry.leak_failure()? {
            out.extend(failure.assume_success(&krate));
        }
    }
    out.extend(input.forward());
    // In expression position the result is one expression: the checks are statements of a block.
    if input.options.as_ref().map_or(false, is_expression_position) {
        return Ok(quote! { { #out } });
    }
    Ok(out)
}

/// The name of an item that `#[announce]` can record a failure of.
pub(crate) fn item_ident(item: &Item) -> Result<Ident> {
    match item {
        Item::Struct(i) => Ok(i.ident.clone()),
        Item::Enum(i) => Ok(i.ident.clone()),
        Item::Union(i) => Ok(i.ident.clone()),
        Item::Trait(i) => Ok(i.ident.clone()),
        Item::Type(i) => Ok(i.ident.clone()),
        other => Err(syn::Error::new_spanned(
            other,
            "announce: the item has no name to check (a `type_leak_error(..)` is recorded for a struct, \
             enum, union, trait or type alias)",
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::helper::Meta;

    fn failure(errors: &[(&str, &str)]) -> LeakFailure {
        let mut f = LeakFailure::new(
            syn::parse_quote!(Leaky),
            syn::parse_quote!(42u64),
            errors
                .iter()
                .map(|(t, r)| LeakError::new(t.parse::<TokenStream>().unwrap(), *r))
                .collect(),
        );
        f.path = syn::parse_quote!(deflib::lm::Leaky);
        f
    }

    fn squash(ts: impl ToTokens) -> String {
        ts.to_token_stream()
            .to_string()
            .split_whitespace()
            .collect()
    }

    fn read(f: &LeakFailure) -> LeakFailure {
        let meta: Meta = syn::parse2(f.meta()).unwrap();
        LeakFailure::from_kv(&mut meta.kv().unwrap(), f.path.clone(), f.ident.clone()).unwrap()
    }

    #[test]
    fn meta_round_trips() {
        let f = failure(&[
            ("Local", WITHOUT_REPEATER),
            (
                "Wrap < u8 , { N } >",
                "with a relative trait path in a bound",
            ),
        ]);
        // Without texts of their own the errors are the two parts, as they were.
        assert_eq!(
            squash(f.meta()),
            "type_leak_error(id=42u64,errors=[(Local)\"without`repeater`\",\
             (Wrap<u8,{N}>)\"witharelativetraitpathinabound\"])"
        );
        let back = read(&f);
        assert_eq!(back.id.base10_digits(), "42");
        assert_eq!(back.errors.len(), 2);
        assert_eq!(squash(&back.errors[1].tokens), "Wrap<u8,{N}>");
        assert_eq!(
            back.errors[1].reason,
            "with a relative trait path in a bound"
        );
        assert!(back.errors[1].message.is_none() && back.errors[1].notes.is_empty());
        assert!(back.errors[1].help.is_none());
    }

    #[test]
    fn meta_carries_the_texts_of_a_cause() {
        let mut f = failure(&[("Wrap < '_ >", REFUSED), ("Local", WITHOUT_REPEATER)]);
        f.errors[0] = LeakError::new("Wrap < '_ >".parse::<TokenStream>().unwrap(), REFUSED)
            .with_message("use absolute path: 'Wrap < '_ >'")
            .with_notes([
                "it cannot be leaked as a whole because of `'_`",
                "a { brace }",
            ])
            .with_help("write `Wrap` with an absolute path (e.g. `crate::Wrap`)");
        // The part in braces is there for a cause that has texts, and only then.
        let meta = squash(f.meta());
        assert_eq!(
            meta,
            "type_leak_error(id=42u64,errors=[(Wrap<'_>)\"withatypethatcannotbecarried\"{\
             message=\"useabsolutepath:'Wrap<'_>'\",notes=[\"itcannotbeleakedasawholebecauseof`'_`\",\
             \"a{brace}\"],help=\"write`Wrap`withanabsolutepath(e.g.`crate::Wrap`)\"},\
             (Local)\"without`repeater`\"])"
        );
        // The round trip keeps each text, in order, and the other cause has none.
        let back = read(&f);
        let e = &back.errors[0];
        assert_eq!(
            e.message.as_deref(),
            Some("use absolute path: 'Wrap < '_ >'")
        );
        assert_eq!(
            e.notes,
            [
                "it cannot be leaked as a whole because of `'_`",
                "a { brace }"
            ]
        );
        assert_eq!(
            e.help.as_deref(),
            Some("write `Wrap` with an absolute path (e.g. `crate::Wrap`)")
        );
        assert!(back.errors[1].message.is_none());
        // Each text is optional on its own, in any order, and a key that is not known is skipped.
        let meta: Meta = syn::parse_str(
            "type_leak_error(id = 1u64, errors = [(A) \"r\" { future = [1], help = \"h\" }, \
             (B) \"s\" { notes = [\"n\"] }])",
        )
        .unwrap();
        let f = LeakFailure::from_kv(
            &mut meta.kv().unwrap(),
            syn::parse_quote!(a::B),
            syn::parse_quote!(B),
        )
        .unwrap();
        assert_eq!(f.errors[0].help.as_deref(), Some("h"));
        assert!(f.errors[0].message.is_none() && f.errors[0].notes.is_empty());
        assert_eq!(f.errors[1].notes, ["n"]);
    }

    #[test]
    fn meta_ignores_unknown_keys_and_needs_errors() {
        let meta: Meta =
            syn::parse_str("type_leak_error(id = 1u64, future = [1, 2], errors = [])").unwrap();
        let f = LeakFailure::from_kv(
            &mut meta.kv().unwrap(),
            syn::parse_quote!(a::B),
            syn::parse_quote!(B),
        )
        .unwrap();
        assert!(f.errors.is_empty());
        let meta: Meta = syn::parse_str("type_leak_error(id = 1u64)").unwrap();
        let err = LeakFailure::from_kv(
            &mut meta.kv().unwrap(),
            syn::parse_quote!(a::B),
            syn::parse_quote!(B),
        )
        .unwrap_err();
        assert!(err.to_string().contains("missing `errors`"), "{}", err);
    }

    /// The texts of the check for the causes of `f`, as `(message, label, notes)`.
    fn cause_texts(f: &LeakFailure) -> Vec<(String, String, Vec<String>)> {
        f.errors
            .iter()
            .map(|e| {
                let t = f.cause_text(e);
                (t.message, t.label, t.notes)
            })
            .collect()
    }

    #[test]
    fn the_texts_of_a_relative_type_are_type_leaks() {
        let f = failure(&[("Local", WITHOUT_REPEATER)]);
        assert_eq!(
            cause_texts(&f),
            [(
                "use absolute path: 'Local'".to_string(),
                "relative type, not in scope where `deflib::lm::Leaky!` is called".to_string(),
                vec![
                    "`deflib::lm::Leaky` is used outside its module `deflib::lm`".to_string(),
                    "write `Local` with an absolute path (e.g. `crate::Local`), or add `repeater = ..` \
                     to the `#[announce]` of `Leaky`"
                        .to_string()
                ]
            )]
        );
        // The path of the type without its arguments is what is written absolutely.
        let f = failure(&[("Wrap < u8 >", WITHOUT_REPEATER)]);
        let (message, _, notes) = &cause_texts(&f)[0];
        assert_eq!(message, "use absolute path: 'Wrap<u8>'");
        assert!(notes[1].starts_with("write `Wrap` with an absolute path (e.g. `crate::Wrap`)"));
    }

    /// The message and the help that a cause of `tokens` gets by default for a type that
    /// needs a `repeater`, in the item `Item`.
    fn default_texts(tokens: &str) -> (String, String) {
        let e = LeakError::new(tokens.parse::<TokenStream>().unwrap(), WITHOUT_REPEATER)
            .described("Item");
        (e.message.unwrap(), e.help.unwrap())
    }

    #[test]
    fn the_texts_of_a_cause_name_the_path_in_it() {
        // `(what the message names, what the help writes absolutely)`: the path in the cause,
        // and nothing that stands around it.
        for (tokens, shown, path) in [
            // A plain path is as before: the message has its arguments, the help does not.
            ("Local", "Local", "Local"),
            ("inner :: Local", "inner::Local", "inner::Local"),
            (
                "crate :: inner :: Local",
                "crate::inner::Local",
                "crate::inner::Local",
            ),
            ("Wrap < 'a , Local >", "Wrap<'a, Local>", "Wrap"),
            (
                "Wrap < Pair < Local , u8 > >",
                "Wrap<Pair<Local, u8>>",
                "Wrap",
            ),
            // A reference, a pointer: the type that they point at.
            ("& 'static UiShared", "UiShared", "UiShared"),
            ("& 'static mut UiUnique", "UiUnique", "UiUnique"),
            ("& mut UiUnique", "UiUnique", "UiUnique"),
            ("* const UiRaw", "UiRaw", "UiRaw"),
            ("* mut UiRaw", "UiRaw", "UiRaw"),
            ("& inner :: Local", "inner::Local", "inner::Local"),
            ("& 'a Wrap < u8 >", "Wrap<u8>", "Wrap"),
            ("& & 'a UiShared", "UiShared", "UiShared"),
            // A trait object.
            ("dyn Base", "Base", "Base"),
            ("& 'a dyn Base", "Base", "Base"),
            ("Holder < dyn Base >", "Holder<dyn Base>", "Holder"),
            // A tuple and an array: their first name.
            ("(Local , u8)", "Local", "Local"),
            ("[Local ; 2]", "Local", "Local"),
            ("[(Local , u8) ; 2]", "Local", "Local"),
            ("& [Local]", "Local", "Local"),
            // A function type: the name in its arguments.
            ("fn (Local) -> u8", "Local", "Local"),
        ] {
            let (message, help) = default_texts(tokens);
            assert_eq!(
                message,
                format!("use absolute path: '{}'", shown),
                "{}",
                tokens
            );
            assert_eq!(
                help,
                format!(
                    "write `{p}` with an absolute path (e.g. `crate::{p}`), or add `repeater = ..` \
                     to the `#[announce]` of `Item`",
                    p = path
                ),
                "{}",
                tokens
            );
        }
    }

    #[test]
    fn the_texts_of_a_cause_never_put_what_stands_before_it_under_crate() {
        for tokens in [
            "& 'static UiShared",
            "* const UiRaw",
            "& 'static mut UiUnique",
            "dyn Base",
            "(Local , u8)",
            "[Local ; 2]",
            "& 'a Wrap < u8 >",
        ] {
            let (message, help) = default_texts(tokens);
            for bad in ["&", "*", "dyn", "(", "[", "'static", "'a", "mut", "const"] {
                // The message is the name in quotes.
                assert!(
                    !message.contains(bad),
                    "`{}` in `{}`: {}",
                    bad,
                    tokens,
                    message
                );
                // Under `crate::`, and in the name that is written absolutely.
                assert!(
                    !help.contains(&format!("crate::{}", bad)),
                    "`{}` in `{}`: {}",
                    bad,
                    tokens,
                    help
                );
                assert!(
                    !help.starts_with(&format!("write `{}", bad)),
                    "`{}` in `{}`: {}",
                    bad,
                    tokens,
                    help
                );
            }
        }
    }

    #[test]
    fn a_name_of_the_std_prelude_behind_a_reference_is_a_prelude_name() {
        // The prelude name is the path in the cause, wherever it stands.
        for tokens in [
            "Option < Local >",
            "& Option < Local >",
            "* const Option < Local >",
        ] {
            let e = LeakError::new(tokens.parse::<TokenStream>().unwrap(), ALIAS).described("Item");
            let help = e.help.unwrap();
            assert_eq!(
                help,
                "write `Option` with an absolute path (it is a name of the std prelude, which a \
                 caller may shadow), or add `allow_std_prelude = true` to the `#[announce]` of \
                 `Item`",
                "{}",
                tokens
            );
        }
        // A path that only ends in a prelude name is not one.
        let e = LeakError::new("& inner :: Option".parse::<TokenStream>().unwrap(), ALIAS)
            .described("Item");
        assert_eq!(
            e.help.as_deref(),
            Some("write `inner::Option` with an absolute path (e.g. `crate::inner::Option`)")
        );
    }

    /// The record of the entries of `errors` (the part in `errors = [..]`), as the META of a
    /// definer that wrote `extra` after it.
    fn read_meta(src: &str) -> Result<LeakFailure> {
        let meta: Meta = syn::parse_str(src)?;
        LeakFailure::from_kv(
            &mut meta.kv()?,
            syn::parse_quote!(a::B),
            syn::parse_quote!(B),
        )
    }

    #[test]
    fn a_later_version_may_add_keys_with_any_value_to_the_texts_of_a_cause() {
        // Values of several tokens (`a::b`, `-1`, `Vec<u8>`, a group) and a flag with no value
        // are skipped, and the texts that are known are read around them.
        let f = read_meta(
            "type_leak_error(id = 1u64, errors = [(A) \"r\" { message = \"m\", span = a::b, \
             level = -1, ty = Vec<u8, Alloc>, internal, notes = [\"n1\", \"n2\"], \
             list = [1, 2], help = \"h\" }])",
        )
        .unwrap();
        let e = &f.errors[0];
        assert_eq!(e.message.as_deref(), Some("m"));
        assert_eq!(e.notes, ["n1", "n2"]);
        assert_eq!(e.help.as_deref(), Some("h"));
        // A flag first, and at the end, with nothing else.
        let f = read_meta(
            "type_leak_error(id = 1u64, errors = [(A) \"r\" { internal, help = \"h\", last }])",
        )
        .unwrap();
        assert_eq!(f.errors[0].help.as_deref(), Some("h"));
        assert!(f.errors[0].message.is_none());
        let f = read_meta("type_leak_error(id = 1u64, errors = [(A) \"r\" { }])").unwrap();
        assert!(f.errors[0].message.is_none() && f.errors[0].help.is_none());
        // A text that is known but not a string is an error, not a skip.
        assert!(
            read_meta("type_leak_error(id = 1u64, errors = [(A) \"r\" { help = 3 }])").is_err()
        );
    }

    #[test]
    fn an_error_may_have_more_after_its_texts() {
        // After the braces, up to the next error, and after a reason that has no braces.
        let f = read_meta(
            "type_leak_error(id = 1u64, errors = [\
             (A) \"r\" { help = \"h\" } span = a::b [1, 2] { x = 1 }, \
             (B) \"s\" later = Vec<u8, A>, \
             (C) \"t\" { message = \"m\" } flag])",
        )
        .unwrap();
        assert_eq!(f.errors.len(), 3);
        assert_eq!(f.errors[0].help.as_deref(), Some("h"));
        assert_eq!(f.errors[1].reason, "s");
        assert!(f.errors[1].message.is_none());
        assert_eq!(f.errors[2].message.as_deref(), Some("m"));
        assert_eq!(squash(&f.errors[2].tokens), "C");
    }

    #[test]
    fn the_crate_of_the_definer_is_in_the_record_when_it_is_known() {
        let mut f = failure(&[("Local", WITHOUT_REPEATER)]);
        assert!(f.krate.is_none());
        // Without it, the record has no key for it, and reads back without it.
        let meta = squash(f.meta());
        assert!(!meta.contains("krate"), "{}", meta);
        assert!(read(&f).krate.is_none());
        // With it: the key is after the errors, and it is read back.
        f = f.with_krate("my_lib");
        let meta = squash(f.meta());
        assert!(meta.ends_with(",krate=\"my_lib\")"), "{}", meta);
        assert_eq!(read(&f).krate.as_deref(), Some("my_lib"));
        // An old reader of the record (one that takes `id` and `errors` and no more) is what
        // `read_meta` is without `krate`: a record written before the key existed.
        let f = read_meta("type_leak_error(id = 1u64, errors = [(A) \"r\"])").unwrap();
        assert!(f.krate.is_none());
        // It is a string.
        assert!(read_meta("type_leak_error(id = 1u64, errors = [], krate = 3)").is_err());
    }

    #[test]
    fn a_dollar_crate_at_the_head_of_the_path_is_the_crate_of_the_definer() {
        let segments = |s: &[&str]| s.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        // With the name: it is in its place.
        assert_eq!(
            resolve_dollar_crate(segments(&["$crate", "m", "Item"]), Some("lib")),
            ["lib", "m", "Item"]
        );
        assert_eq!(
            resolve_dollar_crate(segments(&["$crate", "Item"]), Some("lib")),
            ["lib", "Item"]
        );
        // Without it: it is left out, and a single segment is left for an item at the root.
        assert_eq!(
            resolve_dollar_crate(segments(&["$crate", "m", "Item"]), None),
            ["m", "Item"]
        );
        assert_eq!(
            resolve_dollar_crate(segments(&["$crate", "Item"]), None),
            ["Item"]
        );
        // Any other path is as it is, with a name or without.
        for krate in [Some("lib"), None] {
            for path in [
                &["m", "Item"][..],
                &["crate", "m", "Item"],
                &["Item"],
                &["a", "$crate", "Item"],
            ] {
                assert_eq!(resolve_dollar_crate(segments(path), krate), segments(path));
            }
        }
    }

    #[test]
    fn the_path_of_a_cause_and_how_it_is_written() {
        let path = |s: &str| {
            cause_path(&s.parse::<TokenStream>().unwrap()).map(|c| (c.path, c.written, c.generic))
        };
        let some = |p: &str, w: &str, g: bool| Some((p.to_string(), w.to_string(), g));
        assert_eq!(path("Local"), some("Local", "Local", false));
        assert_eq!(path("& 'static Local"), some("Local", "Local", false));
        assert_eq!(
            path("& inner :: Local"),
            some("inner::Local", "inner::Local", false)
        );
        assert_eq!(
            path("Wrap < 'a , Local >"),
            some("Wrap", "Wrap<'a, Local>", true)
        );
        assert_eq!(
            path("* const Wrap < u8 > :: Out"),
            some("Wrap", "Wrap<u8>", true)
        );
        // An absolute path keeps its lead, and a `$crate` is `crate`.
        assert_eq!(
            path("dyn :: core :: fmt :: Debug"),
            some("::core::fmt::Debug", "::core::fmt::Debug", false)
        );
        assert_eq!(
            path("$crate :: Local"),
            some("crate::Local", "crate::Local", false)
        );
        // Arguments after `::`, and a `->` that is not the end of the arguments.
        assert_eq!(path("Vec :: < u8 >"), some("Vec", "Vec::<u8>", true));
        assert_eq!(
            path("Wrap < fn (u8) -> Local > :: Out"),
            some("Wrap", "Wrap<fn (u8) -> Local>", true)
        );
        // Parenthesized arguments are not generic arguments.
        assert_eq!(path("Fn (Local) -> u8"), some("Fn", "Fn", false));
        // No name in the tokens.
        assert_eq!(path("& 'static"), None);
        assert_eq!(path(""), None);
    }

    #[test]
    fn the_texts_of_a_call_name_the_path_in_the_cause() {
        // As the check at the call shows them, for a record that has no texts of its own.
        let f = failure(&[
            ("& 'static UiShared", WITHOUT_REPEATER),
            ("* const UiRaw", WITHOUT_REPEATER),
            ("dyn Base", TRAIT_PATH),
        ]);
        let texts = cause_texts(&f);
        assert_eq!(texts[0].0, "use absolute path: 'UiShared'");
        assert_eq!(
            texts[0].2[1],
            "write `UiShared` with an absolute path (e.g. `crate::UiShared`), or add `repeater = ..` \
             to the `#[announce]` of `Leaky`"
        );
        assert_eq!(texts[1].0, "use absolute path: 'UiRaw'");
        assert_eq!(texts[2].0, "use absolute path: 'Base'");
        assert_eq!(
            texts[2].2[1],
            "write `Base` with an absolute path (e.g. `crate::Base`)"
        );
        // The same cause in `to_error` and in the definition error is the whole type: they say
        // what cannot be named.
        let e = f.to_error().to_string();
        assert!(
            e.contains("`&'static UiShared` cannot be named outside its module"),
            "{}",
            e
        );
    }

    #[test]
    fn a_name_of_the_std_prelude_is_not_written_under_crate() {
        let f = failure(&[("Option < Local >", ALIAS), ("Clone", TRAIT_PATH)]);
        for (_, _, notes) in cause_texts(&f) {
            let help = notes.last().unwrap();
            assert!(help.contains("a name of the std prelude"), "{}", help);
            assert!(help.contains("allow_std_prelude = true"), "{}", help);
            assert!(!help.contains("crate::"), "{}", help);
        }
    }

    #[test]
    fn the_texts_of_the_other_names_that_resolve_in_the_module_only() {
        let f = failure(&[
            ("Base", TRAIT_PATH),
            ("N", VALUE_PATH),
            ("Wrap < Local >", ALIAS),
        ]);
        let t = cause_texts(&f);
        assert_eq!(
            t[0],
            (
                "use absolute path: 'Base'".to_string(),
                "relative trait path, not in scope where `deflib::lm::Leaky!` is called"
                    .to_string(),
                vec![
                    "`deflib::lm::Leaky` is used outside its module `deflib::lm`".to_string(),
                    "write `Base` with an absolute path (e.g. `crate::Base`)".to_string()
                ]
            )
        );
        assert!(
            t[1].1.starts_with("relative value path, not in scope"),
            "{:?}",
            t[1]
        );
        assert_eq!(t[2].0, "use absolute path: 'Wrap<Local>'");
        assert!(t[2]
            .1
            .starts_with("relative path in a type alias, not in scope"));
        assert_eq!(
            t[2].2[1],
            "write `Wrap` with an absolute path (e.g. `crate::Wrap`)"
        );
    }

    #[test]
    fn the_texts_of_what_cannot_be_carried_are_not_about_scope() {
        // A const generic of a type that is not primitive.
        let f = failure(&[("crate :: inner :: UiIdx", NOT_PRIMITIVE)]);
        assert_eq!(
            cause_texts(&f),
            [(
                "const parameter of a non-primitive type: 'crate::inner::UiIdx'".to_string(),
                "cannot be carried, so `deflib::lm::Leaky!` works only in `deflib::lm`".to_string(),
                vec![
                    "`deflib::lm::Leaky` is used outside its module `deflib::lm`".to_string(),
                    "a const parameter of a type that is not primitive cannot be carried"
                        .to_string(),
                    "use a primitive type for the const parameter".to_string()
                ]
            )]
        );
        // Another reason says what it is, as a note; the headline names the tokens.
        let f = failure(&[("Odd", "with a generic argument that cannot be encoded")]);
        let (message, label, notes) = &cause_texts(&f)[0];
        assert_eq!(message, "cannot be carried: 'Odd'");
        assert!(label.starts_with("cannot be carried, so"), "{}", label);
        assert_eq!(notes[1], "a generic argument that cannot be encoded");
        assert_eq!(notes[2], "write the path absolutely");
        // A single-segment path has no module to name; no tokens, no quotes.
        let mut f = failure(&[("", "with a type that cannot be encoded (x)")]);
        f.path = syn::parse_quote!(Leaky);
        let (message, label, notes) = &cause_texts(&f)[0];
        assert_eq!(message, "a type of the item cannot be carried");
        assert_eq!(
            label,
            "cannot be carried, so `Leaky!` works only in its module"
        );
        assert_eq!(notes[0], "`Leaky` is used outside its module");
    }

    #[test]
    fn what_the_record_carries_is_used_as_it_is() {
        // type-leak's message, why and help for a refusal; the notes come before the help.
        let mut f = failure(&[("Vec < &'_ u8 >", REFUSED)]);
        f.errors[0] = LeakError::new("Vec < &'_ u8 >".parse::<TokenStream>().unwrap(), REFUSED)
            .with_message("use absolute path: 'Vec < &'_ u8 >'")
            .with_notes(["it cannot be leaked as a whole because of `'_`"])
            .with_help("use `::std::vec::Vec` instead of `Vec`");
        assert_eq!(
            cause_texts(&f),
            [(
                "use absolute path: 'Vec < &'_ u8 >'".to_string(),
                "cannot be carried, so `deflib::lm::Leaky!` works only in `deflib::lm`".to_string(),
                vec![
                    "`deflib::lm::Leaky` is used outside its module `deflib::lm`".to_string(),
                    "it cannot be leaked as a whole because of `'_`".to_string(),
                    "use `::std::vec::Vec` instead of `Vec`".to_string()
                ]
            )]
        );
    }

    /// The text of the first and of the last token of a cause where its error is reported.
    fn extent(cause: &str) -> (String, String) {
        let (first, last) = cause_extent(&cause.parse().unwrap()).unwrap();
        (first.to_string(), last.to_string())
    }

    #[test]
    fn the_error_of_a_cause_is_from_the_start_of_the_path_in_it_to_its_end() {
        // The path of the type, whatever stands around it: from its first token to its last.
        for (cause, first, last) in [
            ("Local", "Local", "Local"),
            ("&'static Local", "Local", "Local"),
            ("& 'a mut Local", "Local", "Local"),
            ("&'static mut UiUnique", "UiUnique", "UiUnique"),
            ("&'static UiShared", "UiShared", "UiShared"),
            ("*const UiRaw", "UiRaw", "UiRaw"),
            ("*mut Local", "Local", "Local"),
            ("dyn Base", "Base", "Base"),
            ("&'a dyn Base", "Base", "Base"),
            ("impl Base", "Base", "Base"),
            ("for<'a> fn(&'a Local)", "Local", "Local"),
            ("unsafe extern \"C\" fn(Local)", "Local", "Local"),
            // In a group: the name in it, and not the rest of the group.
            ("[Local; 2]", "Local", "Local"),
            ("(Local, u8)", "Local", "Local"),
            ("((Local, u8), Other)", "Local", "Local"),
            // The segments of a path.
            ("inner :: Local", "inner", "Local"),
            ("crate :: inner :: Local", "crate", "Local"),
            ("&inner :: Local", "inner", "Local"),
            ("dyn :: core :: fmt :: Debug", ":", "Debug"),
            // The arguments in angle brackets are in it, and what follows them is not.
            ("Wrap < 'a , Local >", "Wrap", ">"),
            ("Wrap < Pair < Local , u8 > >", "Wrap", ">"),
            ("& 'a Wrap < u8 > :: Out", "Wrap", ">"),
            ("Vec :: < Local >", "Vec", ">"),
            ("inner :: Wrap < fn (u8) -> Local >", "inner", ">"),
            ("* const UiWrap < '_ >", "UiWrap", ">"),
            // Parenthesized arguments are not generic arguments.
            ("Fn (Local) -> u8", "Fn", "Fn"),
        ] {
            assert_eq!(
                extent(cause),
                (first.to_string(), last.to_string()),
                "{}",
                cause
            );
        }
        // The name of a lifetime is not a name of anything.
        assert_eq!(extent("&'static u8"), ("u8".into(), "u8".into()));
        // Nothing that names: the whole cause, and a group without tokens.
        assert_eq!(extent("&'static"), ("&".into(), "static".into()));
        assert_eq!(extent("'a"), ("'".into(), "a".into()));
        assert_eq!(extent("* const"), ("*".into(), "const".into()));
        assert_eq!(extent("()"), ("()".into(), "()".into()));
        // No tokens: no place for the error, which is at the caller's path.
        assert!(cause_extent(&TokenStream::new()).is_none());
        assert!(cause_range(&TokenStream::new()).is_none());
        assert!(cause_range(&"Local".parse().unwrap()).is_some());
    }

    #[test]
    fn assume_success_is_a_block_for_each_distinct_cause() {
        let f = failure(&[
            ("Local", WITHOUT_REPEATER),
            ("Other", WITHOUT_REPEATER),
            ("Local", WITHOUT_REPEATER),
            ("Local", TRAIT_PATH),
        ]);
        let out = squash(f.assume_success(&syn::parse_quote!(::announce)));
        // Three: `Local` twice is one, and the same tokens with another reason are another.
        assert_eq!(out.matches("const_:()={").count(), 3, "{}", out);
        assert_eq!(out.matches("traitInModuleOf_Leaky{}").count(), 3, "{}", out);
        // Each has the function that needs the trait, an associated function of a type of its own.
        assert_eq!(
            out.matches(
                "structCheck;implCheck{fnused_outside_its_module<T:InModuleOf_Leaky>(_:T){}}"
            )
            .count(),
            3,
            "{}",
            out
        );
        assert_eq!(
            out.matches(
                "Check::used_outside_its_module(__AnnounceProbe::<42u64>.__announce_ctx());"
            )
            .count(),
            3,
            "{}",
            out
        );
        assert!(!out.contains("assume_in_module"), "{}", out);
        // The texts of each, where the compiler knows `on_unimplemented`.
        if cfg!(announce_diagnostic) {
            assert!(
                out.contains("message=\"useabsolutepath:'Other'\""),
                "{}",
                out
            );
            assert!(
                out.contains("label=\"relativetype,notinscopewhere`deflib::lm::Leaky!`iscalled\""),
                "{}",
                out
            );
            assert!(
                out.contains("note=\"`deflib::lm::Leaky`isusedoutsideitsmodule`deflib::lm`\""),
                "{}",
                out
            );
        }
        // No cause: one check, with a headline of its own.
        let none = squash(failure(&[]).assume_success(&syn::parse_quote!(::announce)));
        assert_eq!(none.matches("const_:()={").count(), 1, "{}", none);
    }

    #[test]
    fn assume_success_is_a_block_with_the_texts_and_the_probe() {
        let f = failure(&[("Wrap < { N } >", WITHOUT_REPEATER)]);
        let out = squash(f.assume_success(&syn::parse_quote!(::announce)));
        assert!(out.starts_with("const_:()={"), "{}", out);
        assert!(out.ends_with("};"), "{}", out);
        // The import that the method call needs, the probe, the trait of the item and its impl.
        assert!(
            out.contains(
                "use::announce::__private::ContextFallbackas_;\
                 use::announce::__private::Probeas__AnnounceProbe;"
            ),
            "{}",
            out
        );
        assert!(out.contains("traitInModuleOf_Leaky{}"), "{}", out);
        assert!(
            out.contains("implInModuleOf_Leakyfor::announce::__private::ThisCallInTheModule{}"),
            "{}",
            out
        );
        assert!(
            out.contains(
                "structCheck;implCheck{fnused_outside_its_module<T:InModuleOf_Leaky>(_:T){}}"
            ),
            "{}",
            out
        );
        assert!(
            out.ends_with(
                "fn__check(){Check::used_outside_its_module(__AnnounceProbe::<42u64>.__announce_ctx());}};"
            ),
            "{}",
            out
        );
        // The texts, where the compiler knows `on_unimplemented`. Braces of the type are
        // doubled: they are format strings.
        let texts = out.contains("#[diagnostic::on_unimplemented(");
        assert_eq!(texts, cfg!(announce_diagnostic), "{}", out);
        if texts {
            assert!(
                out.contains("message=\"useabsolutepath:'Wrap<{{N}}>'\","),
                "{}",
                out
            );
            assert!(
                out.contains("label=\"relativetype,notinscopewhere`deflib::lm::Leaky!`iscalled\","),
                "{}",
                out
            );
            assert!(
                out.contains("note=\"`deflib::lm::Leaky`isusedoutsideitsmodule`deflib::lm`\","),
                "{}",
                out
            );
            assert!(
                out.contains("note=\"write`Wrap`withanabsolutepath(e.g.`crate::Wrap`),oradd`repeater=..`tothe`#[announce]`of`Leaky`\""),
                "{}",
                out
            );
        }
        assert_eq!(
            out.contains("#[diagnostic::do_not_recommend]"),
            cfg!(announce_do_not_recommend),
            "{}",
            out
        );
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

    fn same_context_out(krate: &str, input: &str) -> String {
        squash(same_context(
            format!("{}, {}", krate, input).parse().unwrap(),
        ))
    }

    #[test]
    fn same_context_checks_the_failures_and_calls_the_next_element() {
        let failing =
            "a::Leaky! => [type_leak_error(id = 7u64, errors = [(Local) \"without `repeater`\"])] \
                       { pub struct Leaky { pub a: Local } }";
        let plain = "a::Plain! => [] { pub struct Plain; }";
        let out = same_context_out(
            "::announce",
            &format!(
                "@announce_format \"1\" [sc!, cb!] {} {} REST",
                failing, plain
            ),
        );
        // The check, then the next element with the input unchanged: the list without the head,
        // the entries (and none added for `Plain`), the rest.
        assert!(out.starts_with("const_:()={"), "{}", out);
        assert!(out.contains("traitInModuleOf_Leaky{}"), "{}", out);
        assert!(!out.contains("InModuleOf_Plain"), "{}", out);
        assert_eq!(out.matches("const_:()=").count(), 1, "{}", out);
        let call = out
            .split("};cb!")
            .nth(1)
            .unwrap_or_else(|| panic!("{}", out));
        assert_eq!(
            format!("cb!{}", call),
            format!(
                "cb!{{@announce_format\"1\"[cb!]{}{}REST}}",
                squash(failing.parse::<TokenStream>().unwrap()),
                squash(plain.parse::<TokenStream>().unwrap())
            )
        );
        // Several failures: a check for each, in the order of the chain (oldest first).
        let second = failing.replace("Leaky", "Other");
        let out = same_context_out("::announce", &format!("[sc!, cb!] {} {}", failing, second));
        let (l, o) = (
            out.find("InModuleOf_Leaky").unwrap(),
            out.find("InModuleOf_Other").unwrap(),
        );
        assert!(o < l, "the newest entry comes first in the input: {}", out);
        // `sc!` last in the chain: only the checks.
        let out = same_context_out("::announce", &format!("[sc!] {}", failing));
        assert!(out.ends_with("};"), "{}", out);
        // An entry without a failure: no check, but the call goes on.
        let out = same_context_out("::announce", &format!("[sc!, cb!] {}", plain));
        assert_eq!(
            out,
            format!(
                "cb!{{@announce_format\"1\"[cb!]{}}}",
                squash(plain.parse::<TokenStream>().unwrap())
            )
        );
        // In expression position the output is one expression.
        let out = same_context_out("::announce", &format!("(expr) [sc!, cb!] {}", failing));
        assert!(
            out.starts_with("{const_:()={") && out.ends_with("}"),
            "{}",
            out
        );
        // Errors: a malformed input, and a malformed record.
        let e = same_context("::announce [sc!]".parse().unwrap()).to_string();
        assert!(e.contains("compile_error"), "{}", e);
        let e = same_context_out(
            "::announce",
            "[sc!] a::B! => [type_leak_error(id = 1u64)] { struct B; }",
        );
        assert!(
            e.contains("compile_error") && e.contains("missing`errors`"),
            "{}",
            e
        );
    }

    /// The messages of `to_error`, one for each error.
    fn messages(f: &LeakFailure) -> Vec<String> {
        f.to_error().into_iter().map(|e| e.to_string()).collect()
    }

    #[test]
    fn to_error_is_a_plain_line_for_each_cause_that_ends_with_the_fix() {
        // A relative type without a `repeater`, a relative trait path, and the other relative
        // names: the clause and the fix of the error at the definition, with "its" attribute.
        let f = failure(&[
            ("Local", WITHOUT_REPEATER),
            ("Base", TRAIT_PATH),
            ("N", VALUE_PATH),
            ("Option < Local >", ALIAS),
        ]);
        assert_eq!(
            messages(&f),
            [
                "this macro cannot use `deflib::lm::Leaky`: `Local` cannot be named outside its module; \
                 add `repeater = ..` to its `#[announce]`, or write the path absolutely",
                "this macro cannot use `deflib::lm::Leaky`: `Base` cannot be named outside its module \
                 (a relative trait path in a bound); write the path absolutely",
                "this macro cannot use `deflib::lm::Leaky`: `N` cannot be named outside its module \
                 (a relative value path); write the path absolutely",
                "this macro cannot use `deflib::lm::Leaky`: `Option<Local>` cannot be named outside its \
                 module (a relative path in a type alias); write the path absolutely",
            ]
        );
        // One line each, whatever the reasons, and no mention of the record.
        for m in messages(&f) {
            assert!(!m.contains('\n') && !m.contains("strict"), "{}", m);
        }
        // A path of one segment, and a leading `::`, are the path as written.
        let mut f = failure(&[("Local", WITHOUT_REPEATER)]);
        f.path = syn::parse_quote!(Leaky);
        assert!(messages(&f)[0].starts_with("this macro cannot use `Leaky`: "));
        f.path = syn::parse_quote!(::deflib::Leaky);
        assert!(messages(&f)[0].starts_with("this macro cannot use `::deflib::Leaky`: "));
    }

    #[test]
    fn to_error_says_what_a_table_cannot_carry_without_scope() {
        // A const generic of a type that is not primitive: the path may be absolute, and the
        // type of the parameter is what to change.
        let f = failure(&[("crate :: inner :: UiIdx", NOT_PRIMITIVE)]);
        assert_eq!(
            messages(&f),
            [
                "this macro cannot use `deflib::lm::Leaky`: `crate::inner::UiIdx` cannot be carried \
                 (a const generic that is not of a primitive type); \
                 use a primitive type for the const parameter"
            ]
        );
        // A refusal says it once; the other reasons say what is wrong with the type; the fix is
        // the one of the error at the definition for the reason.
        let f = failure(&[
            ("Wrap < '_ >", REFUSED),
            ("G < Self >", "with `Self` in a carried type"),
        ]);
        assert_eq!(
            messages(&f),
            [
                "this macro cannot use `deflib::lm::Leaky`: `Wrap<'_>` cannot be carried; \
                 write the path absolutely",
                "this macro cannot use `deflib::lm::Leaky`: `G<Self>` cannot be carried \
                 (`Self` in a carried type); write the path absolutely",
            ]
        );
        // A reason that this version does not know is a cause that cannot be carried, with the
        // general fix.
        let f = failure(&[("Odd", "because of something new")]);
        assert_eq!(
            messages(&f),
            [
                "this macro cannot use `deflib::lm::Leaky`: `Odd` cannot be carried \
                 (because of something new); write the path absolutely"
            ]
        );
        // A cause without tokens is "a type of the item".
        let f = failure(&[("", "with a type that cannot be encoded (x)")]);
        assert_eq!(
            messages(&f),
            [
                "this macro cannot use `deflib::lm::Leaky`: a type of the item cannot be carried \
                 (a type that cannot be encoded (x)); write the path absolutely"
            ]
        );
        let f = failure(&[("", WITHOUT_REPEATER)]);
        assert_eq!(
            messages(&f),
            [
                "this macro cannot use `deflib::lm::Leaky`: a type of the item cannot be named \
                 outside its module; add `repeater = ..` to its `#[announce]`, or write the path \
                 absolutely"
            ]
        );
    }

    #[test]
    fn to_error_is_one_error_for_each_distinct_cause_and_one_for_none() {
        // The same cause twice is one error; two causes are two, in the order of the record.
        let f = failure(&[
            ("A", WITHOUT_REPEATER),
            ("B", WITHOUT_REPEATER),
            ("A", WITHOUT_REPEATER),
            ("crate :: Idx", NOT_PRIMITIVE),
        ]);
        let m = messages(&f);
        assert_eq!(m.len(), 3, "{:?}", m);
        assert!(m[0].contains("`A` cannot be named") && m[1].contains("`B` cannot be named"));
        assert!(m[2].contains("`crate::Idx` cannot be carried"));
        // Nothing recorded: still one error, without a fix.
        assert_eq!(
            messages(&failure(&[])),
            [
                "this macro cannot use `deflib::lm::Leaky`: a type of the item cannot be named \
                 outside its module"
            ]
        );
    }

    #[test]
    fn the_error_at_the_definition_and_at_the_call_share_their_fixes() {
        // The fix of each reason is the same; the attribute is "the" one where the error points
        // at the item, and "its" one at a call.
        for (reason, fix) in [
            (
                WITHOUT_REPEATER,
                "add `repeater = ..` to {}, or write the path absolutely",
            ),
            (TRAIT_PATH, "write the path absolutely"),
            (
                NOT_PRIMITIVE,
                "use a primitive type for the const parameter",
            ),
            (REFUSED, "write the path absolutely"),
        ] {
            let f = failure(&[("X", reason)]);
            let at_definition = f.to_definition_error().to_string();
            let at_call = messages(&f).remove(0);
            assert!(at_definition.ends_with(&fix.replace("{}", "the `#[announce]`")));
            assert!(at_call.ends_with(&fix.replace("{}", "its `#[announce]`")));
        }
    }
}
