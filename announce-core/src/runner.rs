//! Several announcements of one item: the runner.
//!
//! Downstream attribute macros emit `#[announce(..)]` on the items they produce. When
//! two announcements apply to one item they must make one trampoline and one `Announce`
//! impl, not two (the second would be E0252 and E0119). So the announcements are merged,
//! and **one** attribute makes the trampoline: the *runner*, which stands last among the
//! attributes of the item, and so expands after everything that is above it (rustc
//! expands the attributes of an item from the top).
//!
//! # The runner
//!
//! ```text
//! #[KRATE::__private::__announce_runner(
//!     __announce_format_version = "1",
//!     __announce_owned(my_lib, other),
//!     crate = KRATE, meta = a::b, my_lib::check
//! )]
//! ```
//!
//! It is an attribute macro of the `announce` crate that nobody writes. It is found by the
//! last segment of its path, [`RUNNER`], and by nothing else (never by the path
//! `announce`). Its arguments are:
//!
//! - `__announce_format_version = "1"`: the version of this encoding, [`RUNNER_FORMAT`].
//!   **Bump it whenever the encoding of the arguments changes.** It is read before
//!   anything else, without parsing the rest (the other entries may be in the encoding of
//!   another version), by looking at the entries between the top-level commas. Several
//!   entries are allowed, and so are several runners on the item (two versions of
//!   `announce`, at two crate paths, can both announce one item). If all the versions
//!   that are found are the current one, the runners are merged into one, the last one
//!   is the target, and there is exactly one entry in the result. Otherwise it is one
//!   compile error, with the item, which lists every version found (in order, without
//!   duplicates, with the crate path of each runner where they differ; a runner without
//!   an entry is version `unknown`), says which version this `announce` expects, and tells
//!   to use one version in every crate (`cargo tree -i announce`).
//! - `__announce_sentinel = "impl"` or `"macro"`: the kind of the sentinel that the
//!   announcement that put the runner on the item emitted next to it (see "The sentinel"
//!   below). When runners or announcements are merged, `impl` wins, as it is the one that
//!   the options can break. Left out when there is no sentinel.
//! - `__announce_owned(a, b)`: the names of the attributes that the checks of the
//!   announcements own: the answer of the query. Left out when there are none.
//! - the rest are the options of `#[announce(..)]` as [`Options`] prints them.
//!
//! # Two version markers
//!
//! announce has two version markers. They version different things, and are bumped
//! independently:
//!
//! - `__announce_format_version = "1"` ([`RUNNER_FORMAT`]) is an *entry of the arguments of
//!   the runner*, an attribute macro. It versions the encoding of those arguments, and is
//!   read by the runners of two versions of `announce` that meet on one item.
//! - `@announce_format "1"` ([`crate::helper::ANNOUNCE_FORMAT`]) is the *first tokens of a
//!   chain call*: every macro call that announce makes to the next element of a chain (a
//!   trampoline, a transformer, a callback, a check, the query of the checks, the sentinel
//!   below). It versions the wire format of those calls, and is read by the macros that
//!   receive them, also those of the users (see [`crate::helper::Input`]).
//!
//! The runner attribute is not a chain call, so it has no `@announce_format`; the chain
//! calls are not attributes, so they have no `__announce_format_version`. A version error of
//! the first lists the runners that were found on the item; one of the second names the
//! version found in the call and the one that is read.
//!
//! # `#[announce(ARGS)]`
//!
//! 1. If it has checks, it asks them which attributes they own (the query; it runs once
//!    for an announcement). See [`crate::finish`].
//! 2. An item that has no other outer attribute that could expand has nothing to wait for:
//!    the trampoline is generated at once, and no runner is put on the item. Documentation
//!    (`///` is the attribute `doc`), `#[repr(..)]` and the like cannot expand (see
//!    [`INERT`]: built-in attributes that a bare name cannot make a macro of), and nor can
//!    the attributes that the checks of this announcement own, which are stripped. This is
//!    the common case of a plain item, and it takes one expansion. A `#[derive(..)]`, a
//!    `#[cfg_attr(..)]` (which can expand to a macro), any other attribute (a bare
//!    `#[announce]` too: it may be the macro of another crate) or an attribute with a path
//!    (`#[x::doc]`) makes the runner.
//! 3. Otherwise the item comes back with the runner as its last attribute, or, if the
//!    item has one (the versions are checked first), with the options merged into it
//!    (the runner is the target: its entries come first; options that differ are one
//!    error with the item). The announcement that puts the runner there also emits the
//!    sentinel after the item. Nothing else is emitted yet. The attributes that the checks
//!    own are moved to right after the runner, in their order, after those that are there
//!    already: rustc rejects an attribute that nothing knows above an attribute macro that
//!    has still to expand (`cannot find attribute`), and the runner is below everything
//!    else.
//!
//! # The sentinel
//!
//! A macro below `#[announce]` that returns the item without its outer attributes (one that
//! rebuilds its input from the fields it parsed, say) drops the runner, and then nothing
//! announces the item, with no error at the definition: the only one would be "cannot find
//! macro `S`" at a call, somewhere else. So the announcement that **puts the runner** on the
//! item (only that one: not an announcement that merges into a runner, and not an item that
//! is announced at once, which has no runner to lose) also emits a *sentinel*, as an item (a
//! statement in a block) right after the item. It fails at the definition if the runner never
//! ran, in one of two forms:
//!
//! - `impl`, when the runner will implement `Announce` for a type that can be named without
//!   reading generics: a struct, enum or union with no `<` after its name, announced without
//!   `mock` and `emit_item = false`:
//!
//!   ```text
//!   const _: () = {
//!       trait __AnnounceSentinel {}
//!       impl __AnnounceSentinel for S where S: KRATE::Announce {}
//!   };
//!   ```
//!
//!   (with `#[allow(deprecated)]`). If the runner is gone the error is that of `Announce`
//!   (`on_unimplemented`), at the name of the item. The bound is in the `where` clause of
//!   an impl: rustc checks it (a bound on a type without generic parameters that does not
//!   hold is E0277), and it is not a use of `S` for the dead-code lint, as naming the type
//!   in the body of a function would be: a private announced struct that nothing uses
//!   still warns "struct `S` is never constructed" (not "field `0` is never read").
//! - `macro`, for everything else (generic structs, traits, type aliases, inline modules,
//!   `mock`, `emit_item = false`): a call of the trampoline,
//!   `NAME! { @announce_format "1" @__announce_exists }`, where `NAME` is the `macro_name`,
//!   else the name of the item, as a bare name (so it works in a block too). Like every call
//!   that announce makes, it starts with the format prefix of the wire format (see "Two version
//!   markers" above). The trampoline has an arm for it (the first
//!   one, which expands to nothing, and takes the bare `NAME! { @__announce_exists }` as
//!   well: see `generate`). If the runner is gone the error is "cannot find macro `NAME`",
//!   at the name of the item (at `macro_name = NAME` if that is where the name is).
//!
//! The sentinel carries the `#[cfg(..)]` attributes of the item, as the item and the
//! trampoline go away together. An item with a `#[cfg_attr(..)]` that can make a `cfg`
//! (its arguments mention `cfg`) gets no sentinel, as it could not tell if the item is there.
//!
//! The runner records the kind in `__announce_sentinel`. A later announcement that merges
//! options that take away what the `impl` sentinel relies on (`mock`, `emit_item = false`)
//! is one error with the item, which says that they must be on the first announcement of the
//! item (the type disappears with `emit_item = false`, and a mock has no impl). The item is
//! kept with its runner, so nothing else follows.
//!
//! If generation fails (the capture derive reports a type error, say) it emits a stand-in for
//! what the sentinel needs next to its error (`generate::sentinel_stub`: an impl, or a macro
//! with the arm of the sentinel), so that the sentinel does not add an error about the same
//! item; the dummy of the capture derive tells it the kind. The same is done by the runner
//! when it reports a version error. Errors where the runner and the trampoline go away
//! without a stand-in are followed by the error of the sentinel (the compile error of the
//! first comes first, as the errors of macros are reported at expansion and the sentinel's
//! at name resolution or type check): a runner whose arguments cannot be read, an item that
//! a macro below the runner changed into another kind, and the versions error of an
//! `announce` of another version that removes our runner.
//!
//! `mock = ..` and `emit_item = false` are options like the others: they are merged
//! (a value that is set wins over one that is not, two different values are the error),
//! and the runner drops the item when the merged options say so. So the macros below such
//! an announcement see the item, and expand on it, before the runner drops it: a derive
//! that implements a trait for it fails loudly (`cannot find type`), where it was dropped
//! with the item before.
//!
//! # The runner expands
//!
//! The runner is the last macro on the item. Every attribute macro that was above it has
//! expanded, and every derive: the callbacks see the item after the other attribute
//! macros, and without the derives that have run. It
//!
//! 1. checks the versions of its own arguments and of the other runners of the item. If
//!    there are other runners (they have the same version), it merges itself into the last
//!    one and goes away: that one expands next;
//! 2. reports an announcement that is still on the item. Everything above the runner has
//!    expanded, so an attribute with a path that ends in `announce` (`::announce::announce`)
//!    that is left was put there by a macro *below* the runner (by appending it to its
//!    output, say): too late, it would make a second trampoline. It is an error, and the
//!    attribute is removed (the options of a late announcement are not merged). A bare
//!    `#[announce]` is not seen (it is an attribute macro like any other, of this crate
//!    or of another), and nor is a renamed import (`#[ann]`): a late one of those makes a
//!    second trampoline (E0252);
//! 3. generates the trampoline and the `Announce` impl, with the merged options
//!    ([`crate::capture_output`]): the item without the attributes that the checks own
//!    (they stay in the payload; without the item at all, with `mock` or
//!    `emit_item = false`) and the hidden capture derive, which sees the item with every
//!    nested `#[cfg]` evaluated.
//!
//! # Termination
//!
//! Every expansion of `#[announce]` removes that attribute from the item, and either puts
//! a runner on the item (once: the next ones find it and merge into it), or generates at
//! once. The runner expands once (it never puts a runner
//! on the item, only merges into the last of several, and generates at last), and the
//! queries are chains of calls of the checks that hand over to the next element of a
//! list that gets shorter and end in `finish`. So there is no endless run. Macros of
//! others can still add announcements (each of them is one more `#[announce]`); they do
//! so by their own expansions, which the argument does not cover.

