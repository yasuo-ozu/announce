//! Items with several announcements: `#[announce]` attributes, derives, and macros
//! that announce what they produce. Each of them must make one trampoline and one
//! `Announce` impl, whose META holds the entries of all announcements
//! (`tests/user/tests/merge.rs` reads it). Nothing here may be an error.

#![allow(dead_code)]

use announce::announce;
use announce_test_derive::{
    chain, emit_announce, emit_derive_announce, passthrough, record_attrs, summary, tick,
};

// ---- attribute and attribute -----------------------------------------------

#[announce(meta = merge::a)]
#[::announce::announce(meta = merge::b)]
pub struct AttrAttr(pub u8);

#[announce(meta = merge::a)]
#[::announce::announce(meta = merge::b)]
#[::announce::announce(meta = merge::c)]
pub struct ThreeAttrs(pub u8);

/// The attribute below is the bare `announce` (imported, so a macro).
#[::announce::announce(meta = merge::a)]
#[announce(meta = merge::b)]
pub struct BareBelow(pub u8);

#[announce]
#[::announce::announce]
pub struct NoOptions(pub u8);

#[announce(meta = merge::a)]
#[::announce::announce(meta = merge::b)]
pub enum AttrAttrEnum {
    A,
    B(u8),
}

#[announce(meta = merge::a)]
#[::announce::announce(meta = merge::b)]
pub trait AttrAttrTrait {
    fn f(&self) -> u8;
}

/// Options that differ but do not conflict are all kept. Both announcements name the
/// trampoline `Renamed` (it is one trampoline, so one name).
#[announce(crate = ::announce, macro_name = Renamed, allow_std_prelude = true, meta = merge::a)]
#[::announce::announce(macro_name = Renamed, layout = single, docs = true, meta = merge::b)]
pub struct DifferentOptions(pub String);

// ---- attribute above and below a derive ------------------------------------

#[::announce::announce(meta = merge::a)]
#[derive(::announce::Announce, Debug)]
#[announce(meta = merge::b)]
pub struct AttrAboveDerive(pub u8);

/// The derive has no helper attribute yet: it is made.
#[::announce::announce(meta = merge::a)]
#[derive(::announce::Announce)]
pub struct AttrAboveDeriveNoHelper(pub u8);

#[::announce::announce(meta = merge::a)]
#[::announce::announce(meta = merge::b)]
#[derive(::announce::Announce)]
#[announce(meta = merge::c)]
pub struct TwoAttrsAboveDerive(pub u8);

/// The derive expands first and sees the attribute below it: it leaves the
/// announcement to the attribute, which finds the helper.
#[derive(::announce::Announce)]
#[announce(meta = merge::b)]
#[::announce::announce(meta = merge::a)]
pub struct AttrBelowDerive(pub u8);

#[derive(::announce::Announce, Debug)]
#[::announce::announce(meta = merge::a)]
pub struct AttrBelowDeriveNoHelper(pub u8);

/// The same, with the attribute below some other attribute.
#[derive(::announce::Announce)]
#[announce(meta = merge::b)]
#[derive(Debug)]
#[::announce::announce(meta = merge::a)]
#[derive(Clone)]
pub struct AttrBelowDeriveAmongOthers(pub u8);

/// Two derives: the first leaves the announcement to the last (rustfmt would join
/// two derives that follow each other).
#[derive(::announce::Announce)]
#[announce(meta = merge::a)]
#[derive(::announce::Announce)]
pub struct DeriveTwice(pub u8);

// ---- macros that announce what they produce --------------------------------

#[emit_announce(meta = merge::a)]
#[announce(meta = merge::b)]
pub struct EmitIntoAttr(pub u8);

#[emit_announce(meta = merge::a)]
#[derive(::announce::Announce)]
#[announce(meta = merge::b)]
pub struct EmitIntoDerive(pub u8);

#[emit_derive_announce(meta = merge::a)]
#[announce(meta = merge::b)]
pub struct EmitDeriveIntoAttr(pub u8);

/// `emit_announce` expands first, and its announcement moves below the second
/// `emit_announce` (which announces as well, and moves behind it): they fold.
#[emit_announce(meta = merge::a)]
#[emit_announce(meta = merge::b)]
pub struct BothDefer(pub u8);

/// An announcement that moves below a macro that does not announce.
#[emit_announce(meta = merge::a)]
#[derive(Debug)]
#[passthrough]
pub struct DeferOnce(pub u8);

/// An unknown attribute macro sits between two announcements, and announces itself.
#[announce(meta = merge::one)]
#[emit_announce(meta = merge::two)]
#[::announce::announce(meta = merge::three)]
pub struct Between(pub u8);

/// Five macros that announce one after the other.
#[chain(5)]
pub struct ChainFive(pub u8);

/// With the flag set by hand, an announcement does not move, whatever is left below.
#[announce(meta = merge::a, __announce_deferred)]
#[passthrough]
pub struct MarkedNeverWaits(pub u8);

#[chain(3)]
#[announce(meta = merge::a)]
#[passthrough]
pub struct ChainThenAttr(pub u8);

// ---- checks run once each --------------------------------------------------

/// Both announcements list `tick(shared)`: an exact duplicate, run once. `tick`
/// leaves a constant, so a second run would define it twice.
#[announce(tick(left), tick(shared), meta = merge::a)]
#[::announce::announce(tick(shared), tick(right), meta = merge::b)]
pub struct Ticked(pub u8);

/// The query for the owned attributes comes before the announcement can move.
#[emit_announce(tick(waiting))]
#[passthrough]
pub struct TickedWaiting(pub u8);

#[emit_announce(tick(first))]
#[emit_announce(tick(second))]
pub struct TickedBothDefer(pub u8);

#[derive(::announce::Announce)]
#[announce(tick(helper))]
#[::announce::announce(tick(attr))]
pub struct TickedDerive(pub u8);

/// An attribute that a check owns stays below `#[announce]`, which removes it: the
/// announcement moves below `passthrough` and the owned attribute follows it.
#[emit_announce(record_attrs)]
#[my_lib(on_the_item)]
#[passthrough]
pub struct OwnedOnItem(pub u8);

// ---- items that are not `pub` ----------------------------------------------

mod hidden {
    use super::*;

    #[announce(meta = merge::a)]
    #[::announce::announce(meta = merge::b)]
    struct Private(pub u8);

    #[announce(meta = merge::a)]
    #[::announce::announce(meta = merge::b)]
    pub(crate) struct Crate(pub u8);

    #[emit_announce(meta = merge::a)]
    #[emit_announce(meta = merge::b)]
    pub(super) struct Super(pub u8);

    #[::announce::announce(meta = merge::a)]
    #[derive(::announce::Announce)]
    #[announce(meta = merge::b)]
    pub(in crate::merge) struct InPath(pub u8);

    Private! { [Private!, summary!] PRIVATE_ROWS }
    Crate! { [Crate!, summary!] CRATE_ROWS }
    Super! { [Super!, summary!] SUPER_ROWS }
    InPath! { [InPath!, summary!] IN_PATH_ROWS }
    pub(crate) fn rows() -> [&'static [(&'static str, &'static str, &'static str)]; 4] {
        [PRIVATE_ROWS, CRATE_ROWS, SUPER_ROWS, IN_PATH_ROWS]
    }
}

/// META of the items above that are not `pub`, one slice for each.
pub fn hidden_rows() -> [&'static [(&'static str, &'static str, &'static str)]; 4] {
    hidden::rows()
}
