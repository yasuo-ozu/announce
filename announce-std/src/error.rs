//! announce mocks of `error` traits. Generated from newer-type-std's copies.

#[allow(unused_imports)]
use announce::announce;

#[cfg(announce_std_rust_1_81)]
/// announce mock of [`::core::error::Error`]: invoke `Error!` to receive its definition.
///
/// The real trait is re-exported under the same path.
#[announce(mock = ::core::error::Error)]
pub trait Error: ::core::fmt::Debug + ::core::fmt::Display {
    fn source(&self) -> ::core::option::Option<&(dyn ::core::error::Error + 'static)> {
        ::core::unimplemented!()
    }
}

#[cfg(announce_std_rust_1_81)]
#[doc(no_inline)]
pub use ::core::error::Error;

#[cfg(not(announce_std_rust_1_81))]
#[cfg(feature = "std")]
/// announce mock of [`::std::error::Error`]: invoke `Error!` to receive its definition.
///
/// The real trait is re-exported under the same path.
#[announce(mock = ::std::error::Error)]
pub trait Error: ::core::fmt::Debug + ::core::fmt::Display {
    fn source(&self) -> ::core::option::Option<&(dyn ::std::error::Error + 'static)> {
        ::core::unimplemented!()
    }
}

#[cfg(not(announce_std_rust_1_81))]
#[cfg(feature = "std")]
#[doc(no_inline)]
pub use ::std::error::Error;
