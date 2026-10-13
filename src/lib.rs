#![doc = include_str!("../README.md")]
#![doc(html_logo_url = "https://raw.githubusercontent.com/yasuo-ozu/announce/main/logo.png")]
#![doc(html_favicon_url = "https://raw.githubusercontent.com/yasuo-ozu/announce/main/logo.png")]
#![no_std]

pub use announce_macros::announce;
pub use announce_macros::define_repeater;
pub use announce_macros::Announce;

pub use announce_macros::signature_only;

/// Marks an announced item.
///
/// `#[announce]` implements it for structs, enums and unions, and for `dyn Trait`
/// of simple dyn-compatible traits (see
/// [The `Announce` impl](macro@crate::announce#the-announce-impl)). There is no
/// impl with `emit_item = false` or `mock`.
///
/// A consumer can require `T: Announce` next to a trampoline call. That gives a
/// clear error when the callee was not announced.
/// [`check_type`] and [`check_trait`] of `announce-helper` write that bound.
///
/// [`check_type`]: https://docs.rs/announce-helper/latest/announce_helper/fn.check_type.html
/// [`check_trait`]: https://docs.rs/announce-helper/latest/announce_helper/fn.check_trait.html
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
/// It is the version that `announce-macros` and `announce-helper` use. A macro
/// crate that works with the types announce leaks (for example with
/// `announce_helper::leak::StableReferrer`) can name it through `announce`, and so
/// cannot end up with a second, incompatible copy.
///
/// ```
/// let _leaker = announce::type_leak::Leaker::new();
/// ```
pub use type_leak;

pub mod machinery;

#[doc(hidden)]
pub mod __private {
    pub use announce_macros::{finish, remove, Capture};
}
