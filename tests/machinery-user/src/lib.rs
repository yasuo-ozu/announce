//! Definitions made by a library's macros that call `announce::machinery`
//! (`announce-test-machinery`). This crate has no `announce` dependency.

use announce_test_machinery_lib::{pair, tracked};

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

// The attribute of the `announce` crate, through the library's re-export: this crate has
// no `announce` dependency, so `crate = ..` names the re-export.
#[announce_test_machinery_lib::announce::announce(crate = ::announce_test_machinery_lib::announce, macro_name = PlainAnnounce)]
pub struct PlainStruct(pub u8);

// A check that owns an attribute. The announcement removes it from the item it emits,
// and the check sees it.
#[tracked(::announce_test_machinery_lib::owner, macro_name = OwnedAnnounce)]
pub struct Owned {
    #[field_attr(x)]
    pub x: i32,
    pub y: i32,
}

#[announce_test_machinery_lib::announce::announce(crate = ::announce_test_machinery_lib::announce, macro_name = OwnedPlainAnnounce, ::announce_test_machinery_lib::owner)]
pub struct OwnedPlain(#[field_attr(z)] pub u8);

// Doc attributes: removed from the payload by default, `docs = true`
// (`announce::machinery::Options::docs`) keeps them. Through the macro of the library
// and through the attribute.
/// Doc of Docs.
#[tracked(macro_name = DocsAnnounce)]
pub struct Docs {
    /// Doc of x.
    pub x: u8,
}

/// Doc of DocsKept.
#[tracked(macro_name = DocsKeptAnnounce, docs = true)]
pub struct DocsKept {
    /// Doc of x.
    pub x: u8,
}

/// Doc of DocsPlain.
#[announce_test_machinery_lib::announce::announce(crate = ::announce_test_machinery_lib::announce, macro_name = DocsPlainAnnounce)]
pub struct DocsPlain {
    /// Doc of x.
    pub x: u8,
}

// `tracked` calls `announce::machinery::announce`, which does what `#[announce]` does:
// the item comes back with the runner, which holds the options, as its last attribute.
// The other announcement expands next and merges its options into the runner. This crate
// has no `announce` dependency, so the attribute names the library's re-export and the
// crate in `crate = ..`.
#[tracked(meta = test_lib::tracked)]
#[announce_test_machinery_lib::announce::announce(crate = ::announce_test_machinery_lib::announce, meta = test_lib::attr)]
pub struct MergedBelow {
    pub a: u8,
}

// Here the attribute expands first and puts the runner on the item; `tracked` then
// announces as well, and its options are merged into the runner.
#[announce_test_machinery_lib::announce::announce(crate = ::announce_test_machinery_lib::announce, meta = test_lib::attr)]
#[tracked(meta = test_lib::tracked)]
pub struct MergedAbove {
    pub a: u8,
}
