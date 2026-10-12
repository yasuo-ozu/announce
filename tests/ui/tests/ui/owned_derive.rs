// `#[derive(Announce)]` cannot remove an attribute from its input, so a library
// that owns attributes on a derive declares them with its own derive
// (`attributes(..)`, see `tests/machinery`). The plain derive declares only
// `announce`.
#[derive(announce::Announce)]
#[announce(announce_test_derive::lib_check)]
pub struct S {
    #[my_lib(x)]
    pub a: u8,
}

fn main() {}
