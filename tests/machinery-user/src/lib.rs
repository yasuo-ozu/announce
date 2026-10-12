//! Definitions made by a library's macros that call `announce::machinery`
//! (`announce-test-machinery`). This crate has no `announce` dependency.

use announce_test_machinery_lib::{pair, tracked, Plain, Tracked};

// The macro adds a field and the struct is announced as rewritten. The `cfg`
// is evaluated here, as for `#[announce]`.
#[tracked]
pub struct Point {
    pub x: i32,
    #[cfg(any())]
    pub gone: i32,
    #[cfg(not(any()))]
    pub kept: i32,
}

// Options are passed through.
#[tracked(macro_name = NamedAnnounce, meta = test_lib::info(1))]
pub struct Named {
    pub a: u8,
}

pair!(Left, Right);

#[derive(Tracked)]
#[tracked(meta = test_lib::derived)]
pub struct Derived {
    pub a: u8,
}

#[derive(Plain)]
#[announce(crate = ::announce_test_machinery_lib::announce, macro_name = PlainAnnounce)]
pub struct PlainStruct(pub u8);

// A check that owns an attribute. The attribute route removes it from
// the item it emits, and the check sees it.
#[tracked(::announce_test_machinery_lib::owner, macro_name = OwnedAnnounce)]
pub struct Owned {
    #[field_attr(x)]
    pub x: i32,
    pub y: i32,
}

// The derive cannot remove attributes from its input: the library's own derive
// declares `field_attr`. The query only lists the name in META.
#[derive(Tracked)]
#[tracked(::announce_test_machinery_lib::owner, macro_name = OwnedDerivedAnnounce)]
pub struct OwnedDerived {
    #[field_attr(y)]
    pub y: u8,
}

#[derive(Plain)]
#[announce(crate = ::announce_test_machinery_lib::announce, macro_name = OwnedPlainAnnounce, ::announce_test_machinery_lib::owner)]
pub struct OwnedPlain(#[field_attr(z)] pub u8);
