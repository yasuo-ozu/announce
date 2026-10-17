// A macro below `#[announce]` that returns the item without its outer attributes drops the
// runner, so nothing announces the item. The sentinel that the announcement emits next to
// the item fails at the definition: `Announce` is not implemented for a struct, enum or
// union without generics.
#[::announce::announce(meta = my_lib::a)]
#[announce_test_derive::drop_attrs]
pub struct S(pub u8);

#[::announce::announce]
#[announce_test_derive::drop_attrs]
pub enum E {
    A,
}

fn main() {}
