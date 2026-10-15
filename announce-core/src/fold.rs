//! Several announcements of one item.
//!
//! Downstream attribute and derive macros emit `#[announce(..)]` or
//! `#[derive(Announce)]` on the items they produce. When two announcements apply to
//! one item they must make one trampoline and one `Announce` impl, not two (the second
//! would be E0252 and E0119). The announcements are merged, in two steps.
//!
//! **Folding** ([`fold`]). When `#[announce]` expands and the item it receives has
//! another *pending* announcement, it emits no trampoline. It merges its options into
//! the other announcement ([`Options::merge`]) and re-emits the item with the other
//! attribute updated. A pending announcement is, the first in the order of the
//! attributes:
//!
//! - a qualified attribute `#[x::announce(..)]`;
//! - a `#[derive(.. Announce ..)]`, with its helper attribute, the bare
//!   `#[announce(..)]` (made if there is none, right after the derive).
//!
//! A bare `#[announce(..)]` is the helper of a derive, or an attribute macro that was
//! imported. With a derive in the item it is the derive's helper. Without a derive it is
//! one of the two, and rustc has taken the derive away when the helper is left over
//! (a derive expands before the attribute macros *below* it, and its attribute is gone
//! from what they receive). In both cases the options are the same, so they are read
//! and the attribute is removed ([`Folded::Absorbed`]): the announcement that is
//! expanding takes them over.
//!
//! A derive that has an `#[x::announce]` attribute below it has already expanded when
//! the attribute does. It cannot change the item, so it emits nothing and leaves the
//! announcement to the attribute ([`derive_defers`]), which finds the helper.
//!
//! **Deferring** ([`defer`]). `#[a] #[b] item`: `a` emits `#[announce] #[b] item`. The
//! new `#[announce]` is outermost and sees `#[b]`, which may emit an announcement of
//! its own later. So when no announcement is pending and the item has an attribute that
//! is not known to be inert, `#[announce]` re-emits the item with itself moved to be the
//! last outer attribute, marked with the internal option `__deferred`. `#[b]` expands,
//! and if it emits an announcement, that one folds into the marked one. A check can own
//! attributes of the item (`#[announce(check)] #[my_lib] struct S`), and an attribute
//! above `#[announce]` is rejected by rustc before `#[announce]` can remove it: such an
//! item is never deferred.
//!
//! # Termination
//!
//! Every expansion of an announce attribute is one of:
//!
//! 1. **terminal**: it emits the trampoline, and no announce attribute;
//! 2. **a fold**: it emits the item with *one announce attribute fewer* (itself, which
//!    rustc has consumed; the other one is only updated in place);
//! 3. **a deferral**: it emits itself once more, but only when `__deferred` is not set,
//!    and with `__deferred` set.
//!
//! Let `P` be the number of announce attribute invocations on the item, and `U` the
//! number of them without the marker. A terminal expansion lowers `P`; a fold lowers
//! `P` and does not raise `U` (the target keeps its marker, or gets one); a deferral
//! keeps `P` and lowers `U`. So `(P, U)` falls in the lexicographic order at every
//! expansion of an announce attribute, and there can be no endless run of them. An
//! announcement defers at most once, and an announcement with the marker never defers,
//! even when attributes that are not inert are left: it emits the trampoline. Macros of
//! others can still add announcements (each of them is one more `P`); they do so by
//! their own expansions, which the argument does not cover.
//!
//! A derive never folds and never re-emits anything: it emits the trampoline or nothing.

use crate::helper::OwnedAttrs;
use crate::options::{MergeConflict, Options};
use crate::scan::{self, Outer};
use proc_macro2::{Delimiter, Group, Span, TokenStream, TokenTree};
use template_quote::{quote, ToTokens};

/// What [`fold`] found.
pub(crate) enum Folded {
    /// No other announcement: go on with this one as given.
    No,
    /// This announcement is done: the tokens replace the item (the other announcement
    /// has the options of this one, or an error comes with the item as it was).
    Emit(TokenStream),
    /// This announcement goes on, with the options of the bare `#[announce(..)]`
    /// attributes merged in and those attributes taken off the item.
    Absorbed {
        opts: Box<Options>,
        item: TokenStream,
        /// Errors of the attributes that were taken off: to come before the output.
        errors: TokenStream,
    },
}

