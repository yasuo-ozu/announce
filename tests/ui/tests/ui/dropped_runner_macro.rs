// The same for the items that cannot be named without reading their generics, and for the
// other kinds of item: the sentinel is a call of the trampoline, which is not there.
#[::announce::announce]
#[announce_test_derive::drop_attrs]
pub trait Tr {}

#[::announce::announce]
#[announce_test_derive::drop_attrs]
pub struct Generic<T>(pub T);

#[::announce::announce(macro_name = Renamed)]
#[announce_test_derive::drop_attrs]
pub mod m {}

#[::announce::announce(emit_item = false)]
#[announce_test_derive::drop_attrs]
pub struct NotEmitted(pub u8);

fn main() {}
