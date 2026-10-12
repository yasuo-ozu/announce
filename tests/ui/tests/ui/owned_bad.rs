// A check that owns `my_lib` sees the attributes and can reject them where the
// item is defined, also deep inside the item.
#[announce::announce(announce_test_derive::lib_check)]
pub struct S {
    #[my_lib(fine)]
    pub a: u8,
    #[my_lib(bad)]
    pub b: u8,
}

// A second check, which owns other names; the first one's attributes pass.
#[announce::announce(announce_test_derive::lib_check, announce_test_derive::other_check)]
pub enum E {
    A(#[my_lib(fine)] u8),
    B {
        #[other(forbidden)]
        x: u8,
    },
}

fn main() {}
