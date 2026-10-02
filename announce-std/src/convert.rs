//! announce mocks of `convert` traits. Generated from newer-type-std's copies.

#[allow(unused_imports)]
use announce::announce;

/// announce mock of [`::core::convert::AsRef`]: invoke `AsRef!` to receive its definition.
///
/// The real trait is re-exported under the same path.
#[announce(mock = ::core::convert::AsRef)]
pub trait AsRef<T: ?::core::marker::Sized> {
    fn as_ref(&self) -> &T;
}

#[doc(no_inline)]
pub use ::core::convert::AsRef;

/// announce mock of [`::core::convert::AsMut`]: invoke `AsMut!` to receive its definition.
///
/// The real trait is re-exported under the same path.
#[announce(mock = ::core::convert::AsMut)]
pub trait AsMut<T: ?::core::marker::Sized> {
    fn as_mut(&mut self) -> &mut T;
}

#[doc(no_inline)]
pub use ::core::convert::AsMut;

/// announce mock of [`::core::convert::Into`]: invoke `Into!` to receive its definition.
///
/// The real trait is re-exported under the same path.
#[announce(mock = ::core::convert::Into)]
pub trait Into<T>: ::core::marker::Sized {
    fn into(self) -> T;
}

#[doc(no_inline)]
pub use ::core::convert::Into;

/// announce mock of [`::core::convert::TryInto`]: invoke `TryInto!` to receive its definition.
///
/// The real trait is re-exported under the same path.
#[announce(mock = ::core::convert::TryInto)]
pub trait TryInto<T>: ::core::marker::Sized {
    type Error;
    fn try_into(self) -> ::core::result::Result<T, Self::Error>;
}

#[doc(no_inline)]
pub use ::core::convert::TryInto;
