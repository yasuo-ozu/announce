//! announce mocks of `iter` traits. Generated from newer-type-std's copies.

#[allow(unused_imports)]
use announce::announce;

/// announce mock of [`::core::iter::IntoIterator`]: invoke `IntoIterator!` to receive its definition.
///
/// The real trait is re-exported under the same path.
#[announce(mock = ::core::iter::IntoIterator)]
pub trait IntoIterator {
    type Item;
    type IntoIter: ::core::iter::Iterator<Item = Self::Item>;
    fn into_iter(self) -> Self::IntoIter;
}

#[doc(no_inline)]
pub use ::core::iter::IntoIterator;

/// announce mock of [`::core::iter::Extend`]: invoke `Extend!` to receive its definition.
///
/// The real trait is re-exported under the same path.
#[announce(mock = ::core::iter::Extend)]
pub trait Extend<A> {
    fn extend<T>(&mut self, iter: T)
    where
        T: ::core::iter::IntoIterator<Item = A>;
}

#[doc(no_inline)]
pub use ::core::iter::Extend;

/// announce mock of [`::core::iter::Iterator`]: invoke `Iterator!` to receive its definition.
///
/// The real trait is re-exported under the same path.
#[announce(mock = ::core::iter::Iterator)]
pub trait Iterator {
    type Item;
    fn next(&mut self) -> ::core::option::Option<Self::Item>;
    fn size_hint(
        &self,
    ) -> (
        ::core::primitive::usize,
        ::core::option::Option<::core::primitive::usize>,
    ) {
        ::core::unimplemented!()
    }
    fn count(self) -> ::core::primitive::usize
    where
        Self: ::core::marker::Sized,
    {
        ::core::unimplemented!()
    }
    fn last(self) -> ::core::option::Option<Self::Item>
    where
        Self: ::core::marker::Sized,
    {
        ::core::unimplemented!()
    }
    fn nth(&mut self, n: ::core::primitive::usize) -> ::core::option::Option<Self::Item> {
        ::core::unimplemented!()
    }
}

#[doc(no_inline)]
pub use ::core::iter::Iterator;

/// announce mock of [`::core::iter::FusedIterator`]: invoke `FusedIterator!` to receive its definition.
///
/// The real trait is re-exported under the same path.
#[announce(mock = ::core::iter::FusedIterator)]
pub trait FusedIterator: ::core::iter::Iterator {}

#[doc(no_inline)]
pub use ::core::iter::FusedIterator;

/// announce mock of [`::core::iter::ExactSizeIterator`]: invoke `ExactSizeIterator!` to receive its definition.
///
/// The real trait is re-exported under the same path.
#[announce(mock = ::core::iter::ExactSizeIterator)]
pub trait ExactSizeIterator: ::core::iter::Iterator {
    fn len(&self) -> ::core::primitive::usize {
        ::core::unimplemented!()
    }
}

#[doc(no_inline)]
pub use ::core::iter::ExactSizeIterator;

/// announce mock of [`::core::iter::DoubleEndedIterator`]: invoke `DoubleEndedIterator!` to receive its definition.
///
/// The real trait is re-exported under the same path.
#[announce(mock = ::core::iter::DoubleEndedIterator)]
pub trait DoubleEndedIterator: ::core::iter::Iterator {
    fn next_back(&mut self) -> ::core::option::Option<Self::Item>;

    fn nth_back(&mut self, n: ::core::primitive::usize) -> ::core::option::Option<Self::Item> {
        ::core::unimplemented!()
    }
    // fn try_rfold<B, F, R>(&mut self, init: B, f: F) -> R
    // where
    //     Self: ::core::marker::Sized,
    //     F: ::core::ops::FnMut(B, Self::Item) -> R,
    //     R: ::core::ops::Try<Output = B>;
    fn rfold<B, F>(self, init: B, f: F) -> B
    where
        Self: ::core::marker::Sized,
        F: ::core::ops::FnMut(B, Self::Item) -> B,
    {
        ::core::unimplemented!()
    }
    fn rfind<P>(&mut self, predicate: P) -> ::core::option::Option<Self::Item>
    where
        Self: ::core::marker::Sized,
        P: ::core::ops::FnMut(&Self::Item) -> ::core::primitive::bool,
    {
        ::core::unimplemented!()
    }
}

#[doc(no_inline)]
pub use ::core::iter::DoubleEndedIterator;
