#![doc = include_str!("../README.md")]
#![no_std]

pub use announce_macros::announce;
pub use announce_macros::Announce;

pub use announce_macros::signature_only;

/// Marks an announced item.
///
/// Implemented for announced structs, enums and unions, and for `dyn Trait` of
/// simple dyn-compatible traits. Not implemented with `emit_item = false` or
/// `mock`.
///
/// A consumer can require `T: Announce` next to a trampoline call. That gives a
/// clear error when the callee was not announced (see
/// `announce_helper::check_type` and `check_trait`).
#[cfg_attr(
    announce_diagnostic,
    diagnostic::on_unimplemented(
        message = "`{Self}` is not marked `#[announce]`",
        label = "no `#[announce]` trampoline for this item",
        note = "add `#[announce]` to its definition; for std/core items use the `announce-std` crate"
    )
)]
pub trait Announce {}

/// Re-export of [`type-leak`](https://docs.rs/type-leak), the library that the
/// `repeater` option and `announce_helper::TypeLeak` are built on.
///
/// It is the version that `announce-macros` and `announce-helper` use, so a
/// macro crate that works with the types announce leaks (for example with
/// `announce_helper::leak::StableReferrer`) can name it through `announce`
/// and cannot end up with a second, incompatible copy.
///
/// ```
/// let _leaker = announce::type_leak::Leaker::new();
/// ```
pub use type_leak;

pub mod machinery;

#[doc(hidden)]
pub mod __private {
    pub use announce_macros::{remove, Capture};
}
