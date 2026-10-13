//! Announced copies of std/core traits, so they can be passed to proc macros.
//!
//! `announce_std::iter::Iterator! { [announce_std::iter::Iterator!, cb!] }`
//! passes a copy of `Iterator` to `cb!`. META holds `mock(::core::iter::Iterator)`:
//! implement that real trait, not the copy. In the type namespace
//! `announce_std::iter::Iterator` is the real trait, so it works in bounds too.
//!
//! ```
//! macro_rules! show {
//!     ([$($l:tt)*] $($p:ident)? $(:: $s:ident)* ! => [mock($($m:tt)*)] { $($item:tt)* }) => {
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
//! `Clone`, `Copy` and `Default` are also derive macro names. Before rustc 1.94
//! their type-namespace item is a blanket subtrait instead of the real trait.
//! It works in bounds, but `impl announce_std::fmt::Debug for X` does not
//! implement the real `Debug`. Write impls against the real path from
//! `mock(..)`.
//!
//! **Not mocked.** `BuildHasher::hash_one` is stable only since Rust 1.71, and
//! announce-std supports older compilers (MSRV 1.57): a consumer that forwards it
//! would break there. The `BuildHasher` mock has `build_hasher` only. To hash a
//! value go through `Hash::hash`: `x.hash(&mut b.build_hasher())`, then
//! `finish()` on the hasher.
//!
//! **Version gates.** Mocks of traits newer than the MSRV (1.57), or changed
//! after it, are gated with `#[cfg(announce_std_rust_1_N)]`. `build.rs` sets
//! that cfg on rustc 1.N and later, using `rustversion` attributes
//! (`Termination` 1.61, `IntoFuture` 1.64, and the 1.80, 1.81 and 1.94 variants).

#![cfg_attr(not(feature = "std"), no_std)]
#![allow(clippy::all)]

pub mod alloc;
pub mod any;
pub mod borrow;
pub mod clone;
pub mod cmp;
pub mod convert;
pub mod default;
pub mod error;
pub mod fmt;
pub mod future;
pub mod hash;
#[cfg(feature = "std")]
pub mod io;
pub mod iter;
pub mod marker;
#[cfg(feature = "std")]
pub mod net;
pub mod ops;
#[cfg(feature = "std")]
pub mod process;
#[cfg(feature = "std")]
pub mod string;
#[cfg(feature = "std")]
pub mod task;
