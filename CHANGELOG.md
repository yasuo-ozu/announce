# Changelog

## 0.3.0 (unreleased)

### Added

- `#[announce(strict = false)]` (default `true`; merged announcements are strict unless one says
  `false`). A type or path that cannot be named outside its module is an error at the definition,
  as in 0.2 (for a relative type, or one that has no impl; the errors of type-leak are the
  consumer's, below). With `strict = false` it is recorded in the `META` (`type_leak_error(..)`,
  below) with a private marker that only the item's module can see, and the call checks it.
- `announce::same_context!`, a callback for the chain of a trampoline call that requires the
  call to be in the module of the items it receives (their function bodies and `use super::*`
  submodules too). It gives one error for each cause, at the type or path in the definition,
  worded like the errors at the definition. The texts need Rust 1.78
  (`#[diagnostic::on_unimplemented]`); 1.85 shortens the error. The check is a `const _` item:
  put it beside an `impl`, not in it. A later callback that emits the item elsewhere gets
  rustc's own errors first.
- `announce::helper::LeakFailure` and `LeakError`, read (`syn::parse::Parse`) from the new
  `META` element `type_leak_error(id = R, item = Name, errors = [(TOKENS) "reason" { message =
  "..", notes = [".."], help = ".." }, ..], krate = "..")`; `krate` is the definer's crate, which
  the texts use for a call written through `$crate`. Also `Entry::leak_failure()`,
  `Entry::item_resolved(&krate)` (the item, and the check of `LeakFailure::assume_success` for a
  record) and `same_context`, the function behind the macro. `Entry::type_leak()` returns a
  record as an error, one line for each cause at the path of the call. A reader skips keys and
  tokens it does not know: a value ends at the next `,` outside groups and `<..>`, except that a
  `<` that is never closed (`cmp = a < b`) ends at the first `,` that starts the next element
  (`key =`, or `(TOKENS) "reason"` in the errors), so it does not hide the keys after it.
- The record is the only difference from any other call: `ANNOUNCE_FORMAT` is still `"1"`. A
  callback built with announce 0.2 ignores it and takes the item for one that resolves
  everywhere, which works in the module of the item.
- `announce::helper::try_same_context`, `same_context` with the error returned as a `syn::Error`.
- `announce::machinery::Options::strict` (default `true`), the field that `strict = false` sets.
- `#[announce]` emits type-leak's check (`Leaker::assertions()`) that the primitive names a
  callback copies as written (`u8`) are the primitives in the definition's module: a
  `struct u8;` there is an error at the definition. The fast path writes the same check.

### Changed

- Breaking: announce builds on type-leak 0.9, and `announce::type_leak` and
  `announce::helper::type_leak` are that crate.
  Its errors are `ErrorItem`s with a typed `Cause` and `Reason` (`NotInternableError` is gone;
  `finish()` gives them as a `LeakerResultError`, which `?` turns into one `syn::Error`),
  `Leaker::intern_*` return the `ContextId` (and take no `allow_self`), `LeakerConfig` and
  `Options` are `Rules` and the field `Leaker::check_values` (`Leaker::with_rule(rules)`),
  `Leaker::result()` is a `LeakerResult`
  (the `Referrer`, and the errors that remain; `finish()` is the only way to the `Referrer`, and
  `ignore_if(..)` ignores the errors that a closure picks), the entries are `Referrer::iter()`
  (`Referrer::has_context(id)` tells whether a context is the `Referrer`'s; there are no
  required and optional entries: an absolute path of another crate is an entry unless the rules
  say `external_crates()`, which announce does), and `Referrer::entry_args`, `RepeaterSlots` and
  `UseSite` are `Projection` and `Referrer::complete_args`. A callback must link the same
  type-leak to read the META.
- Breaking: the `type_leak(..)` META is `type_leak(repeater = .., id = .., context = .., result =
  [..])`. `result` is the `LeakerResult` of type-leak: the `Referrer` that `referrer = [..]` had,
  then the errors that remain. A META with `referrer = [..]` and no `result` is read as
  "announced by an older announce", an error. `repeater` and `id` are there only for a
  definition with `repeater = ..`; an item without one that has errors of type-leak and no
  relative type has `type_leak(context = .., result = [..])`. The format of the `Referrer` is 16.
  Reading the META checks that `context` is one of the contexts of `result`: another is an error.
- Breaking: the errors of type-leak are not reported at the definition any more, and `#[announce]`
  no longer rewords or records them. The part is copied as written, and the errors travel in the
  META. The consumer reports them, in type-leak's own words, at their tokens in the definition
  (`Entry::item_resolved`, which is an error for an item with errors that remain), or accepts
  them (`allow`, `ignore` and `ignore_if` of type-leak's `LeakerResult`, which is
  `TypeLeak::result`). This covers a relative trait path, a relative value path, a type with
  `'_`, a prelude name in a bound or value, a bound cycle, a relative macro in an array length,
  the type of a const parameter, `Self` in a type alias, and a method lifetime that only a
  relative type keeps. A definition with only such errors compiles. `strict = false` records
  none of them, and `announce::same_context!` does not check them.