use crate::helper::OwnedAttrs;
use crate::options::{MergeConflict, Options};
use crate::scan::{self, Outer, OuterAttr};
use proc_macro2::{Delimiter, Group, Ident, Span, TokenStream, TokenTree};
use template_quote::{quote, ToTokens};

/// The version of the encoding of the arguments of the runner. **Bump it whenever that
/// encoding changes** (the entries, or the way the options inside are written), so that
/// two versions of `announce` in one build refuse each other's runners with a clear error
/// instead of misreading them.
pub(crate) const RUNNER_FORMAT: &str = "1";

/// The name of the runner, the last segment of its path.
pub(crate) const RUNNER: &str = "__announce_runner";

/// The entry of the arguments that holds the version.
const VERSION_KEY: &str = "__announce_format_version";

/// The entry of the arguments that holds the names that checks own.
const OWNED_KEY: &str = "__announce_owned";

/// The entry of the arguments that holds the kind of the sentinel (`"impl"` or `"macro"`),
/// if the announcement that put the runner on the item emitted one.
const SENTINEL_KEY: &str = "__announce_sentinel";

/// What the announcement that puts the runner on an item emits next to the item, to fail
/// at the definition if the runner never runs (a macro below `#[announce]` that returns
/// the item without its attributes drops the runner, and then nothing is announced).
///
/// - [`Sentinel::Impl`]: `const _: () = { trait __AnnounceSentinel {}
///   impl __AnnounceSentinel for S where S: KRATE::Announce {} };`. It needs the runner to
///   implement `Announce` for a type that can be named without reading generics: a struct,
///   enum or union without generic parameters, announced without `mock` and
///   `emit_item = false`. The bound is in a `where` clause so that the sentinel is not a use
///   of `S` for the dead-code lint.
/// - [`Sentinel::Macro`]: `NAME! { @announce_format "1" @__announce_exists }`, a call of the
///   trampoline, which has an arm for it. Everything else.
///
/// Both are items (or statements) that expand right after the item, and the first one that
/// cannot be resolved is the error: `Announce` is not implemented for `S`, or "cannot find
/// macro `NAME`", at the name of the item.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Sentinel {
    Impl,
    Macro,
}

