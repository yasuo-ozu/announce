pub trait Base {}

// Relative trait paths cannot be covered by a repeater: they must be absolute.
#[announce::announce]
pub trait Sub: Base {}

// `std::` without the leading `::` is relative.
#[announce::announce]
pub trait Show {
    fn show<T: std::fmt::Display>(&self, t: T);
}

// `Sized` and `?Sized` may be written bare (no error for this one)...
#[announce::announce]
pub struct Unsized<T: ?Sized>(pub ::std::boxed::Box<T>);

#[announce::announce]
pub trait SizedBounds: Sized {
    fn f(&self) -> u8
    where
        Self: Sized;
    fn g<U: ?Sized + Sized>(&self, u: &U);
}

// ... but every other prelude trait needs `allow_std_prelude = true`.
#[announce::announce]
pub struct Cloned<T: Clone>(pub T);

// A path that only ends in `Sized` is not the prelude's.
#[announce::announce]
pub struct Qualified<T: marker::Sized>(pub T);

fn main() {}
