//! announce mocks of `clone` traits (hand-written).

#[allow(unused_imports)]
use announce::announce;

/// announce mock of [`::core::clone::Clone`]: invoke `Clone!` to receive its definition.
///
/// The real trait is available under the same path in the type namespace
/// (from rustc 1.94, through a glob re-export, since the macro namespace holds this
/// trampoline).
#[announce(mock = ::core::clone::Clone)]
pub trait Clone: ::core::marker::Sized {
    fn clone(&self) -> Self;
    fn clone_from(&mut self, source: &Self) {
        ::core::unimplemented!()
    }
}

#[cfg(announce_std_rust_1_94)]
#[doc(hidden)]
pub mod __real {
    pub use ::core::clone::Clone;
}
#[cfg(announce_std_rust_1_94)]
#[doc(no_inline)]
pub use self::__real::*;

#[cfg(not(announce_std_rust_1_94))]
/// Fallback for rustc < 1.94: a blanket subtrait of [`::core::clone::Clone`], usable in bounds.
pub trait Clone: ::core::clone::Clone {}
#[cfg(not(announce_std_rust_1_94))]
impl<__T: ?::core::marker::Sized + ::core::clone::Clone> Clone for __T {}
