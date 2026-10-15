# Changelog

## Unreleased

- Several announcements on one item are merged into one trampoline, one `Announce`
  impl and one `META`, instead of failing with E0252 and E0119. An `#[announce]`
  that finds another announcement on its item (`#[announce]`, `#[x::announce]`, or
  `#[derive(Announce)]` with its `#[announce(..)]` helper) emits only the item, with
  its options added to that announcement: checks and `meta` are united,
  `allow_primitive`, `allow_std_prelude` and `docs` are OR-ed, `crate` is the
  other's, and a `macro_name`, `mock`, `repeater`, `layout` or `emit_item` that
  differs is one error. An `#[announce]` that is expanded while the item still has
  attributes of macros that have not run (`#[a] #[b] item`, where `a` emits
  `#[announce]`) waits once, below them, so that their announcements merge into it.
  `#[derive(Announce)]` leaves the announcement to an `#[x::announce]` below it.
  `announce::machinery::announce` merges the same way.
- `#[derive(Announce)]` merges several `#[announce(..)]` helper attributes instead of
  rejecting the second ("only one helper attribute is allowed").
- Announcing the same item twice, with different `macro_name`s, no longer gives two
  trampolines: it is one error. Announce a copy (see `emit_item`) for a second name.
- `Options` gets an internal option `__deferred = true`, written by the waiting
  `#[announce]`.

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
