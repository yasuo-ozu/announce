// `mock` and `emit_item = false` remove the `Announce` impl, which the first announcement of
// a struct, enum or union without generics checks. They belong to the first announcement,
// not to a later one.
#[announce::announce]
#[announce::announce(emit_item = false)]
pub struct A(pub u8);

#[announce::announce]
#[announce::announce(mock = my_lib::Real)]
pub enum B {
    X,
}

// The first announcement has it: fine, and so is a trait or a generic struct, which have no
// impl to check.
#[announce::announce(emit_item = false)]
#[announce::announce]
pub struct C(pub u8);

#[announce::announce]
#[announce::announce(emit_item = false)]
pub trait D {}

#[announce::announce]
#[announce::announce(emit_item = false)]
pub struct E<T>(pub T);

fn main() {
    let _ = (A(0), B::X);
}
