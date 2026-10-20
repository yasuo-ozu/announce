//! The record of a definition whose types do not resolve outside its module: the
//! `type_leak_error(..)` META, and what a callback makes of it.
//!
//! By default `#[announce]` refuses such an item: it is an error at the definition
//! ([`LeakFailure::to_definition_error`]). A definer that writes `strict = false` does not
//! refuse it. It cannot know where the trampoline will be called, and a call in the item's
//! own module is fine: the item, as written, resolves there. So the definer records what it
//! found ([`LeakFailure`]) and leaves the decision to the call. A callback reads it with
//! [`Entry::leak_failure`](crate::helper::Entry::leak_failure). A callback that needs a type
//! table gets [`LeakFailure::to_error`] from
//! [`Entry::type_leak`](crate::helper::Entry::type_leak): the item has none, and the message
//! says so wherever the callback is called.

use crate::helper::KvArgs;
use proc_macro2::{Ident, Span, TokenStream, TokenTree};
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
    /// The headline of an error about this cause at a call, for example
    /// `use absolute path: 'Local'`.
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
/// - `id`: the random number of the announcement.
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
#[derive(Clone, Debug)]
#[non_exhaustive]
pub struct LeakFailure {
    /// The random number of the announcement.
    pub id: LitInt,
    /// What does not resolve.
    pub errors: Vec<LeakError>,
    /// The path the caller wrote to call the trampoline, without the `!`. Its last segment
    /// carries the span of the call: [`to_error`](Self::to_error) points at it. The definer
    /// sets the name of the item.
    pub path: Path,
    /// The name of the item.
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
    /// A record with no cause is one error without a fix.
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
