//! announce mocks of `cmp` traits. Generated from newer-type-std's copies.

#[allow(unused_imports)]
use announce::announce;

/// announce mock of [`::core::cmp::PartialEq`]: invoke `PartialEq!` to receive its definition.
///
/// The real trait is available under the same path in the type namespace
/// (from rustc 1.94, through a glob re-export, since the macro namespace holds this
/// trampoline).
#[announce(mock = ::core::cmp::PartialEq)]
pub trait PartialEq<Rhs = Self>
where
    Rhs: ?::core::marker::Sized,
{
    fn eq(&self, other: &Rhs) -> ::core::primitive::bool;
    fn ne(&self, other: &Rhs) -> ::core::primitive::bool {
        ::core::unimplemented!()
    }
}

#[cfg(not(announce_std_rust_1_94))]
/// Fallback for rustc < 1.94: a blanket subtrait of [`::core::cmp::PartialEq`], usable in bounds.
pub trait PartialEq<Rhs: ?::core::marker::Sized = Self>: ::core::cmp::PartialEq<Rhs> {}
#[cfg(not(announce_std_rust_1_94))]
impl<__T: ?::core::marker::Sized + ::core::cmp::PartialEq<Rhs>, Rhs: ?::core::marker::Sized>
    PartialEq<Rhs> for __T
{
}

/// announce mock of [`::core::cmp::Eq`]: invoke `Eq!` to receive its definition.
///
/// The real trait is available under the same path in the type namespace
/// (from rustc 1.94, through a glob re-export, since the macro namespace holds this
/// trampoline).
#[announce(mock = ::core::cmp::Eq)]
pub trait Eq: ::core::cmp::PartialEq {}

#[cfg(not(announce_std_rust_1_94))]
/// Fallback for rustc < 1.94: a blanket subtrait of [`::core::cmp::Eq`], usable in bounds.
pub trait Eq: ::core::cmp::Eq {}
#[cfg(not(announce_std_rust_1_94))]
impl<__T: ?::core::marker::Sized + ::core::cmp::Eq> Eq for __T {}

/// announce mock of [`::core::cmp::PartialOrd`]: invoke `PartialOrd!` to receive its definition.
///
/// The real trait is available under the same path in the type namespace
/// (from rustc 1.94, through a glob re-export, since the macro namespace holds this
/// trampoline).
#[announce(mock = ::core::cmp::PartialOrd)]
pub trait PartialOrd<Rhs = Self>: ::core::cmp::PartialEq<Rhs>
where
    Rhs: ?::core::marker::Sized,
{
    fn partial_cmp(&self, other: &Rhs) -> ::core::option::Option<::core::cmp::Ordering>;

    fn lt(&self, other: &Rhs) -> ::core::primitive::bool {
        ::core::unimplemented!()
    }
    fn le(&self, other: &Rhs) -> ::core::primitive::bool {
        ::core::unimplemented!()
    }
    fn gt(&self, other: &Rhs) -> ::core::primitive::bool {
        ::core::unimplemented!()
    }
    fn ge(&self, other: &Rhs) -> ::core::primitive::bool {
        ::core::unimplemented!()
    }
}

#[cfg(not(announce_std_rust_1_94))]
/// Fallback for rustc < 1.94: a blanket subtrait of [`::core::cmp::PartialOrd`], usable in bounds.
pub trait PartialOrd<Rhs: ?::core::marker::Sized = Self>: ::core::cmp::PartialOrd<Rhs> {}
#[cfg(not(announce_std_rust_1_94))]
impl<__T: ?::core::marker::Sized + ::core::cmp::PartialOrd<Rhs>, Rhs: ?::core::marker::Sized>
    PartialOrd<Rhs> for __T
{
}

/// announce mock of [`::core::cmp::Ord`]: invoke `Ord!` to receive its definition.
///
/// The real trait is available under the same path in the type namespace
/// (from rustc 1.94, through a glob re-export, since the macro namespace holds this
/// trampoline).
#[announce(mock = ::core::cmp::Ord)]
pub trait Ord: ::core::cmp::Eq + ::core::cmp::PartialOrd {
    fn cmp(&self, other: &Self) -> ::core::cmp::Ordering;
    // fn max(self, other: Self) -> Self
    // where
    //     Self: ::core::marker::Sized;
    // fn min(self, other: Self) -> Self
    // where
    //     Self: ::core::marker::Sized;
    // fn clamp(self, min: Self, max: Self) -> Self
    // where
    //     Self: ::core::marker::Sized;
}

#[cfg(not(announce_std_rust_1_94))]
/// Fallback for rustc < 1.94: a blanket subtrait of [`::core::cmp::Ord`], usable in bounds.
pub trait Ord: ::core::cmp::Ord {}
#[cfg(not(announce_std_rust_1_94))]
impl<__T: ?::core::marker::Sized + ::core::cmp::Ord> Ord for __T {}

// Real traits whose name is also a derive macro: explicit names shadow glob
// imports per namespace, so this glob supplies the *trait* while the announce
// trampoline above keeps the *macro* namespace. Before rustc 1.94 the glob
// breaks the trampoline's resolution, so it is only emitted from 1.94 on.
#[cfg(announce_std_rust_1_94)]
#[doc(hidden)]
pub mod __real {

    pub use ::core::cmp::PartialEq;

    pub use ::core::cmp::Eq;

    pub use ::core::cmp::PartialOrd;

    pub use ::core::cmp::Ord;
}
#[cfg(announce_std_rust_1_94)]
#[doc(no_inline)]
pub use self::__real::*;
