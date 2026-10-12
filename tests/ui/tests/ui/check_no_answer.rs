// A proc-macro check written before the attribute query: it parses
// `CheckInput` and does not answer the query, so `#[announce]` cannot go on.
#[announce::announce(announce_test_derive::old_style_check)]
pub struct S {
    pub a: u8,
}

fn main() {}
