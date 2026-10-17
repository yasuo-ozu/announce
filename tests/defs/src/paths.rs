//! The runner is told by its name (`_actually_run_announce`), never by the path `announce`.
//! `tests/user/tests/paths.rs` reads the META of the trampolines made here. Nothing here
//! may be an error: a second trampoline or `Announce` impl would not compile, and a
//! check that ran twice for one item would define its constant twice.
//!
//! - other crates' attributes that are called `announce` are not announcements, and keep
//!   working next to ours;
//! - a renamed import (`use announce::announce as ann`) is an announcement;
//! - derives of other traits, above and below the attributes;
//! - checks that own attributes, next to other announcements, asked once;
//! - items that are not emitted (`emit_item = false`, `mock`);
//! - macros that announce what they produce.

#![allow(dead_code)]

use announce_test_derive::{
    count_query, emit_announce, lib_check, other_check, passthrough, record_attrs, tick,
};

// ---- attributes of other crates that are named `announce` -------------------

/// The attribute of another crate has the path `announce_test_derive::announce`, which
/// ends in `announce`: `#[announce]` leaves it alone, and it makes its constant.
#[::announce::announce(meta = paths::a)]
#[announce_test_derive::announce(level = 2)]
pub struct ForeignBelow(pub u8);

#[announce_test_derive::announce(level = 3)]
#[::announce::announce(meta = paths::a)]
pub struct ForeignAbove(pub u8);

/// Our two announcements, with the foreign one between them.
#[::announce::announce(meta = paths::a)]
#[announce_test_derive::announce(level = 4)]
#[::announce::announce(meta = paths::b)]
pub struct ForeignBetween(pub u8);

/// Two foreign attributes and a doc attribute (the docs are an attribute as well).
/// Ours is the only announcement.
#[announce_test_derive::announce(level = 5)]
#[::announce::announce(meta = paths::a)]
#[announce_test_derive::announce(level = 6)]
pub struct ForeignTwice(pub u8);

/// An attribute with options `#[announce]` does not know, in front of ours: if it were
/// merged into the foreign one it would be an error in the foreign macro.
#[announce_test_derive::announce(level = 7)]
#[::announce::announce(layout = direct, meta = paths::a)]
#[announce_test_derive::announce(level = 8)]
pub struct ForeignWithOptions(pub u8);

/// The foreign attribute imported, and written as the bare `#[announce(level = N)]`,
/// `#[announce]` or `#[announce(meta = ..)]`.
pub mod bare {
    use announce_test_derive::announce;

    #[::announce::announce(meta = paths::a)]
    #[announce(level = 9)]
    pub struct ForeignBare(pub u8);

    /// A bare `#[announce]` with no arguments, and one with arguments that are options of
    /// ours (`meta = ..`): rustc expands the macro that the name means here, which is
    /// the foreign one. `#[announce]` neither takes it over nor deletes it.
    #[::announce::announce(meta = paths::a)]
    #[announce]
    pub struct ForeignBareNoArgs(pub u8);

    #[::announce::announce(meta = paths::a)]
    #[announce(meta = paths::foreign)]
    pub struct ForeignBareMeta(pub u8);

    #[announce]
    #[::announce::announce(meta = paths::a)]
    pub struct ForeignBareNoArgsAbove(pub u8);

    #[::announce::announce(meta = paths::a)]
    #[announce]
    #[::announce::announce(meta = paths::b)]
    pub struct ForeignBareNoArgsBetween(pub u8);

    #[announce(level = 10)]
    #[::announce::announce(meta = paths::a)]
    pub struct ForeignBareAbove(pub u8);

    #[::announce::announce(meta = paths::a)]
    #[announce(level = 11)]
    #[::announce::announce(meta = paths::b)]
    pub struct ForeignBareBetween(pub u8);
}

// ---- a renamed import is an announcement ------------------------------------

pub mod renamed {
    use ::announce::announce as ann;

    #[ann]
    #[ann(meta = paths::b)]
    pub struct Twice(pub u8);

