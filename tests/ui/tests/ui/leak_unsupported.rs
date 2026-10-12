pub trait Repeater<const ID: u64, const I: usize, G: ?Sized> {
    type Type: ?Sized;
}
pub mod inner {
    pub struct Flag<const B: bool>;
    pub struct LocalL<'a>(pub &'a u8);
}
use inner::{Flag, LocalL};

// A relative type that mentions a const parameter must have a `usize` one.
#[announce::announce(repeater = crate::Repeater)]
pub struct BoolConst<const B: bool>(pub Flag<B>);

// A relative type that names a lifetime bound by `for<..>` of the item.
#[announce::announce(repeater = crate::Repeater)]
pub struct Hr {
    pub f: for<'b> fn(LocalL<'b>) -> u8,
}

// A `for<..>` that only holds absolute types is fine.
#[announce::announce(repeater = crate::Repeater)]
pub struct HrFine {
    pub f: for<'b> fn(&'b u8) -> &'b u8,
}

fn main() {}
