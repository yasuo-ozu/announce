#[announce::announce(export_name = Hidden)]
pub struct A;

#[announce::announce(marker = false)]
pub struct B;

// `mock` and `emit_item` do not apply to the derive.
#[derive(announce::Announce)]
#[announce(emit_item = false)]
pub struct C;

fn main() {}
