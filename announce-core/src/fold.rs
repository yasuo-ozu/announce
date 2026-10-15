//! Several announcements of one item.
//!
//! Downstream attribute and derive macros emit `#[announce(..)]` or
//! `#[derive(Announce)]` on the items they produce. When two announcements apply to
//! one item they must make one trampoline and one `Announce` impl, not two (the second
//! would be E0252 and E0119). The announcements are merged, and an announcement tells
//! another one by a **flag**, never by its path.
//!
//! # The flag
//!
//! The internal option `__announce_deferred` marks an announcement that has *moved*:
//! it stands last among the outer attributes of the item (below every other macro), so
//! that everything above it has expanded or is inert, and every announcement that
//! those macros could add has been added. It is written bare, or with the answer of the
//! query of the checks, `__announce_deferred(attrs(my_lib, ..))` (see Checks). Nothing
//! else tells an announcement from another attribute, so other crates' attributes that
//! are named `announce` (`#[metrics::announce(level = 2)]`) are never touched, and a
//! renamed import (`use announce::announce as ann; #[ann]`) is an announcement like
//! any other.
//!
//! # Moving ([`defer`])
//!
//! An announcement *without* the flag, which is expanding for the first time, re-emits
//! the item and nothing else, with itself moved to be the last outer attribute and
//! flagged: `#[a] #[announce] #[b] item` becomes `#[a] #[b] #[announce(.., flag)]
//! item`. `#[b]` expands before it, and if `#[b]` emits an announcement of its own, that
//! one moves behind `#[b]`'s output too, and then folds. This happens always (the
//! macros below are not known), except:
//!
//! - when the item has no outer attribute: there is nothing to wait for, and the
//!   announcement is made at once;
//! - with `emit_item = false` or `mock`: the item is not emitted, so the macros below
//!   must not see it (a derive below would implement a trait for an item that the
//!   announcement then removes). The flag is set but the announcement stays *in place*:
//!   it is re-emitted as the first attribute, and the attributes above it were inert
//!   already. The announcements below it are not seen then: they would make a second
//!   trampoline, so put the other announcements of such an item above it (they move
//!   below it and fold).
//!
//! # Folding ([`fold`])
//!
//! An announcement *with* the flag is the last macro to expand. It makes the trampoline,
//! unless it merges into another one:
//!
//! 1. every bare `#[announce(..)]` among the other attributes is taken over: its
//!    options come first and the attribute is removed. Everything above a flagged
//!    announcement has expanded already or is inert, so a bare `#[announce(..)]` that
//!    is left can only be a helper attribute of a `#[derive(Announce)]`, which stood
//!    aside ([`derive_defers`]). An attribute whose arguments do not parse as options is
//!    left alone, as it is not ours;
//! 2. if another attribute below it carries the flag (any path), the options are merged
//!    into it, and this attribute is gone ([`Options::merge`], the target's entries
//!    first). It expands next, and folds or makes the trampoline.
//! 3. otherwise the trampoline is made.
//!
//! One item has one trampoline, so the announcements must agree on its name: the
//! `macro_name`, else the name of the item. Different names are one error at the
//! announcement that is expanding, with the item as it was ([`Options::merge`]).
//!
//! # Checks
//!
//! A check can own attributes of the item (`#[announce(check)] #[my_lib] struct S`).
//! Rustc rejects such an attribute above `#[announce]`, so after the move it must be
//! *below* the announcement. The names are known only after the query, so an
//! announcement with checks sends the query first, as it has always done, and moves in
//! `finish` ([`defer`]), with the owned attributes after it, in their order. The answer
//! travels in the flag, `__announce_deferred(attrs(my_lib))`: the flagged announcement
//! does not send the query again, **the query runs once for an announcement**. A fold
//! unites the answers of both sides, and moves the owned attributes that were above the
//! target below it. Only an announcement that is merged with checks that were not asked
//! (a helper attribute with checks of its own, [`pending_query`]) asks again, and asks
//! those checks only.
//!
//! # The derive
//!
//! A derive expands before the attribute macros below it, and cannot change the item.
//! `#[derive(Announce)]` emits nothing when an announcement stands below it, and
//! leaves the trampoline to that attribute, which takes its helper over
//! ([`derive_defers`]). It is the only place where an announcement is told by its path
//! (`x::announce`, or another derive that lists `Announce`) as well as by the flag,
//! because `#[derive(Announce)] #[x::announce]` shows the derive the attribute before it
//! has moved. The risk is a derive that stands aside for an `#[x::announce]` of
//! another crate below it: nothing announces the item then. A derive above `#[announce]`
//! works because `#[announce]` moves below the derive and the derive sees the flag.
//!
//! # Termination
//!
//! Let `P` be the number of announce attribute invocations on the item, and `U` the
//! number of them without the flag. Every expansion of an announce attribute is one of:
//!
//! 1. an unflagged one that moves (or stays in place): it emits itself once more, with
//!    the flag. `P` is the same, `U` is lower by one;
//! 2. an unflagged one that has nothing to wait for: it emits the trampoline and no
//!    announce attribute. `P` is lower by one;
//! 3. a flagged one that folds: it emits the item with *one announce attribute fewer*
//!    (itself; the target keeps its flag). `P` is lower by one, `U` is the same;
//! 4. a flagged one that emits the trampoline. `P` is lower by one.
//!
//! The query of the checks is part of the expansion that sends it: a chain of calls of
//! the checks, each handing over to the next element of a list that gets shorter, that
//! ends in `finish`, which does one of the above (1 or 2 for an unflagged
//! announcement, 4 for a flagged one that asks for the checks it has not asked yet).
//! So `(P, U)` falls in the lexicographic order at every expansion of an announce
//! attribute, and there can be no endless run of them. An announcement never
//! re-emits itself without the flag, and one with the flag never moves again. Macros of
//! others can still add announcements (each of them is one more `P`); they do so by
//! their own expansions, which the argument does not cover.
//!
//! A derive never folds and never re-emits anything: it emits the trampoline or nothing.

