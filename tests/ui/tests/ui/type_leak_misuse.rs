::announce::define_repeater!(pub Repeater);

// Relative repeater paths would resolve at the caller.
#[announce::announce(repeater = Repeater)]
pub struct S;

fn main() {}
