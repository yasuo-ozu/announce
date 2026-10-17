// `emit_item` and `mock` are merged like the other options: the value that is set wins,
// and two values that differ are one error, at the announcement that was expanded.
#[announce::announce(emit_item = false)]
#[announce::announce(emit_item = true)]
pub struct EmitItem(pub u8);

pub trait Real {}

#[announce::announce(mock = Real)]
#[announce::announce(mock = Other)]
pub trait Mocked {}

fn main() {}
