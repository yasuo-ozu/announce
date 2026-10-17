# Changelog

## Unreleased

- Several announcements of one item make one trampoline, one `Announce` impl and one
  `META`, instead of failing with E0252 and E0119. It works for every kind of item, for
  `#[announce]`, `#[derive(Announce)]` and macros that announce what they produce.
- The first `#[announce]` on an item puts a hidden attribute macro, the runner
  (`announce::__private::_actually_run_announce`), last among its attributes; the other
  announcements merge their options into it; the runner makes the trampoline when it
  expands, after every other macro on the item. So callbacks see the item after the other
  attribute macros have run, and without the derives that have expanded.
  `announce::machinery::announce` does the same.
- An item that has no other attribute that could expand is announced at once, with no
  runner: one expansion, as before. `///` documentation, `#[repr(..)]`, `#[allow(..)]` and
  the other built-in attributes that a macro cannot be do not count, and nor do the
  attributes that the checks of the announcement own.
- Merging: checks and `meta` entries are united (the first announcement's first),
  `allow_primitive`, `allow_std_prelude` and `docs` are OR-ed, `crate` is the first's, and
  a `mock`, `repeater`, `layout` or `emit_item` that differs is one error with the item.
- Announcements of one item merge only if they give the same trampoline name: the
  `macro_name`, else the name of the item. `#[announce] #[announce(macro_name = ParseAlt)]
  trait Parse {}` failed in 0.1 with E0119, although the docs of `macro_name` suggested it;
  it is now one error at the attribute that was expanded, and the item is kept.
- The runner is told by its name, never by the path `announce`: attributes of other crates
  that are named `announce` (`#[metrics::announce(level = 2)]`) are not touched, and a
  renamed import (`use announce::announce as ann; #[ann]`) is an announcement.
- The bare `#[announce(..)]` attributes of an item (the helper attributes of a
  `#[derive(Announce)]`) are taken over before the query of the checks, so that their
  checks join its one query. `#[derive(Announce)] #[announce(meta = x::y)]
  #[announce::announce(my_lib::check)] #[my_lib(x)] struct S;` failed with "cannot find
  attribute `announce`".
- The query of the checks runs once for an announcement, also when announcements merge.
- An `#[announce]` or a `#[derive(Announce)]` that a macro appends below the runner is one
  error, and is removed, instead of a second trampoline. A renamed import put there is not
  seen.
- The arguments of the runner carry the version of their encoding
  (`_announce_format_version`). An item announced by two versions of the `announce` crate
  that disagree is one error that lists the versions.
- Behaviour change: `mock = ..` and `emit_item = false` are options like the others. The
  announcement goes through the runner, and announcements below it are merged (they were
  lost). The macros below it now expand on the item before the runner drops it: a derive
  that implements a trait for the item fails with "cannot find type", where 0.1 dropped it
  silently with the item.
- An attribute that a check owns can be in an item-level `#[cfg_attr(.., my_lib(..))]`.
- `#[derive(Announce)]` that the author writes is deprecated: it warns, at the name of the
  item, and says to write `#[announce]`. It still works. `announce::machinery::derive_announce`
  and `derive_announce_with` do not warn. It stands aside, and emits nothing, if the runner,
  an `x::announce` attribute or another derive of `Announce` is below it; the announcement
  takes its helper attributes over. A renamed import below it is not seen
  (`#[derive(Announce)] #[ann]` is E0252).
- `#[derive(Announce)]` merges several `#[announce(..)]` helper attributes instead of
  rejecting the second ("only one helper attribute is allowed"), and reports the options
  that a derive cannot take (`mock`, `emit_item`) and the ones that are not options even
  when it stands aside.

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
