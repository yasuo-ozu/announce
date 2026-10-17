//! Several announcements of one item: the runner.
//!
//! Downstream attribute and derive macros emit `#[announce(..)]` or
//! `#[derive(Announce)]` on the items they produce. When two announcements apply to
//! one item they must make one trampoline and one `Announce` impl, not two (the second
//! would be E0252 and E0119). So the announcements are merged, and **one** attribute
//! makes the trampoline: the *runner*, which stands last among the attributes of the
//! item, and so expands after everything that is above it (rustc expands the attributes
//! of an item from the top).
//!
//! # The runner
//!
//! ```text
//! #[KRATE::__private::_actually_run_announce(
//!     _announce_format_version = "1",
//!     _announce_owned(my_lib, other),
//!     crate = KRATE, meta = a::b, my_lib::check
//! )]
//! ```
//!
//! It is an attribute macro of the `announce` crate that nobody writes. It is found by the
//! last segment of its path, [`RUNNER`], and by nothing else (never by the path
//! `announce`). Its arguments are:
//!
//! - `_announce_format_version = "1"`: the version of this encoding, [`RUNNER_FORMAT`].
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
//! - `_announce_owned(a, b)`: the names of the attributes that the checks of the
//!   announcements own: the answer of the query. Left out when there are none.
//! - the rest are the options of `#[announce(..)]` as [`Options`] prints them.
//!
//! # `#[announce(ARGS)]`
//!
//! 1. Before anything else (before its query), it **takes over** the bare
//!    `#[announce(..)]` attributes of the item whose arguments parse as options: it removes
//!    them, and merges their options into its own ([`Options::merge`], its own options
//!    first). They are the helper attributes of a `#[derive(Announce)]` that stood aside.
//!    A helper attribute does not survive the query: the query hands the item over to
//!    function-like macros, and what comes back is out of the scope of the derive's
//!    helpers (`cannot find attribute announce`). Taken over, their checks join the one
//!    query of this announcement. An attribute whose arguments do not parse as options
//!    is not ours and stays.
//! 2. If it has checks, it asks them which attributes they own (the query; it runs once
//!    for an announcement). See [`crate::finish`].
//! 3. An item that has no other outer attribute that could expand has nothing to wait for:
//!    the trampoline is generated at once, and no runner is put on the item. Documentation
//!    (`///` is the attribute `doc`), `#[repr(..)]` and the like cannot expand (see
//!    [`INERT`]: built-in attributes that a bare name cannot make a macro of), and nor can
//!    the attributes that the checks of this announcement own, which are stripped. This is
//!    the common case of a plain item, and it takes one expansion. A `#[derive(..)]`, a
//!    `#[cfg_attr(..)]` (which can expand to a macro), any other attribute, a bare
//!    `#[announce(..)]` or an attribute with a path (`#[x::doc]`) makes the runner.
//! 4. Otherwise the item comes back with the runner as its last attribute, or, if the
//!    item has one (the versions are checked first), with the options merged into it
//!    (the runner is the target: its entries come first; options that differ are one
//!    error with the item). Nothing else is emitted yet. The attributes that the checks
//!    own are moved to right after the runner, in their order, after those that are there
//!    already: rustc rejects an attribute that nothing knows above an attribute macro that
//!    has still to expand (`cannot find attribute`), and the runner is below everything
//!    else.
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
//!    or a `#[derive(Announce)]` that is left was put there by a macro *below* the runner
//!    (by appending it to its output, say): too late, it would make a second trampoline.
//!    It is an error, and the attribute is removed (the options of a late announcement
//!    are not merged). A renamed import (`#[ann]`) is not seen, and a late `#[ann]` makes
//!    a second trampoline (E0252). A bare `#[announce(..)]` is a helper attribute, and
//!    is taken over as below;
//! 3. takes over the bare `#[announce(..)]` that are left. If they bring checks that nobody
//!    has asked, those are asked (a query of mode `run`, with the answer that the runner
//!    carries in the state); the others are answered in `_announce_owned`;
//! 4. generates the trampoline and the `Announce` impl, with the merged options
//!    ([`crate::capture_output`]): the item without the attributes that the checks own
//!    (they stay in the payload; without the item at all, with `mock` or
//!    `emit_item = false`) and the hidden capture derive, which sees the item with every
//!    nested `#[cfg]` evaluated.
//!
//! # The derive
//!
//! A user-written `#[derive(Announce)]` expands before the attribute macros below it, and
//! cannot change the item. It emits nothing (and says to write `#[announce]`) when an
//! announcement stands below it: a runner, an attribute whose path is `x::announce`, or
//! another `#[derive(Announce)]`. That announcement takes its helper attributes over. The
//! path of the attribute is read there and nowhere else, so the derive stands aside for an
//! `#[x::announce]` of another crate (and then nothing announces the item), and it does not
//! see an `#[ann]` (renamed import) below it, which makes a second trampoline (E0252).
//!
//! # Termination
//!
//! Every expansion of `#[announce]` removes that attribute from the item, and either puts
//! a runner on the item (once: the next ones find it and merge into it), or generates at
//! once. A takeover removes attributes. The runner expands once (it never puts a runner
//! on the item, only merges into the last of several, and generates at last), and the
//! queries are chains of calls of the checks that hand over to the next element of a
//! list that gets shorter and end in `finish`. So there is no endless run. Macros of
//! others can still add announcements (each of them is one more `#[announce]`); they do
//! so by their own expansions, which the argument does not cover.

