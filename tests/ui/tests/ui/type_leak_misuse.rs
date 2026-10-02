pub trait Repeater<const ID: u64, const I: usize, G: ?Sized> { type Type: ?Sized; }

// Relative repeater paths would resolve at the caller.
#[announce::announce(repeater = Repeater)]
pub struct S;

fn main() {}
