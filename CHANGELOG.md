# Changelog

## Unreleased

### Merging

- Several announcements of one item make one trampoline, one `Announce` impl and one
  `META`, instead of failing with E0252 and E0119, for every kind of item, for `#[announce]`
  and for macros that announce what they produce (`announce::machinery::announce` too).
- The first `#[announce]` on an item puts a hidden attribute macro, the runner
  (`announce::__private::__announce_runner`), last among its attributes. The other
  announcements merge their options into it, in the order rustc expands them, and the
  runner makes the trampoline after every other macro on the item. So callbacks see the item
  after the other attribute macros have run, and without the derives that have expanded.
- An item that has no other attribute that could expand is announced at once, with no
  runner: one expansion, as before. `///` documentation, `#[repr(..)]`, `#[allow(..)]` and
  the other built-in attributes that a macro cannot be do not count, and nor do the
  attributes that the checks of the announcement own. `#[announce]` reads the attributes of
  the item for this, and moves the ones its checks own below the runner; it removes no
  other.
- Options: checks and `meta` entries are united (the first announcement's first),
  `allow_primitive`, `allow_std_prelude` and `docs` are OR-ed, `crate` is the first's, and
  a `mock`, `repeater`, `layout` or `emit_item` that differs is one error with the item.
  Announcements merge only if they give the same trampoline name: the `macro_name`, else the
  name of the item (`#[announce] #[announce(macro_name = ParseAlt)] trait Parse {}` failed
  in 0.1 with E0119; it is now one error at the attribute that was expanded, and the item
  is kept).
- The runner is told by its name, never by the path `announce`: attributes of other crates
  that are named `announce` (`#[metrics::announce(level = 2)]`) are not touched, and a
  renamed import (`use announce::announce as ann; #[ann]`) is an announcement. A bare
  `#[announce]` is an attribute macro like any other, which rustc expands: ours if it is
  imported from `announce`, a foreign one if that is what the name means (it runs, as in
  `#[::announce::announce(..)] #[announce] struct S;`).
- The query of the checks runs once for an announcement, also when announcements merge. An
  attribute that a check owns can be in an item-level `#[cfg_attr(.., my_lib(..))]`.
- A macro that returns the item without its outer attributes drops the runner. The
  announcement that puts the runner on the item also emits a sentinel after the item, which
  fails at the definition then: `Announce` is not implemented for a struct, enum or union
  without generics, "cannot find macro" for the trampoline of anything else. Because of the
  sentinel, `mock` and `emit_item = false` have to be on the first announcement of such a
  struct, enum or union (on a later one they are one error with the item). The sentinel
  carries the `#[cfg]` of the item.
- An `#[announce]` with a qualified path (`#[::announce::announce]`, `#[x::announce]`) that a
  macro appends below the runner is one error, and is removed, instead of a second
  trampoline. A bare or renamed import put there is not seen.
- The arguments of the runner carry the version of their encoding
  (`__announce_format_version`). An item announced by two versions of the `announce` crate
  that disagree is one error that lists the versions. An `#[announce]` of announce 0.1 on
  the same item as one of this version is not merged (0.1 has no runner): it makes a second
  trampoline (E0252). Use one version of `announce` in every crate.

### Removed

- `#[derive(Announce)]` and `announce::machinery::derive_announce`, `try_derive_announce` and
  `derive_announce_with`. Write `#[announce]` instead, with the options as its arguments
  (the options of the helper attribute `#[announce(..)]`). A derive of a library cannot
  announce the item it is on any more, because a derive cannot change its item: the library
  asks its users for `#[announce]`, or provides an attribute macro that emits it. The trait
  `Announce` stays.

### Behaviour changes

- `mock = ..` and `emit_item = false` are options like the others. The announcement goes
  through the runner, and announcements below it are merged (they were lost). The macros
  below it now expand on the item before the runner drops it: a derive that implements a
  trait for the item fails with "cannot find type", where 0.1 dropped it silently with the
  item.
- `#[announce] #[cfg(..)]` with the cfg false emits nothing: the trampoline goes with the
  item (0.1 emitted a trampoline for a missing item).
- The message of the error for an item that is not announced (`diagnostic::on_unimplemented`
  of `Announce`, Rust 1.78 and later) is clearer: "`T` is not announced: its definition has
  no `#[announce]`".

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
