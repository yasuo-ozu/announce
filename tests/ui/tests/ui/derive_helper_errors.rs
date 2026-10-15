// Several `#[announce(..)]` helper attributes are one announcement: see
// `merge_conflict.rs` for the errors.

// `mock` does not apply to the derive: the item is emitted as written.
#[derive(announce::Announce)]
#[announce(mock = my_lib::Real)]
pub union Mocked {
    pub a: u32,
}

// Unknown options are reported as for the attribute.
#[derive(announce::Announce)]
#[announce(macro_nam = X)]
pub enum Unknown {
    A,
}

// `repeater` is accepted on the derive: with no relative type the table is empty.
::announce::define_repeater!(pub Repeater);
#[derive(announce::Announce)]
#[announce(repeater = crate::Repeater)]
pub struct Fine(pub u8);

fn main() {}
