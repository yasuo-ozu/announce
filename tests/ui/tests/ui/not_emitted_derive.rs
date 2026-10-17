// The announcement with `emit_item = false` drops the item when the runner expands, after
// the macros below it have expanded on it. A derive that implements a trait for the item
// fails then: the item is gone.
#[announce::announce(emit_item = false)]
#[derive(Debug)]
pub struct Dropped(pub u8);

fn main() {}
