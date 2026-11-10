//! The record of a definition whose types do not resolve outside its module
//! (`type_leak_error(..)` in `META`), and what a callback makes of it.
//!
//! `#[announce]` fails at the definition ([`LeakFailure::to_definition_error`]) unless
//! `strict = false`, which records the failure ([`LeakFailure`]) for the call to check:
//! [`Entry::item_resolved`](crate::helper::Entry::item_resolved) and `announce::same_context!`
//! ([`same_context()`]) emit [`LeakFailure::assume_success`], and
//! [`Entry::type_leak`](crate::helper::Entry::type_leak) returns [`LeakFailure::to_error`].

use crate::helper::keys::{bracketed_list, once, read_keys, skip_to_comma};
use proc_macro2::{Delimiter, Group, Ident, Span, TokenStream, TokenTree};
use syn::ext::IdentExt;
use syn::parse::{Parse, ParseStream, Parser};
use syn::{Error, LitInt, LitStr, Path, Result, Token};
use template_quote::{quote, ToTokens};

// The reasons of a cause (`LeakError::reason`). The first four are names that resolve in the
// item's module only; `REFUSED` is a type that type-leak refuses (`Wrap<'_>`, `Local<Self>`).
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
    if reason == WITHOUT_REPEATER {
        String::new()
    } else {
        format!(" ({})", why_text(reason))
    }
}

/// What the definer can do about a cause; `attr` names the item's attribute for the message.
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
    let ty = type_text(&e.tokens);
    if ty.is_empty() {
        "a type of the item".to_string()
    } else {
        format!("`{}`", ty)
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
        let reason = self.reason.clone();
        let scope = is_scope(&reason);
        if self.message.is_none() {
            self.message = Some(if scope {
                format!("use absolute path: '{}'", written)
            } else if reason == NOT_PRIMITIVE {
                format!("const parameter of a non-primitive type: '{}'", text)
            } else if text.is_empty() {
                "a type of the item cannot be carried".to_string()
            } else {
                format!("cannot be carried: '{}'", text)
            });
        }
        if self.notes.is_empty() {
            if reason == NOT_PRIMITIVE {
                self.notes.push(
                    "a const parameter of a type that is not primitive cannot be carried"
                        .to_string(),
                );
            } else if !scope && reason != REFUSED {
                self.notes.push(why_text(&reason).to_string());
            }
        }
        if self.help.is_none() {
            let write = format!("write `{}` with an absolute path", name);
            self.help = Some(if !scope {
                fix_for(&reason, "")
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
            });
        }
        self
    }
}

impl Parse for LeakError {
    /// Reads `(TOKENS) "reason"`, then optional `{ message = "..", notes = [".."], help = ".." }`
    /// and, up to the next `,`, what a later version adds.
    fn parse(input: ParseStream) -> Result<Self> {
        let tokens;
        syn::parenthesized!(tokens in input);
        let tokens: TokenStream = tokens.parse()?;
        let reason: LitStr = input.parse()?;
        let mut error = LeakError::new(tokens, reason.value());
        if input.peek(syn::token::Brace) {
            let texts;
            syn::braced!(texts in input);
            let (mut message, mut notes, mut help) = (None, None, None);
            read_keys(&texts, &["message", "notes", "help"], |key, input| {
                if key == "message" {
                    once(&mut message, key, input.parse::<LitStr>()?.value())
                } else if key == "notes" {
                    let list = bracketed_list::<LitStr>(input)?;
                    once(&mut notes, key, list.iter().map(LitStr::value).collect())
                } else {
                    once(&mut help, key, input.parse::<LitStr>()?.value())
                }
            })?;
            error.message = message;
            error.notes = notes.unwrap_or_default();
            error.help = help;
        }
        skip_to_comma(input)?;
        Ok(error)
    }
}

