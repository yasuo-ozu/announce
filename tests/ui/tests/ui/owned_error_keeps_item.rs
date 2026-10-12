mod m {
    pub struct Local;
}
use m::Local;

// The type-context error is the only error: the item (without its owned
// attributes) is still emitted, so the use of `S` below is fine.
#[announce::announce(announce_test_derive::lib_check)]
pub struct S {
    #[my_lib(skip)]
    pub a: Local,
}

fn main() {
    let _ = S { a: Local };
}
