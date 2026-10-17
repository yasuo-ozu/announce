// Errors of `announce::machinery` come back as `compile_error!` tokens.
use announce_test_machinery_lib::tracked;

#[tracked(macro_nam = X)]
pub struct A {
    pub a: u8,
}

#[tracked]
fn not_an_item() {}

fn main() {}
