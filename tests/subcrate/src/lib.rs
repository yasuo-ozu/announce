//! Cross-crate tests of `announce`, with the crates written inline by
//! [`subcrate`](https://docs.rs/subcrate).
//!
//! `announce` is made for three crates that never share a compilation: the one that
//! defines an item (`#[announce]`, `$crate` and `crate::` paths in the trampoline), the
//! proc-macro crate with the callbacks, checks and transformers (`announce::helper`), and
//! the crate that calls the trampoline. The other test packages of this workspace
//! (`tests/defs`, `tests/derive`, `tests/user`, ...) are separate Cargo packages for them.
//! Here the crates are `#[subcrate]` modules of one test file: `build.rs` compiles each of
//! them as a real crate with its own `crate::`, and the test file is the calling crate.
//!
//! # Adding a test
//!
//! Make a file `tests/<scenario>.rs` and put the crates in it:
//!
//! ```ignore
//! use subcrate::subcrate;
//!
//! // A proc-macro crate. `::name` links a dependency of this package
//! // (`announce`, `proc_macro2`, `syn`, `template_quote`); `quote!` of template-quote
//! // needs `::proc_macro2` as well.
//! #[subcrate(proc_macro, ::announce, ::proc_macro2, ::syn, ::template_quote)]
//! pub mod macros {
//!     use proc_macro::TokenStream;
//!
//!     #[proc_macro]
//!     pub fn my_callback(input: TokenStream) -> TokenStream {
//!         announce::helper::callback(input.into(), |input| { /* ... */ }).into()
//!     }
//! }
//!
//! // The defining crate. A name without `::` is a sibling subcrate of the file.
//! #[subcrate(::announce, macros)]
//! pub mod defs {
//!     #[announce::announce(macros::my_check)]
//!     pub struct S(pub u8);
//! }
//!
//! // The rest of the file is the calling crate.
//! defs::S! { [defs::S!, macros::my_callback!] REST }
//! ```
//!
//! Things to know:
//!
//! - Put the crates in `tests/*.rs`. A subcrate in `src/lib.rs` is also a `cfg(doc)` module
//!   for rustdoc, which does not suit a crate that is made of `#[announce]`s.
//! - Every `#[subcrate]` module in the package is built by `build.rs` before any test,
//!   so one that does not compile fails the whole package, with the error of `rustc` in
//!   the panic message of the build script. A compile-fail test cannot be a subcrate;
//!   those stay in `tests/ui` (trybuild).
//! - A subcrate is built without the features and cfgs of the test: `cfg(test)` is false
//!   in the definer and true in the calling crate, which makes it a handy cross-crate
//!   `cfg` test (`tests/trampoline.rs`).
//! - The subcrates do not link the `announce` that the test file links. The subcrates are
//!   built before Cargo builds the dependencies, so `build.rs` has Cargo build `::announce`
//!   (and the other `::name`s) for them in a run of its own. That run does not see what
//!   the rest of this package asks of the shared crates: its build-dependency `subcrate`
//!   turns on `proc-macro2/span-locations`, so the `announce` of the test file is another
//!   crate than the `announce` of the subcrates, with the same source (E0277 "there are
//!   multiple different versions of crate `announce`"). Under `cargo check` and clippy they
//!   differ in any case. So a type of `announce` must not cross the boundary at compile
//!   time: do not write `T: announce::Announce` in the test file for a type of a subcrate.
//!   Ask in a generic function of the defining crate instead
//!   (`defs::is_announced::<defs::S>()`), or read the META. Macros are not affected:
//!   they work on tokens.
//! - A subcrate cannot `pub use` a crate it links (the link is a private `extern crate`):
//!   re-export it as a module of globs, as `tests/machinery.rs` does.
//!
//! # The tests
//!
//! | file | what is tested across the boundary |
//! | --- | --- |
//! | `tests/trampoline.rs` | a trampoline of every kind of item, META, a chain through `helper::callback`, `gather`, the derive and attribute routes |
//! | `tests/checks.rs` | `helper::check`: checks that own attributes, the attribute query, `Entry::owned_attrs` |
//! | `tests/transformer.rs` | `helper::transformer` and `signature_only!` in the list of a caller |
//! | `tests/paths.rs` | `crate::` and `$crate` in the payload, emitted again by the caller |
//! | `tests/runner.rs` | several announcements of one item, `use announce::announce as ann`, merged by the runner |
//! | `tests/machinery.rs` | `announce::machinery::announce` in a proc-macro crate, with `crate = ..` |