    #[ann(meta = paths::a)]
    #[ann(meta = paths::b)]
    #[ann(meta = paths::c)]
    pub struct Thrice(pub u8);

    /// With the attribute under its own name as well, and an attribute of another
    /// crate called `announce`.
    #[ann(meta = paths::a)]
    #[::announce::announce(meta = paths::b)]
    #[announce_test_derive::announce(level = 12)]
    pub struct Mixed(pub u8);
}

// ---- derives of other traits --------------------------------------------------
//
// The first attribute that expands puts the runner below the derives, and the others
// are merged into it, wherever they stand.

pub mod derives {
    // Imported, so that the bare `#[announce(..)]` is the attribute macro.
    #[allow(unused_imports)]
    use announce::announce;

    /// The attribute is above the derive: it puts the runner on the item, below the derive.
    #[::announce::announce(meta = paths::a)]
    #[derive(Debug)]
    pub struct AttrAbove(pub u8);

    #[::announce::announce(meta = paths::a)]
    #[derive(Debug)]
    #[announce(meta = paths::b)]
    pub struct AttrAboveBare(pub u8);

    /// The derive is above the attribute.
    #[derive(Debug)]
    #[::announce::announce(meta = paths::a)]
    pub struct DeriveAbove(pub u8);

    #[derive(Debug)]
    #[announce(meta = paths::b)]
    #[::announce::announce(meta = paths::a)]
    pub struct DeriveAboveBare(pub u8);

    /// Two bare attributes below the derive, and an attribute above it.
    #[::announce::announce(meta = paths::a)]
    #[derive(Debug)]
    #[announce(meta = paths::b1)]
    #[announce(meta = paths::b2)]
    pub struct AttrAboveTwoBare(pub u8);

    /// Two attributes and a derive, in the three places.
    #[::announce::announce(meta = paths::a)]
    #[::announce::announce(meta = paths::b)]
    #[derive(Debug)]
    #[announce(meta = paths::c)]
    pub struct TwoAttrsAbove(pub u8);

    #[derive(Debug)]
    #[announce(meta = paths::c)]
    #[::announce::announce(meta = paths::a)]
    #[::announce::announce(meta = paths::b)]
    pub struct TwoAttrsBelow(pub u8);

    #[::announce::announce(meta = paths::a)]
    #[derive(Debug)]
    #[announce(meta = paths::c)]
    #[::announce::announce(meta = paths::b)]
    pub struct DeriveBetween(pub u8);

    /// Several derives next to ours.
    #[::announce::announce(meta = paths::a)]
    #[derive(Clone, Debug)]
    pub struct OtherDerives(pub u8);
}

// ---- checks that own attributes ---------------------------------------------

/// `record_attrs` owns `my_lib`, which stands between two announcements. The other
/// announcement has a check as well, `tick`, that leaves a constant.
#[::announce::announce(record_attrs, meta = paths::a)]
#[my_lib(on_item)]
#[::announce::announce(tick(second), meta = paths::b)]
pub struct OwnedAndAnother(pub u8);

#[::announce::announce(tick(first), meta = paths::a)]
#[::announce::announce(record_attrs, meta = paths::b)]
#[my_lib(on_item)]
pub struct AnotherAndOwned(pub u8);

/// Two checks that own different attributes. Neither stands above the announcement it
/// belongs to when the item is compiled.
#[::announce::announce(lib_check, meta = paths::a)]
#[::announce::announce(other_check, meta = paths::b)]
#[my_lib(one)]
#[other(two)]
#[tag(three)]
pub struct TwoOwners(pub u8);

/// An unknown macro between, and a macro that announces with a check of its own.
#[::announce::announce(record_attrs, meta = paths::a)]
#[passthrough]
#[emit_announce(tick(emitted))]
#[my_lib(on_item)]
pub struct OwnedAndEmitted(pub u8);

/// The query is sent once for each announcement. `count_query` owns the name
/// `queried_<n>`, where `n` counts the queries it has answered for the item: `queried_0`
/// is the first and only query of an announcement.
#[::announce::announce(count_query)]
#[passthrough]
pub struct QueriedOnce(pub u8);