use crate::helper::OwnedAttrs;
use crate::options::{Check, MergeConflict, Options};
use crate::scan::{self, Outer, OuterAttr};
use proc_macro2::{Delimiter, Group, Ident, Span, TokenStream, TokenTree};
use template_quote::{quote, ToTokens};

/// What [`fold`] found.
pub(crate) enum Folded {
    /// This announcement is done: the tokens replace the item (the other announcement
    /// has the options of this one, or an error comes with the item as it was).
    Emit(TokenStream),
    /// This announcement goes on, with the options of the bare `#[announce(..)]`
    /// attributes merged in and those attributes taken off the item.
    Go {
        opts: Box<Options>,
        item: TokenStream,
    },
}

/// The options of an announcement attribute: `None` for the form `path = ..`.
fn options_of(a: &OuterAttr) -> Option<syn::Result<Options>> {
    if a.eq {
        return None;
    }
    Some(match &a.args {
        None => Ok(Options::default()),
        Some(g) => syn::parse2(g.stream()),
    })
}

/// An error at the announcement that is expanding, and the item as it was.
fn error_with_item(message: String, item: &TokenStream) -> TokenStream {
    let mut out = syn::Error::new(Span::call_site(), message).to_compile_error();
    out.extend(item.clone());
    out
}

fn conflict(c: &MergeConflict, item: &TokenStream) -> Folded {
    Folded::Emit(error_with_item(c.message(), item))
}

/// The item with the attributes `removed` gone. With `replace` (the index of the
/// target of a fold and the tokens inside its brackets), the target gets them, and the
/// attributes that `owned` names move right after it, in their order: they must stand
/// below the announcement that is left.
fn rebuild(
    outer: &Outer,
    removed: &[usize],
    replace: Option<(usize, TokenStream)>,
    owned: &OwnedAttrs,
) -> TokenStream {
    let target = replace.as_ref().map(|r| r.0);
    let pair = |a: &OuterAttr| [outer.tts[a.at].clone(), outer.tts[a.at + 1].clone()];
    // Moves below the target.
    let moves =
        |k: usize, a: &OuterAttr| target.is_some() && Some(k) != target && a.is_owned_by(owned);
    let mut out = TokenStream::new();
    for (k, a) in outer.attrs.iter().enumerate() {
        if removed.contains(&k) || moves(k, a) {
            continue;
        }
        match &replace {
            Some((r, inner)) if *r == k => {
                let old = match &outer.tts[a.at + 1] {
                    TokenTree::Group(g) => g.span(),
                    _ => Span::call_site(),
                };
                let mut group = Group::new(Delimiter::Bracket, inner.clone());
                group.set_span(old);
                out.extend([outer.tts[a.at].clone(), TokenTree::Group(group)]);
                for (j, b) in outer.attrs.iter().enumerate() {
                    if !removed.contains(&j) && moves(j, b) {
                        out.extend(pair(b));
                    }
                }
            }
            _ => out.extend(pair(a)),
        }
    }
    out.extend(outer.tts[outer.attrs_end()..].iter().cloned());
    out
}

/// The announcement that is expanding has the flag (`me`, on the item `item` named
/// `name`): merges it into another announcement of the item, if there is one, and takes
/// over the bare `#[announce(..)]` attributes that are left. See the module docs.
///
/// The announcements must agree on the trampoline name: `macro_name`, else `name`
/// ([`Options::merge`]).
pub(crate) fn fold(me: &Options, name: &Ident, item: TokenStream) -> Folded {
    let go = |opts: Options, item: TokenStream| Folded::Go {
        opts: Box::new(opts),
        item,
    };
    if !scan::starts_with_attr(&item) {
        return go(me.clone(), item);
    }
    let outer = scan::outer(&item);
    let mut current = me.clone();
    let mut removed: Vec<usize> = Vec::new();

    // 1. A bare `#[announce(..)]` is the helper attribute of a derive: take it over.
    // Its entries come first.
    for (k, a) in outer
        .attrs
        .iter()
        .enumerate()
        .filter(|(_, a)| a.is_bare("announce"))
    {
        if let Some(Ok(helper)) = options_of(a) {
            match helper.merge(&current, name) {
                Ok(merged) => {
                    current = merged;
                    removed.push(k);
                }
                Err(c) => return conflict(&c, &item),
            }
        }
    }

    // 2. The first attribute below that has the flag (any path) is the target.
    for (k, a) in outer.attrs.iter().enumerate() {
        if removed.contains(&k) || !a.carries_flag() {
            continue;
        }
        let target = match options_of(a) {
            Some(Ok(t)) => t,
            // Not an announcement of ours, or its own expansion reports it.
            _ => continue,
        };
        return match target.merge(&current, name) {
            Ok(merged) => {
                let inner = a.with_args(merged.to_token_stream());
                // Both sides' owned attributes stand below the target.
                let mut owned = me.owned.clone().unwrap_or_default();
                owned.extend(target.owned.iter().flat_map(|o| o.names().iter().cloned()));
                Folded::Emit(rebuild(&outer, &removed, Some((k, inner)), &owned))
            }
            Err(c) => conflict(&c, &item),
        };
    }

    // 3. The trampoline.
    if removed.is_empty() {
        return go(current, item);
    }
    let rest = rebuild(&outer, &removed, None, &OwnedAttrs::new());
    go(current, rest)
}

