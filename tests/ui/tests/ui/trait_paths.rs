pub trait Base {}

// Relative trait paths cannot be covered by a repeater: they must be absolute.
#[announce::announce]
pub trait Sub: Base {}

// `std::` without the leading `::` is relative.
#[announce::announce]
pub trait Show {
    fn show<T: std::fmt::Display>(&self, t: T);
}

#[announce::announce]
pub struct Unsized<T: ?Sized>(pub ::std::boxed::Box<T>);

fn main() {}
