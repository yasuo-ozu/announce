# Changelog

## Unreleased

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
