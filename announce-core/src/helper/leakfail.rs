//! The record of a definition whose types do not resolve outside its module
//! (`type_leak_error(..)` in `META`), and what a callback makes of it.
//!
//! `#[announce]` fails at the definition ([`LeakFailure::to_definition_error`]) unless
//! `strict = false`, which records the failure for the call: `announce::same_context!`
//! ([`same_context()`]) and [`Entry::item_resolved`](crate::helper::Entry::item_resolved) emit
//! [`LeakFailure::assume_success`]; [`Entry::type_leak`](crate::helper::Entry::type_leak)
//! returns [`LeakFailure::to_error`].

use crate::helper::keys::{bracketed_list, need, read_element, read_keys, skip_to_comma};
use proc_macro2::{Delimiter, Group, Ident, Span, TokenStream, TokenTree};
use syn::ext::IdentExt;
use syn::parse::{Parse, ParseStream, Parser};
use syn::{LitInt, LitStr, Path, Result, Token};
use template_quote::{quote, ToTokens};

// The reasons of a cause (`LeakError::reason`): the first four are names that resolve in the
// item's module only, `REFUSED` a type that type-leak refuses (`Wrap<'_>`, `Local<Self>`).
pub(crate) const WITHOUT_REPEATER: &str = "without `repeater`";
pub(crate) const TRAIT_PATH: &str = "with a relative trait path in a bound";
pub(crate) const VALUE_PATH: &str = "with a relative value path";
pub(crate) const ALIAS: &str = "with a relative path in a type alias";
pub(crate) const REFUSED: &str = "with a type that cannot be carried";
pub(crate) const NOT_PRIMITIVE: &str = "with a const generic that is not of a primitive type";

/// Whether a reason is about a name that resolves in the item's module only.
fn is_scope(reason: &str) -> bool {
    matches!(reason, WITHOUT_REPEATER | TRAIT_PATH | VALUE_PATH | ALIAS)
}

fn why_text(reason: &str) -> &str {
    reason.strip_prefix("with ").unwrap_or(reason)
}

/// ` (why)` for a message; nothing for [`WITHOUT_REPEATER`], which the fix already says.
fn why(reason: &str) -> String {
    match reason {
        WITHOUT_REPEATER => String::new(),
        _ => format!(" ({})", why_text(reason)),
    }
}

/// What the definer can do about a cause; `attr` names the item's attribute for the message.
fn fix_for(reason: &str, attr: &str) -> String {
    match reason {
        WITHOUT_REPEATER => format!(
            "add `repeater = ..` to {}, or write the path absolutely",
            attr
        ),
        NOT_PRIMITIVE => "use a primitive type for the const parameter".to_string(),
        _ => "write the path absolutely".to_string(),
    }
}

/// The segments of a path with a leading `$crate` replaced by `krate`, or left out when it is
/// unknown (a proc macro cannot tell which crate `$crate` means).
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

/// The errors as one, or `None` when there are none.
fn combine(errors: impl IntoIterator<Item = syn::Error>) -> Option<syn::Error> {
    errors.into_iter().reduce(|mut all, e| {
        all.combine(e);
        all
    })
}

/// The tokens as the caller reads them.
fn type_text(ts: &TokenStream) -> String {
    crate::leak::tidy(&ts.to_string()).replace("$crate", "crate")
}

/// The tokens in backticks, or "a type of the item" when there are none.
fn what(e: &LeakError) -> String {
    match type_text(&e.tokens) {
        ty if ty.is_empty() => "a type of the item".to_string(),
        ty => format!("`{}`", ty),
    }
}

/// What the definer could not carry: tokens of the definition, a short reason, and the texts of
/// the check's error (defaulted from the reason where missing).
#[derive(Clone, Debug)]
#[non_exhaustive]
pub struct LeakError {
    /// The offending tokens as written, with their spans.
    pub tokens: TokenStream,
    /// A short phrase telling a name that resolves in the module only from what no table can carry.
    pub reason: String,
    /// The headline, for example `use absolute path: 'Local'`.
    pub message: Option<String>,
    /// Why, one note each, the most useful first.
    pub notes: Vec<String>,
    /// What to write instead.
    pub help: Option<String>,
}

impl LeakError {
    /// An error at `tokens` with the phrase `reason` and no texts of its own.
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

    /// This error with the texts its reason has by default where it has none, for the item named
    /// `item`. A relative name is reported by its path ([`cause_path`]): `Local` for `&'static Local`.
    pub(crate) fn described(mut self, item: &str) -> Self {
        let text = type_text(&self.tokens);
        let (name, written) = cause_path(&self.tokens)
            .map_or_else(|| (text.clone(), text.clone()), |c| (c.path, c.written));
        let reason = self.reason.as_str();
        let scope = is_scope(reason);
        self.message.get_or_insert_with(|| {
            if scope {
                format!("use absolute path: '{}'", written)
            } else if reason == NOT_PRIMITIVE {
                format!("const parameter of a non-primitive type: '{}'", text)
            } else if text.is_empty() {
                "a type of the item cannot be carried".to_string()
            } else {
                format!("cannot be carried: '{}'", text)
            }
        });
        if self.notes.is_empty() {
            self.notes.extend(
                match reason {
                    NOT_PRIMITIVE => {
                        Some("a const parameter of a type that is not primitive cannot be carried")
                    }
                    REFUSED => None,
                    r if !scope => Some(why_text(r)),
                    _ => None,
                }
                .map(String::from),
            );
        }
        self.help.get_or_insert_with(|| {
            let write = format!("write `{}` with an absolute path", name);
            if !scope {
                fix_for(reason, "")
            } else if crate::leak::PRELUDE.contains(&name.as_str()) {
                // A prelude name may be shadowed by a caller, and is not under `crate::`.
                format!(
                    "{} (it is a name of the std prelude, which a caller may shadow), or add \
                     `allow_std_prelude = true` to the `#[announce]` of `{}`",
                    write, item
                )
            } else if reason == WITHOUT_REPEATER {
                format!(
                    "{} (e.g. `crate::{}`), or add `repeater = ..` to the `#[announce]` of `{}`",
                    write, name, item
                )
            } else {
                format!("{} (e.g. `crate::{}`)", write, name)
            }
        });
        self
    }
}

