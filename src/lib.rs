#![doc = include_str!("../README.md")]
#![doc(html_logo_url = "https://raw.githubusercontent.com/yasuo-ozu/announce/main/logo.png")]
#![doc(html_favicon_url = "https://raw.githubusercontent.com/yasuo-ozu/announce/main/logo.png")]
#![no_std]

pub use announce_macros::announce;
pub use announce_macros::define_repeater;

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
#[cfg_attr(
    feature = "proc-macro-api",
    doc = "[`helper::check_type`] and [`helper::check_trait`] write that bound."
)]
#[cfg_attr(
    not(feature = "proc-macro-api"),
    doc = "`helper::check_type` and `helper::check_trait` write that bound (they need the feature `proc-macro-api`)."
)]
#[cfg_attr(
    announce_diagnostic,
    diagnostic::on_unimplemented(
        message = "`{Self}` is not announced: its definition has no `#[announce]`",
        label = "not announced, so it has no trampoline to call",
        note = "add `#[announce]` to the definition of `{Self}`; for items of std and core, use the stand-ins of the `announce-std` crate"
    )
)]
pub trait Announce {}

/// Re-export of [`type-leak`](https://docs.rs/type-leak), the library that the
/// `repeater` option and [`helper::TypeLeak`] are built on.
///
/// It is the version that `announce-macros` and `announce::helper` use. A macro
/// crate that works with the types announce leaks (for example with
/// [`helper::leak::StableReferrer`]) can name it through `announce`, and so
/// cannot end up with a second, incompatible copy.
///
/// Needs the feature `proc-macro-api`, which is on by default.
///
/// ```
/// let _leaker = announce::type_leak::Leaker::new();
/// ```
#[cfg(feature = "proc-macro-api")]
pub use type_leak;

// The docs of the module are those of `announce_core::helper`, inlined here.
#[cfg(feature = "proc-macro-api")]
#[doc(inline)]
pub use announce_core::helper;

#[cfg(feature = "proc-macro-api")]
pub mod machinery;

#[doc(hidden)]
pub mod __private {
    pub use announce_macros::{__announce_runner, finish, remove, Capture};
}
