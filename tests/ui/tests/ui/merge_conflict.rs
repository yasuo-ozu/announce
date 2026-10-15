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

// With a derive, the other value is in its helper attribute.
#[announce::announce(layout = single)]
#[derive(announce::Announce)]
#[announce(layout = direct)]
pub struct WithDerive(pub u8);

// Two helper attributes of one derive are merged as well.
#[derive(announce::Announce)]
#[announce(macro_name = A)]
#[announce(macro_name = A, meta = my_lib::info)]
#[announce(macro_name = B)]
pub struct Helpers(pub u8);

#[derive(announce::Announce)]
#[announce(meta = my_lib::info)]
#[announce(macro_name = HelperAlt)]
pub struct UnsetHelper(pub u8);

// A derive cannot take `mock` or `emit_item = false`: it does not change the item.
#[announce::announce(mock = my_lib::Real)]
#[derive(announce::Announce)]
pub struct NoMock(pub u8);

#[announce::announce(emit_item = false)]
#[derive(announce::Announce)]
pub struct NoEmitItem(pub u8);

fn main() {
    let _ = (
        Attrs(0),
        WithDerive(0),
        Helpers(0),
        UnsetHelper(0),
        NoMock(0),
        NoEmitItem(0),
    );
}