use crate::finish;
use crate::helper::OwnedAttrs;
use crate::options::{Check, MergeConflict, Options};
use crate::scan::{self, Outer, OuterAttr};
use proc_macro2::{Delimiter, Group, Ident, Span, TokenStream, TokenTree};
use template_quote::{quote, ToTokens};

/// The version of the encoding of the arguments of the runner. **Bump it whenever that
/// encoding changes** (the entries, or the way the options inside are written), so that
/// two versions of `announce` in one build refuse each other's runners with a clear error
/// instead of misreading them.
pub(crate) const RUNNER_FORMAT: &str = "1";

/// The name of the runner, the last segment of its path.
pub(crate) const RUNNER: &str = "_actually_run_announce";

/// The entry of the arguments that holds the version.
const VERSION_KEY: &str = "_announce_format_version";

/// The entry of the arguments that holds the names that checks own.
const OWNED_KEY: &str = "_announce_owned";

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

/// `_announce_format_version = "lit"`: the text of the literal, without quotes.
fn version_entry(entry: &[TokenTree]) -> Option<String> {
    match entry {
        [TokenTree::Ident(k), TokenTree::Punct(p), TokenTree::Literal(l)]
            if k == VERSION_KEY && p.as_char() == '=' =>
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
}

impl ToTokens for RunnerArgs {
    /// The arguments as they are written, with the one version entry.
    fn to_tokens(&self, tokens: &mut TokenStream) {
        let key = Ident::new(VERSION_KEY, Span::call_site());
        let mut parts = vec![quote! { #key = #RUNNER_FORMAT }];
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
        let mut rest: Vec<TokenStream> = Vec::new();
        for e in entries(args) {
            if version_entry(&e).is_some() {
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
        Ok(RunnerArgs { owned, opts })
    }

    /// Merges `folder` into this one (the target): the options, and the names.
    fn merge(&mut self, folder: &RunnerArgs, name: &Ident) -> Result<(), MergeConflict> {
        self.opts = self.opts.merge(&folder.opts, name)?;
        self.owned.extend(folder.owned.names().iter().cloned());
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

// ---- helper attributes ----------------------------------------------------------------

/// The arguments of an announcement attribute: `None` for the form `path = ..`.
fn options_of(a: &OuterAttr) -> Option<syn::Result<Options>> {
    if a.eq {
        return None;
    }
    Some(match &a.args {
        None => Ok(Options::default()),
        Some(g) => syn::parse2(g.stream()),
    })
}

/// Takes the bare `#[announce(..)]` attributes of the item over: they are removed, and
/// their options are merged into `opts` (the target: its entries come first). They are the
/// helper attributes of a derive. An attribute whose arguments do not parse as options is
/// left alone, as it is not ours. A conflict is one error, with the item as it was
/// (`Err`).
pub(crate) fn take_over(
    opts: Options,
    name: &Ident,
    item: TokenStream,
) -> Result<(Options, TokenStream), TokenStream> {
    if !scan::starts_with_attr(&item) {
        return Ok((opts, item));
    }
    let outer = scan::outer(&item);
    let mut current = opts;
    let mut removed: Vec<usize> = Vec::new();
    for (k, a) in outer
        .attrs
        .iter()
        .enumerate()
        .filter(|(_, a)| a.is_bare("announce"))
    {
        if let Some(Ok(helper)) = options_of(a) {
            current = current
                .merge(&helper, name)
                .map_err(|c| conflict(&c, &item))?;
            removed.push(k);
        }
    }
    if removed.is_empty() {
        return Ok((current, item));
    }
    Ok((
        current,
        assemble(&outer, &removed, &[], &OwnedAttrs::new(), Run::None),
    ))
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
/// Not in the list: `cfg` and `cfg_attr` (a `cfg_attr` can expand to a macro), `derive` (a
/// `#[derive(Announce)]` needs the runner, and so do its helpers), the bare `announce`, and
/// every attribute that is not built in.
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

/// An `#[announce]` that is emitted and has attributes of the item to wait for, with the
/// checks answered (`owned`: what they own; empty without checks): the item with the
/// runner, or with the options merged into the runner that is there. See the module docs.
pub(crate) fn announce(
    opts: &Options,
    name: &Ident,
    item: TokenStream,
    owned: &OwnedAttrs,
) -> TokenStream {
    let outer = scan::outer(&item);
    let runners = runners_of(&outer);
    // The attributes that the checks own, above the runner: below it.
    let above = runners.last().copied().unwrap_or(outer.attrs.len());
    let moved: Vec<usize> = (0..above)
        .filter(|&k| !runners.contains(&k) && owns(&outer.attrs[k], owned))
        .collect();
    if runners.is_empty() {
        let args = RunnerArgs {
            owned: owned.clone(),
            opts: opts.clone(),
        };
        let krate = opts.krate();
        let runner = Ident::new(RUNNER, Span::call_site());
        let attr = quote! { #[#krate::__private::#runner(#args)] };
        return assemble(&outer, &[], &moved, owned, Run::New(attr));
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
    };
    if let Err(c) = target.merge(&mine, name) {
        return conflict(&c, &item);
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
const LATE: &str = "announce: this item has an `#[announce]` or a `#[derive(Announce)]` that \
     expands after its announcements were run, so it makes a second trampoline: a macro of \
     the item has put it below the other attributes of its output (for example by appending it \
     to the attributes of the item). It is removed. Put it first among the attributes of the \
     output (`quote! { #[announce(..)] #item }`)";

/// An attribute that announces the item and is left below the runner.
fn is_late(a: &OuterAttr) -> bool {
    fn mentions(ts: TokenStream, word: &str) -> bool {
        ts.into_iter().any(|t| match t {
            TokenTree::Ident(i) => i == word,
            TokenTree::Group(g) => mentions(g.stream(), word),
            _ => false,
        })
    }
    match a.segments.last() {
        Some(l) if l == "announce" => a.leading_colon || a.segments.len() > 1,
        Some(l) if l == "derive" => a
            .args
            .as_ref()
            .map_or(false, |g| mentions(g.stream(), "Announce")),
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
        // No runner is left, so the others do not report the same error again.
        return error_with_item(
            message,
            &assemble(&outer, &others, &[], &OwnedAttrs::new(), Run::None),
        );
    }
    let head = match scan::head(&item) {
        Ok(h) => h,
        Err(e) => return error_with_item(e.to_string(), &item),
    };
    let mut mine = match RunnerArgs::read(args) {
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
    // The helper attributes that are left.
    let (opts, item) = match take_over(mine.opts.clone(), &head.ident, item.clone()) {
        Ok(x) => x,
        Err(out) => {
            errors.extend(out);
            return errors;
        }
    };
    // Only the checks that nobody has asked are asked.
    let text = |c: &Check| c.to_token_stream().to_string();
    let asked: Vec<String> = mine.opts.checks.iter().map(text).collect();
    let ask: Vec<Check> = opts
        .checks
        .iter()
        .filter(|c| !asked.contains(&text(c)))
        .cloned()
        .collect();
    mine.opts = opts;
    let random = crate::random_u64();
    let attr = mine.opts.to_token_stream();
    errors.extend(if ask.is_empty() {
        crate::capture_output(&mine.opts, &attr, item, random, &mine.owned)
    } else {
        finish::query(
            &mine.opts,
            finish::Mode::Run,
            &attr,
            &item,
            random,
            &ask,
            &mine.owned,
        )
    });
    errors
}

// ---- the derive ------------------------------------------------------------------------

/// Whether the attribute is a `#[derive(..)]` that lists `Announce`.
pub(crate) fn derives_announce(a: &syn::Attribute) -> bool {
    use syn::punctuated::Punctuated;
    a.path().is_ident("derive")
        && a.parse_args_with(Punctuated::<syn::Path, syn::Token![,]>::parse_terminated)
            .map_or(false, |paths| {
                paths
                    .iter()
                    .any(|p| p.segments.last().map_or(false, |s| s.ident == "Announce"))
            })
}

/// For `#[derive(Announce)]`: whether an announcement is left to expand below it.
///
/// A derive expands before the attribute macros below it, and cannot change the item, so
/// when the item has an announcement below it, the derive emits nothing: that announcement
/// makes the trampoline, with the options of the helper attribute that is left in the item
/// (it takes them over). An announcement is the runner (told by the last segment of its
/// path), an attribute whose path is `x::announce` or `::announce`, or another derive that
/// lists `Announce`. The path `announce` is read here and nowhere else (the runner is read
/// by its name), so a derive stands aside for an `#[x::announce]` of another crate below it
/// (and then nothing announces the item), and does not see `#[ann]`, a renamed import
/// (which then makes a second trampoline).
pub(crate) fn derive_defers(attrs: &[syn::Attribute]) -> bool {
    attrs.iter().any(|a| {
        let path = a.path();
        let last = match path.segments.last() {
            Some(l) => &l.ident,
            None => return false,
        };
        if last == RUNNER {
            return true;
        }
        if last == "announce" {
            return path.leading_colon.is_some() || path.segments.len() > 1;
        }
        derives_announce(a)
    })
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
        let n = scan::head(&ts(item)).unwrap().ident;
        norm(&announce(
            &opts(me),
            &n,
            ts(item),
            &OwnedAttrs::from_strs(owned),
        ))
    }

    /// The runner at the crate path `::l::announce` with these entries.
    fn runner(entries: &str) -> String {
        format!(
            "#[::l::announce::__private::_actually_run_announce({})]",
            entries
        )
    }

    const V: &str = "_announce_format_version = \"1\"";

    #[test]
    fn the_runner_is_put_last_with_the_version() {
        assert_eq!(
            made(
                "crate = ::l::announce, meta = a::x",
                &[],
                "#[b] #[c(1)] pub struct S(pub u8);"
            ),
            "#[b]#[c(1)]#[::l::announce::__private::_actually_run_announce(\
             _announce_format_version=\"1\",crate=::l::announce,meta=a::x)]pubstructS(pubu8);"
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
                out.starts_with("#[x]#[::l::announce::__private::_actually_run_announce(")
                    && out.contains("_announce_format_version=\"1\""),
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
            out.ends_with("_actually_run_announce(_announce_format_version=\"1\")]structS;"),
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
            "#[x]#[::l::announce::__private::_actually_run_announce(_announce_format_version=\"1\",\
             _announce_owned(my_lib,other),crate=::l::announce,a::check)]pubstructS;"
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
            "#[doc=\"a\"]#[::my_lib]#[derive(Debug)]\
             #[::l::announce::__private::_actually_run_announce(_announce_format_version=\"1\",\
             _announce_owned(my_lib,other),crate=::l::announce,a::check)]\
             #[my_lib(1)]#[other]#[my_lib::y=1]structS;"
        );
        // A `cfg_attr` that has an owned attribute among its attributes is owned as well,
        // and so is one nested in another; one that has not is not.
        assert_eq!(
            made(
                "crate = ::l::announce",
                &["my_lib"],
                "#[cfg_attr(a, my_lib(1))] #[cfg_attr(a, inline, cfg_attr(b, my_lib))] #[cfg_attr(a, inline)] #[cfg_attr(my_lib, inline)] struct S;"
            ),
            "#[cfg_attr(a,inline)]#[cfg_attr(my_lib,inline)]\
             #[::l::announce::__private::_actually_run_announce(_announce_format_version=\"1\",\
             _announce_owned(my_lib),crate=::l::announce)]\
             #[cfg_attr(a,my_lib(1))]#[cfg_attr(a,inline,cfg_attr(b,my_lib))]structS;"
        );
        // Merged into the runner: those that were there stay first, the new ones follow.
        assert_eq!(
            made(
                "crate = ::l::announce, b::check",
                &["other"],
                &format!(
                    "#[other(2)] #[x] {} #[my_lib(1)] #[y] struct S;",
                    runner(&format!("{}, _announce_owned(my_lib)", V))
                )
            ),
            "#[x]#[::l::announce::__private::_actually_run_announce(_announce_format_version=\"1\",\
             _announce_owned(my_lib,other),crate=::l::announce,b::check)]\
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
                    runner(&format!("{}, _announce_owned(my_lib), crate = ::l::announce, meta = a::x", V))
                )
            ),
            "#[x]#[::l::announce::__private::_actually_run_announce(_announce_format_version=\"1\",\
             _announce_owned(my_lib,other),crate=::l::announce,meta=a::x,meta=b::x,c::check)]#[y]pubstructS(pubu8);"
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
            // A bare `announce` is taken over, and nothing else is left.
            "#[announce(meta = a::x)] /// d\npub struct S;",
        ] {
            let out = attribute(me, item);
            assert!(
                out.contains("Capture") && !out.contains("_actually_run_announce"),
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
            // Not options of ours: it is not taken over, and it may expand.
            "#[announce(level = 2)] pub struct S;",
            "#[derive(Announce)] pub struct S;",
            "/// d\n#[repr(C)] #[x] pub trait S {}",
            // The names of the attributes that are not in the list, or that only look alike.
            "#[docs] pub struct S;",
            "#[reprs] pub struct S;",
        ] {
            let out = attribute(me, item);
            assert!(
                out.contains("_actually_run_announce") && !out.contains("Capture"),
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
        assert!(!out.contains("_actually_run_announce"), "{}", out);
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
        assert!(!out.contains("_actually_run_announce"), "{}", out);
        // Without the answer the attribute is an attribute that may expand: the runner,
        // and the attribute follows it.
        let out = norm(&crate::finish(state("#[my_lib(x)] pub struct S;", "")));
        assert!(out.contains("_actually_run_announce"), "{}", out);
        // Another attribute, next to the owned one: the runner, with the owned attribute
        // after it.
        let out = norm(&crate::finish(state(
            "#[my_lib(x)] #[derive(Debug)] pub struct S;",
            "attrs(my_lib)",
        )));
        assert!(
            out.starts_with("#[derive(Debug)]#[::l::announce::__private::_actually_run_announce(")
                && out.ends_with("#[my_lib(x)]pubstructS;"),
            "{}",
            out
        );
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
            out.matches("_announce_format_version").count(),
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
            runner("_announce_format_version = \"1\", ?? !!, _announce_format_version = \"2\"");
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
        let (a, r) = version_errors(V, &[runner("_announce_format_version = \"2\", ?? !!")]);
        assert_version_error(&a, &["`2`"]);
        assert_version_error(&r, &["`2`"]);
        // The runner that expands is the one with the other version.
        let r = norm(&run(
            ts("_announce_format_version = \"7\""),
            ts("#[x] struct S;"),
        ));
        assert_version_error(&r, &["`7`"]);
    }

    #[test]
    fn runners_of_different_versions_are_listed_with_their_crate_paths() {
        let a = "#[::one::announce::__private::_actually_run_announce(_announce_format_version = \"1\")]";
        let b = "#[::two::announce::__private::_actually_run_announce(_announce_format_version = \"2\", ?? !!)]";
        let c = "#[::three::_actually_run_announce(_announce_format_version = \"2\")]";
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
            ts("_announce_format_version = \"1\""),
            ts(&format!("#[x] {} struct S;", b)),
        ));
        assert_version_error(&r, &["`1`,`2`"]);
        // The same version at the same path is listed without the path.
        let item = format!(
            "#[x] {} {} struct S;",
            runner("_announce_format_version = \"3\""),
            runner("_announce_format_version = \"3\", _announce_format_version = \"4\"")
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
            "#[::l::announce::__private::_actually_run_announce]".to_string(),
            runner("_announce_format_version, meta = a::x"),
            runner("_announce_format_version(1)"),
            runner("x::_announce_format_version = \"1\""),
        ] {
            let item = format!("#[x] {} struct S;", r);
            let out = made("", &[], &item);
            assert_version_error(&out, &["`unknown`"]);
            // The item is still there; the runners are not, so that they do not report
            // the error again.
            assert!(out.ends_with("#[x]structS;"), "{}: {}", r, out);
        }
        // The runner that expands, too: its arguments are all there is.
        for args in ["", "meta = a::x", "_announce_format_version, meta = a::x"] {
            let out = norm(&run(ts(args), ts("#[x] struct S;")));
            assert_version_error(&out, &["`unknown`"]);
        }
        // Unknown next to the current one.
        let item = format!("#[x] {} {} struct S;", runner(V), runner("meta = a::x"));
        assert_version_error(&made("", &[], &item), &["`1`", "`unknown`"]);
    }

    #[test]
    fn several_runners_of_the_current_version_are_merged_into_the_last() {
        let first = "#[::one::announce::__private::_actually_run_announce(_announce_format_version = \"1\", _announce_owned(a), crate = ::one::announce, meta = one::x)]";
        let second = "#[::two::announce::__private::_actually_run_announce(_announce_format_version = \"1\", _announce_owned(b), crate = ::two::announce, meta = two::x)]";
        // From an announcement: the last is the target, the earlier is gone.
        let out = made(
            "meta = three::x",
            &["c"],
            &format!("#[x] {} #[y] {} struct S;", first, second),
        );
        assert_eq!(
            out,
            "#[x]#[y]#[::two::announce::__private::_actually_run_announce(_announce_format_version=\"1\",\
             _announce_owned(b,a,c),crate=::two::announce,meta=two::x,meta=one::x,meta=three::x)]structS;"
        );
        // From the runner that expands: it goes into the last one, which runs next.
        let out = norm(&run(
            ts("_announce_format_version = \"1\", crate = ::zero::announce, meta = zero::x"),
            ts(&format!("#[x] {} {} struct S;", first, second)),
        ));
        assert_eq!(
            out,
            "#[x]#[::two::announce::__private::_actually_run_announce(_announce_format_version=\"1\",\
             _announce_owned(b,a),crate=::two::announce,meta=two::x,meta=one::x,meta=zero::x)]structS;"
        );
        // Options that disagree are one error.
        let clash = "#[::two::announce::__private::_actually_run_announce(_announce_format_version = \"1\", macro_name = Other)]";
        let out = made("", &[], &format!("{} {} struct S;", first, clash));
        assert!(
            out.contains("`Other`") && out.starts_with("::core::compile_error!"),
            "{}",
            out
        );
    }

    // ---- helper attributes -----------------------------------------------------------

    fn taken(me: &str, item: &str) -> Result<(String, String), String> {
        let name = scan::head(&ts(item)).unwrap().ident;
        match take_over(opts(me), &name, ts(item)) {
            Ok((o, i)) => Ok((norm(&o.to_token_stream()), norm(&i))),
            Err(e) => Err(norm(&e)),
        }
    }

    #[test]
    fn a_bare_attribute_is_taken_over() {
        // Its entries come after those of the announcement; the attribute is gone.
        assert_eq!(
            taken(
                "meta = a::x",
                "#[announce(meta = b::x)] #[doc = \"d\"] #[announce] struct S;"
            )
            .unwrap(),
            (
                "meta=a::x,meta=b::x".to_string(),
                "#[doc=\"d\"]structS;".to_string()
            )
        );
        // One that is not ours stays, with no error: not the path of another crate, not a
        // name that contains it, not what does not parse as options.
        let item = "#[announce(level = 2)] #[x::announce(meta = b::x)] #[::announce] #[announced] #[announce = 1] struct S;";
        assert_eq!(
            taken("meta = a::x", item).unwrap(),
            ("meta=a::x".to_string(), norm(&ts(item)))
        );
        // Nothing to take: the item as it is.
        assert_eq!(
            taken("meta = a::x", "struct S;").unwrap(),
            ("meta=a::x".to_string(), "structS;".to_string())
        );
        // A conflict is one error and the item as it was.
        let item = "#[announce(macro_name = B)] struct S;";
        let e = taken("macro_name = A", item).unwrap_err();
        assert!(
            e.starts_with("::core::compile_error!") && e.contains("(`B`and`A`)"),
            "{}",
            e
        );
        assert!(e.ends_with(&norm(&ts(item))), "{}", e);
    }

    // ---- the runner expands -----------------------------------------------------------

    #[test]
    fn the_runner_makes_the_trampoline_without_the_attributes_that_checks_own() {
        let out = norm(&run(
            ts(&format!(
                "{}, _announce_owned(my_lib), crate = ::l::announce, a::check",
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
        assert!(!out.contains("_actually_run_announce"), "{}", out);
    }

    #[test]
    fn an_announcement_below_the_runner_is_an_error_and_is_removed() {
        for late in [
            "#[::announce::announce(meta = l::x)]",
            "#[x::announce]",
            "#[derive(Debug, ::announce::Announce)]",
            "#[derive(Announce)]",
        ] {
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
        // What is not an announcement stays: other derives, other attributes of the crate,
        // a bare `announce` (a helper, taken over), a renamed import (not seen).
        let out = norm(&run(
            ts(V),
            ts("#[derive(Debug)] #[x::announced] #[ann] #[announce(meta = h::x)] pub struct S;"),
        ));
        assert!(!out.contains("compile_error"), "{}", out);
        assert!(
            out.contains("#[derive(Debug)]") && out.contains("#[ann]"),
            "{}",
            out
        );
        assert!(out.contains("meta=h::x"), "{}", out);
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
    /// function; a bare `#[announce(..)]` is the helper of a derive, and is inert), the
    /// runner, `#[derive(..)]` (the real derive if it lists `Announce`, whose output is
    /// collected; the derive of the capture is inert, the others are removed), `emit(x)`
    /// (announces what it produces: the attribute in front of the item), `pass` (returns
    /// its input), `late(x)` (adds the announcement *below* all the attributes) and
    /// `dup(n)` (announces and leaves `dup(n-1)` to expand next).
    ///
    /// Returns the outputs of the derives, the number of expansions, and the item that is
    /// left.
    fn expand(item: &str) -> (Vec<String>, usize, String) {
        let mut item = ts(item);
        let (mut outputs, mut steps) = (Vec::new(), 0);
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
                    "emit" | "pass" | "late" | "dup" | "_actually_run_announce" => true,
                    _ => false,
                }
            });
            let k = match next {
                Some(k) => k,
                None => return (outputs, steps, norm(&item)),
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
                "_actually_run_announce" => crate::run_announce(args, rest),
                "derive" => {
                    if norm(&args).contains("Announce") {
                        outputs.push(norm(&crate::derive_entry(rest.clone())));
                    }
                    rest
                }
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

    /// The trampoline, from the outputs of the derives and the item that is left: exactly
    /// one, and the metas of it.
    fn trampoline(outputs: &[String], left: &str) -> String {
        let all: Vec<&String> = outputs
            .iter()
            .chain(
                std::iter::once(&left.to_string())
                    .map(|_| &outputs[0])
                    .take(0),
            )
            .collect();
        let _ = all;
        let mut made = outputs
            .iter()
            .filter(|o| o.contains("macro_rules!"))
            .count();
        // The capture dummy is the trampoline of an attribute: the derive runs later.
        made += left
            .matches("derive(::l::announce::__private::Capture)")
            .count();
        assert_eq!(made, 1, "{:?} {}", outputs, left);
        outputs
            .iter()
            .find(|o| o.contains("macro_rules!"))
            .cloned()
            .unwrap_or_else(|| left.to_string())
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
            // the derive that the user wrote, above and below the attribute
            (format!("#[derive(::l::announce::Announce)] #[announce(meta = m::h)] {} struct S;", A), &["m::a", "m::h"]),
            (format!("{} #[derive(::l::announce::Announce)] struct S;", A), &["m::a"]),
            (
                format!("{} #[derive(::l::announce::Announce)] #[announce(meta = m::h)] {} struct S;", A, B),
                &["m::a", "m::h", "m::b"],
            ),
            // a bare `announce` is taken over by the attribute above it
            (format!("{} #[announce(meta = m::b)] #[announce(meta = m::c)] struct S;", A), &["m::a", "m::b", "m::c"]),
            // not emitted: through the runner as well
            (format!("{} {} struct S;", "#[::l::announce::announce(crate = ::l::announce, meta = m::a, emit_item = false)]", B), &["m::a", "m::b"]),
            (format!("{} {} struct S;", A, "#[::l::announce::announce(emit_item = false, meta = m::b)]"), &["m::a", "m::b"]),
        ];
        for (item, metas) in cases {
            let (outputs, steps, left) = expand(item);
            let made = trampoline(&outputs, &left);
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
            assert!(
                !left.contains("_actually_run_announce"),
                "{}: {}",
                item,
                left
            );
            // Every announcement expands once, and the runner once.
            assert!(steps <= 4 * metas.len() + 4, "{}: {}", item, steps);
        }
    }

    #[test]
    fn the_number_of_expansions_grows_with_the_announcements() {
        for n in 1..=8u32 {
            let (outputs, steps, left) = expand(&format!("#[dup({})] struct S;", n));
            trampoline(&outputs, &left);
            // `dup` and its announcement for each, the last `dup(0)`, and the runner.
            assert_eq!(steps, 2 * n as usize + 2, "{}", n);
        }
    }

    #[test]
    fn an_announcement_after_the_runner_is_an_error_and_the_trampoline_is_made_once() {
        let (outputs, _, left) = expand(&format!("{} #[x] #[late(l)] struct S;", A));
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
        assert!(outputs.is_empty());
    }
}
