# Changelog

## 0.3.0 (unreleased)

### Added

- `#[announce(strict = false)]` (`strict` is a bool, default `true`; merged announcements are
  strict unless one says `false`). A type or path that cannot be named outside its module is an
  error at the definition, as in 0.2. With `strict = false` the definition records it in the
  `META` instead (`type_leak_error(..)`, below) and emits a private marker that only the module
  of the item can see, and the call checks it. Mistakes in the options stay errors.
- `announce::same_context!`, a callback for the chain of a trampoline call that requires the
  call to be in the module of the items it receives (their function bodies and `use super::*`
  submodules too). It gives one error for each cause, at the type or path in the definition
  (all of it underlined) and worded like the errors of type-leak, with the call in rustc's
  note about the bound. The texts need Rust 1.78 (`#[diagnostic::on_unimplemented]`); 1.85
  shortens the error. The check is a `const _` item: put it beside an `impl`, not in it. A
  callback later in the chain that emits the item outside its module gets rustc's own "cannot
  find type" errors first, and with a relative trait path in a bound only rustc's error appears.
- `announce::helper::LeakFailure` and `LeakError`, read from the new `META` element
  `type_leak_error(id = R, item = Name, errors = [(TOKENS) "reason" { message = "..",
  notes = [".."], help = ".." }, ..], krate = "..")`. `LeakFailure` implements
  `syn::parse::Parse` for it (`item` is the item's name, raw identifiers included). Also
  `Entry::leak_failure()`, `Entry::item_resolved(&krate)` (the item, and the check of
  `LeakFailure::assume_success` for a record) and `same_context`, the function behind the
  macro. `Entry::type_leak()` returns a record as an error, one line for each cause, with all of
  the path of the call underlined
  (``this macro cannot use `m::Item`: `Local` cannot be named outside its module; ..``).
  `krate` is the definer's crate as Cargo names it, which the texts use for a call written
  through `$crate` (left out when unknown). A reader skips keys and tokens it does not know.
- The record is the only difference from any other call: `ANNOUNCE_FORMAT` is still `"1"`. A
  callback built with announce 0.2 ignores it and takes the item for one that resolves
  everywhere, which works in the module of the item; elsewhere rustc reports the names it
  cannot find.

### Changed

- The definition error for a failure of the analysis is one error for each offending type or
  path, at its tokens in the item, saying what the definer can do (``add `repeater = ..` to
  the `#[announce]`, or write the path absolutely``; for a const generic that is not of a
  primitive type, to use a primitive type). All the causes of an item are reported at once (also
  in the record of `strict = false`): a relative trait path or value path no longer hides the
  relative types and the refused types of the item, and nor does any refusal of type-leak (the
  trait of a `&dyn Trait` or of `<Self as Tr>::X`, `Self::X` as a value, a `Self` in a macro, a
  path in an attribute value).
- A trait object (`dyn Base`) and the constants inside a type (`[u8; N]`, `Wrap<{ N }>`) are
  carried with the type by a `repeater`, instead of being errors for their trait or value path;
  without a `repeater` they are relative types. Trait paths in bounds and `impl Trait`, and value
  paths in discriminants, const defaults and the const arguments of a bound, stay errors.
- Breaking: `TypeLeak::from_kv` is removed. `TypeLeak` implements `syn::parse::Parse` for the
  `type_leak(..)` META element instead: `syn::parse2::<TypeLeak>(meta.to_token_stream())`
  (`Entry::type_leak()` is unchanged). Unknown keys are still ignored.
- Breaking: `KvArgs` and `Meta::kv()` are removed. Parse `key = value` args with your own
  `syn::parse::Parse` impl, through `Meta::parse_args_with`.

## 0.2.0

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

### Added

- `announce::helper::callback` and `announce::helper::transformer`: plain functions that
  take a closure, like `announce::helper::check`. You write the `#[proc_macro]` function
  yourself and call them from it:

  ```rust
  #[proc_macro]
  pub fn my_callback(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
      announce::helper::callback(input.into(), |input| { /* Input -> syn::Result<TokenStream> */ })
          .into()
  }

  #[proc_macro]
  pub fn public_fields(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
      announce::helper::transformer(input.into(), |mut entry| { /* Entry -> syn::Result<Entry> */ })
          .into()
  }
  ```

  Both work on `proc_macro2` streams, so they can be tested without a proc-macro crate,
  and a `syn::Error` becomes a `compile_error!` as before.

### Changed

- The macros `announce::helper::callback!` and `announce::helper::transformer!` are kept
  for compatibility, but they are now thin wrappers that call the functions above, and
  they are hidden from the docs. Their syntax and behavior are unchanged, so crates that
  use them (flat_enum 0.2, for example) keep building; new code should call the
  functions. `announce::signature_only!` and the docs now use the functions.

### Wire format

- Every call that announce makes to the next element of a chain starts with
  `@announce_format "1"`, the version of the wire format, so that a macro of yours (a
  `macro_rules!` callback, check or transformer too) can tell that announce called it:
  `(@announce_format "1" $($rest:tt)*) => { .. }`. It is the first thing in the call,
  before `@__announce_self { .. }`, `@__announce_dollar { .. }`, `(OPTIONS)` and
  `[LIST]`. It covers what the trampolines emit (to the next element and to
  themselves, also in the dummy enum of a derive or attribute element), `Input::forward`,
  `fetch`, `gather` and `forward_transformed`, `ContList::invoke`, `redirect` (which
  moves the prefix of the payload to the front), the attribute query of the checks
  (`@announce_format "1" @__announce_attrs [..] { STATE }`, also `Query::answer`) and the
  call of a check at the definition (`@announce_format "1" (ARGS), ITEM`), the call of the
  sentinel of the runner (`NAME! { @announce_format "1" @__announce_exists }`; the arm of the
  trampoline and of the stand-in also takes the bare `NAME! { @__announce_exists }`), and
  what `announce::helper::callback`, `transformer` and the macros `callback!` and
  `transformer!` (which call them) emit. The version is
  `announce::helper::ANNOUNCE_FORMAT`; `announce::helper::format_prefix()` writes the
  prefix, and `Input::format` is the version a call came with.
  - Everything announce parses accepts the prefix (`Input`, `Query`, `CheckInput`,
    `CheckCall`, `check`, `finish!`, the trampolines), and a call without it, written by
    hand (`S! { [S!, cb!] }`), keeps working: with no prefix there is no version, and
    none is checked (no error, no warning). Only a version that is given and is not ours
    is an error. A call with another version (`@announce_format "2"`) is one compile
    error that names both versions; a trampoline, a `macro_rules!`, can only say which
    version it takes.
  - **Wire-format change.** A `macro_rules!` callback, check or transformer written
    for 0.1.x matches the old shape (`([$($list:tt)*] ..)`, `(@__announce_attrs ..)`,
    `(($($args:tt)*), $($item:tt)*)`) and does not match any more: put
    `@announce_format "1"` before the old pattern (and in the calls it forwards), or
    `$(@announce_format "1")?` to take both. The versions of announce in a chain must
    match, in both directions. A trampoline made by 0.1.x has no arm for the prefix: a
    call that this version makes to it (`Input::fetch` or `gather`, `ContList::invoke`,
    `redirect`, a hand-written call with the prefix) reaches its catch-all, and rustc
    reports `announce: invalid trampoline input; expected `Item! { [Item!, next!, ..]
    .. }` ..` at the call. A callback or check that is built with the helpers of 0.1.x
    cannot read the calls of this version either: `Input` fails with "expected square
    brackets", and `check` with a `compile_error!` that says it expected
    `@__announce_attrs`. Calls without the prefix from 0.1.x helpers into trampolines
    of this version work (the prefix is optional there), but what those trampolines
    call next has it.
  - It is not `__announce_format_version`, the entry in the arguments of the runner
    attribute: that one versions the encoding of the arguments of an attribute, which
    the runners of two versions of announce compare when they meet on one item;
    `@announce_format` versions the calls between macros. The runner is not a call of a
    chain, so it has no `@announce_format`.
  - A trampoline has 44 to 54 more tokens (925 to 969 for a struct with one field, 2063
    to 2111 for a struct of 200 fields held once).

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
