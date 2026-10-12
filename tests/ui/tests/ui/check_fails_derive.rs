// The checks of `#[derive(Announce)]` run too, at the definition.
#[derive(announce::Announce)]
#[announce(announce_test_derive::no_tuple(needs named fields))]
pub struct Pair(u8, u8);

fn main() {}
