// `#[derive(Announce)]` takes one `#[announce(..)]` helper attribute.
#[derive(announce::Announce)]
#[announce(meta = my_lib::info(1))]
#[announce(macro_name = Other)]
pub struct Twice;

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
pub trait Repeater<const ID: u64, const I: usize, G: ?Sized> {
    type Type: ?Sized;
}
#[derive(announce::Announce)]
#[announce(repeater = crate::Repeater)]
pub struct Fine(pub u8);

fn main() {}
