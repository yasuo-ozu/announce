//! announce mocks of `borrow` traits. Generated from newer-type-std's copies.

#[allow(unused_imports)]
use announce::announce;

/// announce mock of [`::core::borrow::Borrow`]: invoke `Borrow!` to receive its definition.
///
/// The real trait is re-exported under the same path.
#[announce(mock = ::core::borrow::Borrow)]
pub trait Borrow<Borrowed>
where
    Borrowed: ?::core::marker::Sized,
{
    // Required method
    fn borrow(&self) -> &Borrowed;
}

#[doc(no_inline)]
pub use ::core::borrow::Borrow;

/// announce mock of [`::core::borrow::BorrowMut`]: invoke `BorrowMut!` to receive its definition.
///
/// The real trait is re-exported under the same path.
#[announce(mock = ::core::borrow::BorrowMut)]
pub trait BorrowMut<Borrowed>: ::core::borrow::Borrow<Borrowed>
where
    Borrowed: ?::core::marker::Sized,
{
    fn borrow_mut(&mut self) -> &mut Borrowed;
}

#[doc(no_inline)]
pub use ::core::borrow::BorrowMut;

#[cfg(feature = "std")]
/// announce mock of [`::std::borrow::ToOwned`].
#[announce(mock = ::std::borrow::ToOwned)]
pub trait ToOwned {
    type Owned: ::core::borrow::Borrow<Self>;
    fn to_owned(&self) -> Self::Owned;
    fn clone_into(&self, target: &mut Self::Owned) { ::core::unimplemented!() }
}
#[cfg(feature = "std")]
#[doc(no_inline)]
pub use ::std::borrow::ToOwned;
