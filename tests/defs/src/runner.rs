//! Several announcements of one item: the first announcement puts the runner (the hidden
//! attribute `_actually_run_announce`) last among the attributes of the item, the others are
//! merged into it, and the runner makes one trampoline and one `Announce` impl when it
//! expands (a second trampoline would not compile). The META of the trampolines is read by
//! `tests/user/tests/runner.rs`.
//!
//! - the attribute with its own name, and a renamed import of it, in every order and with
//!   macros between;
//! - derives of other traits, above and below the attribute, and the bare attribute;
//! - checks that own attributes (on the item, in fields, in a `cfg_attr`), two
//!   announcements with checks of their own, the query asked once for each;
//! - the items that are not structs, and the other macros.
#![allow(dead_code)]

use announce::announce;
use announce::announce as ann;
use announce_test_derive::{
    count_query, emit_announce, lib_check, other_check, passthrough, record_attrs, tick,
};

// ---- the attribute, and a renamed import of it ----------------------------------

#[announce]
#[announce(meta = runner::b)]
pub struct BareTwice(pub u8);

#[announce(meta = runner::a)]
#[announce(meta = runner::b)]
pub enum BareTwiceEnum {
    A,
    B(u8),
}

#[announce(meta = runner::a)]
#[announce(meta = runner::b)]
pub union BareTwiceUnion {
    pub a: u8,
    pub b: i8,
}

#[ann]
#[ann(meta = runner::b)]
pub struct RenamedTwice(pub u8);

/// Macros between the announcements, of the two kinds: one that returns its input, and
/// one that announces what it produces.
#[ann(meta = runner::a)]
#[passthrough]
#[ann(meta = runner::b)]
pub struct RenamedAfterPass(pub u8);

#[passthrough]
#[ann(meta = runner::a)]
#[passthrough]
#[ann(meta = runner::b)]
#[passthrough]
pub struct RenamedAmongPass(pub u8);

#[ann(meta = runner::a)]
#[emit_announce(meta = runner::e)]
#[ann(meta = runner::b)]
pub struct RenamedAroundEmit(pub u8);

#[emit_announce(meta = runner::e)]
#[ann(meta = runner::a)]
#[passthrough]
#[ann(meta = runner::b)]
pub struct EmitBeforeRenamed(pub u8);

#[ann(meta = runner::a)]
#[passthrough]
#[ann(meta = runner::b)]
#[emit_announce(meta = runner::e)]
pub struct EmitAfterRenamed(pub u8);

/// Both names of the attribute in one item, and a derive of someone else.
#[ann(meta = runner::a)]
#[derive(Clone, Debug)]
#[announce(meta = runner::b)]
#[::announce::announce(meta = runner::c)]
pub struct Names(pub u8);

// ---- derives of other traits, and the bare attribute ----------------------------------

/// The attribute above the derive: it puts the runner below the derive.
#[ann(meta = runner::a)]
#[derive(Debug)]
pub struct AttrThenDerive(pub u8);

#[ann(meta = runner::a)]
#[derive(Debug)]
#[announce(meta = runner::b)]
pub struct AttrThenDeriveBare(pub u8);

/// The derive above the attribute: the bare attribute expands first, and the attribute
/// below it is merged into its runner.
#[derive(Debug)]
#[announce(meta = runner::b)]
#[::announce::announce(meta = runner::a)]
pub struct DeriveThenAttr(pub u8);

/// Two attributes, the derive and two bare attributes.
#[::announce::announce(meta = runner::a)]
#[derive(Debug)]
#[announce(meta = runner::b1)]
#[announce(meta = runner::b2)]
#[::announce::announce(meta = runner::c)]
pub struct DeriveAmong(pub u8);

/// A check on the attribute below the bare one, which owns an attribute of the item: the
/// bare attribute expands first and puts the runner below the owned attribute.
#[derive(Debug)]
#[announce(meta = runner::b)]
#[::announce::announce(lib_check)]
#[my_lib(x)]
pub struct BareThenCheck(pub u8);

