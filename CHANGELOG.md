# Changelog

## Unreleased

- Several announcements on one item are merged into one trampoline, one `Announce`
  impl and one `META`, instead of failing with E0252 and E0119. `#[announce]` now
  moves itself below the other attributes of the item the first time it expands (the
  item comes back with `#[announce(.., __announce_deferred)]` as its last attribute),
  so it announces the item as it is after the other attribute macros on it have
  expanded; a `#[derive(..)]` below `#[announce]` has run by then and is not in what
  callbacks receive. An item with no outer attribute is announced at once, and
  an announcement with `emit_item = false` or `mock` does not move (the macros below it
  never see the item). The moved announcement makes the trampoline, after it has taken
  over the bare `#[announce(..)]` helper attributes of a derive, or it merges into
  another moved announcement below it (checks and `meta` are united, `allow_primitive`,
  `allow_std_prelude` and `docs` are OR-ed, `crate` is the target's, and a `mock`,
  `repeater`, `layout` or `emit_item` that differs is one error). An announcement is told
  by the mark `__announce_deferred`, never by its path: attributes of other crates that
  are named `announce` (`#[metrics::announce(level = 2)]`) are not touched, and a renamed
  import (`use announce::announce as ann; #[ann]`) is an announcement. The mark carries
  the answer of the query of the checks (`__announce_deferred(attrs(my_lib))`), so the
  query runs once for an announcement. `#[derive(Announce)]` leaves the announcement to
  an announcement below it that has the mark or the path `x::announce` (the derive is
  the one place that goes by the path: a derive above an `#[x::announce]` of another
  crate stands aside). A macro that announces what it produces must put its attribute
  first; one appended below the others is expanded after the moved announcement.
  `announce::machinery::announce` does the same.
- `#[derive(Announce)]` merges several `#[announce(..)]` helper attributes instead of
  rejecting the second ("only one helper attribute is allowed").
- Announcements of one item merge only if they give the same trampoline name: the
  `macro_name`, else the name of the item. Announcing one item twice under different
  names (`#[announce] #[announce(macro_name = ParseAlt)] trait Parse {}`) failed in 0.1
  with E0119 (two `Announce` impls), although the docs of `macro_name` suggested it. It
  is now one error at the attribute that was expanded, and the item is kept. An announcement without `macro_name` is named after
  the item, so it does not merge with one that renames the trampoline. Use the same
  `macro_name` (or none) in every announcement; announce a copy (see `emit_item`) for a
  second name.

## 0.1.1

- `no_std` definers: `announce::helper`, `announce::machinery` and
  `announce::type_leak` are behind the new feature `proc-macro-api`, which is on by
  default. With `default-features = false`, `announce` has only the macros and the
  trait `Announce` and does not pull proc-macro2 and syn into the target graph, so a
  `#![no_std]` crate with `#[announce]` or `#[derive(Announce)]` builds for targets
  such as `x86_64-unknown-none`. `announce-std` uses `announce` this way.
- The minimum Rust version stays 1.57. `announce-core` caps `unicode-width` (needed by
  type-leak through safegraph) below 0.1.13: 0.1.14 needs Rust 1.66 and declares no
  `rust-version`, so a lock file made by a plain `cargo generate-lockfile` broke the
  build on Rust 1.57 to 1.65.
- An error for a std prelude name that cannot stay relative (`Option<Self::Out>` in an
  announced trait without `allow_std_prelude = true`) now says that the name is
  relative and to write `::core::option::Option<..>` or set `allow_std_prelude = true`,
  instead of "cannot be leaked as a whole".
- Docs of `announce::helper::Input`: a callback that mixes the received tokens with its
  own output must reuse the `self` token of the payload (E0424 otherwise).

## 0.1.0

Initial release: `#[announce]` and `#[derive(Announce)]` (trampolines for structs, enums,
unions, traits, type aliases and inline modules), checks and attributes of your own,
`meta`, the `repeater` option (types that are relative to the defining crate),
`announce::helper` and `announce::machinery` for callbacks and for proc-macro libraries,
`signature_only!`, and the `announce-std` crate.
