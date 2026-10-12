//! announce mocks of `process` traits. Generated from newer-type-std's copies.

#[allow(unused_imports)]
use announce::announce;

#[cfg(announce_std_rust_1_61)]
/// announce mock of [`::std::process::Termination`]: invoke `Termination!` to receive its definition.
///
/// The real trait is re-exported under the same path.
#[announce(mock = ::std::process::Termination)]
pub trait Termination {
    fn report(self) -> ::std::process::ExitCode;
}

#[cfg(announce_std_rust_1_61)]
#[doc(no_inline)]
pub use ::std::process::Termination;
