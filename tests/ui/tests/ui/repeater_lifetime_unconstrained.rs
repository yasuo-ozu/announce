// A method lifetime that only a relative type mentions: the copy would have `'a`
// in a projection only, and an implementation `fn i<'a>(&self, a: Wrap<'a>)` is
// rejected with E0195 (early- vs late-bound). type-leak 0.8 reports this at the
// definition; type-leak 0.7 accepted the definition, and the received item
// could not be implemented that way.
pub trait Repeater<const ID: u64, const I: usize, G: ?Sized> {
    type Type: ?Sized;
}
pub mod inner {
    pub struct Wrap<'a>(pub &'a u8);
}
use inner::Wrap;

#[announce::announce(repeater = crate::Repeater)]
pub trait Unconstrained {
    fn i<'a>(&self, a: Wrap<'a>) -> &'a u8;
}

// Fine: another argument keeps `'a` as written ...
#[announce::announce(repeater = crate::Repeater)]
pub trait SelfKeepsIt {
    fn i<'a>(&'a self, a: Wrap<'a>) -> &'a u8;
}

// ... also a plain reference.
#[announce::announce(repeater = crate::Repeater)]
pub trait ArgKeepsIt {
    fn i<'a>(&self, a: Wrap<'a>, b: &'a u8) -> &'a u8;
}

fn main() {}
