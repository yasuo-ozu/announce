//! announce mocks of std/core traits.
//!
//! Each `pub trait X` here is a copy of the real trait marked
//! `#[announce(mock = ::core::path::X)]`: invoking `announce_std::path::X!` passes the
//! copy onward, with `mock(::core::path::X)` in the entry META so consumers
//! implement the real trait. The copy itself is only type-checked in a hidden
//! module; except for traits whose name is also a derive macro (`Debug`,
//! `Clone`, `Hash`, ...), the real trait is re-exported under the same path.
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
//! **Compiler versions.** For traits whose name is also a derive macro
//! (`Debug`, `Hash`, `PartialEq`, `Eq`, `PartialOrd`, `Ord`, `Clone`, `Copy`,
//! `Default`), the type namespace holds the real trait from rustc 1.94 on. On
//! older compilers it holds a blanket subtrait with the same name
//! (`pub trait Debug: ::core::fmt::Debug {}` implemented for every
//! `T: ::core::fmt::Debug`): it works in **bounds**, but it is a different
//! trait, so `impl announce_std::fmt::Debug for X` does not implement the real
//! one there. Write impls against the real path (the `mock(..)` META).

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
