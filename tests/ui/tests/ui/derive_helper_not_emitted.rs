// The helper attribute of a derive cannot make the announcement not emit the item: the
// derive says so, and the announcement below it, which takes the helper over, does not
// drop the item silently on its own.
#[derive(announce::Announce)]
#[announce(emit_item = false)]
#[announce::announce]
pub struct Kept(pub u8);

fn main() {
    let _ = Kept(0);
}