/// The record of a definition that has no type-leak table: the `type_leak_error(..)` META element
/// of an entry ([`Entry::leak_failure`](crate::helper::Entry::leak_failure)).
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
/// - `item`: the item's name; the check's trait is named after it. A raw identifier is kept.
/// - `errors`: one `(TOKENS) "reason" { .. }` for each cause. `TOKENS` keep the definition's
///   spans. The braces and each key in them are optional and default from the reason.
/// - `krate` (optional): the definer's crate as Cargo names it (`CARGO_CRATE_NAME`). A call
///   written through `$crate` shows it in the texts (`lib::m::Item`), even if that `$crate` is
///   another crate; without it the `$crate::` is dropped.
///
/// It is read with [`syn::parse::Parse`] (`syn::parse2::<LeakFailure>(meta)`); `id`, `item` and
/// `errors` are required. A reader skips keys it does not know, in the record and in the braces,
/// and tokens after an error's braces. An entry has this or `type_leak(..)`, never both.
#[derive(Clone, Debug)]
#[non_exhaustive]
pub struct LeakFailure {
    /// The announcement's random number, the `R` of the marker.
    pub id: LitInt,
    /// What does not resolve.
    pub errors: Vec<LeakError>,
    /// The path the caller wrote for the trampoline, without the `!`. Its last segment carries
    /// the call's span. The definer sets the item's name.
    pub path: Path,
    /// The item's name; the check's trait is named after it.
    pub ident: Ident,
    /// The definer's crate name, when known; the texts use it for a leading `$crate` of `path`.
    pub krate: Option<String>,
}