/// The options of an announcement attribute: `None` for the form `path = ..`.
fn options_of(a: &scan::OuterAttr) -> Option<syn::Result<Options>> {
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

/// How [`rebuild`] changes the attributes.
enum Edit {
    None,
    /// New tokens inside the brackets of attribute `.0`.
    Replace(usize, TokenStream),
    /// A whole new attribute after attribute `.0`.
    InsertAfter(usize, TokenStream),
}

/// The item with the attributes `removed` gone and `edit` done.
fn rebuild(outer: &Outer, removed: &[usize], edit: Edit) -> TokenStream {
    let mut out = TokenStream::new();
    for (k, a) in outer.attrs.iter().enumerate() {
        if removed.contains(&k) {
            continue;
        }
        match &edit {
            Edit::Replace(r, inner) if *r == k => {
                let old = match &outer.tts[a.at + 1] {
                    TokenTree::Group(g) => g.span(),
                    _ => Span::call_site(),
                };
                let mut group = Group::new(Delimiter::Bracket, inner.clone());
                group.set_span(old);
                out.extend([outer.tts[a.at].clone(), TokenTree::Group(group)]);
            }
            _ => out.extend([outer.tts[a.at].clone(), outer.tts[a.at + 1].clone()]),
        }
        if let Edit::InsertAfter(i, attr) = &edit {
            if *i == k {
                out.extend(attr.clone());
            }
        }
    }
    out.extend(outer.tts[outer.attrs_end()..].iter().cloned());
    out
}

/// Rule 1: merges the announcement that is expanding (`me`) into another announcement
/// of `item`, if there is one. See the module docs.
pub(crate) fn fold(me: &Options, item: &TokenStream) -> Folded {
    if !scan::starts_with_attr(item) {
        return Folded::No;
    }
    let outer = scan::outer(item);
    let derive_at = outer.attrs.iter().position(|a| a.derives_announce());
    let mut current = me.clone();
    let mut removed: Vec<usize> = Vec::new();
    let mut errors = TokenStream::new();

    // Bare `#[announce(..)]` without a derive: take the options over.
    if derive_at.is_none() {
        for (k, a) in outer
            .attrs
            .iter()
            .enumerate()
            .filter(|(_, a)| a.is_bare("announce"))
        {
            match options_of(a) {
                None => {}
                Some(Err(e)) => {
                    // Nobody else would report it (the derive it belonged to is gone).
                    errors.extend(e.to_compile_error());
                    removed.push(k);
                }
                // The attribute is the target: its entries come first.
                Some(Ok(theirs)) => match theirs.merge(&current) {
                    Ok(merged) => {
                        current = merged;
                        removed.push(k);
                    }
                    Err(c) => return conflict(&c, item),
                },
            }
        }
    }

    // The first pending announcement that is another attribute or a derive.
    for (k, a) in outer.attrs.iter().enumerate() {
        if removed.contains(&k) {
            continue;
        }
        if a.is_qualified("announce") {
            let target = match options_of(a) {
                Some(Ok(t)) => t,
                // Its own expansion reports it.
                _ => continue,
            };
            return match target.merge(&current) {
                Ok(merged) => {
                    let inner = a.with_args(merged.to_token_stream());
                    let mut out = errors;
                    out.extend(rebuild(&outer, &removed, Edit::Replace(k, inner)));
                    Folded::Emit(out)
                }
                Err(c) => conflict(&c, item),
            };
        }
        if Some(k) == derive_at {
            let helper = outer.attrs.iter().position(|b| b.is_bare("announce"));
            let target = match helper.map(|h| options_of(&outer.attrs[h])) {
                None => Options::default(),
                Some(Some(Ok(t))) => t,
                // The derive reports the bad helper.
                Some(_) => continue,
            };
            // The derive emits the item as written and has no mock.
            for (bad, name) in [
                (current.mock.is_some(), "mock"),
                (!current.emit_item, "emit_item"),
            ] {
                if bad {
                    let msg = format!(
                        "announce: `{}` does not apply to an item that has `#[derive(Announce)]`: \
                         the derive cannot change the item, so it cannot be merged with this \
                         announcement",
                        name
                    );
                    return Folded::Emit(error_with_item(msg, item));
                }
            }
            let mut merged = match target.merge(&current) {
                Ok(m) => m,
                Err(c) => return conflict(&c, item),
            };
            // A derive never defers.
            merged.deferred = false;
            let args = merged.to_token_stream();
            let edit = match helper {
                Some(h) if !args.is_empty() => Edit::Replace(h, outer.attrs[h].with_args(args)),
                None if !args.is_empty() => Edit::InsertAfter(k, quote! { #[announce(#args)] }),
                _ => Edit::None,
            };
            let mut out = errors;
            out.extend(rebuild(&outer, &removed, edit));
            return Folded::Emit(out);
        }
    }

    if removed.is_empty() {
        Folded::No
    } else {
        Folded::Absorbed {
            opts: Box::new(current),
            item: rebuild(&outer, &removed, Edit::None),
            errors,
        }
    }
}

/// Rule 2: the item with the announcement moved to be the last outer attribute, if it
/// should be. `None`: emit the announcement now. `attr` is the arguments of the
/// attribute.
pub(crate) fn defer(
    opts: &Options,
    attr: &TokenStream,
    item: &TokenStream,
    owned: &OwnedAttrs,
) -> Option<TokenStream> {
    // The marker makes this the last time. An item that is not emitted (`emit_item =
    // false`, `mock`) is not seen by the macros below, so there is nothing to wait for;
    // and an attribute of a derive that is not there would be rejected above the
    // announcement.
    if opts.deferred || !opts.emit_item || opts.mock.is_some() || !scan::starts_with_attr(item) {
        return None;
    }
    let outer = scan::outer(item);
    let mut unknown = false;
    for a in &outer.attrs {
        if a.is_owned_by(owned) {
            return None;
        }
        unknown |= !a.is_inert();
    }
    if !unknown {
        return None;
    }
    let krate = opts.krate();
    let mut args = attr.clone();
    let comma =
        matches!(args.clone().into_iter().last(), Some(TokenTree::Punct(p)) if p.as_char() == ',');
    if !args.is_empty() && !comma {
        args.extend(quote! { , });
    }
    let end = outer.attrs_end();
    let mut out: TokenStream = outer.tts[..end].iter().cloned().collect();
    out.extend(quote! { #[#krate::announce(#args __deferred = true)] });
    out.extend(outer.tts[end..].iter().cloned());
    Some(out)
}

/// For `#[derive(Announce)]`: whether another announcement is left to expand below it.
///
/// A derive expands before the attribute macros below it, which then do not see its
/// attribute. It cannot change the item, so when the item has an `#[x::announce]`
/// attribute, or another `#[derive(Announce)]`, below it, the derive emits nothing: the
/// announcement below makes the trampoline, with the options of the helper attribute
/// that is left in the item.
pub(crate) fn derive_defers(attrs: &[syn::Attribute]) -> bool {
    use syn::punctuated::Punctuated;
    attrs.iter().any(|a| {
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

    /// `fold` for `me` on `item`, printed: `No`, `Emit(..)` or `Absorbed(options; item)`.
    fn folded(me: &str, item: &str) -> String {
        match fold(&opts(me), &ts(item)) {
            Folded::No => "No".to_string(),
            Folded::Emit(t) => format!("Emit({})", norm(&t)),
            Folded::Absorbed { opts, item, errors } => format!(
                "Absorbed({};{};{})",
                norm(&opts.to_token_stream()),
                norm(&item),
                norm(&errors)
            ),
        }
    }

    #[test]
    fn nothing_pending_is_nothing_to_fold() {
        for item in [
            "pub struct S;",
            "#[derive(Debug)] #[doc = \"x\"] #[repr(C)] pub struct S;",
            // Not an announcement: another path, or a name that only contains it.
            "#[x::announced] #[announce_not] #[derive(Announced)] pub struct S;",
            "#[cfg_attr(a, announce(meta = m::x))] pub struct S;",
            // Not a form we merge into, or it does not parse: its own expansion says so.
            "#[x::announce = 1] pub struct S;",
            "#[x::announce(nope = 1)] pub struct S;",
        ] {
            assert_eq!(folded("meta = a::x", item), "No", "{}", item);
        }
    }

    #[test]
    fn a_qualified_attribute_receives_the_options() {
        assert_eq!(
            folded("meta = a::x", "#[x::announce(meta = b::x)] pub struct S;"),
            "Emit(#[x::announce(meta=b::x,meta=a::x)]pubstructS;)"
        );
        assert_eq!(
            folded(
                "meta = a::x",
                "#[::x::y::announce] #[doc = \"d\"] pub struct S;"
            ),
            "Emit(#[::x::y::announce(meta=a::x)]#[doc=\"d\"]pubstructS;)"
        );
        // Nothing to add: the attribute stays as it is.
        assert_eq!(
            folded("", "#[x::announce(meta = b::x)] struct S;"),
            "Emit(#[x::announce(meta=b::x)]structS;)"
        );
        assert_eq!(
            folded("", "#[x::announce] struct S;"),
            "Emit(#[x::announce]structS;)"
        );
    }

    #[test]
    fn the_first_pending_announcement_is_the_one() {
        assert_eq!(
            folded(
                "meta = a::x",
                "#[derive(Debug)] #[y::announce(meta = y::x)] #[z::announce(meta = z::x)] struct S;"
            ),
            "Emit(#[derive(Debug)]#[y::announce(meta=y::x,meta=a::x)]#[z::announce(meta=z::x)]structS;)"
        );
    }

    #[test]
    fn the_merged_attribute_keeps_the_crate_of_the_target() {
        assert_eq!(
            folded(
                "crate = ::me::announce",
                "#[x::announce(crate = ::other::announce)] struct S;"
            ),
            "Emit(#[x::announce(crate=::other::announce)]structS;)"
        );
        assert_eq!(
            folded("crate = ::me::announce", "#[x::announce] struct S;"),
            "Emit(#[x::announce(crate=::me::announce)]structS;)"
        );
    }

    #[test]
    fn the_marker_of_either_stays() {
        assert!(folded("__deferred = true", "#[x::announce] struct S;").contains("__deferred=true"));
        assert!(
            folded("", "#[x::announce(__deferred = true)] struct S;").contains("__deferred=true")
        );
        assert!(!folded("", "#[x::announce] struct S;").contains("__deferred"));
    }

    #[test]
    fn a_bare_attribute_is_taken_over() {
        // Without a derive, the bare `#[announce(..)]` is a macro that was imported
        // (or the helper of a derive that has expanded): its options come first.
        assert_eq!(
            folded(
                "meta = a::x",
                "#[announce(meta = b::x)] #[doc = \"d\"] struct S;"
            ),
            "Absorbed(meta=b::x,meta=a::x;#[doc=\"d\"]structS;;)"
        );
        assert_eq!(
            folded(
                "meta = a::x",
                "#[announce] #[announce(meta = c::x)] struct S;"
            ),
            "Absorbed(meta=c::x,meta=a::x;structS;;)"
        );
        // A qualified one wins over the bare one: the bare one is read first.
        assert_eq!(
            folded(
                "meta = a::x",
                "#[announce(meta = b::x)] #[x::announce] struct S;"
            ),
            "Emit(#[x::announce(meta=b::x,meta=a::x)]structS;)"
        );
        // A bare attribute that does not parse is reported once, and dropped.
        let out = folded("", "#[announce(nope = 1)] struct S;");
        assert!(
            out.starts_with("Absorbed(;structS;;::core::compile_error!"),
            "{}",
            out
        );
        assert_eq!(out.matches("compile_error").count(), 1, "{}", out);
    }

    #[test]
    fn a_derive_receives_the_options_in_its_helper() {
        assert_eq!(
            folded(
                "meta = a::x",
                "#[derive(Debug, ::announce::Announce)] #[announce(meta = b::x)] #[doc = \"d\"] struct S;"
            ),
            "Emit(#[derive(Debug,::announce::Announce)]#[announce(meta=b::x,meta=a::x)]#[doc=\"d\"]structS;)"
        );
        // A helper is made right after the derive (before it, rustc rejects it).
        assert_eq!(
            folded(
                "meta = a::x",
                "#[derive(Announce)] #[doc = \"d\"] struct S;"
            ),
            "Emit(#[derive(Announce)]#[announce(meta=a::x)]#[doc=\"d\"]structS;)"
        );
        assert_eq!(
            folded(
                "meta = a::x",
                "#[doc = \"d\"] #[derive(a::Announce,)] struct S;"
            ),
            "Emit(#[doc=\"d\"]#[derive(a::Announce,)]#[announce(meta=a::x)]structS;)"
        );
        // Nothing to add: the item as it is.
        assert_eq!(
            folded("", "#[derive(Announce)] struct S;"),
            "Emit(#[derive(Announce)]structS;)"
        );
        // The marker does not go to a derive, which never waits.
        assert_eq!(
            folded("__deferred = true", "#[derive(Announce)] struct S;"),
            "Emit(#[derive(Announce)]structS;)"
        );
        // Only the derive named `Announce`.
        assert_eq!(
            folded("meta = a::x", "#[derive(Announced, Announce2)] struct S;"),
            "No"
        );
    }

    #[test]
    fn a_derive_comes_before_a_bare_attribute() {
        // With a derive in the item the bare attribute is its helper: it is not taken over.
        let out = folded(
            "meta = a::x",
            "#[announce(meta = b::x)] #[derive(Announce)] struct S;",
        );
        assert_eq!(
            out,
            "Emit(#[announce(meta=b::x,meta=a::x)]#[derive(Announce)]structS;)"
        );
    }

    #[test]
    fn a_conflict_is_one_error_and_the_item_as_it_was() {
        for (me, item, option) in [
            (
                "macro_name = A",
                "#[x::announce(macro_name = B)] struct S;",
                "macro_name",
            ),
            (
                "layout = single",
                "#[x::announce(layout = direct)] struct S;",
                "layout",
            ),
            (
                "macro_name = A",
                "#[announce(macro_name = B)] struct S;",
                "macro_name",
            ),
            (
                "macro_name = A",
                "#[derive(Announce)] #[announce(macro_name = B)] struct S;",
                "macro_name",
            ),
        ] {
            let out = folded(me, item);
            assert!(out.starts_with("Emit(::core::compile_error!"), "{}", out);
            assert!(out.contains(&format!("`{}=", option)), "{}", out);
            assert_eq!(out.matches("compile_error").count(), 1, "{}", out);
            // The item is there as it was, with the attribute that stays.
            assert!(out.ends_with(&format!("{})", norm(&ts(item)))), "{}", out);
        }
    }

    #[test]
    fn options_that_a_derive_cannot_have() {
        for me in ["mock = a::B", "emit_item = false"] {
            let out = folded(me, "#[derive(Announce)] struct S;");
            assert!(out.starts_with("Emit(::core::compile_error!"), "{}", out);
            assert!(
                out.contains("doesnotapplytoanitemthathas`#[derive(Announce)]`"),
                "{}",
                out
            );
        }
        assert!(folded("emit_item = true", "#[derive(Announce)] struct S;")
            .starts_with("Emit(#[derive"));
    }

    #[test]
    fn groups_without_delimiters_are_read_in_place() {
        use proc_macro2::{Delimiter, Group};
        let item: TokenStream = [TokenTree::Group(Group::new(
            Delimiter::None,
            ts("#[x::announce(meta = b::x)] pub struct S;"),
        ))]
        .into_iter()
        .collect();
        match fold(&opts("meta = a::x"), &item) {
            Folded::Emit(out) => {
                assert_eq!(norm(&out), "#[x::announce(meta=b::x,meta=a::x)]pubstructS;")
            }
            _ => panic!("not folded"),
        }
    }

    #[test]
    fn what_is_inert() {
        let inert = |attr: &str| {
            let item = ts(&format!("#[{}] struct S;", attr));
            outer(&item).attrs[0].is_inert()
        };
        for a in [
            "doc = \"x\"",
            "doc(hidden)",
            "cfg(a)",
            "cfg_attr(a, b)",
            "repr(C)",
            "allow(a)",
            "warn(a)",
            "deny(a)",
            "forbid(a)",
            "expect(a)",
            "must_use",
            "non_exhaustive",
            "deprecated",
            "inline",
            "cold",
            "automatically_derived",
            "derive(Debug)",
            "announce(meta = a::b)",
            "rustfmt::skip",
            "clippy::all",
            "diagnostic::on_unimplemented",
            "rustc_copy_clone_marker",
            "macro_use",
        ] {
            assert!(inert(a), "{}", a);
        }
        for a in [
            "serde(skip)",
            "b",
            "x::announce",
            "::doc",
            "rustfmt",
            "other::skip",
            "my_lib",
            "a::b::c",
            "doc::x",
        ] {
            assert!(!inert(a), "{}", a);
        }
        // A path that is not one of the forms read is not inert either.
        assert!(!outer(&ts("#[a::] struct S;")).attrs[0].is_inert());
        assert!(!outer(&ts("#[a b] struct S;")).attrs[0].is_inert());
    }

    fn deferred(me: &str, item: &str) -> Option<String> {
        let attr = ts(me);
        defer(&opts(me), &attr, &ts(item), &OwnedAttrs::new()).map(|t| norm(&t))
    }

    #[test]
    fn the_announcement_moves_below_what_it_does_not_know() {
        assert_eq!(
            deferred("crate = ::l::announce, meta = a::x", "#[b] #[derive(Debug)] #[c(1)] #[doc = \"d\"] struct S;"),
            Some("#[b]#[derive(Debug)]#[c(1)]#[doc=\"d\"]#[::l::announce::announce(crate=::l::announce,meta=a::x,__deferred=true)]structS;".to_string())
        );
        assert_eq!(
            deferred("crate = ::l::announce", "#[b] struct S;"),
            Some(
                "#[b]#[::l::announce::announce(crate=::l::announce,__deferred=true)]structS;"
                    .to_string()
            )
        );
        // A trailing comma is not doubled.
        assert_eq!(
            deferred("crate = ::l::announce, meta = a::x,", "#[b] struct S;"),
            Some("#[b]#[::l::announce::announce(crate=::l::announce,meta=a::x,__deferred=true)]structS;".to_string())
        );
        // What comes out is an announcement that parses, with the marker.
        let out = defer(
            &opts("crate = ::l::announce"),
            &ts("crate = ::l::announce"),
            &ts("#[b] struct S;"),
            &OwnedAttrs::new(),
        )
        .unwrap();
        let attr = &outer(&out).attrs[1];
        let again: Options = syn::parse2(attr.args.clone().unwrap().stream()).unwrap();
        assert!(again.deferred);
    }

    #[test]
    fn the_announcement_waits_once() {
        let me = "crate = ::l::announce";
        let owned = OwnedAttrs::new();
        let first = defer(&opts(me), &ts(me), &ts("#[b] struct S;"), &owned).unwrap();
        // What comes back is the announcement, marked, as the last attribute.
        let o = outer(&first);
        assert_eq!(o.attrs.len(), 2);
        let args = o.attrs[1].args.clone().unwrap().stream();
        let marked = opts(&args.to_string());
        assert!(marked.deferred);
        // Asked again with the marker, with unknown attributes still there: it stays.
        let rest = ts("#[b] struct S;");
        assert!(defer(&marked, &args, &rest, &owned).is_none());
        assert!(defer(&marked, &args, &ts("#[b] #[c] #[d(1)] struct S;"), &owned).is_none());
    }

    #[test]
    fn the_announcement_does_not_wait_when_it_need_not() {
        let me = "crate = ::l::announce";
        for item in [
            "struct S;",
            "#[derive(Debug)] #[doc = \"d\"] #[repr(C)] #[allow(dead_code)] struct S;",
            "#[derive(Announce)] #[announce(meta = a::x)] struct S;",
        ] {
            assert_eq!(deferred(me, item), None, "{}", item);
        }
        // An item that is not emitted is not seen by the macros below.
        assert_eq!(
            deferred("crate = ::l::announce, emit_item = false", "#[b] struct S;"),
            None
        );
        assert_eq!(
            deferred("crate = ::l::announce, mock = a::B", "#[b] struct S;"),
            None
        );
    }

    #[test]
    fn the_announcement_does_not_go_below_an_attribute_that_a_check_owns() {
        let me = opts("crate = ::l::announce, c::check");
        let attr = ts("crate = ::l::announce, c::check");
        let item = ts("#[b] #[my_lib(x)] struct S;");
        let owned = OwnedAttrs::from_strs(&["my_lib"]);
        assert!(defer(&me, &attr, &item, &owned).is_none());
        // Owned attributes alone do not make it wait.
        let item = ts("#[my_lib(x)] #[doc = \"d\"] struct S;");
        assert!(defer(&me, &attr, &item, &owned).is_none());
        // Without them the item waits.
        let item = ts("#[b] #[my_lib(x)] struct S;");
        assert!(defer(&me, &attr, &item, &OwnedAttrs::new()).is_some());
    }

    #[test]
    fn a_derive_leaves_the_announcement_to_the_attribute_below_it() {
        let attrs = |src: &str| {
            let item: syn::ItemStruct = syn::parse_str(src).unwrap();
            derive_defers(&item.attrs)
        };
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
        assert!(!attrs("struct S;"));
    }

    // ---- termination ------------------------------------------------------------

    /// A small stand-in for the expansion of attribute macros by rustc: the
    /// first macro attribute from the top is expanded, until none is left.
    /// Macros: `announce` (the real function; the output is the item again, or the
    /// terminal output that holds the capture derive), `emit(x)` (announces what it
    /// produces: `#[x::announce(meta = m::x)]` in front of the item), `pass` (returns
    /// its input) and `dup` (re-emits the item with an `#[x::announce(meta = m::dup)]`
    /// and a copy of itself, until a limit).
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
                matches!(last.as_str(), "announce" | "emit" | "pass" | "dup")
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
            // attribute and attribute, three attributes
            (
                "#[::l::announce::announce(crate = ::l::announce, meta = m::a)] #[::l::announce::announce(meta = m::b)] struct S;",
                "crate=::l::announce,meta=m::b,meta=m::a",
            ),
            (
                "#[::l::announce::announce(crate = ::l::announce, meta = m::a)] #[::l::announce::announce(meta = m::b)] #[::l::announce::announce(meta = m::c)] struct S;",
                "crate=::l::announce,meta=m::c,meta=m::b,meta=m::a",
            ),
            // a macro that announces, with one that does not announce below it
            (
                "#[emit(a)] #[pass] struct S;",
                "meta=m::a,__deferred=true",
            ),
            // both wait for each other's macro
            (
                "#[emit(a)] #[emit(b)] struct S;",
                "__deferred=true,meta=m::a,meta=m::b",
            ),
            // an unknown macro between two announcements, that announces too
            (
                "#[::l::announce::announce(crate = ::l::announce, meta = m::one)] #[emit(two)] #[::l::announce::announce(meta = m::three)] struct S;",
                "crate=::l::announce,meta=m::three,meta=m::one,meta=m::two",
            ),
            // a macro that leaves a copy of itself with a smaller number, announcing at
            // each step
            (
                "#[dup(4)] struct S;",
                "__deferred=true,meta=m::dup4,meta=m::dup3,meta=m::dup2,meta=m::dup1",
            ),
        ];
        for (item, want) in cases {
            let (out, steps, announcements) = run(item);
            // One trampoline: the capture dummy is in the output once.
            assert_eq!(out.matches("Capture").count(), 1, "{}", out);
            assert_eq!(terminal_options(&out), *want, "{}\n{}", item, out);
            // Every announcement attribute ends in a trampoline, a fold or a wait (once).
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
    fn a_marked_announcement_with_unknown_attributes_left_emits() {
        // Nothing moves again: the next expansion is the last.
        let (out, steps, _) = run("#[::l::announce::announce(crate = ::l::announce, __deferred = true)] #[pass] #[other] struct S;");
        assert_eq!(steps, 1, "{}", out);
        assert!(out.contains("Capture"), "{}", out);
    }
}
