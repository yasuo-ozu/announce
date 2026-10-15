//! Announcements are told by their flag (`__announce_deferred`), not by their path.
//! `tests/user/tests/flag.rs` reads the META of the trampolines made here. Nothing here
//! may be an error: a second trampoline or `Announce` impl would not compile, and a
//! check that ran twice for one item would define its constant twice.
//!
//! - other crates' attributes that are called `announce` are not announcements, and keep
//!   working next to ours;
//! - a renamed import (`use announce::announce as ann`) is an announcement;
//! - derives, in the orders that work;
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
#[::announce::announce(meta = flag::a)]
#[announce_test_derive::announce(level = 2)]
pub struct ForeignBelow(pub u8);

#[announce_test_derive::announce(level = 3)]
#[::announce::announce(meta = flag::a)]
pub struct ForeignAbove(pub u8);

/// Our two announcements, with the foreign one between them.
#[::announce::announce(meta = flag::a)]
#[announce_test_derive::announce(level = 4)]
#[::announce::announce(meta = flag::b)]
pub struct ForeignBetween(pub u8);

/// Two foreign attributes and a doc attribute (the docs are an attribute as well).
/// Ours is the only announcement.
#[announce_test_derive::announce(level = 5)]
#[::announce::announce(meta = flag::a)]
#[announce_test_derive::announce(level = 6)]
pub struct ForeignTwice(pub u8);

/// An attribute with options `#[announce]` does not know, in front of ours: if it were
/// merged into the foreign one it would be an error in the foreign macro.
#[announce_test_derive::announce(level = 7)]
#[::announce::announce(layout = direct, meta = flag::a)]
#[announce_test_derive::announce(level = 8)]
pub struct ForeignWithOptions(pub u8);

/// The foreign attribute imported, and written as the bare `#[announce(level = N)]`.
pub mod bare {
    use announce_test_derive::announce;

    #[::announce::announce(meta = flag::a)]
    #[announce(level = 9)]
    pub struct ForeignBare(pub u8);

    #[announce(level = 10)]
    #[::announce::announce(meta = flag::a)]
    pub struct ForeignBareAbove(pub u8);

    #[::announce::announce(meta = flag::a)]
    #[announce(level = 11)]
    #[::announce::announce(meta = flag::b)]
    pub struct ForeignBareBetween(pub u8);
}

// ---- a renamed import is an announcement ------------------------------------

pub mod renamed {
    use ::announce::announce as ann;

    #[ann]
    #[ann(meta = flag::b)]
    pub struct Twice(pub u8);

    #[ann(meta = flag::a)]
    #[ann(meta = flag::b)]
    #[ann(meta = flag::c)]
    pub struct Thrice(pub u8);

    /// With the attribute under its own name as well, and an attribute of another
    /// crate called `announce`.
    #[ann(meta = flag::a)]
    #[::announce::announce(meta = flag::b)]
    #[announce_test_derive::announce(level = 12)]
    pub struct Mixed(pub u8);
}

// ---- derives ------------------------------------------------------------------
//
// A derive stands aside for an announcement below it that has the flag or whose path
// is qualified, and the announcement takes its helper attributes over.

pub mod derives {
    // Imported, so that `#[announce(..)]` is the attribute macro as well as the helper of
    // the derive (which only the helper is, below it).
    #[allow(unused_imports)]
    use announce::announce;

    /// The attribute is above the derive: it moves below it, and the derive sees the flag.
    #[::announce::announce(meta = flag::a)]
    #[derive(::announce::Announce)]
    pub struct AttrAbove(pub u8);

    #[::announce::announce(meta = flag::a)]
    #[derive(::announce::Announce, Debug)]
    #[announce(meta = flag::h)]
    pub struct AttrAboveHelper(pub u8);

    /// The derive is above the attribute.
    #[derive(::announce::Announce)]
    #[::announce::announce(meta = flag::a)]
    pub struct DeriveAbove(pub u8);

    #[derive(::announce::Announce)]
    #[announce(meta = flag::h)]
    #[::announce::announce(meta = flag::a)]
    pub struct DeriveAboveHelper(pub u8);

    /// Two helpers of the derive, and an attribute above it.
    #[::announce::announce(meta = flag::a)]
    #[derive(::announce::Announce)]
    #[announce(meta = flag::h1)]
    #[announce(meta = flag::h2)]
    pub struct AttrAboveTwoHelpers(pub u8);

    /// Two attributes and a derive, in the three places.
    #[::announce::announce(meta = flag::a)]
    #[::announce::announce(meta = flag::b)]
    #[derive(::announce::Announce)]
    #[announce(meta = flag::h)]
    pub struct TwoAttrsAbove(pub u8);