/// The texts of the check's error for one cause.
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

    /// The span of the call: the last segment of [`path`](Self::path).
    fn call_span(&self) -> Span {
        self.path
            .segments
            .last()
            .map_or_else(Span::call_site, |s| s.ident.span())
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

    /// One error per cause at the call's span, for a callback that needs a table and has none:
    /// "this macro cannot use `PATH`: `TYPE` cannot be named outside its module (WHY); FIX".
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
            let fix = fix_for(reason, "its `#[announce]`");
            let message = format!("{}: {}; {}", lead, clause, fix);
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
        let span = self.call_span();
        combine(messages.into_iter().map(|m| syn::Error::new(span, m)))
            .expect("at least one message")
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

    /// The texts of the check's error for one cause, in type-leak's layout.
    fn cause_text(&self, e: &LeakError) -> CauseText {
        let e = e.clone().described(&self.ident.unraw().to_string());
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
        // The way in, why, then what to do, all as notes: `on_unimplemented` has no help.
        let mut notes = vec![used];
        notes.extend(e.notes);
        notes.extend(e.help);
        CauseText {
            message: e.message.unwrap_or_default(),
            label,
            notes,
        }
    }

    /// The check that a callback emits next to an item it uses as written. It compiles where the
    /// item's own names resolve (its module, function bodies and `use super::*` submodules) and
    /// fails elsewhere, with one error per distinct cause that underlines the cause's whole path
    /// in the definition. Causes are distinct by the headline of their error and their reason:
    /// `&'static Local` and `[Local; 2]` both read `'Local'` and are one cause, `Wrap<u8>` and
    /// `Wrap<u16>` are two. The first of equal causes in the order of the record gives the place.
    /// The call is in rustc's note about the bound:
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
    /// The texts need Rust 1.78 (`#[diagnostic::on_unimplemented]`), and 1.85 keeps the error
    /// short (`#[diagnostic::do_not_recommend]`); earlier compilers say that a trait is not
    /// implemented, at the same place.
    ///
    /// The check is a `const _` item, so it cannot stand in an `impl` or `trait` block: emit it
    /// beside the impl, which gives the same answer. `krate` is the path of the `announce` crate
    /// where the tokens are emitted (`::announce` or `$crate`), the one the definition used.
    pub fn assume_success(&self, krate: &Path) -> TokenStream {
        let causes = self.distinct_causes();
        if causes.is_empty() {
            let text = CauseText {
                message: "a type of the item cannot be named outside its module".to_string(),
                label: format!("`{}!` is used outside its module", self.call_path().0),
                notes: Vec::new(),
            };
            return self.check(krate, text, None);
        }
        causes
            .into_iter()
            .map(|e| self.check(krate, self.cause_text(e), cause_range(&e.tokens)))
            .collect()
    }

    /// The headline of the check's error for a cause.
    fn shown(&self, e: &LeakError) -> String {
        let item = self.ident.unraw().to_string();
        e.clone().described(&item).message.unwrap_or_default()
    }

    /// The causes that get a check of their own: the first of those with the same headline
    /// ([`shown`](Self::shown)) and reason, in the order of the record.
    fn distinct_causes(&self) -> Vec<&LeakError> {
        let mut causes: Vec<&LeakError> = Vec::new();
        for e in &self.errors {
            let same = |c: &&LeakError| self.shown(c) == self.shown(e) && c.reason == e.reason;
            if !causes.iter().any(same) {
                causes.push(e);
            }
        }
        causes
    }

    /// The check for one cause; `cause` is where it starts and ends in the definition. Without
    /// it the error is at the caller's path.
    fn check(&self, krate: &Path, text: CauseText, cause: Option<(Span, Span)>) -> TokenStream {
        let span = self.call_span();
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
        let (first, last) = match cause {
            Some((start, end)) => (span.located_at(start), span.located_at(end)),
            None => (span, span),
        };
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
                    || (named && crate::leak::PRIMITIVES.contains(&name.as_str()));
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

/// The path that a cause names, found in the tokens of one level: `level[start..end]` is the
/// path with its leading `::` and angle-bracketed arguments, `level[start..segments]` without them.
struct PathAt {
    level: Vec<TokenTree>,
    start: usize,
    segments: usize,
    end: usize,
}

/// Finds the path of a cause: the first identifier that names something ([`locate`]) with the
/// leading `::`, the `::` segments and the angle-bracketed arguments around it (also `Vec::<T>`).
fn find_path(ts: &TokenStream) -> Option<PathAt> {
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
    Some(PathAt {
        level,
        start,
        segments,
        end,
    })
}

/// The first and last token of the path a cause names, where its error starts and ends:
/// `Local` to `Local` for `&'static Local`, `Wrap` to `>` for `Wrap<'a, Local>`, `inner` to
/// `Local` for `inner::Local`. A cause without a path is whole; `None` without tokens.
fn cause_extent(ts: &TokenStream) -> Option<(TokenTree, TokenTree)> {
    match find_path(ts) {
        Some(p) => Some((p.level[p.start].clone(), p.level[p.end - 1].clone())),
        None => {
            let all: Vec<TokenTree> = ts.clone().into_iter().collect();
            Some((all.first()?.clone(), all.last()?.clone()))
        }
    }
}

fn cause_range(ts: &TokenStream) -> Option<(Span, Span)> {
    cause_extent(ts).map(|(first, last)| (first.span(), last.span()))
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
/// `Wrap` (written `Wrap<'a, Local>`) for `Wrap<'a, Local>`. A `$crate` reads as `crate`; `None`
/// without an identifier.
pub(crate) fn cause_path(ts: &TokenStream) -> Option<CausePath> {
    let p = find_path(ts)?;
    let names: Vec<String> = p.level[p.start..p.segments]
        .iter()
        .filter_map(|tt| match tt {
            TokenTree::Ident(id) => Some(id.to_string()),
            _ => None,
        })
        .collect();
    let lead = if is_colon(p.level.get(p.start)) {
        "::"
    } else {
        ""
    };
    Some(CausePath {
        path: format!("{}{}", lead, names.join("::")).replace("$crate", "crate"),
        written: type_text(&p.level[p.start..p.end].iter().cloned().collect()),
        generic: p.end > p.segments,
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
        let name: Ident = input.parse()?;
        if name != "type_leak_error" {
            return Err(Error::new(
                name.span(),
                format!("expected `type_leak_error(..)`, found `{}`", name),
            ));
        }
        let args;
        syn::parenthesized!(args in input);
        let (mut id, mut item, mut krate, mut errors) = (None, None, None, None);
        read_keys(&args, &["id", "item", "krate", "errors"], |key, input| {
            if key == "id" {
                once(&mut id, key, input.parse::<LitInt>()?)
            } else if key == "item" {
                once(&mut item, key, Ident::parse_any(input)?)
            } else if key == "krate" {
                once(&mut krate, key, input.parse::<LitStr>()?.value())
            } else {
                once(&mut errors, key, bracketed_list::<LeakError>(input)?)
            }
        })?;
        let missing = |key: &str| {
            Error::new(
                name.span(),
                format!("missing `{}` in the `type_leak_error(..)` META", key),
            )
        };
        let id = id.ok_or_else(|| missing("id"))?;
        let ident = item.ok_or_else(|| missing("item"))?;
        let errors = errors.ok_or_else(|| missing("errors"))?;
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
        syn::parse2(f.meta()).unwrap()
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
        assert_eq!(
            squash(f.meta()),
            "type_leak_error(id=42u64,item=Leaky,errors=[(Local)\"without`repeater`\",\
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
        let meta = squash(f.meta());
        assert_eq!(
            meta,
            "type_leak_error(id=42u64,item=Leaky,errors=[(Wrap<'_>)\"withatypethatcannotbecarried\"{\
             message=\"useabsolutepath:'Wrap<'_>'\",notes=[\"itcannotbeleakedasawholebecauseof`'_`\",\
             \"a{brace}\"],help=\"write`Wrap`withanabsolutepath(e.g.`crate::Wrap`)\"},\
             (Local)\"without`repeater`\"])"
        );
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
        let f = read_meta(
            "type_leak_error(id = 1u64, item = B, errors = [(A) \"r\" { future = [1], help = \"h\" }, \
             (B) \"s\" { notes = [\"n\"] }])",
        )
        .unwrap();
        assert_eq!(f.errors[0].help.as_deref(), Some("h"));
        assert!(f.errors[0].message.is_none() && f.errors[0].notes.is_empty());
        assert_eq!(f.errors[1].notes, ["n"]);
    }

    #[test]
    fn meta_ignores_unknown_keys_and_needs_id_item_and_errors() {
        let f = read_meta("type_leak_error(id = 1u64, future = [1, 2], item = B, errors = [])")
            .unwrap();
        assert!(f.errors.is_empty());
        let err = read_meta("type_leak_error(id = 1u64, item = B)").unwrap_err();
        assert!(err.to_string().contains("missing `errors`"), "{}", err);
        let err = read_meta("type_leak_error(item = B, errors = [])").unwrap_err();
        assert!(err.to_string().contains("missing `id`"), "{}", err);
        let err = read_meta("type_leak_error(id = 1u64, errors = [])").unwrap_err();
        assert!(err.to_string().contains("missing `item`"), "{}", err);
    }

    #[test]
    fn the_item_is_read_as_written_and_a_raw_identifier_is_kept() {
        let mut f = failure(&[("Local", WITHOUT_REPEATER)]);
        f.ident = syn::parse_quote!(r#type);
        f.path = Path::from(f.ident.clone());
        assert_eq!(
            squash(f.meta()),
            "type_leak_error(id=42u64,item=r#type,errors=[(Local)\"without`repeater`\"])"
        );
        let back = read(&f);
        assert_eq!(back.ident, "r#type");
        assert_eq!(back.ident.unraw().to_string(), "type");
        assert_eq!(squash(&back.path), "r#type");
        let f = read_meta("type_leak_error(id = 1u64, item = Holder, errors = [])").unwrap();
        assert_eq!(f.ident, "Holder");
        assert_eq!(squash(&f.path), "Holder");
        assert!(read_meta("type_leak_error(id = 1u64, item = a::B, errors = [])").is_err());
        assert!(read_meta("type_leak_error(id = 1u64, item = \"B\", errors = [])").is_err());
    }

    #[test]
    fn the_record_is_read_back_as_it_was_written() {
        let mut f = failure(&[("Local", WITHOUT_REPEATER), ("Wrap < '_ >", REFUSED)]);
        f.errors[1] = f.errors[1]
            .clone()
            .with_message("m")
            .with_notes(["n1", "n2"])
            .with_help("h");
        let f = f.with_krate("lib");
        let back = syn::parse2::<LeakFailure>(f.meta()).unwrap();
        assert_eq!(back.id.base10_digits(), f.id.base10_digits());
        assert_eq!(back.ident, f.ident);
        assert_eq!(back.krate, f.krate);
        assert_eq!(back.errors.len(), f.errors.len());
        for (b, e) in back.errors.iter().zip(&f.errors) {
            assert_eq!(squash(&b.tokens), squash(&e.tokens));
            assert_eq!(b.reason, e.reason);
            assert_eq!(
                (&b.message, &b.notes, &b.help),
                (&e.message, &e.notes, &e.help)
            );
        }
        assert_eq!(squash(back.meta()), squash(f.meta()));
    }

    #[test]
    fn only_a_type_leak_error_element_is_a_record() {
        let err = read_meta("type_leak(id = 1u64, item = B, errors = [])").unwrap_err();
        assert!(
            err.to_string()
                .contains("expected `type_leak_error(..)`, found `type_leak`"),
            "{}",
            err
        );
        assert!(read_meta("type_leak_error").is_err());
        assert!(read_meta("type_leak_error(id = 1u64, item = B, errors = []) more").is_err());
    }

    #[test]
    fn a_key_with_no_value_or_given_twice_is_an_error() {
        let err = read_meta("type_leak_error(id, item = B, errors = [])").unwrap_err();
        assert!(err.to_string().contains("`id` needs a value"), "{}", err);
        let err =
            read_meta("type_leak_error(id = 1u64, id = 2u64, item = B, errors = [])").unwrap_err();
        assert!(err.to_string().contains("duplicate key `id`"), "{}", err);
        let err = read_meta("type_leak_error(id = 1u64, item = B, errors = [(A) \"r\" { help }])")
            .unwrap_err();
        assert!(err.to_string().contains("`help` needs a value"), "{}", err);
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
        let f = failure(&[("Wrap < u8 >", WITHOUT_REPEATER)]);
        let (message, _, notes) = &cause_texts(&f)[0];
        assert_eq!(message, "use absolute path: 'Wrap<u8>'");
        assert!(notes[1].starts_with("write `Wrap` with an absolute path (e.g. `crate::Wrap`)"));
    }

    /// The message and the help that a cause of `tokens` gets by default in the item `Item`.
    fn default_texts(tokens: &str) -> (String, String) {
        let e = LeakError::new(tokens.parse::<TokenStream>().unwrap(), WITHOUT_REPEATER)
            .described("Item");
        (e.message.unwrap(), e.help.unwrap())
    }

    #[test]
    fn the_texts_of_a_cause_name_the_path_in_it() {
        for (tokens, shown, path) in [
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
            ("& 'static UiShared", "UiShared", "UiShared"),
            ("& 'static mut UiUnique", "UiUnique", "UiUnique"),
            ("& mut UiUnique", "UiUnique", "UiUnique"),
            ("* const UiRaw", "UiRaw", "UiRaw"),
            ("* mut UiRaw", "UiRaw", "UiRaw"),
            ("& inner :: Local", "inner::Local", "inner::Local"),
            ("& 'a Wrap < u8 >", "Wrap<u8>", "Wrap"),
            ("& & 'a UiShared", "UiShared", "UiShared"),
            ("dyn Base", "Base", "Base"),
            ("& 'a dyn Base", "Base", "Base"),
            ("Holder < dyn Base >", "Holder<dyn Base>", "Holder"),
            ("(Local , u8)", "Local", "Local"),
            ("[Local ; 2]", "Local", "Local"),
            ("[(Local , u8) ; 2]", "Local", "Local"),
            ("& [Local]", "Local", "Local"),
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
                assert!(
                    !message.contains(bad),
                    "`{}` in `{}`: {}",
                    bad,
                    tokens,
                    message
                );
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
        let e = LeakError::new("& inner :: Option".parse::<TokenStream>().unwrap(), ALIAS)
            .described("Item");
        assert_eq!(
            e.help.as_deref(),
            Some("write `inner::Option` with an absolute path (e.g. `crate::inner::Option`)")
        );
    }

    /// The record that the META `src` holds.
    fn read_meta(src: &str) -> Result<LeakFailure> {
        syn::parse_str(src)
    }

    #[test]
    fn a_later_version_may_add_keys_with_any_value_to_the_texts_of_a_cause() {
        let f = read_meta(
            "type_leak_error(id = 1u64, item = B, errors = [(A) \"r\" { message = \"m\", span = a::b, \
             level = -1, ty = Vec<u8, Alloc>, internal, notes = [\"n1\", \"n2\"], \
             list = [1, 2], help = \"h\" }])",
        )
        .unwrap();
        let e = &f.errors[0];
        assert_eq!(e.message.as_deref(), Some("m"));
        assert_eq!(e.notes, ["n1", "n2"]);
        assert_eq!(e.help.as_deref(), Some("h"));
        let f = read_meta(
            "type_leak_error(id = 1u64, item = B, errors = [(A) \"r\" { internal, help = \"h\", last }])",
        )
        .unwrap();
        assert_eq!(f.errors[0].help.as_deref(), Some("h"));
        assert!(f.errors[0].message.is_none());
        let f =
            read_meta("type_leak_error(id = 1u64, item = B, errors = [(A) \"r\" { }])").unwrap();
        assert!(f.errors[0].message.is_none() && f.errors[0].help.is_none());
        assert!(read_meta(
            "type_leak_error(id = 1u64, item = B, errors = [(A) \"r\" { help = 3 }])"
        )
        .is_err());
    }

    #[test]
    fn an_error_may_have_more_after_its_texts() {
        let f = read_meta(
            "type_leak_error(id = 1u64, item = B, errors = [\
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
        let meta = squash(f.meta());
        assert!(!meta.contains("krate"), "{}", meta);
        assert!(read(&f).krate.is_none());
        f = f.with_krate("my_lib");
        let meta = squash(f.meta());
        assert!(meta.ends_with(",krate=\"my_lib\")"), "{}", meta);
        assert_eq!(read(&f).krate.as_deref(), Some("my_lib"));
        let f = read_meta("type_leak_error(id = 1u64, item = B, errors = [(A) \"r\"])").unwrap();
        assert!(f.krate.is_none());
        assert!(read_meta("type_leak_error(id = 1u64, item = B, errors = [], krate = 3)").is_err());
    }

    #[test]
    fn a_dollar_crate_at_the_head_of_the_path_is_the_crate_of_the_definer() {
        let segments = |s: &[&str]| s.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(
            resolve_dollar_crate(segments(&["$crate", "m", "Item"]), Some("lib")),
            ["lib", "m", "Item"]
        );
        assert_eq!(
            resolve_dollar_crate(segments(&["$crate", "Item"]), Some("lib")),
            ["lib", "Item"]
        );
        assert_eq!(
            resolve_dollar_crate(segments(&["$crate", "m", "Item"]), None),
            ["m", "Item"]
        );
        assert_eq!(
            resolve_dollar_crate(segments(&["$crate", "Item"]), None),
            ["Item"]
        );
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
        // A primitive type names nothing that can be written absolutely.
        assert_eq!(path("[u8 ; N]"), some("N", "N", false));
        assert_eq!(path("(u8 , Local)"), some("Local", "Local", false));
        assert_eq!(
            path("[u8 ; inner :: N]"),
            some("inner::N", "inner::N", false)
        );
        assert_eq!(path("dyn Base"), some("Base", "Base", false));
        assert_eq!(path("crate :: Buf < { N } >"), some("N", "N", false));
        // ... unless there is no other path.
        assert_eq!(path("& 'static u8"), some("u8", "u8", false));
        assert_eq!(path("[u8 ; 4]"), some("u8", "u8", false));
        // Parenthesized arguments are not generic arguments.
        assert_eq!(path("Fn (Local) -> u8"), some("Fn", "Fn", false));
        assert_eq!(path("& 'static"), None);
        assert_eq!(path(""), None);
    }

    #[test]
    fn the_texts_of_a_call_name_the_path_in_the_cause() {
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
        // `to_error` names the whole type, as the definition error does.
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
        let f = failure(&[("Odd", "with a generic argument that cannot be encoded")]);
        let (message, label, notes) = &cause_texts(&f)[0];
        assert_eq!(message, "cannot be carried: 'Odd'");
        assert!(label.starts_with("cannot be carried, so"), "{}", label);
        assert_eq!(notes[1], "a generic argument that cannot be encoded");
        assert_eq!(notes[2], "write the path absolutely");
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
            ("[Local; 2]", "Local", "Local"),
            ("(Local, u8)", "Local", "Local"),
            ("((Local, u8), Other)", "Local", "Local"),
            ("inner :: Local", "inner", "Local"),
            ("crate :: inner :: Local", "crate", "Local"),
            ("&inner :: Local", "inner", "Local"),
            ("dyn :: core :: fmt :: Debug", ":", "Debug"),
            ("Wrap < 'a , Local >", "Wrap", ">"),
            ("Wrap < Pair < Local , u8 > >", "Wrap", ">"),
            ("& 'a Wrap < u8 > :: Out", "Wrap", ">"),
            ("Vec :: < Local >", "Vec", ">"),
            ("inner :: Wrap < fn (u8) -> Local >", "inner", ">"),
            ("* const UiWrap < '_ >", "UiWrap", ">"),
            ("Fn (Local) -> u8", "Fn", "Fn"),
            // A primitive names nothing: the path is the constant or the type next to it.
            ("[u8 ; N]", "N", "N"),
            ("[u8 ; inner :: N]", "inner", "N"),
            ("& 'a [bool ; N]", "N", "N"),
            ("(u8 , Local)", "Local", "Local"),
            ("(u8 , (usize , Local))", "Local", "Local"),
            ("Wrap < { N } >", "Wrap", ">"),
            // An absolute path names nothing relative either: the constant in it does.
            ("crate :: Buf < { N } >", "N", "N"),
            (":: std :: vec :: Vec < [u8 ; N] >", "N", "N"),
            ("crate :: inner :: Buf < Local >", "Local", "Local"),
            ("dyn :: core :: fmt :: Debug + Local", "Local", "Local"),
            ("dyn Base + Send", "Base", "Base"),
            ("< u8 as Tr > :: X", "Tr", "Tr"),
        ] {
            assert_eq!(
                extent(cause),
                (first.to_string(), last.to_string()),
                "{}",
                cause
            );
        }
        // Only primitives: the primitive is the path.
        assert_eq!(extent("&'static u8"), ("u8".into(), "u8".into()));
        assert_eq!(extent("[u8 ; 4]"), ("u8".into(), "u8".into()));
        assert_eq!(extent("(u8 , bool)"), ("u8".into(), "u8".into()));
        assert_eq!(extent("&'static"), ("&".into(), "static".into()));
        assert_eq!(extent("'a"), ("'".into(), "a".into()));
        assert_eq!(extent("* const"), ("*".into(), "const".into()));
        assert_eq!(extent("()"), ("()".into(), "()".into()));
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
        // `Local` twice is one cause; the same tokens with another reason are another.
        assert_eq!(out.matches("const_:()={").count(), 3, "{}", out);
        assert_eq!(out.matches("traitInModuleOf_Leaky{}").count(), 3, "{}", out);
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
        let none = squash(failure(&[]).assume_success(&syn::parse_quote!(::announce)));
        assert_eq!(none.matches("const_:()={").count(), 1, "{}", none);
    }

    #[test]
    fn assume_success_merges_causes_that_name_the_same_path() {
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
    }

    #[test]
    fn assume_success_is_a_block_with_the_texts_and_the_probe() {
        let f = failure(&[("Wrap < { N } >", WITHOUT_REPEATER)]);
        let out = squash(f.assume_success(&syn::parse_quote!(::announce)));
        assert!(out.starts_with("const_:()={"), "{}", out);
        assert!(out.ends_with("};"), "{}", out);
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
        // Braces of the type are doubled: they are format strings.
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
            "a::Leaky! => [type_leak_error(id = 7u64, item = Leaky, errors = [(Local) \"without `repeater`\"])] \
                       { pub struct Leaky { pub a: Local } }";
        let plain = "a::Plain! => [] { pub struct Plain; }";
        let out = same_context_out(
            "::announce",
            &format!(
                "@announce_format \"1\" [sc!, cb!] {} {} REST",
                failing, plain
            ),
        );
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
        // Several failures: a check for each, in the order of the chain.
        let second = failing.replace("Leaky", "Other");
        let out = same_context_out("::announce", &format!("[sc!, cb!] {} {}", failing, second));
        let (l, o) = (
            out.find("InModuleOf_Leaky").unwrap(),
            out.find("InModuleOf_Other").unwrap(),
        );
        assert!(o < l, "the newest entry comes first in the input: {}", out);
        let out = same_context_out("::announce", &format!("[sc!] {}", failing));
        assert!(out.ends_with("};"), "{}", out);
        let out = same_context_out("::announce", &format!("[sc!, cb!] {}", plain));
        assert_eq!(
            out,
            format!(
                "cb!{{@announce_format\"1\"[cb!]{}}}",
                squash(plain.parse::<TokenStream>().unwrap())
            )
        );
        let out = same_context_out("::announce", &format!("(expr) [sc!, cb!] {}", failing));
        assert!(
            out.starts_with("{const_:()={") && out.ends_with("}"),
            "{}",
            out
        );
        let e = same_context("::announce [sc!]".parse().unwrap()).to_string();
        assert!(e.contains("compile_error"), "{}", e);
        let e = same_context_out(
            "::announce",
            "[sc!] a::B! => [type_leak_error(id = 1u64, item = B)] { struct B; }",
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
        for m in messages(&f) {
            assert!(!m.contains('\n') && !m.contains("strict"), "{}", m);
        }
        let mut f = failure(&[("Local", WITHOUT_REPEATER)]);
        f.path = syn::parse_quote!(Leaky);
        assert!(messages(&f)[0].starts_with("this macro cannot use `Leaky`: "));
        f.path = syn::parse_quote!(::deflib::Leaky);
        assert!(messages(&f)[0].starts_with("this macro cannot use `::deflib::Leaky`: "));
    }

    #[test]
    fn to_error_says_what_a_table_cannot_carry_without_scope() {
        let f = failure(&[("crate :: inner :: UiIdx", NOT_PRIMITIVE)]);
        assert_eq!(
            messages(&f),
            [
                "this macro cannot use `deflib::lm::Leaky`: `crate::inner::UiIdx` cannot be carried \
                 (a const generic that is not of a primitive type); \
                 use a primitive type for the const parameter"
            ]
        );
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
        // A reason this version does not know is a cause that cannot be carried.
        let f = failure(&[("Odd", "because of something new")]);
        assert_eq!(
            messages(&f),
            [
                "this macro cannot use `deflib::lm::Leaky`: `Odd` cannot be carried \
                 (because of something new); write the path absolutely"
            ]
        );
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