impl Sentinel {
    fn word(self) -> &'static str {
        match self {
            Sentinel::Impl => "impl",
            Sentinel::Macro => "macro",
        }
    }

    /// The sentinel of two runners that are merged: `Impl` wins, as it is the one that
    /// the options can break.
    fn merge(target: Option<Sentinel>, folder: Option<Sentinel>) -> Option<Sentinel> {
        match (target, folder) {
            (Some(Sentinel::Impl), _) | (_, Some(Sentinel::Impl)) => Some(Sentinel::Impl),
            (a, b) => a.or(b),
        }
    }

    /// The word that tells the `Capture` derive, in its `#[__announce(..)]`, which
    /// stand-in to emit if it fails.
    pub(crate) fn capture_word(self) -> &'static str {
        match self {
            Sentinel::Impl => "sentinel_impl",
            Sentinel::Macro => "sentinel_macro",
        }
    }

    pub(crate) fn from_capture_word(word: &str) -> Option<Sentinel> {
        match word {
            "sentinel_impl" => Some(Sentinel::Impl),
            "sentinel_macro" => Some(Sentinel::Macro),
            _ => None,
        }
    }

    fn from_word(word: &str) -> Option<Sentinel> {
        match word {
            "impl" => Some(Sentinel::Impl),
            "macro" => Some(Sentinel::Macro),
            _ => None,
        }
    }

    /// The sentinel that an announcement with these options makes for this item.
    pub(crate) fn of(head: &scan::Head, opts: &Options) -> Sentinel {
        let adt = matches!(
            head.kind,
            scan::Kind::Struct | scan::Kind::Enum | scan::Kind::Union
        );
        if adt && !head.generic && opts.mock.is_none() && opts.emit_item {
            Sentinel::Impl
        } else {
            Sentinel::Macro
        }
    }

    /// The sentinel itself. `cfgs` are the `#[cfg(..)]` attributes of the item, which the
    /// sentinel has to have too (an item that is `cfg`'d out is not announced).
    fn tokens(self, head: &scan::Head, opts: &Options, cfgs: &TokenStream) -> TokenStream {
        match self {
            Sentinel::Impl => {
                let krate = opts.krate();
                let ident = &head.ident;
                // The bound has the span of the name of the item, so that its error is
                // reported there, not at the attribute.
                let bound = respan(quote! { #krate::Announce }, ident.span());
                quote! {
                    #cfgs
                    #[allow(deprecated)]
                    const _: () = {
                        trait __AnnounceSentinel {}
                        impl __AnnounceSentinel for #ident where #ident: #bound {}
                    };
                }
            }
            Sentinel::Macro => {
                let name = opts
                    .macro_name
                    .clone()
                    .unwrap_or_else(|| head.ident.clone());
                let format = crate::helper::format_prefix();
                quote! { #cfgs #name! { #format @__announce_exists } }
            }
        }
    }
}

/// The tokens with this span, but for `$crate` (its span is what tells the crate it names).
pub(crate) fn respan(ts: TokenStream, span: Span) -> TokenStream {
    ts.into_iter()
        .map(|mut tt| {
            match &mut tt {
                TokenTree::Group(g) => {
                    let mut ng = Group::new(g.delimiter(), respan(g.stream(), span));
                    ng.set_span(span);
                    *g = ng;
                }
                TokenTree::Ident(i) if i == "$crate" => {}
                t => t.set_span(span),
            }
            tt
        })
        .collect()
}

/// An error at the announcement that is expanding, and the item as it was.
pub(crate) fn error_with_item(message: String, item: &TokenStream) -> TokenStream {
    let mut out = syn::Error::new(Span::call_site(), message).to_compile_error();
    out.extend(item.clone());
    out
}

fn conflict(c: &MergeConflict, item: &TokenStream) -> TokenStream {
    error_with_item(c.message(), item)
}

// ---- the arguments of the runner -------------------------------------------------

/// The tokens split at the commas of the top level (an empty entry is dropped).
fn entries(ts: TokenStream) -> Vec<Vec<TokenTree>> {
    let mut out = vec![Vec::new()];
    for t in ts {
        match &t {
            TokenTree::Punct(p) if p.as_char() == ',' => out.push(Vec::new()),
            _ => out.last_mut().unwrap().push(t),
        }
    }
    out.retain(|e| !e.is_empty());
    out
}

/// `__announce_format_version = "lit"`: the text of the literal, without quotes.
fn version_entry(entry: &[TokenTree]) -> Option<String> {
    string_entry(entry, VERSION_KEY)
}

/// `key = "lit"`: the text of the literal, without quotes.
fn string_entry(entry: &[TokenTree], key: &str) -> Option<String> {
    match entry {
        [TokenTree::Ident(k), TokenTree::Punct(p), TokenTree::Literal(l)]
            if k == key && p.as_char() == '=' =>
        {
            let text = l.to_string();
            Some(
                match text.strip_prefix('"').and_then(|t| t.strip_suffix('"')) {
                    Some(inner) => inner.to_string(),
                    None => text,
                },
            )
        }
        _ => None,
    }
}

/// The versions in the arguments of a runner, in order, without reading anything else.
fn versions_in(args: &TokenStream) -> Vec<String> {
    entries(args.clone())
        .iter()
        .filter_map(|e| version_entry(e))
        .collect()
}

/// The arguments of the runner, but the version.
#[derive(Clone)]
pub(crate) struct RunnerArgs {
    /// The attribute names that the checks of the announcements own: the answer of the
    /// query.
    pub owned: OwnedAttrs,
    /// The merged options.
    pub opts: Options,
    /// The sentinel that was emitted next to the item, `None` if there is none.
    pub sentinel: Option<Sentinel>,
}

impl ToTokens for RunnerArgs {
    /// The arguments as they are written, with the one version entry.
    fn to_tokens(&self, tokens: &mut TokenStream) {
        let key = Ident::new(VERSION_KEY, Span::call_site());
        let mut parts = vec![quote! { #key = #RUNNER_FORMAT }];
        if let Some(sentinel) = self.sentinel {
            let key = Ident::new(SENTINEL_KEY, Span::call_site());
            let word = sentinel.word();
            parts.push(quote! { #key = #word });
        }
        if !self.owned.is_empty() {
            let key = Ident::new(OWNED_KEY, Span::call_site());
            let names = self.owned.names();
            parts.push(quote! { #key(#(#names),*) });
        }
        let opts = self.opts.to_token_stream();
        if !opts.is_empty() {
            parts.push(opts);
        }
        tokens.extend(quote! { #(#parts),* });
    }
}

impl RunnerArgs {
    /// Reads the arguments, whose versions are all the current one (see [`version_error`]):
    /// the version entries are dropped, the names are read, and the rest are options.
    pub(crate) fn read(args: TokenStream) -> syn::Result<RunnerArgs> {
        let mut owned = OwnedAttrs::new();
        let mut sentinel = None;
        let mut rest: Vec<TokenStream> = Vec::new();
        for e in entries(args) {
            if version_entry(&e).is_some() {
                continue;
            }
            if let Some(word) = string_entry(&e, SENTINEL_KEY) {
                match Sentinel::from_word(&word) {
                    Some(s) => sentinel = Sentinel::merge(sentinel, Some(s)),
                    None => {
                        return Err(syn::Error::new(
                            Span::call_site(),
                            format!(
                                "announce: unknown sentinel `{}` in the arguments of `#[{}]`",
                                word, RUNNER
                            ),
                        ))
                    }
                }
                continue;
            }
            match e.as_slice() {
                [TokenTree::Ident(k), TokenTree::Group(g)]
                    if k == OWNED_KEY && g.delimiter() == Delimiter::Parenthesis =>
                {
                    owned.extend(OwnedAttrs::parse_names(g.stream())?.names().iter().cloned());
                }
                _ => rest.push(e.into_iter().collect()),
            }
        }
        let opts = syn::parse2(quote! { #(#rest),* })?;
        Ok(RunnerArgs {
            owned,
            opts,
            sentinel,
        })
    }

    /// Merges `folder` into this one (the target): the options, the names and the
    /// sentinel.
    fn merge(&mut self, folder: &RunnerArgs, name: &Ident) -> Result<(), MergeConflict> {
        self.opts = self.opts.merge(&folder.opts, name)?;
        self.owned.extend(folder.owned.names().iter().cloned());
        self.sentinel = Sentinel::merge(self.sentinel, folder.sentinel);
        Ok(())
    }

    /// The error for merged options that remove what the sentinel relies on: the `Announce`
    /// impl of the item (`mock` and `emit_item = false` have none).
    fn check_sentinel(&self) -> Result<(), String> {
        if self.sentinel == Some(Sentinel::Impl)
            && (self.opts.mock.is_some() || !self.opts.emit_item)
        {
            let option = if self.opts.mock.is_some() {
                "mock"
            } else {
                "emit_item = false"
            };
            return Err(format!(
                "announce: `{}` must be on the first announcement of an item (the one that expands \
                 first, the highest in the attributes of the item), not on a later one: the first \
                 announcement checks that the item has its `Announce` impl, which `mock` and \
                 `emit_item = false` remove",
                option
            ));
        }
        Ok(())
    }
}

// ---- the runners of an item -----------------------------------------------------

fn is_runner(a: &OuterAttr) -> bool {
    a.segments.last().map_or(false, |s| s == RUNNER)
}

/// The indices of the runners among the outer attributes.
fn runners_of(outer: &Outer) -> Vec<usize> {
    (0..outer.attrs.len())
        .filter(|&k| is_runner(&outer.attrs[k]))
        .collect()
}

/// A version found on a runner, and the path of the runner if it is known.
struct Seen {
    version: String,
    path: Option<String>,
}

/// The crate path of a runner as it is written (`::announce`): the path without
/// the name of the runner and the hidden module that holds it.
fn crate_path(a: &OuterAttr) -> String {
    let mut segments: Vec<String> = a.segments.iter().map(|s| s.to_string()).collect();
    segments.pop();
    if segments.last().map_or(false, |s| s == "__private") {
        segments.pop();
    }
    let path = segments.join("::");
    if a.leading_colon {
        format!("::{}", path)
    } else {
        path
    }
}

/// What the runners at `at` say about their versions.
fn seen_on(outer: &Outer, at: &[usize]) -> Vec<Seen> {
    let mut seen = Vec::new();
    for &k in at {
        let a = &outer.attrs[k];
        let path = Some(crate_path(a));
        let versions = a.args.as_ref().map(|g| versions_in(&g.stream()));
        match versions {
            Some(v) if !v.is_empty() => {
                seen.extend(v.into_iter().map(|version| Seen {
                    version,
                    path: path.clone(),
                }));
            }
            _ => seen.push(Seen {
                version: "unknown".to_string(),
                path,
            }),
        }
    }
    seen
}

/// The error for versions that are not all the current one, if they are not: it lists
/// every version found, in order and once, with the crate paths of the runners that have
/// it where the paths of the runners differ.
fn version_error(seen: &[Seen]) -> Option<String> {
    if seen.iter().all(|s| s.version == RUNNER_FORMAT) {
        return None;
    }
    let mut paths: Vec<&String> = seen.iter().filter_map(|s| s.path.as_ref()).collect();
    paths.dedup();
    let mut distinct: Vec<&String> = Vec::new();
    for p in paths {
        if !distinct.contains(&p) {
            distinct.push(p);
        }
    }
    let paths_differ = distinct.len() > 1;
    let mut versions: Vec<&String> = Vec::new();
    for s in seen {
        if !versions.contains(&&s.version) {
            versions.push(&s.version);
        }
    }
    let listed: Vec<String> = versions
        .iter()
        .map(|v| {
            let mut at: Vec<String> = Vec::new();
            for s in seen.iter().filter(|s| &&s.version == v) {
                if let Some(p) = &s.path {
                    let p = format!("`{}`", p);
                    if !at.contains(&p) {
                        at.push(p);
                    }
                }
            }
            if paths_differ && !at.is_empty() {
                format!("`{}` (in {})", v, at.join(", "))
            } else {
                format!("`{}`", v)
            }
        })
        .collect();
    Some(format!(
        "announce: this item is announced by several versions of the `announce` crate (or by \
         an incompatible one): the format versions found on its `#[{}]` attributes are {}; \
         this `announce` expects `{}`. Use one version of `announce` in every crate that \
         announces the item (`cargo tree -i announce` shows the versions)",
        RUNNER,
        listed.join(", "),
        RUNNER_FORMAT
    ))
}

/// Merges the runners at `at` (the last one is the target; their versions are fine) and
/// `last`'s own, if any, into one.
fn merged_runners(
    outer: &Outer,
    at: &[usize],
    name: &Ident,
) -> Result<RunnerArgs, Result<MergeConflict, syn::Error>> {
    let read = |k: usize| {
        let args = outer.attrs[k]
            .args
            .as_ref()
            .map(|g| g.stream())
            .unwrap_or_default();
        RunnerArgs::read(args).map_err(Err)
    };
    let (last, earlier) = at.split_last().expect("a runner");
    let mut target = read(*last)?;
    for &k in earlier {
        target.merge(&read(k)?, name).map_err(Ok)?;
    }
    Ok(target)
}

/// Whether the outer attribute is owned by a check: it names one of the attributes, or
/// is a `cfg_attr` that has one among its attributes.
fn owns(a: &OuterAttr, owned: &OwnedAttrs) -> bool {
    fn in_cfg_attr(args: TokenStream, owned: &OwnedAttrs) -> bool {
        // The first entry is the condition.
        entries(args).iter().skip(1).any(|e| match e.as_slice() {
            [TokenTree::Ident(id), TokenTree::Group(g)]
                if id == "cfg_attr" && g.delimiter() == Delimiter::Parenthesis =>
            {
                in_cfg_attr(g.stream(), owned)
            }
            [TokenTree::Ident(id), ..] => owned.contains(id.to_string().trim_start_matches("r#")),
            _ => false,
        })
    }
    a.is_owned_by(owned)
        || (a.is_bare("cfg_attr")
            && a.args
                .as_ref()
                .map_or(false, |g| in_cfg_attr(g.stream(), owned)))
}

// ---- #[announce(..)] ------------------------------------------------------------------

/// The built-in attributes that no macro can be: the shortcut of `can_expand_after` skips them.
///
/// They are matched as a single segment without a leading `::`, and rustc makes a bare name of
/// a built-in attribute ambiguous with any macro of that name that is in scope, so that
/// no attribute macro can be one of these (and a path such as `x::doc` is not one). None
/// of them expands to anything, none changes which attributes the item has, and none can
/// be a derive helper that matters here:
///
/// - `doc` (`///` and `//!` are `doc`), `repr`, `must_use`, `deprecated`, `non_exhaustive`,
///   `inline`, `cold`, `track_caller`, `no_mangle`, `automatically_derived` and `macro_use`
///   describe the item to the compiler;
/// - `allow`, `warn`, `deny`, `forbid` and `expect` set lint levels.
///
/// Not in the list: `cfg` and `cfg_attr` (a `cfg_attr` can expand to a macro), `derive`, and
/// every attribute that is not built in (the bare `announce` too: it can be a macro of
/// another crate).
const INERT: &[&str] = &[
    "doc",
    "repr",
    "allow",
    "warn",
    "deny",
    "forbid",
    "expect",
    "must_use",
    "deprecated",
    "non_exhaustive",
    "inline",
    "cold",
    "track_caller",
    "no_mangle",
    "automatically_derived",
    "macro_use",
];

/// Whether something can still expand on the item after the announcement that is being
/// made: an outer attribute that is neither [`INERT`] nor owned by the checks of the
/// announcement (`owned`, which `#[announce]` strips). If not, the announcement is made at
/// once, with no runner.
pub(crate) fn can_expand_after(item: &TokenStream, owned: &OwnedAttrs) -> bool {
    if !scan::starts_with_attr(item) {
        return false;
    }
    scan::outer(item).attrs.iter().any(|a| {
        let inert = !a.leading_colon
            && a.segments.len() == 1
            && INERT.contains(&a.segments[0].to_string().as_str());
        !inert && !owns(a, owned)
    })
}

/// What [`assemble`] does with the runner.
enum Run {
    /// Nothing: the item without the attributes `drop`.
    None,
    /// There is none: this attribute (`#[..]`) is put after the last attribute.
    New(TokenStream),
    /// The attribute at this index is the last runner: its arguments are replaced.
    Replace(usize, TokenStream),
}

/// The item without the attributes at `drop`, with the runner as `run` says, and the
/// attributes at `moved` right after it (after those attributes of `owned` that follow it
/// already).
fn assemble(
    outer: &Outer,
    drop: &[usize],
    moved: &[usize],
    owned: &OwnedAttrs,
    run: Run,
) -> TokenStream {
    let tokens = |k: usize| {
        let a = &outer.attrs[k];
        [outer.tts[a.at].clone(), outer.tts[a.at + 1].clone()]
    };
    let gone = |k: usize| drop.contains(&k) || moved.contains(&k);
    // The owned attributes that follow the runner already come first.
    let after = match &run {
        Run::Replace(at, _) => {
            let mut last = *at;
            while last + 1 < outer.attrs.len()
                && !gone(last + 1)
                && owns(&outer.attrs[last + 1], owned)
            {
                last += 1;
            }
            Some(last)
        }
        _ => None,
    };
    let mut out = TokenStream::new();
    let emit_moved = |out: &mut TokenStream| {
        for &k in moved {
            out.extend(tokens(k));
        }
    };
    for (k, a) in outer.attrs.iter().enumerate() {
        if gone(k) {
            continue;
        }
        match &run {
            Run::Replace(at, args) if *at == k => {
                let span = match &outer.tts[a.at + 1] {
                    TokenTree::Group(g) => g.span(),
                    _ => Span::call_site(),
                };
                let mut group = Group::new(Delimiter::Bracket, a.with_args(args.clone()));
                group.set_span(span);
                out.extend([outer.tts[a.at].clone(), TokenTree::Group(group)]);
            }
            _ => out.extend(tokens(k)),
        }
        if after == Some(k) {
            emit_moved(&mut out);
        }
    }
    if let Run::New(attr) = &run {
        out.extend(attr.clone());
        emit_moved(&mut out);
    }
    out.extend(outer.tts[outer.attrs_end()..].iter().cloned());
    out
}

/// The `#[cfg(..)]` attributes of the item, which its sentinel has to have as well, or
/// `None` if the item has a `#[cfg_attr(..)]` that can make a `cfg` of its own: the sentinel
/// is left out then, as it could not tell whether the item is there.
fn sentinel_cfgs(outer: &Outer) -> Option<TokenStream> {
    fn mentions_cfg(ts: TokenStream) -> bool {
        ts.into_iter().any(|t| match t {
            TokenTree::Ident(i) => i == "cfg",
            TokenTree::Group(g) => mentions_cfg(g.stream()),
            _ => false,
        })
    }
    let mut cfgs = TokenStream::new();
    for a in &outer.attrs {
        if a.is_bare("cfg") {
            cfgs.extend([outer.tts[a.at].clone(), outer.tts[a.at + 1].clone()]);
        } else if a.is_bare("cfg_attr")
            && a.args.as_ref().map_or(true, |g| mentions_cfg(g.stream()))
        {
            return None;
        }
    }
    Some(cfgs)
}

/// An `#[announce]` that is emitted and has attributes of the item to wait for, with the
/// checks answered (`owned`: what they own; empty without checks): the item with the
/// runner and the sentinel after it, or with the options merged into the runner that is
/// there (no sentinel then: the announcement that put the runner there emitted it). See the
/// module docs.
pub(crate) fn announce(
    opts: &Options,
    head: &scan::Head,
    item: TokenStream,
    owned: &OwnedAttrs,
) -> TokenStream {
    let name = &head.ident;
    let outer = scan::outer(&item);
    let runners = runners_of(&outer);
    // The attributes that the checks own, above the runner: below it.
    let above = runners.last().copied().unwrap_or(outer.attrs.len());
    let moved: Vec<usize> = (0..above)
        .filter(|&k| !runners.contains(&k) && owns(&outer.attrs[k], owned))
        .collect();
    if runners.is_empty() {
        let cfgs = sentinel_cfgs(&outer);
        let sentinel = cfgs.as_ref().map(|_| Sentinel::of(head, opts));
        let args = RunnerArgs {
            owned: owned.clone(),
            opts: opts.clone(),
            sentinel,
        };
        let krate = opts.krate();
        let runner = Ident::new(RUNNER, Span::call_site());
        let attr = quote! { #[#krate::__private::#runner(#args)] };
        let mut out = assemble(&outer, &[], &moved, owned, Run::New(attr));
        if let (Some(sentinel), Some(cfgs)) = (sentinel, cfgs) {
            out.extend(sentinel.tokens(head, opts, &cfgs));
        }
        return out;
    }
    // The versions first, then the rest is read.
    if let Some(message) = version_error(&seen_on(&outer, &runners)) {
        // No runner is left, so they do not report the same error again.
        return error_with_item(
            message,
            &assemble(&outer, &runners, &[], &OwnedAttrs::new(), Run::None),
        );
    }
    let mut target = match merged_runners(&outer, &runners, name) {
        Ok(t) => t,
        Err(Ok(c)) => return conflict(&c, &item),
        Err(Err(e)) => return error_with_item(e.to_string(), &item),
    };
    let mine = RunnerArgs {
        owned: owned.clone(),
        opts: opts.clone(),
        sentinel: None,
    };
    if let Err(c) = target.merge(&mine, name) {
        return conflict(&c, &item);
    }
    if let Err(message) = target.check_sentinel() {
        return error_with_item(message, &item);
    }
    let last = *runners.last().unwrap();
    let earlier = &runners[..runners.len() - 1];
    assemble(
        &outer,
        earlier,
        &moved,
        &target.owned,
        Run::Replace(last, target.to_token_stream()),
    )
}

// ---- the runner expands ---------------------------------------------------------------

/// The message of the error for an announcement below the runner.
const LATE: &str = "announce: this item has an `#[announce]` that expands after its \
     announcements were run, so it makes a second trampoline: a macro of the item has put it \
     below the other attributes of its output (for example by appending it to the attributes \
     of the item). It is removed. Put it first among the attributes of the output \
     (`quote! { #[announce(..)] #item }`)";

/// An attribute that announces the item and is left below the runner: a path that ends in
/// `announce` and is qualified (`::announce::announce`, `x::announce`).
fn is_late(a: &OuterAttr) -> bool {
    match a.segments.last() {
        Some(l) if l == "announce" => a.leading_colon || a.segments.len() > 1,
        _ => false,
    }
}

/// What the runner expands to: `args` are its arguments, `item` the item without it.
/// See the module docs.
pub(crate) fn run(args: TokenStream, item: TokenStream) -> TokenStream {
    let outer = scan::outer(&item);
    let others = runners_of(&outer);
    // The versions of this runner, and of the others.
    let mut seen: Vec<Seen> = versions_in(&args)
        .into_iter()
        .map(|version| Seen {
            version,
            path: None,
        })
        .collect();
    if seen.is_empty() {
        seen.push(Seen {
            version: "unknown".to_string(),
            path: None,
        });
    }
    seen.extend(seen_on(&outer, &others));
    if let Some(message) = version_error(&seen) {
        // No runner is left, so the others do not report the same error again, and no
        // trampoline is made. The sentinel that this runner promised (if it can be read) is
        // satisfied by a stand-in, so that it does not add an error to this one.
        let mut out = error_with_item(
            message,
            &assemble(&outer, &others, &[], &OwnedAttrs::new(), Run::None),
        );
        if let (Ok(mine), Ok(head)) = (RunnerArgs::read(args), scan::head(&item)) {
            if let Some(sentinel) = mine.sentinel {
                out.extend(crate::generate::sentinel_stub(
                    sentinel,
                    &head.ident,
                    &mine.opts,
                    crate::random_u64(),
                ));
            }
        }
        return out;
    }
    let head = match scan::head(&item) {
        Ok(h) => h,
        Err(e) => return error_with_item(e.to_string(), &item),
    };
    let mine = match RunnerArgs::read(args) {
        Ok(a) => a,
        Err(e) => return error_with_item(e.to_string(), &item),
    };
    // Another runner is on the item (put by another `announce`): this one goes into the
    // last, which is the target. It expands next.
    if !others.is_empty() {
        let mut target = match merged_runners(&outer, &others, &head.ident) {
            Ok(t) => t,
            Err(Ok(c)) => return conflict(&c, &item),
            Err(Err(e)) => return error_with_item(e.to_string(), &item),
        };
        if let Err(c) = target.merge(&mine, &head.ident) {
            return conflict(&c, &item);
        }
        if let Err(message) = target.check_sentinel() {
            return error_with_item(message, &item);
        }
        let last = *others.last().unwrap();
        return assemble(
            &outer,
            &others[..others.len() - 1],
            &[],
            &target.owned,
            Run::Replace(last, target.to_token_stream()),
        );
    }
    // Announcements that a macro below the runner has put on the item.
    let mut errors = TokenStream::new();
    let late: Vec<usize> = (0..outer.attrs.len())
        .filter(|&k| is_late(&outer.attrs[k]))
        .collect();
    let item = if late.is_empty() {
        item
    } else {
        errors = syn::Error::new(Span::call_site(), LATE).to_compile_error();
        assemble(&outer, &late, &[], &OwnedAttrs::new(), Run::None)
    };
    let random = crate::random_u64();
    let attr = mine.opts.to_token_stream();
    errors.extend(crate::capture_output(
        &mine.opts,
        &attr,
        item,
        random,
        &mine.owned,
        mine.sentinel,
    ));
    errors
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ts(s: &str) -> TokenStream {
        s.parse().unwrap()
    }

    fn norm(t: &TokenStream) -> String {
        t.to_string().split_whitespace().collect()
    }

    fn opts(s: &str) -> Options {
        syn::parse2(ts(s)).unwrap()
    }

    /// `announce` for `me` on `item`, printed.
    fn made(me: &str, owned: &[&str], item: &str) -> String {
        let head = scan::head(&ts(item)).unwrap();
        norm(&announce(
            &opts(me),
            &head,
            ts(item),
            &OwnedAttrs::from_strs(owned),
        ))
    }

    /// The runner at the crate path `::l::announce` with these entries.
    fn runner(entries: &str) -> String {
        format!(
            "#[::l::announce::__private::__announce_runner({})]",
            entries
        )
    }

    const V: &str = "__announce_format_version = \"1\"";

    /// The sentinel of a struct `S` without generics, as printed (after the item).
    const CHK: &str = "#[allow(deprecated)]const_:()={trait__AnnounceSentinel{}\
                       impl__AnnounceSentinelforSwhereS:::l::announce::Announce{}};";

    #[test]
    fn the_runner_is_put_last_with_the_version() {
        assert_eq!(
            made(
                "crate = ::l::announce, meta = a::x",
                &[],
                "#[b] #[c(1)] pub struct S(pub u8);"
            ),
            [
                "#[b]#[c(1)]#[::l::announce::__private::__announce_runner(\
                 __announce_format_version=\"1\",__announce_sentinel=\"impl\",\
                 crate=::l::announce,meta=a::x)]pubstructS(pubu8);",
                CHK
            ]
            .concat()
        );
        // Whatever the kind of the item.
        for item in [
            "pub enum S {}",
            "union S { a: u8 }",
            "pub trait S {}",
            "type S = u8;",
            "mod S {}",
        ] {
            let out = made("crate = ::l::announce", &[], &format!("#[x] {}", item));
            assert!(
                out.starts_with("#[x]#[::l::announce::__private::__announce_runner(")
                    && out.contains("__announce_format_version=\"1\""),
                "{}",
                out
            );
            assert!(
                !out.contains("Capture") && !out.contains("macro_rules"),
                "{}",
                out
            );
        }
        // No options: the version is all there is. The krate is read from Cargo.toml,
        // which has `announce` in this crate's dev-dependency graph as `::announce`.
        let out = made("", &[], "#[x] struct S;");
        assert!(
            out.contains(
                "__announce_runner(__announce_format_version=\"1\",__announce_sentinel=\"impl\")]structS;"
            ),
            "{}",
            out
        );
    }

    #[test]
    fn the_names_that_checks_own_are_in_the_runner() {
        assert_eq!(
            made(
                "crate = ::l::announce, a::check",
                &["my_lib", "other"],
                "#[x] pub struct S;"
            ),
            [
                "#[x]#[::l::announce::__private::__announce_runner(__announce_format_version=\"1\",\
                 __announce_sentinel=\"impl\",__announce_owned(my_lib,other),\
                 crate=::l::announce,a::check)]pubstructS;",
                CHK
            ]
            .concat()
        );
    }

    #[test]
    fn the_attributes_that_checks_own_follow_the_runner() {
        // Rustc rejects an attribute that nothing knows above an attribute macro: the
        // owned ones come after the runner, in their order; the others stay.
        assert_eq!(
            made(
                "crate = ::l::announce, a::check",
                &["my_lib", "other"],
                "#[my_lib(1)] #[doc = \"a\"] #[other] #[my_lib::y = 1] #[::my_lib] #[derive(Debug)] struct S;"
            ),
            [
                "#[doc=\"a\"]#[::my_lib]#[derive(Debug)]\
                 #[::l::announce::__private::__announce_runner(__announce_format_version=\"1\",\
                 __announce_sentinel=\"impl\",__announce_owned(my_lib,other),\
                 crate=::l::announce,a::check)]\
                 #[my_lib(1)]#[other]#[my_lib::y=1]structS;",
                CHK
            ]
            .concat()
        );
        // A `cfg_attr` that has an owned attribute among its attributes is owned as well,
        // and so is one nested in another; one that has not is not.
        assert_eq!(
            made(
                "crate = ::l::announce",
                &["my_lib"],
                "#[cfg_attr(a, my_lib(1))] #[cfg_attr(a, inline, cfg_attr(b, my_lib))] #[cfg_attr(a, inline)] #[cfg_attr(my_lib, inline)] struct S;"
            ),
            [
                "#[cfg_attr(a,inline)]#[cfg_attr(my_lib,inline)]\
                 #[::l::announce::__private::__announce_runner(__announce_format_version=\"1\",\
                 __announce_sentinel=\"impl\",__announce_owned(my_lib),crate=::l::announce)]\
                 #[cfg_attr(a,my_lib(1))]#[cfg_attr(a,inline,cfg_attr(b,my_lib))]structS;",
                CHK
            ]
            .concat()
        );
        // Merged into the runner: those that were there stay first, the new ones follow.
        assert_eq!(
            made(
                "crate = ::l::announce, b::check",
                &["other"],
                &format!(
                    "#[other(2)] #[x] {} #[my_lib(1)] #[y] struct S;",
                    runner(&format!("{}, __announce_owned(my_lib)", V))
                )
            ),
            "#[x]#[::l::announce::__private::__announce_runner(__announce_format_version=\"1\",\
             __announce_owned(my_lib,other),crate=::l::announce,b::check)]\
             #[my_lib(1)]#[other(2)]#[y]structS;"
        );
    }

    #[test]
    fn an_announcement_merges_into_the_runner_that_is_there() {
        // The runner is the target: its entries come first, the names are united, there
        // is one version entry.
        assert_eq!(
            made(
                "meta = b::x, c::check",
                &["other"],
                &format!(
                    "#[x] {} #[y] pub struct S(pub u8);",
                    runner(&format!("{}, __announce_owned(my_lib), crate = ::l::announce, meta = a::x", V))
                )
            ),
            "#[x]#[::l::announce::__private::__announce_runner(__announce_format_version=\"1\",\
             __announce_owned(my_lib,other),crate=::l::announce,meta=a::x,meta=b::x,c::check)]#[y]pubstructS(pubu8);"
        );
        // Options that disagree are one error, with the item.
        let item = format!(
            "#[x] {} struct S;",
            runner(&format!("{}, macro_name = A", V))
        );
        let out = made("macro_name = B", &[], &item);
        assert!(out.starts_with("::core::compile_error!"), "{}", out);
        assert!(out.contains("(`B`and`A`)"), "{}", out);
        assert!(out.ends_with(&norm(&ts(&item))), "{}", out);
        assert_eq!(out.matches("compile_error").count(), 1);
    }

    // ---- when nothing can expand after the announcement --------------------------------

    fn attribute(me: &str, item: &str) -> String {
        norm(&crate::announce(ts(me), ts(item)))
    }

    #[test]
    fn an_item_with_inert_attributes_only_is_announced_at_once() {
        let me = "crate = ::l::announce";
        for item in [
            "pub struct S;",
            "/// d\n#[repr(C)] pub struct S;",
            "#[doc = \"d\"] #[repr(C)] #[allow(dead_code)] #[must_use] #[deprecated] #[non_exhaustive] pub struct S;",
            "#[inline] #[cold] #[track_caller] #[no_mangle] #[automatically_derived] #[macro_use] #[warn(x)] #[deny(x)] #[forbid(x)] #[expect(x)] pub struct S;",
            "/// d\npub trait S {}",
        ] {
            let out = attribute(me, item);
            assert!(
                out.contains("Capture") && !out.contains("__announce_runner"),
                "{}: {}",
                item,
                out
            );
        }
    }

    #[test]
    fn an_item_with_an_attribute_that_may_expand_has_the_runner() {
        let me = "crate = ::l::announce";
        for item in [
            "/// d\n#[repr(C)] #[derive(Debug)] pub struct S;",
            "#[cfg_attr(a, derive(Debug))] pub struct S;",
            "#[cfg(a)] pub struct S;",
            "#[x::doc] pub struct S;",
            "#[::doc] pub struct S;",
            "#[other] pub struct S;",
            // A bare `announce` may be the macro of another crate: it may expand.
            "#[announce] pub struct S;",
            "#[announce(level = 2)] pub struct S;",
            "/// d\n#[repr(C)] #[x] pub trait S {}",
            // The names of the attributes that are not in the list, or that only look alike.
            "#[docs] pub struct S;",
            "#[reprs] pub struct S;",
        ] {
            let out = attribute(me, item);
            assert!(
                out.contains("__announce_runner") && !out.contains("Capture"),
                "{}: {}",
                item,
                out
            );
        }
    }

    #[test]
    fn the_attributes_that_the_checks_own_do_not_count() {
        // After the query: `lib_check` owns `my_lib`. Nothing else can expand, so the
        // trampoline is made, and the attribute is stripped from the item that is emitted.
        let state = |item: &str, owned: &str| {
            ts(&format!(
                "@__announce_attrs [ ::l::announce::__private::finish! ] {{ [7u64; attr (crate=::l::announce,a::check) {{ {} }}] {} }}",
                item, owned
            ))
        };
        let out = norm(&crate::finish(state(
            "/// d\n#[my_lib(x)] pub struct S { #[my_lib(y)] pub a: u8 }",
            "attrs(my_lib)",
        )));
        assert!(!out.contains("__announce_runner"), "{}", out);
        assert!(
            out.starts_with("#[derive(::l::announce::__private::Capture)]"),
            "{}",
            out
        );
        assert!(
            out.contains("[my_lib];crate=::l::announce,a::check)]"),
            "{}",
            out
        );
        // The item is only in the dummy: the derive emits it without the attributes. There it
        // keeps them.
        assert_eq!(out.matches("#[my_lib(x)]").count(), 1, "{}", out);
        // A `cfg_attr` that has one of them counts as one of them.
        let out = norm(&crate::finish(state(
            "#[cfg_attr(a, my_lib(x))] pub struct S;",
            "attrs(my_lib)",
        )));
        assert!(!out.contains("__announce_runner"), "{}", out);
        // Without the answer the attribute is an attribute that may expand: the runner,
        // and the attribute follows it.
        let out = norm(&crate::finish(state("#[my_lib(x)] pub struct S;", "")));
        assert!(out.contains("__announce_runner"), "{}", out);
        // Another attribute, next to the owned one: the runner, with the owned attribute
        // after it.
        let out = norm(&crate::finish(state(
            "#[my_lib(x)] #[derive(Debug)] pub struct S;",
            "attrs(my_lib)",
        )));
        assert!(
            out.starts_with("#[derive(Debug)]#[::l::announce::__private::__announce_runner(")
                && out.contains("#[my_lib(x)]pubstructS;"),
            "{}",
            out
        );
    }

    // ---- the sentinel -----------------------------------------------------------------

    #[test]
    fn the_sentinel_is_an_impl_check_for_a_struct_enum_or_union_without_generics() {
        for item in [
            "pub struct S(pub u8);",
            "enum S { A }",
            "pub(crate) union S { a: u8 }",
        ] {
            let out = made("crate = ::l::announce", &[], &format!("#[x] {}", item));
            assert!(out.contains("__announce_sentinel=\"impl\""), "{}", out);
            // After the item, a sibling.
            assert!(out.ends_with(CHK), "{}", out);
            assert_eq!(out.matches("__AnnounceSentinel").count(), 2, "{}", out);
            assert!(!out.contains("__announce_exists"), "{}", out);
        }
    }

    #[test]
    fn the_sentinel_is_the_call_of_the_trampoline_for_everything_else() {
        for (opts, item, name) in [
            ("", "struct S<T>(T);", "S"),
            ("", "struct S<'a>(&'a u8);", "S"),
            ("", "pub enum S<const N: usize> { A }", "S"),
            ("", "union S<T: Copy> { a: T }", "S"),
            ("", "pub trait S {}", "S"),
            ("", "type S = u8;", "S"),
            ("", "mod S {}", "S"),
            ("mock = a::B", "struct S;", "S"),
            ("emit_item = false", "struct S;", "S"),
            ("macro_name = N", "trait S {}", "N"),
            ("macro_name = N, emit_item = false", "mod S {}", "N"),
        ] {
            let opts = format!("crate = ::l::announce, {}", opts);
            let out = made(&opts, &[], &format!("#[x] {}", item));
            assert!(
                out.contains("__announce_sentinel=\"macro\""),
                "{}: {}",
                item,
                out
            );
            assert!(
                out.ends_with(&format!(
                    "{}!{{@announce_format\"1\"@__announce_exists}}",
                    name
                )),
                "{}: {}",
                item,
                out
            );
            assert!(!out.contains("__AnnounceSentinel"), "{}: {}", item, out);
        }
        // A raw name is called as it is written.
        let out = made("", &[], "#[x] trait r#match {}");
        assert!(
            out.ends_with("r#match!{@announce_format\"1\"@__announce_exists}"),
            "{}",
            out
        );
    }

    #[test]
    fn the_sentinel_has_the_cfgs_of_the_item() {
        // The item and its sentinel go away together.
        let out = made(
            "crate = ::l::announce",
            &[],
            "#[cfg(a)] #[x] #[cfg(not(b))] struct S;",
        );
        assert!(
            out.ends_with(&format!("#[cfg(a)]#[cfg(not(b))]{}", CHK)),
            "{}",
            out
        );
        let out = made("crate = ::l::announce", &[], "#[cfg(a)] #[x] trait S {}");
        assert!(
            out.ends_with("#[cfg(a)]S!{@announce_format\"1\"@__announce_exists}"),
            "{}",
            out
        );
        // A `cfg_attr` that is not about a `cfg` does not matter; one that can make a `cfg`
        // leaves the sentinel out, as it could not tell if the item is there.
        let out = made(
            "crate = ::l::announce",
            &[],
            "#[cfg_attr(a, derive(Debug))] struct S;",
        );
        assert!(out.ends_with(CHK), "{}", out);
        for item in [
            "#[cfg_attr(a, cfg(b))] #[x] struct S;",
            "#[x] #[cfg_attr(a, cfg_attr(b, cfg(c)))] trait S {}",
        ] {
            let out = made("crate = ::l::announce", &[], item);
            assert!(
                out.contains("__announce_runner(__announce_format_version=\"1\"")
                    && !out.contains("__announce_sentinel")
                    && !out.contains("__AnnounceSentinel")
                    && !out.contains("__announce_exists"),
                "{}: {}",
                item,
                out
            );
        }
    }

    #[test]
    fn only_the_announcement_that_puts_the_runner_emits_a_sentinel() {
        // Merged into the runner: no sentinel, and the one of the runner is kept.
        let item = format!(
            "#[x] {} struct S;",
            runner(&format!("{}, __announce_sentinel = \"impl\"", V))
        );
        let out = made("meta = b::x", &[], &item);
        assert!(!out.contains("compile_error"), "{}", out);
        assert_eq!(out.matches("__announce_sentinel").count(), 1, "{}", out);
        assert!(!out.contains("__AnnounceSentinel"), "{}", out);
        // The runner that expands, merged into another one.
        let out = norm(&run(
            ts(&format!(
                "{}, __announce_sentinel = \"macro\", meta = a::x",
                V
            )),
            ts(&item),
        ));
        assert!(
            out.contains("__announce_sentinel=\"impl\"") && !out.contains("\"macro\""),
            "{}",
            out
        );
        assert!(!out.contains("__AnnounceSentinel"), "{}", out);
        // At once: no runner, no sentinel.
        let out = attribute("crate = ::l::announce", "/// d\npub struct S;");
        assert!(
            !out.contains("__announce_sentinel") && !out.contains("__AnnounceSentinel"),
            "{}",
            out
        );
        // The runner reads its sentinel, and refuses one that it does not know.
        let args =
            RunnerArgs::read(ts(&format!("{}, __announce_sentinel = \"macro\"", V))).unwrap();
        assert_eq!(args.sentinel, Some(Sentinel::Macro));
        assert_eq!(
            norm(&args.to_token_stream()),
            "__announce_format_version=\"1\",__announce_sentinel=\"macro\""
        );
        let e = RunnerArgs::read(ts(&format!("{}, __announce_sentinel = \"cake\"", V)))
            .err()
            .unwrap();
        assert!(e.to_string().contains("unknown sentinel `cake`"), "{}", e);
    }

    #[test]
    fn mock_and_emit_item_false_belong_to_the_first_announcement_of_a_struct() {
        let with = |sentinel: &str, opt: &str| {
            format!(
                "#[x] {} struct S;",
                runner(&format!(
                    "{}, __announce_sentinel = \"{}\", {}",
                    V, sentinel, opt
                ))
            )
        };
        for (opt, shown) in [
            ("emit_item = false", "`emit_item=false`"),
            ("mock = a::B", "`mock`"),
        ] {
            // A later announcement that takes the `Announce` impl away is one error, with
            // the item (and its runner, so that nothing else follows).
            let item = format!(
                "#[x] {} struct S;",
                runner(&format!("{}, __announce_sentinel = \"impl\"", V))
            );
            let out = made(opt, &[], &item);
            assert!(out.starts_with("::core::compile_error!"), "{}", out);
            assert!(
                out.contains("mustbeonthefirstannouncementofanitem") && out.contains(shown),
                "{}",
                out
            );
            assert_eq!(out.matches("compile_error").count(), 1, "{}", out);
            assert!(out.ends_with(&norm(&ts(&item))), "{}", out);
            // The same when the runner that expands is the one that does it.
            let out = norm(&run(ts(&format!("{}, {}", V, opt)), ts(&item)));
            assert!(
                out.contains("mustbeonthefirstannouncementofanitem"),
                "{}",
                out
            );
            assert!(out.ends_with(&norm(&ts(&item))), "{}", out);
            // The first announcement has it already (no impl, so a macro for the sentinel).
            let item = with("macro", opt);
            let out = made("meta = a::x", &[], &item);
            assert!(!out.contains("compile_error"), "{}", out);
        }
        // It is the same option when the runner that expands has it.
        let out = made(
            "meta = a::x",
            &[],
            &format!(
                "#[x] {} struct S;",
                runner(&format!(
                    "{}, __announce_sentinel = \"macro\", mock = a::B",
                    V
                ))
            ),
        );
        assert!(!out.contains("compile_error"), "{}", out);
    }

    // ---- the versions ----------------------------------------------------------------

    #[test]
    fn a_runner_of_the_current_version_is_read() {
        let item = format!("#[x] {} struct S;", runner(&format!("{}, meta = a::x", V)));
        let out = made("meta = b::x", &[], &item);
        assert!(!out.contains("compile_error"), "{}", out);
        // Duplicate entries of the version are fine, and there is one in the result.
        let item = format!(
            "#[x] {} struct S;",
            runner(&format!("{}, meta = a::x, {}", V, V))
        );
        let out = made("meta = b::x", &[], &item);
        assert!(!out.contains("compile_error"), "{}", out);
        assert_eq!(
            out.matches("__announce_format_version").count(),
            1,
            "{}",
            out
        );
        // Entries can come in any place and the rest need not be in the encoding of this
        // version when the version is not ours.
        let item = format!("#[x] {} struct S;", runner(&format!("meta = a::x, {}", V)));
        let out = made("", &[], &item);
        assert!(
            out.contains("meta=a::x") && !out.contains("compile_error"),
            "{}",
            out
        );
    }

    /// The text of the error for an item with these runners, from `announce` and from the
    /// runner (`first` is the arguments of the runner that expands).
    fn version_errors(first: &str, others: &[String]) -> (String, String) {
        let item = format!("#[x] {} struct S;", others.join(" "));
        let from_announce = made("", &[], &item);
        let from_runner = norm(&run(ts(first), ts(&item)));
        (from_announce, from_runner)
    }

    fn assert_version_error(out: &str, listed: &[&str]) {
        assert!(out.starts_with("::core::compile_error!"), "{}", out);
        assert_eq!(out.matches("compile_error").count(), 1, "{}", out);
        assert!(
            out.contains("severalversionsofthe`announce`crate"),
            "{}",
            out
        );
        assert!(out.contains("this`announce`expects`1`"), "{}", out);
        assert!(out.contains("cargotree-iannounce"), "{}", out);
        for l in listed {
            assert!(out.contains(l), "{}: {}", l, out);
        }
        // Nothing is read: no trampoline.
        assert!(!out.contains("macro_rules"), "{}", out);
    }

    #[test]
    fn a_runner_of_another_version_is_refused_without_reading_it() {
        // Two different entries in one runner. The rest is not in our encoding.
        let both =
            runner("__announce_format_version = \"1\", ?? !!, __announce_format_version = \"2\"");
        let (a, r) = version_errors(V, std::slice::from_ref(&both));
        for out in [norm(&ts(&a)), r] {
            assert_version_error(&out, &["`1`,`2`"]);
        }
        // The item is there, without the runner that reported (no cascade).
        let (a, _) = version_errors(V, std::slice::from_ref(&both));
        assert!(a.ends_with("#[x]structS;"), "{}", a);
        let out = norm(&run(ts(V), ts(&format!("#[x] {} struct S;", both))));
        assert!(out.ends_with("#[x]structS;"), "{}", out);
        // One entry of another version.
        let (a, r) = version_errors(V, &[runner("__announce_format_version = \"2\", ?? !!")]);
        assert_version_error(&a, &["`2`"]);
        assert_version_error(&r, &["`2`"]);
        // The runner that expands is the one with the other version.
        let r = norm(&run(
            ts("__announce_format_version = \"7\""),
            ts("#[x] struct S;"),
        ));
        assert_version_error(&r, &["`7`"]);
    }

    #[test]
    fn runners_of_different_versions_are_listed_with_their_crate_paths() {
        let a =
            "#[::one::announce::__private::__announce_runner(__announce_format_version = \"1\")]";
        let b = "#[::two::announce::__private::__announce_runner(__announce_format_version = \"2\", ?? !!)]";
        let c = "#[::three::__announce_runner(__announce_format_version = \"2\")]";
        let item = format!("#[x] {} {} {} struct S;", a, b, c);
        let out = made("", &[], &item);
        assert_version_error(
            &out,
            &["`1`(in`::one::announce`),`2`(in`::two::announce`,`::three`)"],
        );
        assert_eq!(out.matches("`2`(in`::two").count(), 1, "{}", out);
        // Once for a version at one path, in the order they come.
        let item = format!("#[x] {} {} {} struct S;", b, a, b);
        let out = made("", &[], &item);
        assert_version_error(&out, &["`2`(in`::two::announce`),`1`(in`::one::announce`)"]);
        assert_eq!(out.matches("`2`(in`::two").count(), 1, "{}", out);
        // The runner that expands is one of them (its path is not known, so the paths do
        // not differ).
        let r = norm(&run(
            ts("__announce_format_version = \"1\""),
            ts(&format!("#[x] {} struct S;", b)),
        ));
        assert_version_error(&r, &["`1`,`2`"]);
        // The same version at the same path is listed without the path.
        let item = format!(
            "#[x] {} {} struct S;",
            runner("__announce_format_version = \"3\""),
            runner("__announce_format_version = \"3\", __announce_format_version = \"4\"")
        );
        let out = made("", &[], &item);
        assert_version_error(&out, &["`3`,`4`"]);
        assert!(!out.contains("(in"), "{}", out);
    }

    #[test]
    fn a_runner_without_a_version_is_version_unknown() {
        for r in [
            runner("meta = a::x"),
            runner(""),
            "#[::l::announce::__private::__announce_runner]".to_string(),
            runner("__announce_format_version, meta = a::x"),
            runner("__announce_format_version(1)"),
            runner("x::__announce_format_version = \"1\""),
        ] {
            let item = format!("#[x] {} struct S;", r);
            let out = made("", &[], &item);
            assert_version_error(&out, &["`unknown`"]);
            // The item is still there; the runners are not, so that they do not report
            // the error again.
            assert!(out.ends_with("#[x]structS;"), "{}: {}", r, out);
        }
        // The runner that expands, too: its arguments are all there is.
        for args in ["", "meta = a::x", "__announce_format_version, meta = a::x"] {
            let out = norm(&run(ts(args), ts("#[x] struct S;")));
            assert_version_error(&out, &["`unknown`"]);
        }
        // Unknown next to the current one.
        let item = format!("#[x] {} {} struct S;", runner(V), runner("meta = a::x"));
        assert_version_error(&made("", &[], &item), &["`1`", "`unknown`"]);
    }

    #[test]
    fn several_runners_of_the_current_version_are_merged_into_the_last() {
        let first = "#[::one::announce::__private::__announce_runner(__announce_format_version = \"1\", __announce_owned(a), crate = ::one::announce, meta = one::x)]";
        let second = "#[::two::announce::__private::__announce_runner(__announce_format_version = \"1\", __announce_owned(b), crate = ::two::announce, meta = two::x)]";
        // From an announcement: the last is the target, the earlier is gone.
        let out = made(
            "meta = three::x",
            &["c"],
            &format!("#[x] {} #[y] {} struct S;", first, second),
        );
        assert_eq!(
            out,
            "#[x]#[y]#[::two::announce::__private::__announce_runner(__announce_format_version=\"1\",\
             __announce_owned(b,a,c),crate=::two::announce,meta=two::x,meta=one::x,meta=three::x)]structS;"
        );
        // From the runner that expands: it goes into the last one, which runs next.
        let out = norm(&run(
            ts("__announce_format_version = \"1\", crate = ::zero::announce, meta = zero::x"),
            ts(&format!("#[x] {} {} struct S;", first, second)),
        ));
        assert_eq!(
            out,
            "#[x]#[::two::announce::__private::__announce_runner(__announce_format_version=\"1\",\
             __announce_owned(b,a),crate=::two::announce,meta=two::x,meta=one::x,meta=zero::x)]structS;"
        );
        // Options that disagree are one error.
        let clash = "#[::two::announce::__private::__announce_runner(__announce_format_version = \"1\", macro_name = Other)]";
        let out = made("", &[], &format!("{} {} struct S;", first, clash));
        assert!(
            out.contains("`Other`") && out.starts_with("::core::compile_error!"),
            "{}",
            out
        );
    }

    // ---- the runner expands -----------------------------------------------------------

    #[test]
    fn the_runner_makes_the_trampoline_without_the_attributes_that_checks_own() {
        let out = norm(&run(
            ts(&format!(
                "{}, __announce_owned(my_lib), crate = ::l::announce, a::check",
                V
            )),
            ts("#[doc = \"d\"] #[my_lib(1)] pub struct S { #[my_lib(2)] pub a: u8 }"),
        ));
        // The derive emits the item, from its copy without them; the names go in the
        // dummy, and the item in it keeps the attributes.
        assert!(
            out.starts_with("#[derive(::l::announce::__private::Capture)]"),
            "{}",
            out
        );
        assert!(
            out.contains("#[__announce(")
                && out.contains("[my_lib];crate=::l::announce,a::check)]"),
            "{}",
            out
        );
        assert_eq!(out.matches("#[my_lib(1)]").count(), 1, "{}", out);
        // No runner is left.
        assert!(!out.contains("__announce_runner"), "{}", out);
    }

    #[test]
    fn an_announcement_below_the_runner_is_an_error_and_is_removed() {
        for late in ["#[::announce::announce(meta = l::x)]", "#[x::announce]"] {
            let out = norm(&run(
                ts(&format!("{}, crate = ::l::announce", V)),
                ts(&format!("#[x] {} {} pub struct S;", late, "#[y]")),
            ));
            assert!(
                out.starts_with("::core::compile_error!"),
                "{}: {}",
                late,
                out
            );
            assert!(
                out.contains("expandsafteritsannouncementswererun"),
                "{}",
                out
            );
            assert_eq!(out.matches("compile_error").count(), 1, "{}", out);
            // It does not make a second trampoline: it is not in the item, and the
            // trampoline of the runner is.
            // The item, and its copy in the dummy.
            assert!(
                out.contains("Capture") && out.matches("pubstructS").count() == 2,
                "{}",
                out
            );
            assert!(
                !out.contains("announce::announce") && !out.contains("derive(Debug"),
                "{}",
                out
            );
        }
        // What is not a qualified announcement stays: other derives (also one that lists
        // `Announce`), other attributes of the crate, a bare `announce` (it is the macro
        // of another crate, or rustc reports it), a renamed import (not seen).
        let out = norm(&run(
            ts(V),
            ts("#[derive(Debug, Announce)] #[x::announced] #[ann] #[announce(meta = h::x)] pub struct S;"),
        ));
        assert!(!out.contains("compile_error"), "{}", out);
        assert!(
            out.contains("#[derive(Debug,Announce)]")
                && out.contains("#[ann]")
                && out.contains("#[announce(meta=h::x)]"),
            "{}",
            out
        );
        // The bare attribute is not taken over: it is on the item and on its copy.
        assert_eq!(out.matches("#[announce(meta=h::x)]").count(), 2, "{}", out);
    }

    #[test]
    fn the_runner_of_an_item_that_is_not_emitted_drops_it() {
        for opt in ["emit_item = false", "mock = a::B"] {
            let out = norm(&run(
                ts(&format!("{}, crate = ::l::announce, {}", V, opt)),
                ts("#[doc = \"d\"] pub struct S;"),
            ));
            assert!(
                out.starts_with("#[derive(::l::announce::__private::Capture)]"),
                "{}",
                out
            );
            assert_eq!(out.matches("pubstructS").count(), 1, "{}", out);
        }
    }

    // ---- the way rustc expands the attributes ----------------------------------------

    /// A stand-in for rustc: the first attribute from the top that is a macro is
    /// expanded, until none is left. Macros: `::l::announce::announce(..)` (the real
    /// function; a bare `#[announce(..)]` is the macro of another crate, and is inert),
    /// the runner, `#[derive(..)]` (the derive of the capture is inert, the others are
    /// removed), `emit(x)` (announces what it produces: the attribute in front of the
    /// item), `pass` (returns its input), `late(x)` (adds the announcement *below* all the
    /// attributes) and `dup(n)` (announces and leaves `dup(n-1)` to expand next).
    ///
    /// Returns the number of expansions, and the item that is left.
    fn expand(item: &str) -> (usize, String) {
        let mut item = ts(item);
        let mut steps = 0;
        loop {
            let o = scan::outer(&item);
            let next = o.attrs.iter().position(|a| {
                let last = a.segments.last().map(|l| l.to_string()).unwrap_or_default();
                match last.as_str() {
                    "announce" => a.segments.len() > 1,
                    "derive" => !a
                        .args
                        .as_ref()
                        .map_or(false, |g| g.stream().to_string().contains("Capture")),
                    "emit" | "pass" | "late" | "dup" | "__announce_runner" => true,
                    _ => false,
                }
            });
            let k = match next {
                Some(k) => k,
                None => return (steps, norm(&item)),
            };
            steps += 1;
            assert!(steps < 100, "no end: {}", norm(&item));
            let a = &o.attrs[k];
            let args = a.args.as_ref().map(|g| g.stream()).unwrap_or_default();
            let mut rest = TokenStream::new();
            for (i, t) in o.tts.iter().enumerate() {
                if i != a.at && i != a.at + 1 {
                    rest.extend([t.clone()]);
                }
            }
            let name = a.segments.last().unwrap().to_string();
            item = match name.as_str() {
                "announce" => crate::announce(args, rest),
                "__announce_runner" => crate::run_announce(args, rest),
                "derive" => rest,
                "emit" => {
                    let mut out = ts(&format!(
                        "#[::l::announce::announce(crate = ::l::announce, meta = m::{})]",
                        args
                    ));
                    out.extend(rest);
                    out
                }
                "late" => {
                    let o = scan::outer(&rest);
                    let end = o.attrs_end();
                    let mut out: TokenStream = o.tts[..end].iter().cloned().collect();
                    out.extend(ts(&format!(
                        "#[::l::announce::announce(crate = ::l::announce, meta = m::{})]",
                        args
                    )));
                    out.extend(o.tts[end..].iter().cloned());
                    out
                }
                "pass" => rest,
                "dup" => {
                    let n: u32 = args.to_string().trim().parse().unwrap();
                    if n == 0 {
                        rest
                    } else {
                        let mut out = ts(&format!(
                            "#[::l::announce::announce(crate = ::l::announce, meta = m::dup{n})] #[dup({m})]",
                            n = n,
                            m = n - 1
                        ));
                        out.extend(rest);
                        out
                    }
                }
                _ => unreachable!(),
            };
        }
    }

    const A: &str = "#[::l::announce::announce(crate = ::l::announce, meta = m::a)]";
    const B: &str = "#[::l::announce::announce(meta = m::b)]";
    const C: &str = "#[::l::announce::announce(meta = m::c)]";

    /// The trampoline, from the item that is left: exactly one capture dummy, which the
    /// capture derive turns into the trampoline later.
    fn trampoline(left: &str) -> String {
        let made = left
            .matches("derive(::l::announce::__private::Capture)")
            .count();
        assert_eq!(made, 1, "{}", left);
        left.to_string()
    }

    #[test]
    fn expansion_ends_with_one_trampoline() {
        let cases: &[(String, &[&str])] = &[
            // the attributes merge in the order they expand
            (format!("{} {} #[x] struct S;", A, B), &["m::a", "m::b"]),
            (format!("{} {} {} #[x] struct S;", A, B, C), &["m::a", "m::b", "m::c"]),
            (format!("{} #[pass] {} trait S {{}}", A, B), &["m::a", "m::b"]),
            // a macro that announces, with one that does not announce below it
            ("#[emit(a)] #[pass] struct S;".to_string(), &["m::a"]),
            ("#[emit(a)] #[emit(b)] struct S;".to_string(), &["m::a", "m::b"]),
            // an unknown macro between two announcements, that announces too
            (format!("{} #[emit(two)] {} struct S;", A, B), &["m::a", "m::two", "m::b"]),
            // a macro that leaves a copy of itself with a smaller number
            ("#[dup(4)] struct S;".to_string(), &["m::dup4", "m::dup3", "m::dup2", "m::dup1"]),
            // derives of others, before and after
            (format!("#[derive(Debug)] {} #[derive(Clone)] struct S;", A), &["m::a"]),
            // no other attribute: at once
            (format!("{} struct S;", A), &["m::a"]),
            // not emitted: through the runner as well
            (format!("{} {} struct S;", "#[::l::announce::announce(crate = ::l::announce, meta = m::a, emit_item = false)]", B), &["m::a", "m::b"]),
            (format!("{} {} trait S {{}}", A, "#[::l::announce::announce(emit_item = false, meta = m::b)]"), &["m::a", "m::b"]),
        ];
        for (item, metas) in cases {
            let (steps, left) = expand(item);
            let made = trampoline(&left);
            let positions: Vec<usize> = metas
                .iter()
                .map(|m| made.find(m).unwrap_or_else(|| panic!("{}: {}", m, made)))
                .collect();
            assert!(
                positions.windows(2).all(|w| w[0] < w[1]),
                "{}\n{}",
                item,
                made
            );
            assert!(!left.contains("compile_error"), "{}: {}", item, left);
            assert!(!left.contains("__announce_runner"), "{}: {}", item, left);
            // Every announcement expands once, and the runner once.
            assert!(steps <= 4 * metas.len() + 4, "{}: {}", item, steps);
        }
    }

    #[test]
    fn the_number_of_expansions_grows_with_the_announcements() {
        for n in 1..=8u32 {
            let (steps, left) = expand(&format!("#[dup({})] struct S;", n));
            trampoline(&left);
            // `dup` and its announcement for each, the last `dup(0)`, and the runner.
            assert_eq!(steps, 2 * n as usize + 2, "{}", n);
        }
    }

    #[test]
    fn an_announcement_after_the_runner_is_an_error_and_the_trampoline_is_made_once() {
        let (_, left) = expand(&format!("{} #[x] #[late(l)] struct S;", A));
        // The runner expanded first (the announcement is added after it): it reports
        // the late one and removes it.
        assert!(left.starts_with("::core::compile_error!"), "{}", left);
        assert_eq!(left.matches("compile_error").count(), 1, "{}", left);
        assert_eq!(
            left.matches("derive(::l::announce::__private::Capture)")
                .count(),
            1,
            "{}",
            left
        );
    }
}
