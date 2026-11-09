//! Announced copies of std/core traits, so they can be passed to proc macros.
//!
//! `announce_std::iter::Iterator! { [announce_std::iter::Iterator!, cb!] }` passes
//! a copy of `Iterator` to `cb!`. META holds `mock(::core::iter::Iterator)`:
//! implement that real trait, not the copy. In the type namespace,
//! `announce_std::iter::Iterator` is the real trait, so it works in bounds too.
//!
//! ```
//! macro_rules! show {
//!     (@announce_format "1" [$($l:tt)*] $($p:ident)? $(:: $s:ident)* ! => [mock($($m:tt)*)] { $($item:tt)* }) => {
//!         const MOCKED: &str = stringify!($($m)*);
//!     };
//! }
//! announce_std::iter::Iterator! { [announce_std::iter::Iterator!, show!] }
//! assert_eq!(MOCKED, ":: core :: iter :: Iterator");
//! // The type namespace holds the real trait:
//! fn _f(_: &dyn announce_std::iter::Iterator<Item = u8>) {}
//! ```
//!
//! **Old compilers.** `Debug`, `Hash`, `PartialEq`, `Eq`, `PartialOrd`, `Ord`,
//! `Clone`, `Copy` and `Default` are also derive macro names. Before rustc 1.94,
//! their type-namespace item is a blanket subtrait instead of the real trait. It
//! works in bounds, but `impl announce_std::fmt::Debug for X` does not implement
//! the real `Debug`. Write impls against the real path from `mock(..)`.
//!
//! **Version gates.** The mocks have what the compiling rustc has. A trait that is newer
//! than the compiler is not mocked, and neither is a method that is newer (they are
//! left out, not gated by a `cfg`): `Termination` from 1.61, `ToOwned::clone_into` from
//! 1.63, `IntoFuture` from 1.64, `BuildHasher::hash_one` from 1.71,
//! `Seek::seek_relative` from 1.80, and `core::error::Error` from 1.81 (before that the
//! mock is of `std::error::Error`, with the `std` feature). On a compiler before 1.71,
//! hash a value with `x.hash(&mut b.build_hasher())`, then `finish()` on the hasher.
//!
//! **How the mocks are made.** `std-traits.txt` holds the signatures of the mocked
//! traits, each with the Rust version since which it is stable. The dev tool
//! `tools/gen-std` writes it from std's own source (the `rust-src` component of a
//! toolchain):
//!
//! ```text
//! cargo run --manifest-path tools/gen-std/Cargo.toml --target-dir target/gen-std
//! ```
//!
//! With `-- --check` it tells whether the file is up to date. Which traits and methods
//! are mocked is listed in the tool. `build.rs` reads the file, keeps what the compiling
//! rustc has, and writes the mocks to `OUT_DIR`, where this file includes them.

#![cfg_attr(not(feature = "std"), no_std)]
#![allow(clippy::all)]

include!(concat!(env!("OUT_DIR"), "/mocks.rs"));
