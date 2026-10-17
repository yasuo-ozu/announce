//! Several announcements of one item make one trampoline, one `Announce` impl and one
//! META with the entries of all of them. The items are in `announce_test_defs::merge`,
//! where a second trampoline or impl would not compile.

use announce::Announce;
use announce_test_defs::merge;
use announce_test_derive::summary;

type Rows = &'static [(&'static str, &'static str, &'static str)];

mod rows_a {
    use super::{merge, summary};
    merge::AttrAttr! {
        [merge::AttrAttr!, merge::ThreeAttrs!, merge::BareBelow!, merge::NoOptions!,
         merge::AttrAttrEnum!, merge::AttrAttrTrait!, merge::Renamed!,
         merge::AttrAboveDerive!, merge::AttrAboveDeriveAlone!, merge::TwoAttrsAboveDerive!,
         merge::AttrBelowDerive!, merge::AttrBelowDeriveAlone!,
         merge::AttrBelowDeriveAmongOthers!, summary!] ALL
    }
}

mod rows_b {
    use super::{merge, summary};
    merge::EmitIntoAttr! {
        [merge::EmitIntoAttr!, merge::EmitAroundDerive!,
         merge::BothDefer!, merge::DeferOnce!, merge::Between!, merge::ChainFive!,
         merge::ChainThenAttr!, merge::Ticked!,
         merge::TickedWaiting!, merge::TickedBothDefer!, merge::TickedAroundDerive!,
         merge::OwnedOnItem!, summary!] ALL
    }
}

/// The META of the trampoline called `key`, without spaces.
fn meta(rows: Rows, key: &str) -> String {
    let mut found = rows.iter().filter(|r| r.0 == key);
    let row = found
        .next()
        .unwrap_or_else(|| panic!("{} is not in {:?}", key, rows));
    assert!(found.next().is_none(), "{} is there twice", key);
    row.1.split_whitespace().collect()
}

fn is_announce<T: Announce>() {}

#[test]
fn attribute_and_attribute() {
    let rows = rows_a::ALL;
    assert_eq!(rows.len(), 13);
    // The announcements are merged in the order they expand: the entries of the first
    // come first, for every kind of item.
    assert_eq!(meta(rows, "AttrAttr"), "merge::a,merge::b");
    assert_eq!(meta(rows, "AttrAttrEnum"), "merge::a,merge::b");
    assert_eq!(meta(rows, "AttrAttrTrait"), "merge::a,merge::b");
    is_announce::<merge::AttrAttr>();
    is_announce::<merge::AttrAttrEnum>();
}

#[test]
fn three_attributes() {
    assert_eq!(
        meta(rows_a::ALL, "ThreeAttrs"),
        "merge::a,merge::b,merge::c"
    );
    is_announce::<merge::ThreeAttrs>();
}

#[test]
fn a_bare_attribute_below_is_merged() {
    assert_eq!(meta(rows_a::ALL, "BareBelow"), "merge::a,merge::b");
    is_announce::<merge::BareBelow>();
}

#[test]
fn announcements_without_options() {
    assert_eq!(meta(rows_a::ALL, "NoOptions"), "");
    is_announce::<merge::NoOptions>();
}

#[test]
fn options_are_merged_and_the_names_are_kept() {
    // `Renamed` is the `macro_name` of the first announcement; `String` in the item
    // needs the `allow_std_prelude` of the first as well (it would not compile).
    assert_eq!(meta(rows_a::ALL, "Renamed"), "merge::a,merge::b");
    let _ = merge::DifferentOptions(String::new());
}

#[test]
fn attributes_above_a_derive_of_another_trait() {
    let rows = rows_a::ALL;
    // The first attribute puts the runner below the derive; the others merge into it,
    // also the bare `#[announce]` below the derive (it is the macro `announce`).
    assert_eq!(meta(rows, "AttrAboveDerive"), "merge::a,merge::b");
    assert_eq!(meta(rows, "AttrAboveDeriveAlone"), "merge::a");
    assert_eq!(
        meta(rows, "TwoAttrsAboveDerive"),
        "merge::a,merge::b,merge::c"
    );
    is_announce::<merge::AttrAboveDerive>();
    is_announce::<merge::AttrAboveDeriveAlone>();
    is_announce::<merge::TwoAttrsAboveDerive>();
}

#[test]
fn attributes_below_a_derive_of_another_trait() {
    let rows = rows_a::ALL;
    // The attribute that expands first is the target: the first from the top.
    assert_eq!(meta(rows, "AttrBelowDerive"), "merge::b,merge::a");
    assert_eq!(meta(rows, "AttrBelowDeriveAlone"), "merge::a");
    assert_eq!(
        meta(rows, "AttrBelowDeriveAmongOthers"),
        "merge::b,merge::a"
    );
    is_announce::<merge::AttrBelowDerive>();
    is_announce::<merge::AttrBelowDeriveAlone>();
    is_announce::<merge::AttrBelowDeriveAmongOthers>();
    // The derives ran.
    let _ = format!("{:?}", merge::AttrBelowDerive(1));
    let _ = merge::AttrBelowDeriveAmongOthers(1).clone() == merge::AttrBelowDeriveAmongOthers(1);
}

