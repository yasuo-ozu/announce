pub const N: usize = 3;
pub struct Buf<const M: usize>;

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

// ... in a const generic argument ...
#[announce::announce]
pub struct ConstArg(pub crate::Buf<{ N }>);

// ... and in the default of a const parameter.
#[announce::announce]
pub struct ConstParamDefault<const K: usize = N>;

// In a bound a repeater does not help (in a type it carries the constant with it).
pub trait Tr<const M: usize> {}
::announce::define_repeater!(pub Repeater);
#[announce::announce(repeater = crate::Repeater)]
pub struct ConstArgInBound<T: crate::Tr<{ N }>>(pub T);

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

fn main() {}
