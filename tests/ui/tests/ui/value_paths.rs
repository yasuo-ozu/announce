pub const N: usize = 3;
pub struct Buf<const M: usize>;
pub trait Tr<const M: usize> {}

// A relative constant in an enum discriminant.
#[announce::announce]
pub enum Discr {
    A = N as isize,
}

// ... in the default of an associated const of a trait ...
#[announce::announce]
pub trait ConstDefault {
    const M: usize = N;
}

// ... in the default of a const parameter ...
#[announce::announce]
pub struct ConstParamDefault<const K: usize = N>;

// ... and in a const generic argument of a bound. A repeater does not help with any of them.
::announce::define_repeater!(pub Repeater);
#[announce::announce(repeater = crate::Repeater)]
pub struct ConstArgInBound<T: crate::Tr<{ N }>>(pub T);

// In a type (an array length, a const generic argument) the constant is carried with the type,
// which is a relative type: it needs a `repeater` (tests/defs/src/carried.rs), or an absolute path.
#[announce::announce]
pub struct RelLen(pub [u8; N]);
#[announce::announce]
pub struct ConstArg(pub crate::Buf<{ N }>);

// Absolute paths, `Self::..` and const parameters are fine.
#[announce::announce]
pub enum Fine {
    A = crate::N as isize,
}
#[announce::announce]
pub trait FineTrait {
    const A: usize;
    const B: usize = Self::A + 1;
    fn f<const K: usize>(&self) -> [u8; K];
}
#[announce::announce]
pub struct FineType(pub [u8; crate::N], pub crate::Buf<{ crate::N }>);

fn main() {}