/// The checks that a flagged announcement still has to ask after [`fold`] has taken
/// helper attributes over: the checks of `after` that `before` (the announcement as it
/// was expanded) had not asked, with the names that `before` knew. `None`: nothing is
/// left to ask, and `after.answer()` is the names the checks own.
pub(crate) fn pending_query(before: &Options, after: &Options) -> Option<(Vec<Check>, OwnedAttrs)> {
    if after.answer().is_some() {
        return None;
    }
    let text = |c: &Check| c.to_token_stream().to_string();
    match before.answer() {
        // `before` was asked about its checks: only the new ones are left.
        Some(seed) => {
            let asked: Vec<String> = before.checks.iter().map(text).collect();
            let ask = after
                .checks
                .iter()
                .filter(|c| !asked.contains(&text(c)))
                .cloned()
                .collect();
            Some((ask, seed))
        }
        None => Some((after.checks.clone(), OwnedAttrs::new())),
    }
}

/// An announcement without the flag, which is expanding for the first time, with the
/// checks answered (`owned` is what they own: empty without checks): the item with the
/// announcement moved to be the last outer attribute, flagged, and the attributes that
/// the checks own after it. `None`: the announcement can be made now (the item has no
/// outer attribute, or it is the flagged announcement itself). See the module docs.
///
/// `attr` is the arguments of the attribute as it was written.
pub(crate) fn defer(
    opts: &Options,
    attr: &TokenStream,
    item: &TokenStream,
    owned: &OwnedAttrs,
) -> Option<TokenStream> {
    if opts.deferred || !scan::starts_with_attr(item) {
        return None;
    }
    let krate = opts.krate();
    let mut args = attr.clone();
    let comma =
        matches!(args.clone().into_iter().last(), Some(TokenTree::Punct(p)) if p.as_char() == ',');
    if !args.is_empty() && !comma {
        args.extend(quote! { , });
    }
    args.extend(crate::options::flag_tokens(
        Some(owned),
        !opts.checks.is_empty(),
    ));
    let announcement = quote! { #[#krate::announce(#args)] };
    // Not emitted: the macros below must not see the item. Stay in place, which is the
    // front: what stands above was inert already.
    if !opts.emit_item || opts.mock.is_some() {
        let mut out = announcement;
        out.extend(item.clone());
        return Some(out);
    }
    let outer = scan::outer(item);
    let mut out = TokenStream::new();
    let mut owned_attrs = TokenStream::new();
    for a in &outer.attrs {
        let tokens = [outer.tts[a.at].clone(), outer.tts[a.at + 1].clone()];
        if a.is_owned_by(owned) {
            owned_attrs.extend(tokens);
        } else {
            out.extend(tokens);
        }
    }
    out.extend(announcement);
    out.extend(owned_attrs);
    out.extend(outer.tts[outer.attrs_end()..].iter().cloned());
    Some(out)
}

/// For `#[derive(Announce)]`: whether an announcement is left to expand below it.
///
/// A derive expands before the attribute macros below it, which then do not see its
/// attribute. It cannot change the item, so when the item has an announcement below it,
/// the derive emits nothing: that announcement makes the trampoline, with the options of
/// the helper attribute that is left in the item. An announcement is an attribute that
/// carries the flag (an `#[announce]` that has moved), or, because
/// `#[derive(Announce)] #[x::announce]` shows the derive the attribute before it has
/// moved, an attribute whose path is `x::announce` or `::announce`, or another derive that
/// lists `Announce`. The path is not matched anywhere else. So a derive stands aside for
/// an `#[x::announce]` of another crate below it (and then nothing announces the item).
pub(crate) fn derive_defers(attrs: &[syn::Attribute]) -> bool {
    use syn::punctuated::Punctuated;
    attrs.iter().any(|a| {
        if let syn::Meta::List(l) = &a.meta {
            if scan::tokens_carry_flag(l.tokens.clone()) {
                return true;
            }
        }
        let path = a.path();
        let last = match path.segments.last() {
            Some(l) => &l.ident,
            None => return false,
        };
        if last == "announce" {
            return path.leading_colon.is_some() || path.segments.len() > 1;
        }
        last == "derive"
            && a.parse_args_with(Punctuated::<syn::Path, syn::Token![,]>::parse_terminated)
                .map_or(false, |paths| {
                    paths
                        .iter()
                        .any(|p| p.segments.last().map_or(false, |s| s.ident == "Announce"))
                })
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scan::outer;

    fn ts(s: &str) -> TokenStream {
        s.parse().unwrap()
    }

    fn norm(t: &TokenStream) -> String {
        t.to_string().split_whitespace().collect()
    }

    fn opts(s: &str) -> Options {
        syn::parse2(ts(s)).unwrap()
    }

    /// `fold` for `me` on `item`, printed: `Emit(..)` or `Go(options;item)`.
    fn folded(me: &str, item: &str) -> String {
        let name = scan::head(&ts(item)).unwrap().ident;
        match fold(&opts(me), &name, ts(item)) {
            Folded::Emit(t) => format!("Emit({})", norm(&t)),
            Folded::Go { opts, item } => {
                format!("Go({};{})", norm(&opts.to_token_stream()), norm(&item))
            }
        }
    }

    const FLAG: &str = "__announce_deferred";

    #[test]
    fn nothing_to_merge_into_is_nothing_to_fold() {
        for item in [
            "pub struct S;",
            "#[derive(Debug)] #[doc = \"x\"] #[repr(C)] pub struct S;",
            // Not an announcement: another path, or a name that only contains it.
            "#[x::announced] #[announce_not] #[derive(Announced)] pub struct S;",
            "#[cfg_attr(a, announce(meta = m::x))] pub struct S;",
            // An attribute that is called `announce`, and has no flag, is not ours: it
            // is not touched, whatever its path.
            "#[metrics::announce(level = 2)] pub struct S;",
            "#[::x::y::announce] #[x::announce(meta = b::x)] pub struct S;",
            "#[announce(level = 2)] pub struct S;",
            // The flag is a whole entry of the arguments, and nothing else.
            "#[x::y(not_a_flag)] pub struct S;",
            "#[x::y(meta = __announce_deferred)] pub struct S;",
            "#[x::y(__announce_deferred = 1)] pub struct S;",
            "#[x::y(a::__announce_deferred)] pub struct S;",
            // The form `path = ..` has no arguments to merge into.
            "#[x::announce = 1] pub struct S;",
        ] {
            let want = format!("Go(__announce_deferred,meta=a::x;{})", norm(&ts(item)));
            assert_eq!(
                folded(&format!("meta = a::x, {}", FLAG), item),
                want,
                "{}",
                item
            );
        }
    }

    #[test]
    fn a_flagged_attribute_receives_the_options_whatever_its_path() {
        let me = format!("meta = a::x, {}", FLAG);
        for path in ["x::announce", "::x::y::announce", "other", "a::b"] {
            let item = format!("#[{}(meta = b::x, {})] pub struct S;", path, FLAG);
            let out = folded(&me, &item);
            assert_eq!(
                out,
                format!(
                    "Emit(#[{}(__announce_deferred,meta=b::x,meta=a::x)]pubstructS;)",
                    path
                ),
                "{}",
                path
            );
        }
        // The flag alone, and next to other entries.
        assert_eq!(
            folded(
                &me,
                "#[x::announce(__announce_deferred)] #[doc = \"d\"] pub struct S;"
            ),
            "Emit(#[x::announce(__announce_deferred,meta=a::x)]#[doc=\"d\"]pubstructS;)"
        );
        // Nothing to add: the attribute stays as it is.
        assert_eq!(
            folded(
                FLAG,
                "#[x::announce(meta = b::x, __announce_deferred)] struct S;"
            ),
            "Emit(#[x::announce(__announce_deferred,meta=b::x)]structS;)"
        );
    }

    #[test]
    fn the_first_flagged_attribute_is_the_one() {
        let me = format!("meta = a::x, {}", FLAG);
        assert_eq!(
            folded(
                &me,
                "#[derive(Debug)] #[y::announce(meta = y::x)] #[y2(meta = y::x, __announce_deferred)] #[z::announce(meta = z::x, __announce_deferred)] struct S;"
            ),
            "Emit(#[derive(Debug)]#[y::announce(meta=y::x)]#[y2(__announce_deferred,meta=y::x,meta=a::x)]#[z::announce(meta=z::x,__announce_deferred)]structS;)"
        );
    }

    #[test]
    fn an_attribute_that_does_not_parse_is_not_a_target() {
        let me = format!("meta = a::x, {}", FLAG);
        // The flag is there but the attribute is not ours: the next one is the target.
        assert_eq!(
            folded(
                &me,
                "#[x::y(nope = 1, __announce_deferred)] #[z::announce(__announce_deferred)] struct S;"
            ),
            "Emit(#[x::y(nope=1,__announce_deferred)]#[z::announce(__announce_deferred,meta=a::x)]structS;)"
        );
        assert!(folded(&me, "#[x::y(nope = 1, __announce_deferred)] struct S;").starts_with("Go("));
    }

    #[test]
    fn the_merged_attribute_keeps_the_crate_of_the_target() {
        assert_eq!(
            folded(
                "crate = ::me::announce, __announce_deferred",
                "#[x::announce(crate = ::other::announce, __announce_deferred)] struct S;"
            ),
            "Emit(#[x::announce(crate=::other::announce,__announce_deferred)]structS;)"
        );
        assert_eq!(
            folded(
                "crate = ::me::announce, __announce_deferred",
                "#[x::announce(__announce_deferred)] struct S;"
            ),
            "Emit(#[x::announce(crate=::me::announce,__announce_deferred)]structS;)"
        );
    }

    #[test]
    fn the_answers_of_both_sides_are_united() {
        // Neither has checks: the flag stays bare.
        assert_eq!(
            folded(FLAG, "#[x::announce(__announce_deferred)] struct S;"),
            "Emit(#[x::announce(__announce_deferred)]structS;)"
        );
        // Both asked their checks: the names are united, the target's first.
        assert_eq!(
            folded(
                "b::check, __announce_deferred(attrs(mine, shared))",
                "#[x::announce(a::check, __announce_deferred(attrs(theirs, shared)))] struct S;"
            ),
            "Emit(#[x::announce(__announce_deferred(attrs(theirs,shared,mine)),a::check,b::check)]structS;)"
        );
        // One side has no checks to ask: its answer is empty.
        assert_eq!(
            folded(
                "b::check, __announce_deferred(attrs(mine))",
                "#[x::announce(__announce_deferred)] struct S;"
            ),
            "Emit(#[x::announce(__announce_deferred(attrs(mine)),b::check)]structS;)"
        );
        assert_eq!(
            folded(
                FLAG,
                "#[x::announce(a::check, __announce_deferred(attrs()))] struct S;"
            ),
            "Emit(#[x::announce(__announce_deferred(attrs()),a::check)]structS;)"
        );
        // One side has checks that were not asked: the merged one is not answered.
        assert_eq!(
            folded(
                "b::check, __announce_deferred",
                "#[x::announce(a::check, __announce_deferred(attrs(theirs)))] struct S;"
            ),
            "Emit(#[x::announce(__announce_deferred,a::check,b::check)]structS;)"
        );
    }

    #[test]
    fn the_attributes_that_checks_own_move_below_the_target() {
        // Rustc rejects an attribute that nothing knows above the announcement, so the
        // ones of the announcement that goes (`mine`) and of the target (`theirs`) all
        // stand right after the target.
        let me = "c::check, __announce_deferred(attrs(mine))";
        let target = "#[x::announce(d::check, __announce_deferred(attrs(theirs)))]";
        assert_eq!(
            folded(
                me,
                &format!(
                    "#[mine(1)] #[doc = \"a\"] #[theirs] {} #[doc = \"b\"] #[mine(2)] #[theirs(3)] struct S;",
                    target
                )
            ),
            "Emit(#[doc=\"a\"]#[x::announce(__announce_deferred(attrs(theirs,mine)),d::check,c::check)]\
             #[mine(1)]#[theirs]#[mine(2)]#[theirs(3)]#[doc=\"b\"]structS;)"
        );
        // Without owned names nothing moves.
        assert_eq!(
            folded(
                FLAG,
                "#[mine(1)] #[x::announce(__announce_deferred)] #[mine(2)] struct S;"
            ),
            "Emit(#[mine(1)]#[x::announce(__announce_deferred)]#[mine(2)]structS;)"
        );
    }

    #[test]
    fn a_bare_attribute_is_taken_over() {
        let me = format!("meta = a::x, {}", FLAG);
        // The bare `#[announce(..)]` is the helper of a derive: its entries come first.
        assert_eq!(
            folded(&me, "#[announce(meta = b::x)] #[doc = \"d\"] struct S;"),
            "Go(__announce_deferred,meta=b::x,meta=a::x;#[doc=\"d\"]structS;)"
        );
        assert_eq!(
            folded(&me, "#[announce] #[announce(meta = c::x)] struct S;"),
            "Go(__announce_deferred,meta=c::x,meta=a::x;structS;)"
        );
        // And then merged into a flagged attribute below, helpers' entries and all.
        assert_eq!(
            folded(
                &me,
                "#[announce(meta = b::x)] #[x::announce(__announce_deferred)] struct S;"
            ),
            "Emit(#[x::announce(__announce_deferred,meta=b::x,meta=a::x)]structS;)"
        );
        // An attribute that does not parse as options is not ours: it stays, with no
        // error.
        assert_eq!(
            folded(&me, "#[announce(nope = 1)] struct S;"),
            "Go(__announce_deferred,meta=a::x;#[announce(nope=1)]structS;)"
        );
        // Only the bare path: not `x::announce`, not `::announce`, not `announced`.
        assert_eq!(
            folded(&me, "#[x::announce(meta = b::x)] #[::announce] #[announced] struct S;"),
            "Go(__announce_deferred,meta=a::x;#[x::announce(meta=b::x)]#[::announce]#[announced]structS;)"
        );
    }

    #[test]
    fn a_conflict_is_one_error_and_the_item_as_it_was() {
        const NAMES: &str = "announcedtwicewithdifferenttrampolinenames";
        for (me, item, needle) in [
            (
                "macro_name = A",
                "#[x::announce(macro_name = B, __announce_deferred)] struct S;",
                format!("{}(`A`and`B`)", NAMES),
            ),
            (
                "layout = single",
                "#[x::announce(layout = direct, __announce_deferred)] struct S;",
                "`layout=".to_string(),
            ),
            (
                "macro_name = A",
                "#[announce(macro_name = B)] struct S;",
                format!("{}(`A`and`B`)", NAMES),
            ),
            // A name that is set and one that is not: the item's name is the other.
            (
                "",
                "#[x::announce(macro_name = ParseAlt, __announce_deferred)] pub trait Parse {}",
                format!("{}(`Parse`and`ParseAlt`)", NAMES),
            ),
            (
                "macro_name = ParseAlt",
                "#[x::announce(__announce_deferred)] pub trait Parse {}",
                format!("{}(`ParseAlt`and`Parse`)", NAMES),
            ),
            (
                "",
                "#[announce(macro_name = ParseAlt)] pub trait Parse {}",
                format!("{}(`Parse`and`ParseAlt`)", NAMES),
            ),
            (
                "macro_name = ParseAlt",
                "#[announce] pub trait Parse {}",
                format!("{}(`ParseAlt`and`Parse`)", NAMES),
            ),
        ] {
            let me = if me.is_empty() {
                FLAG.to_string()
            } else {
                format!("{}, {}", me, FLAG)
            };
            let out = folded(&me, item);
            assert!(out.starts_with("Emit(::core::compile_error!"), "{}", out);
            assert!(out.contains(&needle), "{}\n{}", needle, out);
            assert_eq!(out.matches("compile_error").count(), 1, "{}", out);
            // The item is there as it was, with the attribute that stays.
            assert!(out.ends_with(&format!("{})", norm(&ts(item)))), "{}", out);
        }
    }

    #[test]
    fn equal_trampoline_names_merge() {
        // The item's name is the default name: `macro_name = S` on `S` is the same.
        let me = |o: &str| {
            if o.is_empty() {
                FLAG.to_string()
            } else {
                format!("{}, {}", o, FLAG)
            }
        };
        assert_eq!(
            folded(
                &me("macro_name = S"),
                "#[x::announce(__announce_deferred)] struct S;"
            ),
            "Emit(#[x::announce(macro_name=S,__announce_deferred)]structS;)"
        );
        assert_eq!(
            folded(
                &me(""),
                "#[x::announce(macro_name = S, __announce_deferred)] struct S;"
            ),
            "Emit(#[x::announce(macro_name=S,__announce_deferred)]structS;)"
        );
        // The explicit name stays in whichever announcement it was.
        assert_eq!(
            folded(&me("macro_name = S"), "#[announce] struct S;"),
            "Go(macro_name=S,__announce_deferred;structS;)"
        );
        assert_eq!(
            folded(
                &me(""),
                "#[announce(macro_name = S)] #[doc = \"d\"] struct S;"
            ),
            "Go(macro_name=S,__announce_deferred;#[doc=\"d\"]structS;)"
        );
    }

    #[test]
    fn groups_without_delimiters_are_read_in_place() {
        use proc_macro2::{Delimiter, Group};
        let item: TokenStream = [TokenTree::Group(Group::new(
            Delimiter::None,
            ts("#[x::announce(meta = b::x, __announce_deferred)] pub struct S;"),
        ))]
        .into_iter()
        .collect();
        let name = scan::head(&item).unwrap().ident;
        match fold(&opts("meta = a::x, __announce_deferred"), &name, item) {
            Folded::Emit(out) => assert_eq!(
                norm(&out),
                "#[x::announce(__announce_deferred,meta=b::x,meta=a::x)]pubstructS;"
            ),
            _ => panic!("not folded"),
        }
    }

    #[test]
    fn what_is_left_to_ask() {
        let ask = |before: &str, after: &str| {
            pending_query(&opts(before), &opts(after)).map(|(c, seed)| {
                (
                    c.iter()
                        .map(|c| norm(&c.to_token_stream()))
                        .collect::<Vec<_>>(),
                    seed.names()
                        .iter()
                        .map(|n| n.to_string())
                        .collect::<Vec<_>>(),
                )
            })
        };
        // Everything is known: nothing to ask.
        assert_eq!(ask(FLAG, FLAG), None);
        assert_eq!(
            ask(
                "a::check, __announce_deferred(attrs(x))",
                "a::check, __announce_deferred(attrs(x))"
            ),
            None
        );
        // A helper brought a check that nobody asked: only that one is asked, and the
        // names already known go in the state.
        assert_eq!(
            ask(
                "a::check, __announce_deferred(attrs(x))",
                "a::check, b::check(1), __announce_deferred"
            ),
            Some((vec!["b::check(1)".to_string()], vec!["x".to_string()]))
        );
        // The announcement had no checks (nothing to ask): all of them are new.
        assert_eq!(
            ask(FLAG, "b::check, __announce_deferred"),
            Some((vec!["b::check".to_string()], vec![]))
        );
        // It was not asked (a flag written by hand): all of them are asked.
        assert_eq!(
            ask(
                "a::check, __announce_deferred",
                "a::check, b::check, __announce_deferred"
            ),
            Some((vec!["a::check".to_string(), "b::check".to_string()], vec![]))
        );
    }

    fn deferred(me: &str, item: &str) -> Option<String> {
        let attr = ts(me);
        defer(&opts(me), &attr, &ts(item), &OwnedAttrs::new()).map(|t| norm(&t))
    }

    #[test]
    fn the_announcement_moves_below_the_other_attributes() {
        assert_eq!(
            deferred("crate = ::l::announce, meta = a::x", "#[b] #[derive(Debug)] #[c(1)] #[doc = \"d\"] struct S;"),
            Some("#[b]#[derive(Debug)]#[c(1)]#[doc=\"d\"]#[::l::announce::announce(crate=::l::announce,meta=a::x,__announce_deferred)]structS;".to_string())
        );
        // Whatever the attributes are, also the ones that are known to be inert: the
        // macros are not told by their names.
        for item in [
            "#[b] struct S;",
            "#[doc = \"d\"] struct S;",
            "#[derive(Debug)] struct S;",
            "#[derive(Announce)] #[announce(meta = a::x)] struct S;",
        ] {
            assert!(
                deferred("crate = ::l::announce", item).is_some(),
                "{}",
                item
            );
        }
        // A trailing comma is not doubled.
        assert_eq!(
            deferred("crate = ::l::announce, meta = a::x,", "#[b] struct S;"),
            Some("#[b]#[::l::announce::announce(crate=::l::announce,meta=a::x,__announce_deferred)]structS;".to_string())
        );
        assert_eq!(
            deferred("crate = ::l::announce", "#[b] struct S;"),
            Some(
                "#[b]#[::l::announce::announce(crate=::l::announce,__announce_deferred)]structS;"
                    .to_string()
            )
        );
        // What comes out is an announcement that parses, with the flag.
        let out = defer(
            &opts("crate = ::l::announce"),
            &ts("crate = ::l::announce"),
            &ts("#[b] struct S;"),
            &OwnedAttrs::new(),
        )
        .unwrap();
        let attr = &outer(&out).attrs[1];
        assert!(attr.carries_flag());
        let again: Options = syn::parse2(attr.args.clone().unwrap().stream()).unwrap();
        assert!(again.deferred);
    }

    #[test]
    fn the_announcement_moves_once() {
        let me = "crate = ::l::announce";
        let owned = OwnedAttrs::new();
        let first = defer(&opts(me), &ts(me), &ts("#[b] struct S;"), &owned).unwrap();
        // What comes back is the announcement, flagged, as the last attribute.
        let o = outer(&first);
        assert_eq!(o.attrs.len(), 2);
        let args = o.attrs[1].args.clone().unwrap().stream();
        let flagged = opts(&args.to_string());
        assert!(flagged.deferred);
        // Asked again with the flag, whatever is left in the item, it does not move.
        let rest = ts("#[b] struct S;");
        assert!(defer(&flagged, &args, &rest, &owned).is_none());
        assert!(defer(&flagged, &args, &ts("#[b] #[c] #[d(1)] struct S;"), &owned).is_none());
    }

    #[test]
    fn the_announcement_is_made_at_once_if_there_is_nothing_to_wait_for() {
        for me in [
            "crate = ::l::announce",
            "crate = ::l::announce, emit_item = false",
            "crate = ::l::announce, mock = a::B",
        ] {
            assert_eq!(deferred(me, "struct S;"), None);
            assert_eq!(deferred(me, "pub(crate) trait T {}"), None);
        }
    }

    #[test]
    fn an_item_that_is_not_emitted_stays_where_it_is() {
        // The macros below must not see an item that the announcement removes: it is
        // re-emitted as the first attribute, flagged.
        assert_eq!(
            deferred(
                "crate = ::l::announce, emit_item = false",
                "#[derive(Debug)] #[b] struct S;"
            ),
            Some("#[::l::announce::announce(crate=::l::announce,emit_item=false,__announce_deferred)]#[derive(Debug)]#[b]structS;".to_string())
        );
        assert_eq!(
            deferred("crate = ::l::announce, mock = a::B", "#[b] struct S;"),
            Some("#[::l::announce::announce(crate=::l::announce,mock=a::B,__announce_deferred)]#[b]structS;".to_string())
        );
    }

    #[test]
    fn the_attributes_that_a_check_owns_come_after_the_announcement() {
        let attr = ts("crate = ::l::announce, c::check");
        let me = opts("crate = ::l::announce, c::check");
        let owned = OwnedAttrs::from_strs(&["my_lib", "other"]);
        let item = ts("#[my_lib(x)] #[b] #[other] #[my_lib::y = 1] #[doc = \"d\"] struct S;");
        // In their order, and the answer travels in the flag.
        assert_eq!(
            norm(&defer(&me, &attr, &item, &owned).unwrap()),
            "#[b]#[doc=\"d\"]#[::l::announce::announce(crate=::l::announce,c::check,__announce_deferred(attrs(my_lib,other)))]\
             #[my_lib(x)]#[other]#[my_lib::y=1]structS;"
        );
        // An answer without names is still an answer.
        assert_eq!(
            norm(&defer(&me, &attr, &ts("#[b] struct S;"), &OwnedAttrs::new()).unwrap()),
            "#[b]#[::l::announce::announce(crate=::l::announce,c::check,__announce_deferred(attrs()))]structS;"
        );
        // Only the attributes of the item, not a path that merely contains the name.
        assert_eq!(
            norm(&defer(&me, &attr, &ts("#[::my_lib] #[x::my_lib] struct S;"), &owned).unwrap()),
            "#[::my_lib]#[x::my_lib]#[::l::announce::announce(crate=::l::announce,c::check,__announce_deferred(attrs(my_lib,other)))]structS;"
        );
        // The answer that comes back from the flag is the one that was given.
        let out = defer(&me, &attr, &item, &owned).unwrap();
        let moved = &outer(&out).attrs[2];
        let again: Options = syn::parse2(moved.args.clone().unwrap().stream()).unwrap();
        assert_eq!(
            again
                .answer()
                .unwrap()
                .names()
                .iter()
                .map(|n| n.to_string())
                .collect::<Vec<_>>(),
            ["my_lib", "other"]
        );
    }

    #[test]
    fn a_derive_leaves_the_announcement_to_the_attribute_below_it() {
        let attrs = |src: &str| {
            let item: syn::ItemStruct = syn::parse_str(src).unwrap();
            derive_defers(&item.attrs)
        };
        // By the flag, whatever the path.
        assert!(attrs(
            "#[announce(meta = a::x, __announce_deferred)] struct S;"
        ));
        assert!(attrs("#[x::y(__announce_deferred(attrs(a)))] struct S;"));
        assert!(attrs("#[y(__announce_deferred)] struct S;"));
        // By the path: an attribute that has not moved yet.
        assert!(attrs(
            "#[announce(meta = a::x)] #[x::announce(meta = b::x)] struct S;"
        ));
        assert!(attrs("#[::announce::announce] struct S;"));
        assert!(attrs("#[derive(Debug, announce::Announce)] struct S;"));
        // Its own helper, and attributes that are not announcements.
        assert!(!attrs("#[announce(meta = a::x)] struct S;"));
        assert!(!attrs(
            "#[derive(Debug)] #[x::announced] #[doc = \"d\"] struct S;"
        ));
        assert!(!attrs("#[derive(AnnounceNot)] struct S;"));
        assert!(!attrs("#[x::y(meta = __announce_deferred)] struct S;"));
        assert!(!attrs("#[x::y(a::__announce_deferred)] struct S;"));
        assert!(!attrs("struct S;"));
    }

    // ---- termination ------------------------------------------------------------

    /// A small stand-in for the expansion of attribute macros by rustc: the
    /// first macro attribute from the top is expanded, until none is left.
    /// Macros: `announce` (the real function; the output is the item again, or the
    /// terminal output that holds the capture derive), `emit(x)` (announces what it
    /// produces: `#[x::announce(meta = m::x)]` in front of the item), `pass` (returns
    /// its input), `dup` (re-emits the item with an `#[x::announce(meta = m::dup)]`
    /// and a copy of itself, until a limit) and `late(x)` (adds the announcement
    /// *below* itself, among the remaining attributes).
    ///
    /// Returns the final tokens, the number of expansions and how many of them were
    /// announcements.
    fn run(item: &str) -> (String, usize, usize) {
        let mut item = ts(item);
        let (mut steps, mut announcements) = (0, 0);
        loop {
            let o = outer(&item);
            let next = o.attrs.iter().position(|a| {
                let last = a.segments.last().map(|l| l.to_string()).unwrap_or_default();
                matches!(last.as_str(), "announce" | "emit" | "pass" | "dup" | "late")
            });
            let k = match next {
                Some(k) => k,
                None => return (norm(&item), steps, announcements),
            };
            steps += 1;
            assert!(steps < 200, "no end: {}", norm(&item));
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
                "announce" => {
                    announcements += 1;
                    let out = crate::try_announce(args, rest).unwrap();
                    if norm(&out).contains("Capture") {
                        // The terminal output: the trampoline's capture. Count it
                        // and stop expanding announcements of this item.
                        return (norm(&out), steps, announcements);
                    }
                    out
                }
                "emit" => {
                    let mut out = ts(&format!("#[::l::announce::announce(meta = m::{})]", args));
                    out.extend(rest);
                    out
                }
                "late" => {
                    let mut out = TokenStream::new();
                    out.extend(ts(&format!(
                        "#[::l::announce::announce(meta = m::{})]",
                        args
                    )));
                    out.extend(rest);
                    out
                }
                "pass" => rest,
                "dup" => {
                    let n: u32 = args.to_string().trim().parse().unwrap();
                    if n == 0 {
                        rest
                    } else {
                        let mut out = ts(&format!(
                            "#[::l::announce::announce(meta = m::dup{n})] #[dup({m})]",
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

    /// The options in the terminal output, as the capture dummy holds them.
    fn terminal_options(out: &str) -> String {
        let at = out.find("#[__announce(").expect(out);
        let tail = &out[at + "#[__announce(".len()..];
        let tail = &tail[..tail.find(")]#[").expect(tail)];
        tail.split_once(';').unwrap().1.to_string()
    }

    #[test]
    fn expansion_ends_with_one_trampoline() {
        let cases: &[(&str, &str)] = &[
            // attribute and attribute, three attributes: the second to expand moves
            // behind the first, which folds into it
            (
                "#[::l::announce::announce(crate = ::l::announce, meta = m::a)] #[::l::announce::announce(meta = m::b)] struct S;",
                "crate=::l::announce,meta=m::b,meta=m::a",
            ),
            (
                "#[::l::announce::announce(crate = ::l::announce, meta = m::a)] #[::l::announce::announce(meta = m::b)] #[::l::announce::announce(meta = m::c)] struct S;",
                "crate=::l::announce,meta=m::c,meta=m::b,meta=m::a",
            ),
            // a macro that announces, with one that does not announce below it
            ("#[emit(a)] #[pass] struct S;", "meta=m::a"),
            // both wait for each other's macro
            ("#[emit(a)] #[emit(b)] struct S;", "meta=m::b,meta=m::a"),
            // an unknown macro between two announcements, that announces too
            (
                "#[::l::announce::announce(crate = ::l::announce, meta = m::one)] #[emit(two)] #[::l::announce::announce(meta = m::three)] struct S;",
                "crate=::l::announce,meta=m::three,meta=m::two,meta=m::one",
            ),
            // a macro that leaves a copy of itself with a smaller number, announcing at
            // each step
            (
                "#[dup(4)] struct S;",
                "meta=m::dup1,meta=m::dup2,meta=m::dup3,meta=m::dup4",
            ),
            // a macro that announces below itself
            ("#[late(a)] #[pass] struct S;", "meta=m::a"),
        ];
        for (item, want) in cases {
            let (out, steps, announcements) = run(item);
            // One trampoline: the capture dummy is in the output once.
            assert_eq!(out.matches("Capture").count(), 1, "{}", out);
            assert_eq!(terminal_options(&out), *want, "{}\n{}", item, out);
            // Every announcement attribute ends in a trampoline, a fold or a move (once).
            assert!(
                announcements <= 2 * steps,
                "{} {} {}",
                item,
                steps,
                announcements
            );
        }
    }

    #[test]
    fn a_flagged_announcement_with_attributes_left_emits() {
        // Nothing moves again: the next expansion is the last.
        let (out, steps, _) = run("#[::l::announce::announce(crate = ::l::announce, __announce_deferred)] #[pass] #[other] struct S;");
        assert_eq!(steps, 1, "{}", out);
        assert!(out.contains("Capture"), "{}", out);
    }

    #[test]
    fn the_number_of_expansions_is_bounded_by_the_announcements() {
        // `(P, U)` of the module docs: an announcement is expanded twice at most, once
        // to move and once to fold or emit.
        for n in 1..=6u32 {
            let item = format!("#[dup({})] struct S;", n);
            let (out, steps, announcements) = run(&item);
            assert!(out.contains("Capture"), "{}", out);
            // `n` dup expansions and `n` announcements, each moves once and one more
            // expansion for each fold and for the trampoline.
            assert!(
                announcements <= 2 * n as usize,
                "{} {}",
                announcements,
                steps
            );
            assert!(steps <= 4 * n as usize + 1, "{} {}", n, steps);
        }
    }
}
