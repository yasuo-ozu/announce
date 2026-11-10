//! The runner is the one announcement that is told by a name, never by the path `announce`:
//! the items of `announce_tests::defs::paths` make one trampoline each, whatever other
//! attributes they have (also ones that are called `announce`), and the META holds the
//! entries of all the announcements of the item.

// The `Announce` of the `announce` that `defs` links. Under `cargo check` and clippy the
// subcrates and this file link different builds of `announce`, so `announce::Announce`
// would be another trait there.
use announce_tests::defs::announce::Announce;
use announce_tests::defs::paths;
use announce_tests::derive::summary;

type Rows = &'static [(&'static str, &'static str, &'static str)];

mod rows_foreign {
    use super::{paths, summary};
    paths::ForeignBelow! {
        [paths::ForeignBelow!, paths::ForeignAbove!, paths::ForeignBetween!, paths::ForeignTwice!,
         paths::ForeignWithOptions!, paths::bare::ForeignBare!, paths::bare::ForeignBareAbove!,
         paths::bare::ForeignBareBetween!, paths::bare::ForeignBareNoArgs!,
         paths::bare::ForeignBareMeta!, paths::bare::ForeignBareNoArgsAbove!,
         paths::bare::ForeignBareNoArgsBetween!, paths::renamed::Twice!, paths::renamed::Thrice!,
         paths::renamed::Mixed!, summary!] ALL
    }
}

mod rows_derives {
    use super::{paths, summary};
    paths::derives::AttrAbove! {
        [paths::derives::AttrAbove!, paths::derives::AttrAboveBare!, paths::derives::DeriveAbove!,
         paths::derives::DeriveAboveBare!, paths::derives::AttrAboveTwoBare!,
         paths::derives::TwoAttrsAbove!, paths::derives::TwoAttrsBelow!,
         paths::derives::DeriveBetween!, paths::derives::OtherDerives!, summary!] ALL
    }
}

mod rows_checks {
    use super::{paths, summary};
    paths::OwnedAndAnother! {
        [paths::OwnedAndAnother!, paths::AnotherAndOwned!, paths::TwoOwners!,
         paths::OwnedAndEmitted!, paths::QueriedOnce!, paths::QueriedOnceNoAttributes!,
         paths::QueriedTwice!, paths::QueriedWithDerive!, paths::QueriedEmitted!,
         paths::QueriedAndOwned!, summary!] ALL
    }
}

