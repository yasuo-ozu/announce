//! The definitions behind the wire format example in the docs of
//! `announce::helper::Input`. The test that captures the chain is
//! `tests/user/tests/wire.rs`.

use announce::announce;

::announce::define_repeater!(pub Repeater);

mod inner {
    pub struct Meters(pub f64);
}
use inner::Meters;

/// A point.
///
/// A check (that owns the attribute `my_lib`), a `meta = ..` entry and a doc
/// comment: the docs are not in the payload, the owned attribute is.
#[announce(announce_test_derive::lib_check, meta = crate::info(1))]
pub struct Point {
    /// The x coordinate.
    #[my_lib(skip)]
    pub x: u8,
    pub y: u8,
}

/// A length: `Meters` is private to this crate, so the definition has a
/// `repeater` and its META gets a `type_leak(..)` element.
#[announce(repeater = crate::wire::Repeater)]
pub struct Length(pub Meters);
