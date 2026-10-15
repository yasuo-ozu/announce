//! Announcements are told by their flag, not by their path: the items of
//! `announce_test_defs::flag` make one trampoline each, whatever other attributes they
//! have, and the META holds the entries of all the announcements of the item.

use announce::Announce;
use announce_test_defs::flag;
use announce_test_derive::summary;

type Rows = &'static [(&'static str, &'static str, &'static str)];

mod rows_foreign {
    use super::{flag, summary};
    flag::ForeignBelow! {
        [flag::ForeignBelow!, flag::ForeignAbove!, flag::ForeignBetween!, flag::ForeignTwice!,
         flag::ForeignWithOptions!, flag::bare::ForeignBare!, flag::bare::ForeignBareAbove!,
         flag::bare::ForeignBareBetween!, flag::renamed::Twice!, flag::renamed::Thrice!,
         flag::renamed::Mixed!, summary!] ALL
    }
}

mod rows_derives {
    use super::{flag, summary};
    flag::derives::AttrAbove! {
        [flag::derives::AttrAbove!, flag::derives::AttrAboveHelper!, flag::derives::DeriveAbove!,
         flag::derives::DeriveAboveHelper!, flag::derives::AttrAboveTwoHelpers!,
         flag::derives::TwoAttrsAbove!, flag::derives::TwoAttrsBelow!,
         flag::derives::DeriveBetween!, flag::derives::OtherDerives!, summary!] ALL
    }
}

mod rows_checks {
    use super::{flag, summary};
    flag::OwnedAndAnother! {
        [flag::OwnedAndAnother!, flag::AnotherAndOwned!, flag::TwoOwners!,
         flag::OwnedAndEmitted!, flag::QueriedOnce!, flag::QueriedOnceNoAttributes!,
         flag::QueriedTwice!, flag::QueriedWithDerive!, flag::QueriedEmitted!,
         flag::QueriedAndOwned!, summary!] ALL
    }
}

