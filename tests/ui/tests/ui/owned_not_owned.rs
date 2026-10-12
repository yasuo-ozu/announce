// An attribute that no check owns is rustc's: it is not declared by any macro,
// so it is rejected, with or without checks.
#[announce::announce]
pub struct A {
    #[my_lib(x)]
    pub a: u8,
}

#[announce::announce(announce_test_derive::no_tuple)]
pub struct B {
    #[my_lib(x)]
    pub b: u8,
}

// The item's own attribute that is not owned either.
#[announce::announce(announce_test_derive::lib_check)]
pub struct C {
    #[other(x)]
    pub c: u8,
}

fn main() {}
