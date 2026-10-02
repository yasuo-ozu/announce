//! announce mocks of `default` traits (hand-written).

#[allow(unused_imports)]
use announce::announce;

/// announce mock of [`::core::default::Default`]: invoke `Default!` to receive its definition.
///
/// The real trait is available under the same path in the type namespace
/// (from rustc 1.94, through a glob re-export, since the macro namespace holds this
/// trampoline).
#[announce(mock = ::core::default::Default)]
pub trait Default: ::core::marker::Sized {
    fn default() -> Self;
}

#[cfg(announce_std_rust_1_94)]
#[doc(hidden)]
pub mod __real {
    pub use ::core::default::Default;
}
#[cfg(announce_std_rust_1_94)]
#[doc(no_inline)]
pub use self::__real::*;

#[cfg(not(announce_std_rust_1_94))]
/// Fallback for rustc < 1.94: a blanket subtrait of [`::core::default::Default`], usable in bounds.
pub trait Default: ::core::default::Default {}
#[cfg(not(announce_std_rust_1_94))]
impl<__T: ?::core::marker::Sized + ::core::default::Default> Default for __T {}