impl Parse for LeakError {
    /// Reads `(TOKENS) "reason"`, then optional `{ message = "..", notes = [".."], help = ".." }`
    /// and, up to the next `,`, what a later version adds.
    fn parse(input: ParseStream) -> Result<Self> {
        let tokens;
        syn::parenthesized!(tokens in input);
        let mut error = LeakError::new(
            tokens.parse::<TokenStream>()?,
            input.parse::<LitStr>()?.value(),
        );
        if input.peek(syn::token::Brace) {
            let texts;
            syn::braced!(texts in input);
            read_keys(&texts, |key, input| {
                match key {
                    "message" => error.message = Some(input.parse::<LitStr>()?.value()),
                    "notes" => {
                        let notes = bracketed_list::<LitStr>(input)?;
                        error.notes = notes.iter().map(LitStr::value).collect();
                    }
                    "help" => error.help = Some(input.parse::<LitStr>()?.value()),
                    _ => return Ok(false),
                }
                Ok(true)
            })?;
        }
        skip_to_comma(input)?;
        Ok(error)
    }
}

/// The record of a definition that has no type-leak table: the `type_leak_error(..)` META element
/// of an entry ([`Entry::leak_failure`](crate::helper::Entry::leak_failure)), read with
/// `syn::parse2::<LeakFailure>(meta)`.
///
/// ```text
/// type_leak_error(id = 42u64, item = Holder,
///                 errors = [(Local) "without `repeater`" { message = "..", notes = [".."], help = ".." },
///                           (Base) "with a relative trait path in a bound"],
///                 krate = "lib")
/// ```
///
/// - `id`: the announcement's random number, the `R` of the private marker that the definer emits
///   next to the item. [`assume_success`](Self::assume_success) calls the marker's method on
///   `announce::__private::Probe<R>`, which only the item's module has in scope.
/// - `item`: the item's name (a raw identifier is kept); the check's trait is named after it.
/// - `errors`: one `(TOKENS) "reason" { .. }` for each cause, `TOKENS` with the definition's
///   spans. The braces and each key in them are optional and default from the reason.
/// - `krate` (optional): the definer's crate as Cargo names it (`CARGO_CRATE_NAME`), which the
///   texts use for a call written through `$crate`; without it the `$crate::` is dropped.
///
/// `id`, `item` and `errors` are required. A reader skips keys it does not know, in the record
/// and in the braces, and tokens after an error's braces. An entry has this or `type_leak(..)`.
#[derive(Clone, Debug)]
#[non_exhaustive]
pub struct LeakFailure {
    /// The announcement's random number, the `R` of the marker.
    pub id: LitInt,
    /// What does not resolve.
    pub errors: Vec<LeakError>,
    /// The path the caller wrote for the trampoline, without the `!`. Its last segment carries
    /// the call's span, and an error about the call underlines all of it. The definer sets the
    /// item's name.
    pub path: Path,
    /// The item's name; the check's trait is named after it.
    pub ident: Ident,
    /// The definer's crate name, when known.
    pub krate: Option<String>,
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

    /// This record with the definer's crate name.
    pub fn with_krate(mut self, krate: impl Into<String>) -> Self {
        self.krate = Some(krate.into());
        self
    }

