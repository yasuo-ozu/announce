//! announce mocks of `process` traits. Generated from newer-type-std's copies.

#[allow(unused_imports)]
use announce::announce;

/// announce mock of [`::std::process::Termination`]: invoke `Termination!` to receive its definition.
///
/// The real trait is re-exported under the same path.
#[announce(mock = ::std::process::Termination)]
pub trait Termination {
    fn report(self) -> ::std::process::ExitCode;
}

#[doc(no_inline)]
pub use ::std::process::Termination;
