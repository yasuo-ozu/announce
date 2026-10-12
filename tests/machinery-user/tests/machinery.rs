use announce_test_derive::summary;
use announce_test_machinery_user as lib;

macro_rules! capture {
    ([$($l:tt)*] $($t:tt)*) => { pub const OUT: &str = stringify!($($t)*); };
}

fn norm(s: &str) -> String {
    s.split_whitespace().collect()
}

mod point {
    use super::lib;
    lib::Point! { [lib::Point!, capture!] }
}

#[test]
fn the_rewritten_item_is_announced() {
    let out = norm(point::OUT);
    assert!(out.contains("pubstructPoint"), "{}", out);
    assert!(out.contains("pubtracked:bool"), "{}", out);
    // `cfg` is evaluated at the definition.
    assert!(out.contains("pubkept:i32"), "{}", out);
    assert!(!out.contains("gone"), "{}", out);
    assert!(!out.contains("#[cfg"), "{}", out);
}

mod chain {
    use super::{lib, summary};
    lib::Point! { [lib::Point!, lib::NamedAnnounce!, lib::Left!, lib::Right!, lib::Derived!, lib::PlainAnnounce!, summary!] ALL }
}

#[test]
fn definitions_chain_into_one_callback() {
    let rows: Vec<(&str, &str, &str)> = chain::ALL.to_vec();
    let items: Vec<&str> = rows.iter().map(|r| r.2).collect();
    assert_eq!(
        items,
        [
            "struct Point",
            "struct Named",
            "struct Left",
            "struct Right",
            "struct Derived",
            "struct PlainStruct"
        ]
    );
    let metas: Vec<String> = rows.iter().map(|r| norm(r.1)).collect();
    assert_eq!(metas[0], "");
    assert_eq!(metas[1], "test_lib::info(1)");
    assert_eq!(metas[2], "test_lib::pair(Left)");
    assert_eq!(metas[3], "test_lib::pair(Right)");
    assert_eq!(metas[4], "test_lib::derived");
    assert_eq!(metas[5], "");
}

#[test]
fn the_announce_impl_resolves_through_the_facade() {
    fn assert_announced<T: ?Sized + announce_test_machinery_lib::announce::Announce>() {}
    assert_announced::<lib::Point>();
    assert_announced::<lib::Named>();
    assert_announced::<lib::Left>();
    assert_announced::<lib::Right>();
    assert_announced::<lib::Derived>();
    assert_announced::<lib::PlainStruct>();
    // The item is emitted as written (and rewritten) by the macro.
    let p = lib::Point {
        x: 1,
        kept: 2,
        tracked: true,
    };
    assert_eq!((p.x, p.kept, p.tracked), (1, 2, true));
}

mod owned {
    use super::{lib, summary};
    lib::OwnedAnnounce! { [lib::OwnedAnnounce!, lib::OwnedDerivedAnnounce!, lib::OwnedPlainAnnounce!, summary!] ALL }
}

#[test]
fn checks_that_own_attributes_work_through_the_machinery() {
    let rows = owned::ALL;
    assert_eq!(rows.len(), 3);
    for (row, name) in rows.iter().zip(["Owned", "OwnedDerived", "OwnedPlain"]) {
        assert_eq!(row.2, format!("struct {}", name), "{:?}", row);
        // The check, and the names it owns.
        assert_eq!(
            norm(row.1),
            "::announce_test_machinery_lib::owner,attrs(field_attr)",
            "{:?}",
            row
        );
    }
    // The attribute route emits the item without the attribute.
    let o = lib::Owned {
        x: 1,
        y: 2,
        tracked: true,
    };
    assert_eq!(o.x + o.y, 3);
    // The derive route keeps the attribute: the derive declares it.
    assert_eq!(lib::OwnedDerived { y: 4 }.y, 4);
    assert_eq!(lib::OwnedPlain(5).0, 5);
}