mod rows_other {
    use super::{flag, summary};
    flag::NotEmittedDerive! {
        [flag::NotEmittedDerive!, flag::NotEmittedPassthrough!, flag::MockedDerive!,
         flag::NotEmittedAfterOther!, flag::MockedAfterOther!, flag::NotEmittedOwned!,
         flag::IntoFlagged!, flag::TwoUnknown!, flag::TwoUnknownAndUser!,
         flag::UserBetweenTwoUnknown!, summary!] ALL
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
fn an_attribute_of_another_crate_is_not_an_announcement() {
    let rows = rows_foreign::ALL;
    // The foreign attribute ran (its constant is there, once for each of its uses), and
    // the item is announced once, with the options of our attributes only.
    assert_eq!(flag::FOREIGN_FOREIGNBELOW_2, 2);
    assert_eq!(meta(rows, "ForeignBelow"), "flag::a");
    is_announce::<flag::ForeignBelow>();

    assert_eq!(flag::FOREIGN_FOREIGNABOVE_3, 3);
    assert_eq!(meta(rows, "ForeignAbove"), "flag::a");
    is_announce::<flag::ForeignAbove>();

    // The later of our attributes is the target.
    assert_eq!(flag::FOREIGN_FOREIGNBETWEEN_4, 4);
    assert_eq!(meta(rows, "ForeignBetween"), "flag::b,flag::a");
    is_announce::<flag::ForeignBetween>();

    assert_eq!(
        (flag::FOREIGN_FOREIGNTWICE_5, flag::FOREIGN_FOREIGNTWICE_6),
        (5, 6)
    );
    assert_eq!(meta(rows, "ForeignTwice"), "flag::a");
    is_announce::<flag::ForeignTwice>();

    // `layout = direct` is an option of ours: the foreign macros would not parse it.
    assert_eq!(
        (
            flag::FOREIGN_FOREIGNWITHOPTIONS_7,
            flag::FOREIGN_FOREIGNWITHOPTIONS_8
        ),
        (7, 8)
    );
    assert_eq!(meta(rows, "ForeignWithOptions"), "flag::a");
    is_announce::<flag::ForeignWithOptions>();
}

#[test]
fn a_bare_attribute_of_another_crate_is_not_an_announcement() {
    // `use announce_test_derive::announce; #[announce(level = 9)]`: a macro that is
    // not ours is not taken over, not deleted, not given our options.
    let rows = rows_foreign::ALL;
    assert_eq!(flag::bare::FOREIGN_FOREIGNBARE_9, 9);
    assert_eq!(meta(rows, "ForeignBare"), "flag::a");
    assert_eq!(flag::bare::FOREIGN_FOREIGNBAREABOVE_10, 10);
    assert_eq!(meta(rows, "ForeignBareAbove"), "flag::a");
    assert_eq!(flag::bare::FOREIGN_FOREIGNBAREBETWEEN_11, 11);
    assert_eq!(meta(rows, "ForeignBareBetween"), "flag::b,flag::a");
    is_announce::<flag::bare::ForeignBare>();
    is_announce::<flag::bare::ForeignBareAbove>();
    is_announce::<flag::bare::ForeignBareBetween>();
}

#[test]
fn a_renamed_import_is_an_announcement() {
    let rows = rows_foreign::ALL;
    assert_eq!(meta(rows, "Twice"), "flag::b");
    is_announce::<flag::renamed::Twice>();
    // The later of the three is the target, the others follow in the order they moved.
    let m = meta(rows, "Thrice");
    assert!(
        m.contains("flag::a") && m.contains("flag::b") && m.contains("flag::c"),
        "{}",
        m
    );
    is_announce::<flag::renamed::Thrice>();
    // Under another name and under its own, next to a foreign `announce`.
    assert_eq!(flag::renamed::FOREIGN_MIXED_12, 12);
    assert_eq!(meta(rows, "Mixed"), "flag::b,flag::a");
    is_announce::<flag::renamed::Mixed>();
}

#[test]
fn derives_in_every_order() {
    let rows = rows_derives::ALL;
    assert_eq!(rows.len(), 9);
    // The attribute is above the derive.
    assert_eq!(meta(rows, "AttrAbove"), "flag::a");
    assert_eq!(meta(rows, "AttrAboveHelper"), "flag::h,flag::a");
    assert_eq!(
        meta(rows, "AttrAboveTwoHelpers"),
        "flag::h2,flag::h1,flag::a"
    );
    // The derive is above the attribute.
    assert_eq!(meta(rows, "DeriveAbove"), "flag::a");
    assert_eq!(meta(rows, "DeriveAboveHelper"), "flag::h,flag::a");
    // Two attributes and a derive.
    assert_eq!(meta(rows, "TwoAttrsAbove"), "flag::b,flag::h,flag::a");
    assert_eq!(meta(rows, "TwoAttrsBelow"), "flag::b,flag::h,flag::a");
    assert_eq!(meta(rows, "DeriveBetween"), "flag::b,flag::h,flag::a");
    assert_eq!(meta(rows, "OtherDerives"), "flag::a");
    is_announce::<flag::derives::AttrAbove>();
    is_announce::<flag::derives::AttrAboveHelper>();
    is_announce::<flag::derives::AttrAboveTwoHelpers>();
    is_announce::<flag::derives::DeriveAbove>();
    is_announce::<flag::derives::DeriveAboveHelper>();
    is_announce::<flag::derives::TwoAttrsAbove>();
    is_announce::<flag::derives::TwoAttrsBelow>();
    is_announce::<flag::derives::DeriveBetween>();
    is_announce::<flag::derives::OtherDerives>();
    // The other derives ran.
    let _ = format!("{:?}", flag::derives::OtherDerives(1).clone());
}

#[test]
fn an_owned_attribute_and_another_announcement() {
    let rows = rows_checks::ALL;
    // The attribute `my_lib` is the check's, wherever the announcements are: the
    // check saw it once, and the item compiles without it.
    assert_eq!(flag::SEEN_OWNEDANDANOTHER, ["my_lib(on_item)"]);
    assert_eq!(flag::SEEN_ANOTHERANDOWNED, ["my_lib(on_item)"]);
    // Both checks ran once (a second run would define the constant twice).
    assert_eq!(flag::TICK_OWNEDANDANOTHER_SECOND, 0);
    assert_eq!(flag::TICK_ANOTHERANDOWNED_FIRST, 0);
    let m = meta(rows, "OwnedAndAnother");
    assert!(
        m.contains("tick(second)") && m.contains("record_attrs") && m.contains("attrs(my_lib)"),
        "{}",
        m
    );
    assert_eq!(m.matches("attrs(").count(), 1, "{}", m);
    let m = meta(rows, "AnotherAndOwned");
    assert!(
        m.contains("tick(first)") && m.contains("record_attrs") && m.contains("attrs(my_lib)"),
        "{}",
        m
    );
    is_announce::<flag::OwnedAndAnother>();
    is_announce::<flag::AnotherAndOwned>();
    let _ = flag::OwnedAndAnother(0);
    let _ = flag::AnotherAndOwned(0);
}

#[test]
fn two_checks_that_own_different_attributes() {
    let rows = rows_checks::ALL;
    let m = meta(rows, "TwoOwners");
    // The names of both are listed, and the item compiles without any of them.
    assert_eq!(m.matches("attrs(").count(), 1, "{}", m);
    for name in ["my_lib", "other", "tag"] {
        assert!(m.contains(name), "{}: {}", name, m);
    }
    let _ = flag::TwoOwners(0);
    is_announce::<flag::TwoOwners>();
}

#[test]
fn an_owned_attribute_with_a_macro_that_announces() {
    assert_eq!(flag::SEEN_OWNEDANDEMITTED, ["my_lib(on_item)"]);
    assert_eq!(flag::TICK_OWNEDANDEMITTED_EMITTED, 0);
    let m = meta(rows_checks::ALL, "OwnedAndEmitted");
    assert!(
        m.contains("record_attrs") && m.contains("tick(emitted)"),
        "{}",
        m
    );
    is_announce::<flag::OwnedAndEmitted>();
}

/// The numbers `queried_<n>` that META lists for `key`.
fn queried(key: &str) -> Vec<String> {
    let m = meta(rows_checks::ALL, key);
    let mut out: Vec<String> = m
        .split(|c: char| !(c.is_alphanumeric() || c == '_'))
        .filter(|w| w.starts_with("queried_"))
        .map(|w| w.to_string())
        .collect();
    out.sort();
    out
}

#[test]
fn the_query_is_sent_once_for_each_announcement() {
    // `count_query` owns `queried_<n>` for the n-th query it answers for an item, so a
    // query that was sent twice would list `queried_1`.
    assert_eq!(queried("QueriedOnce"), ["queried_0"]);
    assert_eq!(queried("QueriedOnceNoAttributes"), ["queried_0"]);
    assert_eq!(queried("QueriedWithDerive"), ["queried_0"]);
    // Two announcements, two queries: the names of both are there, and no third.
    assert_eq!(queried("QueriedTwice"), ["queried_0", "queried_1"]);
    assert_eq!(queried("QueriedEmitted"), ["queried_0", "queried_1"]);
    assert_eq!(queried("QueriedAndOwned"), ["queried_0"]);
    // The check is listed once for an item, whatever the number of announcements.
    for key in ["QueriedTwice", "QueriedEmitted"] {
        assert_eq!(
            meta(rows_checks::ALL, key).matches("count_query").count(),
            1
        );
    }
}

#[test]
fn a_query_and_an_owned_attribute_in_one_item() {
    let m = meta(rows_checks::ALL, "QueriedAndOwned");
    assert!(m.contains("attrs(") && m.contains("my_lib"), "{}", m);
    assert_eq!(flag::SEEN_QUERIEDANDOWNED, ["my_lib(on_item)"]);
    assert_eq!(flag::TICK_QUERIEDANDOWNED_SHARED, 0);
    let _ = flag::QueriedAndOwned(0);
    is_announce::<flag::QueriedAndOwned>();
}

#[test]
fn an_item_that_is_not_emitted_stays_above_the_macros_below_it() {
    let rows = rows_other::ALL;
    // `#[derive(Debug)]` below `emit_item = false` never sees the item: the announcement
    // did not move below it (it would be an error: a `Debug` impl for a missing type).
    assert_eq!(meta(rows, "NotEmittedDerive"), "");
    assert_eq!(meta(rows, "NotEmittedPassthrough"), "flag::a");
    // A mock is compiled in a hidden module, with its derive.
    assert!(meta(rows, "MockedDerive").contains("flag::a"));
    // Another announcement above moves below, and the two fold.
    assert_eq!(meta(rows, "NotEmittedAfterOther"), "flag::b,flag::a");
    assert!(meta(rows, "MockedAfterOther").contains("flag::a"));
    assert!(meta(rows, "MockedAfterOther").contains("flag::b"));
    assert_eq!(flag::SEEN_NOTEMITTEDOWNED, ["my_lib(never_compiled)"]);
    assert!(meta(rows, "NotEmittedOwned").contains("attrs(my_lib)"));
}

#[test]
fn macros_that_announce_what_they_produce() {
    let rows = rows_other::ALL;
    // The user's announcement has moved to the end, and the macro below it announces into
    // the item: the announcement that moved last is the target.
    assert_eq!(meta(rows, "IntoFlagged"), "flag::b,flag::a");
    assert_eq!(meta(rows, "TwoUnknown"), "flag::b,flag::a");
    let m = meta(rows, "TwoUnknownAndUser");
    for name in ["flag::u", "flag::a", "flag::b"] {
        assert!(m.contains(name), "{}: {}", name, m);
    }
    let m = meta(rows, "UserBetweenTwoUnknown");
    for name in ["flag::u", "flag::a", "flag::b"] {
        assert!(m.contains(name), "{}: {}", name, m);
    }
    is_announce::<flag::IntoFlagged>();
    is_announce::<flag::TwoUnknown>();
    is_announce::<flag::TwoUnknownAndUser>();
    is_announce::<flag::UserBetweenTwoUnknown>();
}
