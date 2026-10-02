//! announce mocks of `ops` traits. Generated from newer-type-std's copies.

#[allow(unused_imports)]
use announce::announce;

/// announce mock of [`::core::ops::Deref`]: invoke `Deref!` to receive its definition.
///
/// The real trait is re-exported under the same path.
#[announce(mock = ::core::ops::Deref)]
pub trait Deref {
    type Target: ?::core::marker::Sized;
    fn deref(&self) -> &Self::Target;
}

#[doc(no_inline)]
pub use ::core::ops::Deref;

/// announce mock of [`::core::ops::DerefMut`]: invoke `DerefMut!` to receive its definition.
///
/// The real trait is re-exported under the same path.
#[announce(mock = ::core::ops::DerefMut)]
pub trait DerefMut: ::core::ops::Deref {
    fn deref_mut(&mut self) -> &mut Self::Target;
}

#[doc(no_inline)]
pub use ::core::ops::DerefMut;

/// announce mock of [`::core::ops::Index`]: invoke `Index!` to receive its definition.
///
/// The real trait is re-exported under the same path.
#[announce(mock = ::core::ops::Index)]
pub trait Index<Idx: ?::core::marker::Sized> {
    type Output: ?::core::marker::Sized;
    fn index(&self, index: Idx) -> &Self::Output;
}

#[doc(no_inline)]
pub use ::core::ops::Index;

/// announce mock of [`::core::ops::IndexMut`]: invoke `IndexMut!` to receive its definition.
///
/// The real trait is re-exported under the same path.
#[announce(mock = ::core::ops::IndexMut)]
pub trait IndexMut<Idx: ?::core::marker::Sized>: ::core::ops::Index<Idx> {
    fn index_mut(&mut self, index: Idx) -> &mut Self::Output;
}

#[doc(no_inline)]
pub use ::core::ops::IndexMut;

/// announce mock of [`::core::ops::Not`]: invoke `Not!` to receive its definition.
///
/// The real trait is re-exported under the same path.
#[announce(mock = ::core::ops::Not)]
pub trait Not {
    type Output;
    fn not(self) -> Self::Output;
}

#[doc(no_inline)]
pub use ::core::ops::Not;

/// announce mock of [`::core::ops::BitAnd`]: invoke `BitAnd!` to receive its definition.
///
/// The real trait is re-exported under the same path.
#[announce(mock = ::core::ops::BitAnd)]
pub trait BitAnd<Rhs = Self> {
    type Output;
    fn bitand(self, rhs: Rhs) -> Self::Output;
}

#[doc(no_inline)]
pub use ::core::ops::BitAnd;

/// announce mock of [`::core::ops::BitOr`]: invoke `BitOr!` to receive its definition.
///
/// The real trait is re-exported under the same path.
#[announce(mock = ::core::ops::BitOr)]
pub trait BitOr<Rhs = Self> {
    type Output;
    fn bitor(self, rhs: Rhs) -> Self::Output;
}

#[doc(no_inline)]
pub use ::core::ops::BitOr;

/// announce mock of [`::core::ops::BitXor`]: invoke `BitXor!` to receive its definition.
///
/// The real trait is re-exported under the same path.
#[announce(mock = ::core::ops::BitXor)]
pub trait BitXor<Rhs = Self> {
    type Output;
    fn bitxor(self, rhs: Rhs) -> Self::Output;
}

#[doc(no_inline)]
pub use ::core::ops::BitXor;

/// announce mock of [`::core::ops::Shl`]: invoke `Shl!` to receive its definition.
///
/// The real trait is re-exported under the same path.
#[announce(mock = ::core::ops::Shl)]
pub trait Shl<Rhs = Self> {
    type Output;
    fn shl(self, rhs: Rhs) -> Self::Output;
}

#[doc(no_inline)]
pub use ::core::ops::Shl;

/// announce mock of [`::core::ops::Shr`]: invoke `Shr!` to receive its definition.
///
/// The real trait is re-exported under the same path.
#[announce(mock = ::core::ops::Shr)]
pub trait Shr<Rhs = Self> {
    type Output;
    fn shr(self, rhs: Rhs) -> Self::Output;
}

#[doc(no_inline)]
pub use ::core::ops::Shr;

/// announce mock of [`::core::ops::BitAndAssign`]: invoke `BitAndAssign!` to receive its definition.
///
/// The real trait is re-exported under the same path.
#[announce(mock = ::core::ops::BitAndAssign)]
pub trait BitAndAssign<Rhs = Self> {
    fn bitand_assign(&mut self, rhs: Rhs);
}

#[doc(no_inline)]
pub use ::core::ops::BitAndAssign;

/// announce mock of [`::core::ops::BitOrAssign`]: invoke `BitOrAssign!` to receive its definition.
///
/// The real trait is re-exported under the same path.
#[announce(mock = ::core::ops::BitOrAssign)]
pub trait BitOrAssign<Rhs = Self> {
    fn bitor_assign(&mut self, rhs: Rhs);
}

#[doc(no_inline)]
pub use ::core::ops::BitOrAssign;

/// announce mock of [`::core::ops::BitXorAssign`]: invoke `BitXorAssign!` to receive its definition.
///
/// The real trait is re-exported under the same path.
#[announce(mock = ::core::ops::BitXorAssign)]
pub trait BitXorAssign<Rhs = Self> {
    fn bitxor_assign(&mut self, rhs: Rhs);
}