    #[derive(::announce::Announce)]
    #[announce(meta = flag::h)]
    #[::announce::announce(meta = flag::a)]
    #[::announce::announce(meta = flag::b)]
    pub struct TwoAttrsBelow(pub u8);

    #[::announce::announce(meta = flag::a)]
    #[derive(::announce::Announce)]
    #[announce(meta = flag::h)]
    #[::announce::announce(meta = flag::b)]
    pub struct DeriveBetween(pub u8);

    /// A derive of another trait next to ours.
    #[::announce::announce(meta = flag::a)]
    #[derive(Clone, ::announce::Announce, Debug)]
    pub struct OtherDerives(pub u8);
}

// ---- checks that own attributes ---------------------------------------------

/// `record_attrs` owns `my_lib`, which stands between two announcements. The other
/// announcement has a check as well, `tick`, that leaves a constant.
#[::announce::announce(record_attrs, meta = flag::a)]
#[my_lib(on_item)]
#[::announce::announce(tick(second), meta = flag::b)]
pub struct OwnedAndAnother(pub u8);

#[::announce::announce(tick(first), meta = flag::a)]
#[::announce::announce(record_attrs, meta = flag::b)]
#[my_lib(on_item)]
pub struct AnotherAndOwned(pub u8);

/// Two checks that own different attributes. Neither stands above the announcement it
/// belongs to when the item is compiled.
#[::announce::announce(lib_check, meta = flag::a)]
#[::announce::announce(other_check, meta = flag::b)]
#[my_lib(one)]
#[other(two)]
#[tag(three)]
pub struct TwoOwners(pub u8);

/// An unknown macro between, and a macro that announces with a check of its own.
#[::announce::announce(record_attrs, meta = flag::a)]
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
#[::announce::announce(count_query, meta = flag::b)]
#[passthrough]
pub struct QueriedTwice(pub u8);

#[::announce::announce(count_query)]
#[derive(::announce::Announce)]
#[announce(meta = flag::h)]
pub struct QueriedWithDerive(pub u8);

#[emit_announce(count_query)]
#[passthrough]
#[emit_announce(count_query, meta = flag::b)]
pub struct QueriedEmitted(pub u8);

#[::announce::announce(count_query, record_attrs)]
#[my_lib(on_item)]
#[passthrough]
#[::announce::announce(tick(shared), meta = flag::b)]
pub struct QueriedAndOwned(pub u8);

// ---- items that are not emitted ---------------------------------------------

/// The item is not emitted, so the derive below must not see it: the announcement stays
/// above the derive. If it moved below, `Debug` would be implemented for a type that
/// does not exist.
#[::announce::announce(emit_item = false)]
#[derive(Debug)]
pub struct NotEmittedDerive(pub u8);

#[::announce::announce(emit_item = false, meta = flag::a)]
#[passthrough]
pub struct NotEmittedPassthrough(pub u8);

/// A mock is compiled in a hidden module.
#[::announce::announce(mock = crate::flag::MockedDerive, meta = flag::a)]
#[derive(Debug)]
pub struct MockedDerive(pub u8);

/// Another announcement above it moves below it and folds in: one trampoline, and the
/// item is not emitted.
#[::announce::announce(meta = flag::b)]
#[::announce::announce(emit_item = false, meta = flag::a)]
pub struct NotEmittedAfterOther(pub u8);

#[::announce::announce(meta = flag::b)]
#[::announce::announce(mock = crate::flag::MockedAfterOther, meta = flag::a)]
pub struct MockedAfterOther(pub u8);

/// Not emitted, with a check that owns an attribute.
#[::announce::announce(emit_item = false, record_attrs)]
#[my_lib(never_compiled)]
pub struct NotEmittedOwned(pub u8);

// ---- macros that announce what they produce ---------------------------------

/// The user's announcement moves first; the macro below it announces into the item
/// that has the flagged announcement already.
#[::announce::announce(meta = flag::a)]
#[emit_announce(meta = flag::b)]
pub struct IntoFlagged(pub u8);

/// Two macros that do not know each other announce, with a macro that does not
/// announce between them.
#[emit_announce(meta = flag::a)]
#[passthrough]
#[emit_announce(meta = flag::b)]
pub struct TwoUnknown(pub u8);

#[::announce::announce(meta = flag::u)]
#[emit_announce(meta = flag::a)]
#[passthrough]
#[emit_announce(meta = flag::b)]
pub struct TwoUnknownAndUser(pub u8);

#[emit_announce(meta = flag::a)]
#[::announce::announce(meta = flag::u)]
#[emit_announce(meta = flag::b)]
#[passthrough]
pub struct UserBetweenTwoUnknown(pub u8);