mod rows_other {
    use super::{paths, summary};
    paths::NotEmittedPassthrough! {
        [paths::NotEmittedPassthrough!, paths::NotEmittedBelow!, paths::NotEmittedAbove!,
         paths::NotEmittedBetween!, paths::MockedBelow!, paths::MockedAbove!,
         paths::NotEmittedOwned!, paths::NotEmittedTwoOwners!,
         paths::IntoRunner!, paths::TwoUnknown!, paths::TwoUnknownAndUser!,
         paths::UserBetweenTwoUnknown!, summary!] ALL
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
    assert_eq!(paths::FOREIGN_FOREIGNBELOW_2, 2);
    assert_eq!(meta(rows, "ForeignBelow"), "paths::a");
    is_announce::<paths::ForeignBelow>();

    assert_eq!(paths::FOREIGN_FOREIGNABOVE_3, 3);
    assert_eq!(meta(rows, "ForeignAbove"), "paths::a");
    is_announce::<paths::ForeignAbove>();

    // The later of our attributes is the target.
    assert_eq!(paths::FOREIGN_FOREIGNBETWEEN_4, 4);
    assert_eq!(meta(rows, "ForeignBetween"), "paths::a,paths::b");
    is_announce::<paths::ForeignBetween>();

    assert_eq!(
        (paths::FOREIGN_FOREIGNTWICE_5, paths::FOREIGN_FOREIGNTWICE_6),
        (5, 6)
    );
    assert_eq!(meta(rows, "ForeignTwice"), "paths::a");
    is_announce::<paths::ForeignTwice>();

    // `layout = direct` is an option of ours: the foreign macros would not parse it.
    assert_eq!(
        (
            paths::FOREIGN_FOREIGNWITHOPTIONS_7,
            paths::FOREIGN_FOREIGNWITHOPTIONS_8
        ),
        (7, 8)
    );
    assert_eq!(meta(rows, "ForeignWithOptions"), "paths::a");
    is_announce::<paths::ForeignWithOptions>();
}

#[test]
fn a_bare_attribute_of_another_crate_is_not_an_announcement() {
    // `use announce_tests::derive::announce; #[announce(level = 9)]`: a macro that is
    // not ours is not deleted, not given our options.
    let rows = rows_foreign::ALL;
    assert_eq!(paths::bare::FOREIGN_FOREIGNBARE_9, 9);
    assert_eq!(meta(rows, "ForeignBare"), "paths::a");
    assert_eq!(paths::bare::FOREIGN_FOREIGNBAREABOVE_10, 10);
    assert_eq!(meta(rows, "ForeignBareAbove"), "paths::a");
    assert_eq!(paths::bare::FOREIGN_FOREIGNBAREBETWEEN_11, 11);
    assert_eq!(meta(rows, "ForeignBareBetween"), "paths::a,paths::b");
    is_announce::<paths::bare::ForeignBare>();
    is_announce::<paths::bare::ForeignBareAbove>();
    is_announce::<paths::bare::ForeignBareBetween>();
}

#[test]
fn a_bare_attribute_of_another_crate_runs_next_to_ours() {
    // `#[::announce::announce(meta = ..)] #[announce] struct S;` with a foreign macro
    // imported as `announce`: the foreign macro runs, whatever its arguments (also ones
    // that are options of ours), and the item is announced by ours alone.
    let rows = rows_foreign::ALL;
    assert_eq!(paths::bare::FOREIGN_FOREIGNBARENOARGS_ARGS, "");
    assert_eq!(meta(rows, "ForeignBareNoArgs"), "paths::a");
    assert_eq!(
        paths::bare::FOREIGN_FOREIGNBAREMETA_ARGS,
        "meta=paths::foreign"
    );
    assert_eq!(meta(rows, "ForeignBareMeta"), "paths::a");
    assert_eq!(paths::bare::FOREIGN_FOREIGNBARENOARGSABOVE_ARGS, "");
    assert_eq!(meta(rows, "ForeignBareNoArgsAbove"), "paths::a");
    assert_eq!(paths::bare::FOREIGN_FOREIGNBARENOARGSBETWEEN_ARGS, "");
    assert_eq!(meta(rows, "ForeignBareNoArgsBetween"), "paths::a,paths::b");
    is_announce::<paths::bare::ForeignBareNoArgs>();
    is_announce::<paths::bare::ForeignBareMeta>();
    is_announce::<paths::bare::ForeignBareNoArgsAbove>();
    is_announce::<paths::bare::ForeignBareNoArgsBetween>();
}

#[test]
fn a_renamed_import_is_an_announcement() {
    let rows = rows_foreign::ALL;
    assert_eq!(meta(rows, "Twice"), "paths::b");
    is_announce::<paths::renamed::Twice>();
    // The three are merged in the order they expand.
    let m = meta(rows, "Thrice");
    assert!(
        m.contains("paths::a") && m.contains("paths::b") && m.contains("paths::c"),
        "{}",
        m
    );
    is_announce::<paths::renamed::Thrice>();
    // Under another name and under its own, next to a foreign `announce`.
    assert_eq!(paths::renamed::FOREIGN_MIXED_12, 12);
    assert_eq!(meta(rows, "Mixed"), "paths::a,paths::b");
    is_announce::<paths::renamed::Mixed>();
}

#[test]
fn derives_of_other_traits_in_every_order() {
    let rows = rows_derives::ALL;
    assert_eq!(rows.len(), 9);
    // The attribute is above the derive.
    assert_eq!(meta(rows, "AttrAbove"), "paths::a");
    assert_eq!(meta(rows, "AttrAboveBare"), "paths::a,paths::b");
    assert_eq!(
        meta(rows, "AttrAboveTwoBare"),
        "paths::a,paths::b1,paths::b2"
    );
    // The derive is above the attribute.
    assert_eq!(meta(rows, "DeriveAbove"), "paths::a");
    assert_eq!(meta(rows, "DeriveAboveBare"), "paths::b,paths::a");
    // Two attributes and a derive: the announcements are merged in the order they
    // expand, from the top.
    assert_eq!(meta(rows, "TwoAttrsAbove"), "paths::a,paths::b,paths::c");
    assert_eq!(meta(rows, "TwoAttrsBelow"), "paths::c,paths::a,paths::b");
    assert_eq!(meta(rows, "DeriveBetween"), "paths::a,paths::c,paths::b");
    assert_eq!(meta(rows, "OtherDerives"), "paths::a");
    is_announce::<paths::derives::AttrAbove>();
    is_announce::<paths::derives::AttrAboveBare>();
    is_announce::<paths::derives::AttrAboveTwoBare>();
    is_announce::<paths::derives::DeriveAbove>();
    is_announce::<paths::derives::DeriveAboveBare>();
    is_announce::<paths::derives::TwoAttrsAbove>();
    is_announce::<paths::derives::TwoAttrsBelow>();
    is_announce::<paths::derives::DeriveBetween>();
    is_announce::<paths::derives::OtherDerives>();
    // The other derives ran.
    let _ = format!("{:?}", paths::derives::OtherDerives(1).clone());
}

#[test]
fn an_owned_attribute_and_another_announcement() {
    let rows = rows_checks::ALL;
    // The attribute `my_lib` is the check's, wherever the announcements are: the
    // check saw it once, and the item compiles without it.
    assert_eq!(paths::SEEN_OWNEDANDANOTHER, ["my_lib(on_item)"]);
    assert_eq!(paths::SEEN_ANOTHERANDOWNED, ["my_lib(on_item)"]);
    // Both checks ran once (a second run would define the constant twice).
    assert_eq!(paths::TICK_OWNEDANDANOTHER_SECOND, 0);
    assert_eq!(paths::TICK_ANOTHERANDOWNED_FIRST, 0);
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
    is_announce::<paths::OwnedAndAnother>();
    is_announce::<paths::AnotherAndOwned>();
    let _ = paths::OwnedAndAnother(0);
    let _ = paths::AnotherAndOwned(0);
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
    let _ = paths::TwoOwners(0);
    is_announce::<paths::TwoOwners>();
}

#[test]
fn an_owned_attribute_with_a_macro_that_announces() {
    assert_eq!(paths::SEEN_OWNEDANDEMITTED, ["my_lib(on_item)"]);
    assert_eq!(paths::TICK_OWNEDANDEMITTED_EMITTED, 0);
    let m = meta(rows_checks::ALL, "OwnedAndEmitted");
    assert!(
        m.contains("record_attrs") && m.contains("tick(emitted)"),
        "{}",
        m
    );
    is_announce::<paths::OwnedAndEmitted>();
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
    assert_eq!(paths::SEEN_QUERIEDANDOWNED, ["my_lib(on_item)"]);
    assert_eq!(paths::TICK_QUERIEDANDOWNED_SHARED, 0);
    let _ = paths::QueriedAndOwned(0);
    is_announce::<paths::QueriedAndOwned>();
}

#[test]
fn an_item_that_is_not_emitted_is_announced_like_the_others() {
    let rows = rows_other::ALL;
    // The runner drops the item: one trampoline, no item, and the announcements below
    // and above are merged into it.
    assert_eq!(meta(rows, "NotEmittedPassthrough"), "paths::a");
    assert_eq!(meta(rows, "NotEmittedBelow"), "paths::a,paths::b");
    assert_eq!(meta(rows, "NotEmittedAbove"), "paths::b,paths::a");
    assert_eq!(meta(rows, "NotEmittedBetween"), "paths::a,paths::b");
    // A mock is compiled in a hidden module, and nothing else is emitted.
    assert_eq!(
        meta(rows, "MockedBelow"),
        "paths::a,paths::b,mock($crate::paths::Real)"
    );
    assert_eq!(
        meta(rows, "MockedAbove"),
        "paths::b,paths::a,mock($crate::paths::Real)"
    );
    assert_eq!(paths::SEEN_NOTEMITTEDOWNED, ["my_lib(never_compiled)"]);
    assert!(meta(rows, "NotEmittedOwned").contains("attrs(my_lib)"));
    assert_eq!(paths::SEEN_NOTEMITTEDTWOOWNERS, ["my_lib(never_compiled)"]);
    let m = meta(rows, "NotEmittedTwoOwners");
    for name in ["my_lib", "other", "tag", "paths::b"] {
        assert!(m.contains(name), "{}: {}", name, m);
    }
}

#[test]
fn macros_that_announce_what_they_produce() {
    let rows = rows_other::ALL;
    // The user's announcement has put the runner on the item, and the macro below it
    // announces into it: the entries of the first announcement come first.
    assert_eq!(meta(rows, "IntoRunner"), "paths::a,paths::b");
    assert_eq!(meta(rows, "TwoUnknown"), "paths::a,paths::b");
    let m = meta(rows, "TwoUnknownAndUser");
    for name in ["paths::u", "paths::a", "paths::b"] {
        assert!(m.contains(name), "{}: {}", name, m);
    }
    let m = meta(rows, "UserBetweenTwoUnknown");
    for name in ["paths::u", "paths::a", "paths::b"] {
        assert!(m.contains(name), "{}: {}", name, m);
    }
    is_announce::<paths::IntoRunner>();
    is_announce::<paths::TwoUnknown>();
    is_announce::<paths::TwoUnknownAndUser>();
    is_announce::<paths::UserBetweenTwoUnknown>();
}