#[doc(no_inline)]
pub use ::core::ops::BitXorAssign;

/// announce mock of [`::core::ops::ShlAssign`]: invoke `ShlAssign!` to receive its definition.
///
/// The real trait is re-exported under the same path.
#[announce(mock = ::core::ops::ShlAssign)]
pub trait ShlAssign<Rhs = Self> {
    fn shl_assign(&mut self, rhs: Rhs);
}

#[doc(no_inline)]
pub use ::core::ops::ShlAssign;

/// announce mock of [`::core::ops::ShrAssign`]: invoke `ShrAssign!` to receive its definition.
///
/// The real trait is re-exported under the same path.
#[announce(mock = ::core::ops::ShrAssign)]
pub trait ShrAssign<Rhs = Self> {
    fn shr_assign(&mut self, rhs: Rhs);
}

#[doc(no_inline)]
pub use ::core::ops::ShrAssign;

/// announce mock of [`::core::ops::Add`]: invoke `Add!` to receive its definition.
///
/// The real trait is re-exported under the same path.
#[announce(mock = ::core::ops::Add)]
pub trait Add<Rhs = Self> {
    type Output;
    fn add(self, rhs: Rhs) -> Self::Output;
}

#[doc(no_inline)]
pub use ::core::ops::Add;

/// announce mock of [`::core::ops::Sub`]: invoke `Sub!` to receive its definition.
///
/// The real trait is re-exported under the same path.
#[announce(mock = ::core::ops::Sub)]
pub trait Sub<Rhs = Self> {
    type Output;
    fn sub(self, rhs: Rhs) -> Self::Output;
}

#[doc(no_inline)]
pub use ::core::ops::Sub;

/// announce mock of [`::core::ops::Mul`]: invoke `Mul!` to receive its definition.
///
/// The real trait is re-exported under the same path.
#[announce(mock = ::core::ops::Mul)]
pub trait Mul<Rhs = Self> {
    type Output;
    fn mul(self, rhs: Rhs) -> Self::Output;
}

#[doc(no_inline)]
pub use ::core::ops::Mul;

/// announce mock of [`::core::ops::Div`]: invoke `Div!` to receive its definition.
///
/// The real trait is re-exported under the same path.
#[announce(mock = ::core::ops::Div)]
pub trait Div<Rhs = Self> {
    type Output;
    fn div(self, rhs: Rhs) -> Self::Output;
}

#[doc(no_inline)]
pub use ::core::ops::Div;

/// announce mock of [`::core::ops::Rem`]: invoke `Rem!` to receive its definition.
///
/// The real trait is re-exported under the same path.
#[announce(mock = ::core::ops::Rem)]
pub trait Rem<Rhs = Self> {
    type Output;
    fn rem(self, rhs: Rhs) -> Self::Output;
}

#[doc(no_inline)]
pub use ::core::ops::Rem;

/// announce mock of [`::core::ops::Neg`]: invoke `Neg!` to receive its definition.
///
/// The real trait is re-exported under the same path.
#[announce(mock = ::core::ops::Neg)]
pub trait Neg {
    type Output;
    fn neg(self) -> Self::Output;
}

#[doc(no_inline)]
pub use ::core::ops::Neg;

/// announce mock of [`::core::ops::AddAssign`]: invoke `AddAssign!` to receive its definition.
///
/// The real trait is re-exported under the same path.
#[announce(mock = ::core::ops::AddAssign)]
pub trait AddAssign<Rhs = Self> {
    fn add_assign(&mut self, rhs: Rhs);
}

#[doc(no_inline)]
pub use ::core::ops::AddAssign;

/// announce mock of [`::core::ops::SubAssign`]: invoke `SubAssign!` to receive its definition.
///
/// The real trait is re-exported under the same path.
#[announce(mock = ::core::ops::SubAssign)]
pub trait SubAssign<Rhs = Self> {
    fn sub_assign(&mut self, rhs: Rhs);
}

#[doc(no_inline)]
pub use ::core::ops::SubAssign;

/// announce mock of [`::core::ops::MulAssign`]: invoke `MulAssign!` to receive its definition.
///
/// The real trait is re-exported under the same path.
#[announce(mock = ::core::ops::MulAssign)]
pub trait MulAssign<Rhs = Self> {
    fn mul_assign(&mut self, rhs: Rhs);
}

#[doc(no_inline)]
pub use ::core::ops::MulAssign;

/// announce mock of [`::core::ops::DivAssign`]: invoke `DivAssign!` to receive its definition.
///
/// The real trait is re-exported under the same path.
#[announce(mock = ::core::ops::DivAssign)]
pub trait DivAssign<Rhs = Self> {
    fn div_assign(&mut self, rhs: Rhs);
}

#[doc(no_inline)]
pub use ::core::ops::DivAssign;

/// announce mock of [`::core::ops::RemAssign`]: invoke `RemAssign!` to receive its definition.
///
/// The real trait is re-exported under the same path.
#[announce(mock = ::core::ops::RemAssign)]
pub trait RemAssign<Rhs = Self> {
    fn rem_assign(&mut self, rhs: Rhs);
}

#[doc(no_inline)]
pub use ::core::ops::RemAssign;
