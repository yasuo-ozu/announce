// The announcements of one item are merged into one trampoline, so an option that
// has one value must have one value. The error is one, at the attribute that was
// expanded, and the item is still there.
#[announce::announce(macro_name = A)]
#[announce::announce(macro_name = B)]
pub struct Attrs(pub u8);

// The name of the trampoline is `macro_name`, else the name of the item: a name that
// is set in one announcement and not in the other is a different name (`Parse` and
// `ParseAlt` here).
#[announce::announce]
#[announce::announce(macro_name = ParseAlt)]
pub trait Parse {}

// With a derive of another trait between them.
#[announce::announce(layout = single)]
#[derive(Debug)]
#[announce::announce(layout = direct)]
pub struct WithDerive(pub u8);

// Three announcements: the third disagrees with the merge of the first two.
#[announce::announce(macro_name = C)]
#[announce::announce(macro_name = C, meta = my_lib::info)]
#[announce::announce(macro_name = D)]
pub struct Three(pub u8);

fn main() {
    let _ = (Attrs(0), WithDerive(0), Three(0));
}