#[::announce::announce(count_query)]
pub struct QueriedOnceNoAttributes(pub u8);

/// Two announcements, two queries: `queried_0` and `queried_1`, and no more.
#[::announce::announce(count_query)]
#[::announce::announce(count_query, meta = paths::b)]
#[passthrough]
pub struct QueriedTwice(pub u8);

#[::announce::announce(count_query)]
#[derive(Debug)]
#[::announce::announce(meta = paths::b)]
pub struct QueriedWithDerive(pub u8);

#[emit_announce(count_query)]
#[passthrough]
#[emit_announce(count_query, meta = paths::b)]
pub struct QueriedEmitted(pub u8);

#[::announce::announce(count_query, record_attrs)]
#[my_lib(on_item)]
#[passthrough]
#[::announce::announce(tick(shared), meta = paths::b)]
pub struct QueriedAndOwned(pub u8);

// ---- items that are not emitted ---------------------------------------------

/// The item is not emitted: one trampoline, and no item. The runner drops it, so the
/// macros below see it first (a derive that implements a trait for it fails: see the UI
/// test `not_emitted_derive`).
#[::announce::announce(emit_item = false, meta = paths::a)]
#[passthrough]
pub struct NotEmittedPassthrough(pub u8);

/// The announcements below are merged into the one that is not emitted, and the other
/// way round, and the trampoline is named once.
#[::announce::announce(emit_item = false, macro_name = NotEmittedBelow, meta = paths::a)]
#[::announce::announce(meta = paths::b)]
pub struct NotEmittedBelow(pub u8);

/// `emit_item = false` is on a later announcement of a trait here. For a struct, enum or
/// union without generics it has to be on the first (see the UI test `merge_not_first`).
#[::announce::announce(meta = paths::b)]
#[::announce::announce(emit_item = false, meta = paths::a)]
pub trait NotEmittedAbove {}

#[::announce::announce(meta = paths::a)]
#[passthrough]
#[::announce::announce(emit_item = false, meta = paths::b)]
#[passthrough]
pub trait NotEmittedBetween {}

/// A mock is compiled in a hidden module, and the item is not emitted.
pub trait Real {
    fn f(&self) -> u8;
}

#[::announce::announce(mock = crate::paths::Real, meta = paths::a)]
#[::announce::announce(meta = paths::b)]
pub trait MockedBelow {
    fn f(&self) -> u8;
}

#[::announce::announce(meta = paths::b)]
#[::announce::announce(mock = crate::paths::Real, meta = paths::a)]
pub trait MockedAbove {
    fn f(&self) -> u8;
}

/// Not emitted, with a check that owns an attribute.
#[::announce::announce(emit_item = false, record_attrs)]
#[my_lib(never_compiled)]
pub struct NotEmittedOwned(pub u8);

/// Another announcement with a check that owns an attribute, below the one that is not
/// emitted.
#[::announce::announce(emit_item = false, record_attrs)]
#[::announce::announce(other_check, meta = paths::b)]
#[my_lib(never_compiled)]
#[other(never_compiled)]
pub struct NotEmittedTwoOwners(pub u8);

// ---- macros that announce what they produce ---------------------------------

/// The user's announcement puts the runner on the item first; the macro below it announces
/// into the item that has the runner already.
#[::announce::announce(meta = paths::a)]
#[emit_announce(meta = paths::b)]
pub struct IntoRunner(pub u8);

/// Two macros that do not know each other announce, with a macro that does not
/// announce between them.
#[emit_announce(meta = paths::a)]
#[passthrough]
#[emit_announce(meta = paths::b)]
pub struct TwoUnknown(pub u8);

#[::announce::announce(meta = paths::u)]
#[emit_announce(meta = paths::a)]
#[passthrough]
#[emit_announce(meta = paths::b)]
pub struct TwoUnknownAndUser(pub u8);

#[emit_announce(meta = paths::a)]
#[::announce::announce(meta = paths::u)]
#[emit_announce(meta = paths::b)]
#[passthrough]
pub struct UserBetweenTwoUnknown(pub u8);
