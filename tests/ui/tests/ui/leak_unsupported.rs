::announce::define_repeater!(pub Repeater);
pub mod inner {
    pub struct Arr<const N: usize>(pub [u8; N]);
    pub struct LocalL<'a>(pub &'a u8);
}
use inner::{Arr, LocalL};

pub type Idx = usize;

// A relative type that uses a const generic must have a primitive type: an
// alias (even an absolute one) cannot be encoded in the repeater's slots.
#[announce::announce(repeater = crate::Repeater)]
pub struct AliasConst<const N: crate::Idx>(pub Arr<N>);

// A const generic of an alias that no carried type uses is fine.
#[announce::announce(repeater = crate::Repeater)]
pub struct AliasConstUnused<const N: crate::Idx>(pub [u8; N], pub LocalL<'static>);

// `bool`, `char`, `u8`.. and `usize` const generics are all fine.
#[announce::announce(repeater = crate::Repeater)]
pub struct Primitive<const B: bool, const C: char>(pub inner::Arr<3>, pub [bool; 1]);

// A `for<..>` that holds a relative type is fine too: the impl declares the lifetime.
#[announce::announce(repeater = crate::Repeater)]
pub struct Hr {
    pub f: for<'b> fn(LocalL<'b>) -> u8,
}

fn main() {}