- The causes at the definition are announce's own: a relative type without a `repeater`
  (or in a type alias), and an entry that has no impl, because its impl is for `()`: a relative
  type that names `Self` (`Local<Self>`, `Local<Self::A>`), or a const parameter that is not of a
  primitive type. They are all reported at once, each once. An entry that names `Self` was an
  error of type-leak before and is announce's now.
- The meaning of an absolute path is unchanged: `::std::vec::Vec<u8>` and `::other::X` are
  written as they are. Where one has a relative type in it, the whole absolute type is the entry
  (`::std::vec::Vec<Local>`), as it was one of the entries before.
- Breaking: the repeater has one const parameter, `KEY: u128` (`Projection::key(entry)`), where
  it had `ID: u64` and `NTH: usize`. The impls of `#[announce]` write it; a repeater from
  `define_repeater!` of 0.2 must be defined again, and a callback that wrote `Repeater<ARGS, ID,
  NTH>` by hand must use `type_leak::Projection::of` or `Referrer::visitor`.
- Breaking: `announce::helper::TypeLeak` is the parsed `type_leak(..)` META. It has the fields
  `projection: Option<Projection>` (the repeater and the `id`, for the marker `()`; `None`
  without a repeater), `context: ContextId` and `result: LeakerResult`, and
  `TypeLeak::new(projection, context, result)`, `projection_of(repeater, id)`, `meta()` and
  `syn::parse::Parse`. It has no methods for the errors or for applying: use type-leak's API on
  `result` (`errors()`, `allow(..)`, `ignore(..)`, `ignore_if(..)`, then `finish()`, then
  `Referrer::visitor` and `Referrer::complete_args` for a use site), or `Entry::item_resolved`,
  which reports all the errors that remain at once and rewrites the item. Removed: the fields
  `repeater`, `id` and `referrer`, `types()`, `lookup()`, `slots()`, `entry_args()`,
  `projection(ix, args)`, `apply`, `try_apply` and `try_apply_at(item, &UseSite)`.
  `helper::leak::StableReferrer` numbers every entry (it numbered the required ones), and has
  `stable_index(EntryId)`; its `expand` and `visit_item_mut` give the closure the type of the
  entry and its number as before.
- The analysis runs type-leak once and keeps its result, where announce rewrote the item and ran
  type-leak again until it accepted it. The messages of type-leak are no longer reworded by
  announce (no more "the table" for "the Referrer", no `crate::` for its example crate
  `::my_crate`): the consumer sees them as they are.
- The default of an associated const of a trait is read, and the bodies of provided methods are
  still not: `const N: usize = ::core::mem::size_of::<Local>();` carries `Local` with a
  `repeater` (and is an error without one, like any relative type), and a relative macro in it
  is an error. `None`, `Some`, `Ok`, `Err` and `drop` in it are names of the std prelude:
  relative paths, unless `allow_std_prelude = true`.
- A value path that starts with a primitive type (`u8::MAX`) is fine with `allow_primitive`, the
  default. The value of an attribute (`#[name = Wrap]`), which type-leak reads, is a relative
  value path, an error of type-leak for the consumer.
- The definition error is one error for each relative type or entry without an impl, at its
  tokens in the item, saying what the definer can do (``add `repeater = ..` to the
  `#[announce]`, or write the path absolutely``). All the causes of an item are reported at
  once (also in the record of `strict = false`).
- A trait object (`dyn Base`) and the constants inside a type (`[u8; N]`, `Wrap<{ N }>`) are
  carried with the type by a `repeater`, instead of being errors for their trait or value path;
  without a `repeater` they are relative types. Trait paths in bounds and `impl Trait`, and value
  paths in discriminants, const defaults and the const arguments of a bound, are errors of
  type-leak: the consumer reports them.
- `Entry::type_leak()` (and `leak_failure()`, `item_resolved()`) read only the bare `type_leak(..)`
  and `type_leak_error(..)` elements. A user check or `meta =` with the same last segment
  (`my::type_leak_error(1)`) is no longer taken for them. `Entry::find_meta` still compares the
  last segment.
- A value path names a const parameter only where the parameter is in scope: the const
  parameter `N` of a method no longer lets `const M: usize = N;` of the trait pass (an error of
  type-leak, for the consumer).
- Breaking: `TypeLeak::from_kv` is removed. `TypeLeak` implements `syn::parse::Parse` for the
  `type_leak(..)` META element instead (`Entry::type_leak()` is unchanged).
- Breaking: `KvArgs` and `Meta::kv()` are removed. Parse `key = value` args with your own
  `syn::parse::Parse` impl, through `Meta::parse_args_with`.

### announce-std

- The mocks are generated at build time, for the compiling rustc, from a list of std
  traits that is read from std's own source. A trait or a method that the compiler does
  not have is left out of the mock. The `announce_std_rust_1_N` cfgs and the `rustversion`
  build dependency are gone.
- The list is `std-traits.txt`. The dev tool `tools/gen-std` writes it
  (`cargo run --manifest-path tools/gen-std/Cargo.toml --target-dir target/gen-std`). It
  replaces the Python generator `tools/gen_std.py`, which copied the traits from
  newer-type-std.
- `BuildHasher::hash_one` is mocked from Rust 1.71, where it became stable.
- `ToOwned::clone_into` is mocked from Rust 1.63, where it became stable. It was in the
  mock on every compiler before.

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