/// The check of both is the same: it is asked once for each announcement, and listed
/// once.
#[derive(Debug)]
#[announce(lib_check)]
#[::announce::announce(lib_check)]
#[my_lib(x)]
pub struct SameCheckTwice(pub u8);

/// Two announcements with a check of their own: they are asked, each once.
#[derive(Debug)]
#[announce(other_check, tick(first))]
#[::announce::announce(lib_check, tick(second))]
#[my_lib(x)]
pub struct TwoChecksAroundDerive(pub u8);

// ---- checks that own attributes -----------------------------------------------------

/// Two announcements with checks of their own, and attributes of both on the item, in the
/// fields, and between the announcements.
#[ann(lib_check, meta = runner::a)]
#[my_lib(on_item)]
#[ann(other_check, meta = runner::b)]
#[other(on_item)]
pub struct TwoChecks {
    #[my_lib(a)]
    pub a: u8,
    #[other(b)]
    pub b: u8,
    #[tag]
    pub t: u8,
}

/// An attribute that a check owns, below a macro that returns its input: the runner is
/// below the macro, the attribute follows the runner, and the runner removes it.
#[::announce::announce(lib_check)]
#[passthrough]
#[my_lib(x)]
pub struct OwnedAfterPass(pub u8);

/// Two announcements with different checks, and their attributes mixed with other macros.
#[::announce::announce(lib_check)]
#[passthrough]
#[my_lib(x)]
#[ann(other_check)]
#[passthrough]
#[other(y)]
#[tag]
#[passthrough]
pub struct OwnersMixed(pub u8);

/// The names are in the runner, whatever the order of the announcements.
#[ann(other_check, meta = runner::b)]
#[ann(lib_check, meta = runner::a)]
#[my_lib(on_item)]
#[other(on_item)]
pub enum TwoChecksEnum {
    #[my_lib(variant)]
    A,
    B(#[other(field)] u8),
}

/// `record_attrs` records what it sees: the attributes with their `cfg_attr` evaluated, in
/// source order.
#[ann(record_attrs, meta = runner::a)]
#[cfg_attr(not(feature = "x"), my_lib(outer_cfg_attr))]
#[ann(count_query)]
pub struct RecordsCfgAttr {
    #[my_lib(first)]
    pub a: u8,
    #[cfg_attr(not(feature = "x"), my_lib(from_cfg_attr))]
    pub b: u8,
    #[cfg_attr(feature = "x", my_lib(not_here))]
    pub c: u8,
    #[cfg(feature = "x")]
    #[my_lib(not_here_either)]
    pub d: u8,
}

/// The query is sent once for each announcement: `count_query` owns `queried_<n>` for
/// the n-th query that it answers for an item.
#[ann(count_query)]
pub struct RunQueriedOnce(pub u8);

#[ann(count_query)]
#[passthrough]
pub struct RunQueriedAfterPass(pub u8);

#[ann(count_query)]
#[ann(count_query, meta = runner::b)]
#[passthrough]
pub struct RunQueriedTwice(pub u8);

#[ann(count_query, meta = runner::a)]
#[derive(Debug)]
#[announce(meta = runner::b)]
pub struct RunQueriedWithDerive(pub u8);

/// Each check runs once (`tick` leaves a constant: a second run would define it twice).
#[ann(tick(left), tick(shared), meta = runner::a)]
#[ann(tick(shared), tick(right), meta = runner::b)]
pub struct Ticked(pub u8);

// ---- the other items -----------------------------------------------------------------

/// A trait is announced like a struct.
#[ann(meta = runner::a)]
#[passthrough]
#[ann(meta = runner::b)]
pub trait TraitTwice {
    fn f(&self) -> u8;
}

#[ann(meta = runner::a)]
#[emit_announce(meta = runner::e)]
pub trait TraitEmit {
    fn f(&self) -> u8;
}

/// An inline module, and a type alias.
#[ann(meta = runner::a)]
#[passthrough]
#[ann(meta = runner::b)]
pub mod twice_mod {
    pub fn f() -> u8 {
        1
    }
}

#[ann(meta = runner::a)]
#[passthrough]
#[ann(meta = runner::b)]
pub type TwiceAlias = [u8; 2];