    /// The `type_leak_error(..)` META element, as the definer writes it.
    pub fn meta(&self) -> TokenStream {
        let (id, item) = (&self.id, &self.ident);
        let errors = self.errors.iter().map(|e| {
            let (tokens, reason, notes) = (&e.tokens, &e.reason, &e.notes);
            let texts: Vec<TokenStream> = vec![
                e.message.as_ref().map(|m| quote! { message = #m }),
                (!notes.is_empty()).then(|| quote! { notes = [ #(#notes),* ] }),
                e.help.as_ref().map(|h| quote! { help = #h }),
            ]
            .into_iter()
            .flatten()
            .collect();
            let braces = (!texts.is_empty()).then(|| quote! { { #(#texts),* } });
            quote! { (#tokens) #reason #braces }
        });
        let krate = self.krate.as_ref().map(|k| quote! { , krate = #k });
        quote! { type_leak_error(id = #id, item = #item, errors = [ #(#errors),* ] #krate) }
    }

    /// The trampoline path as the texts show it (`a::b::Item`), and its module's path (`a::b`)
    /// when it has one. A leading `$crate` is the definer's crate, or is left out when unknown.
    fn call_path(&self) -> (String, Option<String>) {
        let segments = self.path.segments.iter().map(|s| s.ident.to_string());
        let segments = resolve_dollar_crate(segments.collect(), self.krate.as_deref());
        let lead = if self.path.leading_colon.is_some() {
            "::"
        } else {
            ""
        };
        let join = |s: &[String]| format!("{}{}", lead, s.join("::"));
        let module = (segments.len() >= 2).then(|| join(&segments[..segments.len() - 1]));
        (join(&segments), module)
    }

    /// One error per cause at the call (all of its path is underlined), for a callback that needs
    /// a table and has none: "this macro cannot use `PATH`: `TYPE` cannot be named outside its
    /// module (WHY); FIX".
    pub fn to_error(&self) -> syn::Error {
        let lead = format!("this macro cannot use `{}`", self.call_path().0);
        let mut messages: Vec<String> = Vec::new();
        for e in &self.errors {
            let (what, reason) = (what(e), e.reason.as_str());
            let clause = if is_scope(reason) {
                format!("{} cannot be named outside its module{}", what, why(reason))
            } else if reason == REFUSED {
                format!("{} cannot be carried", what)
            } else {
                format!("{} cannot be carried{}", what, why(reason))
            };
            let message = format!(
                "{}: {}; {}",
                lead,
                clause,
                fix_for(reason, "its `#[announce]`")
            );
            if !messages.contains(&message) {
                messages.push(message);
            }
        }
        let at_call = |m| syn::Error::new_spanned(&self.path, m);
        combine(messages.into_iter().map(at_call)).unwrap_or_else(|| {
            at_call(format!(
                "{}: a type of the item cannot be named outside its module",
                lead
            ))
        })
    }

    /// One error per cause at its tokens in the item:
    /// "TYPE cannot be named outside its module (WHY): FIX".
    pub(crate) fn to_definition_error(&self) -> syn::Error {
        combine(self.errors.iter().map(|e| {
            let fix = fix_for(&e.reason, "the `#[announce]`");
            let message = format!(
                "{} cannot be named outside its module{}: {}",
                what(e),
                why(&e.reason),
                fix
            );
            syn::Error::new_spanned(&e.tokens, message)
        }))
        .unwrap_or_else(|| {
            syn::Error::new(
                Span::call_site(),
                "the item cannot be named outside its module",
            )
        })
    }

    /// The causes that get a check of their own, with the default texts of their reason: the
    /// first of those with the same headline and reason, in the order of the record.
    fn distinct_causes(&self) -> Vec<LeakError> {
        let item = self.ident.unraw().to_string();
        let mut causes: Vec<LeakError> = Vec::new();
        for e in &self.errors {
            let e = e.clone().described(&item);
            if !causes
                .iter()
                .any(|c| (&c.message, &c.reason) == (&e.message, &e.reason))
            {
                causes.push(e);
            }
        }
        causes
    }

    /// The headline, the label and the notes of the check's error for a described cause, in
    /// type-leak's layout. The way in, why, then what to do are all notes: `on_unimplemented`
    /// has no help.
    fn cause_text(&self, e: LeakError) -> (String, String, Vec<String>) {
        let (path, module) = self.call_path();
        let used = match &module {
            Some(m) => format!("`{}` is used outside its module `{}`", path, m),
            None => format!("`{}` is used outside its module", path),
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
            let module = module.map_or("its module".to_string(), |m| format!("`{}`", m));
            format!("cannot be carried, so `{}!` works only in {}", path, module)
        };
        let mut notes = vec![used];
        notes.extend(e.notes);
        notes.extend(e.help);
        (e.message.unwrap_or_default(), label, notes)
    }

    /// The check that a callback emits next to an item it uses as written. It compiles where the
    /// item's own names resolve (its module, function bodies and `use super::*` submodules) and
    /// fails elsewhere, with one error per distinct cause (same headline and reason, so
    /// `&'static Local` and `[Local; 2]` are one, `Wrap<u8>` and `Wrap<u16>` two) that underlines
    /// all of the cause in the definition. `announce::same_context!` shows the error. The texts
    /// need Rust 1.78 (`#[diagnostic::on_unimplemented]`); 1.85 keeps the error short
    /// (`#[diagnostic::do_not_recommend]`).
    ///
    /// The check is a `const _` item, so it cannot stand in an `impl` or `trait` block: emit it
    /// beside the impl. `krate` is the path of the `announce` crate where the tokens are emitted
    /// (`::announce` or `$crate`), the one the definition used.
    pub fn assume_success(&self, krate: &Path) -> TokenStream {
        let causes = self.distinct_causes();
        if causes.is_empty() {
            let label = format!("`{}!` is used outside its module", self.call_path().0);
            let message = "a type of the item cannot be named outside its module".to_string();
            return self.check(krate, (message, label, Vec::new()), None);
        }
        let check =
            |e: LeakError| self.check(krate, self.cause_text(e.clone()), cause_extent(&e.tokens));
        causes.into_iter().map(check).collect()
    }

    /// The check for one cause; `range` is where it starts and ends in the definition, else the
    /// path of the call.
    fn check(
        &self,
        krate: &Path,
        (message, label, notes): (String, String, Vec<String>),
        range: Option<(TokenTree, TokenTree)>,
    ) -> TokenStream {
        let span = self
            .path
            .segments
            .last()
            .map_or_else(Span::call_site, |s| s.ident.span());
        let tr = Ident::new(
            &format!("InModuleOf_{}", self.ident.unraw()),
            self.ident.span(),
        );
        let id = &self.id;
        // The texts are format strings of `on_unimplemented`: a brace is doubled.
        let lit =
            |s: &str| LitStr::new(&s.replace('{', "{{").replace('}', "}}"), Span::call_site());
        let (message, label) = (lit(&message), lit(&label));
        let notes: Vec<LitStr> = notes.iter().map(|n| lit(n)).collect();
        let texts = cfg!(announce_diagnostic).then(|| {
            quote! {
                #[diagnostic::on_unimplemented(
                    message = #message,
                    label = #label,
                    #(note = #notes),*
                )]
            }
        });
        let not_recommended =
            cfg!(announce_do_not_recommend).then(|| quote! { #[diagnostic::do_not_recommend] });
        // Every token has the caller's span, so rustc's note about the bound points at the call,
        // except the call itself: its first tokens are located at the start of the cause and its
        // last at the end, so the error underlines all of it. `located_at` keeps the caller's
        // hygiene, so names still resolve there.
        let respan = crate::runner::respan;
        let (first, last) = range
            .or_else(|| cause_extent(&self.path.to_token_stream()))
            .map_or((span, span), |(a, b)| {
                (span.located_at(a.span()), span.located_at(b.span()))
            });
        let paren = |inner: TokenStream, span: Span| {
            let mut group = Group::new(Delimiter::Parenthesis, inner);
            group.set_span(span);
            group
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
                // Associated: the callee's path can then span the cause (a free fn adds a second
                // span, "required by a bound introduced by this call").
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
        let check = respan(quote! { Check:: }, first);
        let name = respan(quote! { used_outside_its_module }, last);
        let probe = respan(quote! { __AnnounceProbe::<#id>.__announce_ctx }, first);
        let unit = paren(TokenStream::new(), last);
        let args = paren(quote! { #probe #unit }, last);
        let semi = respan(quote! { ; }, span);
        quote! { #konst { #head { #check #name #args #semi } } #semi }
    }
}

/// Keywords that only qualify a type.
const QUALIFIERS: &[&str] = &[
    "dyn", "impl", "mut", "const", "for", "fn", "unsafe", "extern", "as",
];

/// The level of `ts` (a group is its own level) that holds the first identifier that names
/// something, and its index. With `named`, a primitive type (`u8` in `[u8; N]`) is not one, nor
/// is an absolute path (`crate::Buf` in `crate::Buf<{ N }>`): they name nothing that could be
/// written absolutely. `quote` is whether the previous token was a lifetime's `'`.
fn locate(ts: &TokenStream, quote: &mut bool, named: bool) -> Option<(Vec<TokenTree>, usize)> {
    let level: Vec<TokenTree> = ts.clone().into_iter().collect();
    let is_ident = |i: usize| matches!(level.get(i), Some(TokenTree::Ident(_)));
    // In the segments of an absolute path.
    let mut absolute = false;
    let mut hit = None;
    for (i, tt) in level.iter().enumerate() {
        match tt {
            TokenTree::Group(g) => {
                *quote = false;
                absolute = false;
                if let Some(found) = locate(&g.stream(), quote, named) {
                    return Some(found);
                }
            }
            TokenTree::Punct(p) => {
                *quote = p.as_char() == '\'';
                absolute &= p.as_char() == ':';
            }
            TokenTree::Literal(_) => {
                *quote = false;
                absolute = false;
            }
            TokenTree::Ident(id) => {
                let name = id.to_string();
                let colons = i >= 2 && is_colon(level.get(i - 1)) && is_colon(level.get(i - 2));
                // `::a`, but not the `b` of `a::b`; a keyword such as `dyn` may come before.
                let leading = colons
                    && (i < 3
                        || !(is_ident(i - 3) || is_punct(level.get(i - 3), '>'))
                        || matches!(&level[i - 3], TokenTree::Ident(q)
                            if QUALIFIERS.contains(&q.to_string().as_str())));
                let head = name == "crate" || name == "$crate" || leading;
                absolute = named && (head || (absolute && colons));
                let skip = *quote
                    || QUALIFIERS.contains(&name.as_str())
                    || absolute
                    || (named && crate::fastpath::PRIMITIVES.contains(&name.as_str()));
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

fn is_punct(tt: Option<&TokenTree>, c: char) -> bool {
    matches!(tt, Some(TokenTree::Punct(p)) if p.as_char() == c)
}

fn is_colon(tt: Option<&TokenTree>) -> bool {
    is_punct(tt, ':')
}

/// The first and last token of a cause, where its error starts and ends: all of it, as the error
/// at the definition ([`LeakFailure::to_definition_error`]) is. `None` without tokens.
fn cause_extent(ts: &TokenStream) -> Option<(TokenTree, TokenTree)> {
    let mut all = ts.clone().into_iter();
    let first = all.next()?;
    let last = all.last().unwrap_or_else(|| first.clone());
    Some((first, last))
}

/// The path that a cause names, as [`cause_path`] reads it.
pub(crate) struct CausePath {
    /// The path without generic arguments, the name to write absolutely: `Wrap`.
    pub path: String,
    /// The path with its generic arguments as written: `Wrap<'a, Local>`.
    pub written: String,
    /// Whether generic arguments follow the path.
    pub generic: bool,
}

/// The path that a cause names, without what surrounds it: `Local` for `&'static Local`,
/// `Wrap` (written `Wrap<'a, Local>`) for `Wrap<'a, Local>`. It is the first identifier that
/// names something ([`locate`]) with the leading `::`, the `::` segments and the angle-bracketed
/// arguments around it (also `Vec::<T>`). A `$crate` reads as `crate`; `None` without an
/// identifier.
pub(crate) fn cause_path(ts: &TokenStream) -> Option<CausePath> {
    // A type of primitives only (`&'static u8`) is its primitive.
    let (level, i) = locate(ts, &mut false, true).or_else(|| locate(ts, &mut false, false))?;
    let start = if i >= 2 && is_colon(level.get(i - 1)) && is_colon(level.get(i - 2)) {
        i - 2
    } else {
        i
    };
    let mut segments = i + 1;
    while is_colon(level.get(segments))
        && is_colon(level.get(segments + 1))
        && matches!(level.get(segments + 2), Some(TokenTree::Ident(_)))
    {
        segments += 3;
    }
    let open = if is_colon(level.get(segments)) && is_colon(level.get(segments + 1)) {
        segments + 2
    } else {
        segments
    };
    let mut end = segments;
    if is_punct(level.get(open), '<') {
        let mut depth = 0usize;
        end = open;
        while let Some(tt) = level.get(end) {
            end += 1;
            if is_punct(Some(tt), '<') {
                depth += 1;
            } else if is_punct(Some(tt), '>') && !is_punct(level.get(end.wrapping_sub(2)), '-') {
                depth -= 1;
                if depth == 0 {
                    break;
                }
            }
        }
    }
    let text = |tokens: &[TokenTree]| tokens.iter().map(ToString::to_string).collect::<String>();
    Some(CausePath {
        path: text(&level[start..segments]).replace("$crate", "crate"),
        written: type_text(&level[start..end].iter().cloned().collect()),
        generic: end > segments,
    })
}

/// The function behind `announce::same_context!`: for each entry with a recorded failure it
/// emits [`LeakFailure::assume_success`], then forwards the input to the next element of the
/// list. `input` is `KRATE, INPUT`, the path of the `announce` crate and the callback input
/// ([`Input`](crate::helper::Input)). An error comes back as a `compile_error!`; see
/// [`try_same_context`] for a [`syn::Error`].
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
    // In expression position the checks are statements of a block.
    if input.options.as_ref().map_or(false, is_expression_position) {
        return Ok(quote! { { #out } });
    }
    Ok(out)
}

impl Parse for LeakFailure {
    /// Reads the `type_leak_error(..)` META element ([`meta`](Self::meta)). Its
    /// [`path`](Self::path) is the item's name; the caller sets the one it was called with.
    fn parse(input: ParseStream) -> Result<Self> {
        let (mut id, mut item, mut krate, mut errors) = (None, None, None, None);
        let name = read_element(input, "type_leak_error", |key, input| {
            match key {
                "id" => id = Some(input.parse::<LitInt>()?),
                "item" => item = Some(Ident::parse_any(input)?),
                "krate" => krate = Some(input.parse::<LitStr>()?.value()),
                "errors" => errors = Some(bracketed_list(input)?),
                _ => return Ok(false),
            }
            Ok(true)
        })?;
        let id = need(id, &name, "id")?;
        let ident = need(item, &name, "item")?;
        let errors = need(errors, &name, "errors")?;
        Ok(LeakFailure {
            id,
            errors,
            path: Path::from(ident.clone()),
            ident,
            krate,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tokens(s: &str) -> TokenStream {
        s.parse().unwrap()
    }

    fn squash(ts: impl ToTokens) -> String {
        ts.to_token_stream()
            .to_string()
            .split_whitespace()
            .collect()
    }

    fn has(out: &str, needles: &[&str]) {
        for n in needles {
            assert!(out.contains(n), "`{}` in {}", n, out);
        }
    }

    /// A failure of the item `Leaky`, called as `deflib::lm::Leaky!`.
    fn failure(errors: &[(&str, &str)]) -> LeakFailure {
        let errors = errors.iter().map(|(t, r)| LeakError::new(tokens(t), *r));
        let mut f = LeakFailure::new(
            syn::parse_quote!(Leaky),
            syn::parse_quote!(42u64),
            errors.collect(),
        );
        f.path = syn::parse_quote!(deflib::lm::Leaky);
        f
    }

    fn read_meta(src: &str) -> Result<LeakFailure> {
        syn::parse_str(src)
    }

    /// The texts of the check for the distinct causes of `f`: `(message, label, notes)`.
    fn cause_texts(f: &LeakFailure) -> Vec<(String, String, Vec<String>)> {
        let causes = f.distinct_causes();
        causes.into_iter().map(|e| f.cause_text(e)).collect()
    }

    #[test]
    fn the_meta_is_read_back_as_it_was_written() {
        let mut f = failure(&[("Wrap < '_ >", REFUSED), ("Local", WITHOUT_REPEATER)]);
        f.errors[0] = f.errors[0]
            .clone()
            .with_message("use absolute path: 'Wrap < '_ >'")
            .with_notes([
                "it cannot be leaked as a whole because of `'_`",
                "a { brace }",
            ])
            .with_help("write `Wrap` with an absolute path (e.g. `crate::Wrap`)");
        assert_eq!(
            squash(f.meta()),
            "type_leak_error(id=42u64,item=Leaky,errors=[(Wrap<'_>)\"withatypethatcannotbecarried\"{\
             message=\"useabsolutepath:'Wrap<'_>'\",notes=[\"itcannotbeleakedasawholebecauseof`'_`\",\
             \"a{brace}\"],help=\"write`Wrap`withanabsolutepath(e.g.`crate::Wrap`)\"},\
             (Local)\"without`repeater`\"])"
        );
        let mut raw = failure(&[]);
        raw.ident = syn::parse_quote!(r#type);
        for (f, krate) in [
            (f.clone(), None),
            (f.with_krate("my_lib"), Some("my_lib")),
            (raw, None),
        ] {
            let back: LeakFailure = syn::parse2(f.meta()).unwrap();
            assert_eq!(squash(back.meta()), squash(f.meta()));
            assert_eq!(back.ident, f.ident);
            assert_eq!(squash(&back.path), squash(&f.ident));
            assert_eq!(back.krate.as_deref(), krate);
            assert_eq!(squash(f.meta()).contains("krate"), krate.is_some());
        }
    }

    #[test]
    fn a_reader_skips_what_it_does_not_know_and_needs_id_item_and_errors() {
        // Keys with any value, or none, in the record, in the braces and after an error.
        let f = read_meta(
            "type_leak_error(flag, id = 1u64, future = [1, 2], item = B, ty = Vec<u8, A>, errors = [\
             (A) \"r\" { message = \"m\", span = a::b, level = -1, ty = Vec<u8, A>, internal, \
             notes = [\"n1\", \"n2\"], help = \"h\", last } span = a::b [1, 2] { x = 1 }, \
             (B) \"s\" later = Vec<u8, A>, (C) \"t\" { } flag], krate = \"k\")",
        )
        .unwrap();
        assert_eq!(f.krate.as_deref(), Some("k"));
        let (a, b, c) = (&f.errors[0], &f.errors[1], &f.errors[2]);
        assert_eq!(
            (a.message.as_deref(), a.help.as_deref()),
            (Some("m"), Some("h"))
        );
        assert_eq!(a.notes, ["n1", "n2"]);
        assert_eq!((b.reason.as_str(), &b.message), ("s", &None));
        assert_eq!(squash(&c.tokens), "C");
        assert!(c.message.is_none() && c.notes.is_empty() && c.help.is_none());
        for (src, key) in [
            ("id = 1u64, item = B", "errors"),
            ("item = B, errors = []", "id"),
            ("id = 1u64, errors = []", "item"),
        ] {
            let err = read_meta(&format!("type_leak_error({})", src)).unwrap_err();
            assert_eq!(
                err.to_string(),
                format!("missing `{}` in the `type_leak_error(..)` META", key)
            );
        }
        let err = read_meta("type_leak(id = 1u64, item = B, errors = [])").unwrap_err();
        assert_eq!(
            err.to_string(),
            "expected `type_leak_error(..)`, found `type_leak`"
        );
    }

    #[test]
    fn a_dollar_crate_at_the_head_of_the_path_is_the_crate_of_the_definer() {
        let segments = |s: &[&str]| s.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        let resolved = |s: &[&str], krate| resolve_dollar_crate(segments(s), krate);
        assert_eq!(
            resolved(&["$crate", "m", "Item"], Some("lib")),
            ["lib", "m", "Item"]
        );
        assert_eq!(resolved(&["$crate", "m", "Item"], None), ["m", "Item"]);
        assert_eq!(
            resolved(&["a", "$crate", "Item"], Some("lib")),
            ["a", "$crate", "Item"]
        );
        assert_eq!(resolved(&["crate", "Item"], None), ["crate", "Item"]);
    }

    #[test]
    fn the_path_of_a_cause_and_the_default_texts_that_name_it() {
        let check = |ts: &str, path: &str, written: &str| {
            let c = cause_path(&tokens(ts)).unwrap_or_else(|| panic!("{}", ts));
            let got = (c.path.as_str(), c.written.as_str(), c.generic);
            assert_eq!(got, (path, written, written != path), "{}", ts);
            // A prelude name has its own help.
            if !crate::leak::PRELUDE.contains(&path) {
                let e = LeakError::new(tokens(ts), WITHOUT_REPEATER).described("Item");
                assert_eq!(
                    e.message.unwrap(),
                    format!("use absolute path: '{}'", written)
                );
                let help =
                    "write `P` with an absolute path (e.g. `crate::P`), or add `repeater = ..` \
                            to the `#[announce]` of `Item`";
                assert_eq!(e.help.unwrap(), help.replace('P', path), "{}", ts);
            }
        };
        for (ts, path) in [
            ("Local", "Local"),
            ("inner :: Local", "inner::Local"),
            ("crate :: inner :: Local", "crate::inner::Local"),
            ("$crate :: Local", "crate::Local"),
            ("& 'static UiShared", "UiShared"),
            ("& 'static mut UiUnique", "UiUnique"),
            ("& mut UiUnique", "UiUnique"),
            ("* const UiRaw", "UiRaw"),
            ("* mut UiRaw", "UiRaw"),
            ("& inner :: Local", "inner::Local"),
            ("& & 'a UiShared", "UiShared"),
            ("dyn Base", "Base"),
            ("& 'a dyn Base", "Base"),
            ("dyn :: core :: fmt :: Debug", "::core::fmt::Debug"),
            ("(Local , u8)", "Local"),
            ("[Local ; 2]", "Local"),
            ("[(Local , u8) ; 2]", "Local"),
            ("& [Local]", "Local"),
            ("fn (Local) -> u8", "Local"),
            // A primitive type or an absolute path names nothing that can be written absolutely ...
            ("[u8 ; N]", "N"),
            ("(u8 , Local)", "Local"),
            ("[u8 ; inner :: N]", "inner::N"),
            ("crate :: Buf < { N } >", "N"),
            // ... unless there is no other path.
            ("& 'static u8", "u8"),
            ("[u8 ; 4]", "u8"),
            // Parenthesized arguments are not generic arguments.
            ("Fn (Local) -> u8", "Fn"),
        ] {
            check(ts, path, path);
        }
        for (ts, path, written) in [
            ("Wrap < 'a , Local >", "Wrap", "Wrap<'a, Local>"),
            (
                "Wrap < Pair < Local , u8 > >",
                "Wrap",
                "Wrap<Pair<Local, u8>>",
            ),
            ("& 'a Wrap < u8 >", "Wrap", "Wrap<u8>"),
            ("Holder < dyn Base >", "Holder", "Holder<dyn Base>"),
            ("* const Wrap < u8 > :: Out", "Wrap", "Wrap<u8>"),
            ("Vec :: < u8 >", "Vec", "Vec::<u8>"),
            (
                "Wrap < fn (u8) -> Local > :: Out",
                "Wrap",
                "Wrap<fn (u8) -> Local>",
            ),
        ] {
            check(ts, path, written);
        }
        assert!(cause_path(&tokens("& 'static")).is_none() && cause_path(&tokens("")).is_none());
    }

    #[test]
    fn a_name_of_the_std_prelude_is_not_written_under_crate() {
        for (ts, reason) in [
            ("Option < Local >", ALIAS),
            ("& Option < Local >", ALIAS),
            ("* const Option < Local >", ALIAS),
            ("Clone", TRAIT_PATH),
        ] {
            let name = if ts.contains("Option") { "Option" } else { ts };
            let help = LeakError::new(tokens(ts), reason)
                .described("Item")
                .help
                .unwrap();
            assert_eq!(
                help,
                format!(
                    "write `{}` with an absolute path (it is a name of the std prelude, which a caller \
                     may shadow), or add `allow_std_prelude = true` to the `#[announce]` of `Item`",
                    name
                )
            );
        }
        let e = LeakError::new(tokens("& inner :: Option"), ALIAS).described("Item");
        let help = "write `inner::Option` with an absolute path (e.g. `crate::inner::Option`)";
        assert_eq!(e.help.as_deref(), Some(help));
    }

    #[test]
    fn the_texts_of_the_check_follow_the_reason() {
        let way_in = "`deflib::lm::Leaky` is used outside its module `deflib::lm`";
        // Names that resolve in the module only: (tokens, reason, the kind in the label).
        for (ts, reason, kind) in [
            ("Local", WITHOUT_REPEATER, "relative type"),
            ("Base", TRAIT_PATH, "relative trait path"),
            ("N", VALUE_PATH, "relative value path"),
            ("W < L >", ALIAS, "relative path in a type alias"),
        ] {
            let (message, label, notes) = cause_texts(&failure(&[(ts, reason)])).remove(0);
            assert_eq!(
                message,
                format!("use absolute path: '{}'", ts.replace(' ', ""))
            );
            let scope = "not in scope where `deflib::lm::Leaky!` is called";
            assert_eq!(label, format!("{}, {}", kind, scope));
            let path = ts.split(' ').next().unwrap();
            let mut fix = format!(
                "write `{0}` with an absolute path (e.g. `crate::{0}`)",
                path
            );
            if reason == WITHOUT_REPEATER {
                fix += ", or add `repeater = ..` to the `#[announce]` of `Leaky`";
            }
            assert_eq!(notes, [way_in, &fix]);
        }
        // What no table can carry.
        let carried = "cannot be carried, so `deflib::lm::Leaky!` works only in `deflib::lm`";
        let (message, label, notes) =
            cause_texts(&failure(&[("crate :: UiIdx", NOT_PRIMITIVE)])).remove(0);
        assert_eq!(
            message,
            "const parameter of a non-primitive type: 'crate::UiIdx'"
        );
        assert_eq!(label, carried);
        assert_eq!(
            notes,
            [
                way_in,
                "a const parameter of a type that is not primitive cannot be carried",
                "use a primitive type for the const parameter"
            ]
        );
        let (message, label, notes) = cause_texts(&failure(&[(
            "Odd",
            "with a generic argument that cannot be encoded",
        )]))
        .remove(0);
        assert_eq!(
            (message.as_str(), label.as_str()),
            ("cannot be carried: 'Odd'", carried)
        );
        assert_eq!(
            notes[1..],
            [
                "a generic argument that cannot be encoded",
                "write the path absolutely"
            ]
        );
        // No tokens, and a call without a module.
        let mut f = failure(&[("", "with a type that cannot be encoded (x)")]);
        f.path = syn::parse_quote!(Leaky);
        let (message, label, notes) = cause_texts(&f).remove(0);
        assert_eq!(message, "a type of the item cannot be carried");
        assert_eq!(
            label,
            "cannot be carried, so `Leaky!` works only in its module"
        );
        assert_eq!(notes[0], "`Leaky` is used outside its module");
        // What the record carries is used as it is: the notes come before the help.
        let mut f = failure(&[("Vec < &'_ u8 >", REFUSED)]);
        f.errors[0] = f.errors[0]
            .clone()
            .with_message("use absolute path: 'Vec < &'_ u8 >'")
            .with_notes(["it cannot be leaked as a whole because of `'_`"])
            .with_help("use `::std::vec::Vec` instead of `Vec`");
        let (message, label, notes) = cause_texts(&f).remove(0);
        assert_eq!(
            (message.as_str(), label.as_str()),
            ("use absolute path: 'Vec < &'_ u8 >'", carried)
        );
        assert_eq!(
            notes[1..],
            [
                "it cannot be leaked as a whole because of `'_`",
                "use `::std::vec::Vec` instead of `Vec`"
            ]
        );
    }

    #[test]
    fn the_error_of_a_cause_is_all_of_it() {
        for (cause, first, last) in [
            ("Local", "Local", "Local"),
            ("&'static Local", "&", "Local"),
            ("*const UiRaw", "*", "UiRaw"),
            ("dyn Base", "dyn", "Base"),
            ("[Local; 2]", "[Local ; 2]", "[Local ; 2]"),
            ("(u8 , Local)", "(u8 , Local)", "(u8 , Local)"),
            ("inner :: Local", "inner", "Local"),
            (":: std :: vec :: Vec < [u8 ; N] >", ":", ">"),
            ("Wrap < 'a , Local >", "Wrap", ">"),
            ("& 'a Wrap < u8 > :: Out", "&", "Out"),
            ("Fn (Local) -> u8", "Fn", "u8"),
            ("< u8 as Tr > :: X", "<", "X"),
            ("&'static", "&", "static"),
            ("'a", "'", "a"),
            ("()", "()", "()"),
        ] {
            let (start, end) = cause_extent(&tokens(cause)).unwrap();
            assert_eq!(
                (start.to_string(), end.to_string()),
                (first.to_string(), last.to_string()),
                "{}",
                cause
            );
        }
        assert!(cause_extent(&TokenStream::new()).is_none());
    }

    #[test]
    fn assume_success_is_a_block_for_each_distinct_cause() {
        let f = failure(&[
            ("& 'static Local", WITHOUT_REPEATER),
            ("Local", WITHOUT_REPEATER),
            ("[Local ; 2]", WITHOUT_REPEATER),
            ("Wrap < u8 >", WITHOUT_REPEATER),
            ("Wrap < u16 >", WITHOUT_REPEATER),
            ("Wrap < u8 >", WITHOUT_REPEATER),
            // The same path with another reason is another cause.
            ("Local", TRAIT_PATH),
            // Without a path the whole tokens are shown.
            ("* const", WITHOUT_REPEATER),
            ("* const", WITHOUT_REPEATER),
        ]);
        // The first of the causes that show the same stands for the others.
        let causes: Vec<String> = f
            .distinct_causes()
            .iter()
            .map(|e| squash(&e.tokens))
            .collect();
        assert_eq!(
            causes,
            ["&'staticLocal", "Wrap<u8>", "Wrap<u16>", "Local", "*const"]
        );
        let out = squash(f.assume_success(&syn::parse_quote!(::announce)));
        assert_eq!(out.matches("const_:()={").count(), 5, "{}", out);
        // A failure without a cause has one check.
        let none = squash(failure(&[]).assume_success(&syn::parse_quote!(::announce)));
        assert_eq!(none.matches("const_:()={").count(), 1, "{}", none);
    }

    #[test]
    fn assume_success_is_a_block_with_the_texts_and_the_probe() {
        let f = failure(&[("Wrap < { N } >", WITHOUT_REPEATER)]);
        let out = squash(f.assume_success(&syn::parse_quote!(::announce)));
        assert!(
            out.starts_with("const_:()={") && out.ends_with("};"),
            "{}",
            out
        );
        has(
            &out,
            &[
                "use::announce::__private::ContextFallbackas_;use::announce::__private::Probeas__AnnounceProbe;",
                "traitInModuleOf_Leaky{}",
                "implInModuleOf_Leakyfor::announce::__private::ThisCallInTheModule{}",
                "structCheck;implCheck{fnused_outside_its_module<T:InModuleOf_Leaky>(_:T){}}",
                "fn__check(){Check::used_outside_its_module(__AnnounceProbe::<42u64>.__announce_ctx());}};",
            ],
        );
        let texts = out.contains("#[diagnostic::on_unimplemented(");
        assert_eq!(texts, cfg!(announce_diagnostic), "{}", out);
        if texts {
            // Braces of the type are doubled: they are format strings.
            has(
                &out,
                &[
                    "message=\"useabsolutepath:'Wrap<{{N}}>'\",",
                    "label=\"relativetype,notinscopewhere`deflib::lm::Leaky!`iscalled\",",
                    "note=\"`deflib::lm::Leaky`isusedoutsideitsmodule`deflib::lm`\",",
                    "note=\"write`Wrap`withanabsolutepath(e.g.`crate::Wrap`),oradd`repeater=..`tothe`#[announce]`of`Leaky`\"",
                ],
            );
        }
        let not_recommended = out.contains("#[diagnostic::do_not_recommend]");
        assert_eq!(not_recommended, cfg!(announce_do_not_recommend), "{}", out);
    }

    #[test]
    fn the_definition_error_says_what_the_definer_can_do() {
        let f = failure(&[
            ("A", WITHOUT_REPEATER),
            ("B < T >", TRAIT_PATH),
            ("", "with a type that cannot be encoded (x)"),
            ("crate :: Idx", NOT_PRIMITIVE),
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
                "`crate::Idx` cannot be named outside its module \
                 (a const generic that is not of a primitive type): \
                 use a primitive type for the const parameter",
            ]
        );
        assert_eq!(failure(&[]).to_definition_error().into_iter().count(), 1);
    }

    #[test]
    fn to_error_is_a_plain_line_for_each_cause_that_ends_with_the_fix() {
        // (tokens, reason, what follows "this macro cannot use `deflib::lm::Leaky`: ")
        for (ts, reason, message) in [
            ("Local", WITHOUT_REPEATER, "`Local` cannot be named outside its module; {R}"),
            ("& 'static UiShared", WITHOUT_REPEATER, "`&'static UiShared` cannot be named outside its module; {R}"),
            ("", WITHOUT_REPEATER, "a type of the item cannot be named outside its module; {R}"),
            ("Base", TRAIT_PATH, "`Base` cannot be named outside its module (a relative trait path in a bound); {A}"),
            ("crate :: UiIdx", NOT_PRIMITIVE, "`crate::UiIdx` cannot be carried (a const generic that is not of a primitive type); {P}"),
            ("Wrap < '_ >", REFUSED, "`Wrap<'_>` cannot be carried; {A}"),
            ("Odd", "because of something new", "`Odd` cannot be carried (because of something new); {A}"),
            ("", "with a type that cannot be encoded (x)", "a type of the item cannot be carried (a type that cannot be encoded (x)); {A}"),
        ] {
            let message = message
                .replace("{R}", "add `repeater = ..` to its `#[announce]`, or write the path absolutely")
                .replace("{A}", "write the path absolutely")
                .replace("{P}", "use a primitive type for the const parameter");
            let e = failure(&[(ts, reason)]).to_error();
            assert_eq!(e.to_string(), format!("this macro cannot use `deflib::lm::Leaky`: {}", message));
        }
        // One error for each distinct cause, in the order of the record, and one for none.
        let f = failure(&[
            ("A", WITHOUT_REPEATER),
            ("B", WITHOUT_REPEATER),
            ("A", WITHOUT_REPEATER),
        ]);
        let m: Vec<String> = f.to_error().into_iter().map(|e| e.to_string()).collect();
        assert!(
            m.len() == 2 && m[0].contains("`A` cannot") && m[1].contains("`B` cannot"),
            "{:?}",
            m
        );
        assert_eq!(
            failure(&[]).to_error().to_string(),
            "this macro cannot use `deflib::lm::Leaky`: a type of the item cannot be named \
             outside its module"
        );
        // The path of the call as written.
        let mut f = failure(&[]);
        for path in ["Leaky", "::deflib::Leaky"] {
            f.path = syn::parse_str(path).unwrap();
            let lead = format!("this macro cannot use `{}`: ", path);
            assert!(f.to_error().to_string().starts_with(&lead));
        }
    }

    fn same_context_out(input: &str) -> String {
        squash(same_context(
            format!("::announce, {}", input).parse().unwrap(),
        ))
    }

    #[test]
    fn same_context_checks_the_failures_and_calls_the_next_element() {
        let failing = "a::Leaky! => [type_leak_error(id = 7u64, item = Leaky, errors = [(Local) \"without `repeater`\"])] \
                       { pub struct Leaky { pub a: Local } }";
        let plain = "a::Plain! => [] { pub struct Plain; }";
        let (failing_s, plain_s) = (squash(tokens(failing)), squash(tokens(plain)));
        let input = format!(
            "@announce_format \"1\" [sc!, cb!] {} {} REST",
            failing, plain
        );
        let out = same_context_out(&input);
        let (check, call) = out.split_once("};cb!").unwrap_or_else(|| panic!("{}", out));
        assert!(check.starts_with("const_:()={") && check.contains("traitInModuleOf_Leaky{}"));
        assert_eq!(out.matches("const_:()=").count(), 1, "{}", out);
        let rest = format!("{{@announce_format\"1\"[cb!]{}{}REST}}", failing_s, plain_s);
        assert_eq!(call, rest);
        // A check for each failure, the newest entry first; only the check at the end of the chain.
        let second = failing.replace("Leaky", "Other");
        let out = same_context_out(&format!("[sc!, cb!] {} {}", failing, second));
        assert!(out.find("InModuleOf_Other").unwrap() < out.find("InModuleOf_Leaky").unwrap());
        assert!(same_context_out(&format!("[sc!] {}", failing)).ends_with("};"));
        // Nothing to check: the call is forwarded.
        let out = same_context_out(&format!("[sc!, cb!] {}", plain));
        assert_eq!(out, format!("cb!{{@announce_format\"1\"[cb!]{}}}", plain_s));
        // In expression position the checks are statements of a block.
        let out = same_context_out(&format!("(expr) [sc!, cb!] {}", failing));
        assert!(
            out.starts_with("{const_:()={") && out.ends_with("}"),
            "{}",
            out
        );
        // Errors are `compile_error!`s.
        assert!(same_context("::announce [sc!]".parse().unwrap())
            .to_string()
            .contains("compile_error"));
        let out =
            same_context_out("[sc!] a::B! => [type_leak_error(id = 1u64, item = B)] { struct B; }");
        has(&out, &["compile_error", "missing`errors`"]);
    }
}