#[test]
fn a_macro_that_announces_into_a_pending_announcement() {
    let rows = rows_b::ALL;
    assert_eq!(rows.len(), 12);
    assert_eq!(meta(rows, "EmitIntoAttr"), "merge::a,merge::b");
    assert_eq!(meta(rows, "EmitAroundDerive"), "merge::a,merge::b");
    is_announce::<merge::EmitIntoAttr>();
    is_announce::<merge::EmitAroundDerive>();
}

#[test]
fn an_announcement_waits_for_the_macros_below_it() {
    let rows = rows_b::ALL;
    // The first announcement put the runner below the second `emit_announce`, which
    // announced as well: its announcement was merged into the runner.
    assert_eq!(meta(rows, "BothDefer"), "merge::a,merge::b");
    // Nothing announces later: the runner makes the trampoline.
    assert_eq!(meta(rows, "DeferOnce"), "merge::a");
    // An unknown macro between two announcements, announcing too.
    assert_eq!(meta(rows, "Between"), "merge::one,merge::two,merge::three");
    is_announce::<merge::BothDefer>();
    is_announce::<merge::DeferOnce>();
    is_announce::<merge::Between>();
}

#[test]
fn macros_that_keep_announcing_end() {
    let rows = rows_b::ALL;
    // Five macros, one after the other, each announcing and leaving the rest to
    // the next: one trampoline, with the first announcement first.
    assert_eq!(
        meta(rows, "ChainFive"),
        "merge::step(5),merge::step(4),merge::step(3),merge::step(2),merge::step(1)"
    );
    // With an attribute of the user's below: it expands after the macros above it, the
    // chain included.
    assert_eq!(
        meta(rows, "ChainThenAttr"),
        "merge::step(3),merge::step(2),merge::step(1),merge::a"
    );
    is_announce::<merge::ChainFive>();
    is_announce::<merge::ChainThenAttr>();
}

#[test]
fn checks_run_once_each() {
    // Each `tick` leaves a constant; a check that ran twice for an item would
    // not compile. `tick(shared)` is in both announcements and runs once.
    assert_eq!(merge::TICK_TICKED_LEFT, 0);
    assert_eq!(merge::TICK_TICKED_SHARED, 0);
    assert_eq!(merge::TICK_TICKED_RIGHT, 0);
    assert_eq!(merge::TICK_TICKEDWAITING_WAITING, 0);
    assert_eq!(merge::TICK_TICKEDBOTHDEFER_FIRST, 0);
    assert_eq!(merge::TICK_TICKEDBOTHDEFER_SECOND, 0);
    assert_eq!(merge::TICK_TICKEDAROUNDDERIVE_FIRST, 0);
    assert_eq!(merge::TICK_TICKEDAROUNDDERIVE_SECOND, 0);
    let rows = rows_b::ALL;
    // A check is listed in META like a `meta` entry, the target's first, the
    // duplicate once.
    let m = meta(rows, "Ticked");
    let order = ["tick(left)", "tick(shared)", "tick(right)"];
    let at: Vec<usize> = order.iter().map(|o| m.find(o).unwrap()).collect();
    assert!(at.windows(2).all(|w| w[0] < w[1]), "{}", m);
    assert_eq!(m.matches("tick(shared)").count(), 1, "{}", m);
    assert!(m.contains("merge::a") && m.contains("merge::b"), "{}", m);
    assert!(meta(rows, "TickedWaiting").contains("tick(waiting)"));
    let m = meta(rows, "TickedBothDefer");
    assert!(m.find("tick(first)").unwrap() < m.find("tick(second)").unwrap());
    let m = meta(rows, "TickedAroundDerive");
    assert!(
        m.find("tick(first)").unwrap() < m.find("tick(second)").unwrap(),
        "{}",
        m
    );
    is_announce::<merge::Ticked>();
    is_announce::<merge::TickedBothDefer>();
    is_announce::<merge::TickedAroundDerive>();
}

#[test]
fn an_attribute_of_a_check_on_the_item() {
    // `my_lib` is owned by `record_attrs`: the runner removed it, and `#[announce]`
    // had put it after the runner, so that rustc did not see it above a macro.
    assert_eq!(merge::SEEN_OWNEDONITEM, ["my_lib(on_the_item)"]);
    assert!(meta(rows_b::ALL, "OwnedOnItem").contains("record_attrs"));
}

#[test]
fn items_that_are_not_pub() {
    // Private, `pub(crate)`, `pub(super)` and `pub(in path)`.
    let rows = merge::hidden_rows();
    assert_eq!(meta(rows[0], "Private"), "merge::a,merge::b");
    assert_eq!(meta(rows[1], "Crate"), "merge::a,merge::b");
    assert_eq!(meta(rows[2], "Super"), "merge::a,merge::b");
    assert_eq!(meta(rows[3], "InPath"), "merge::a,merge::b");
}
