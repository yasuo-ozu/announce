// The checks run at the definition when the item has a derive of another trait too: the
// runner makes the trampoline, after the derive.
#[announce::announce(announce_test_derive::no_tuple(needs named fields))]
#[derive(Debug)]
pub struct Pair(u8, u8);

fn main() {}
