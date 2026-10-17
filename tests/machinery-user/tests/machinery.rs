use announce_test_derive::summary;
use announce_test_machinery_user as lib;

macro_rules! capture {
    (@announce_format "1" [$($l:tt)*] $($t:tt)*) => { pub const OUT: &str = stringify!($($t)*); };
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
    lib::Point! { [lib::Point!, lib::NamedAnnounce!, lib::Left!, lib::Right!, lib::PlainAnnounce!, summary!] ALL }
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
            "struct PlainStruct"
        ]
    );
    let metas: Vec<String> = rows.iter().map(|r| norm(r.1)).collect();
    assert_eq!(metas[0], "");
    assert_eq!(metas[1], "test_lib::info(1)");
    assert_eq!(metas[2], "test_lib::pair(Left)");
    assert_eq!(metas[3], "test_lib::pair(Right)");
    assert_eq!(metas[4], "");
}

#[test]
fn the_announce_impl_resolves_through_the_facade() {
    fn assert_announced<T: ?Sized + announce_test_machinery_lib::announce::Announce>() {}
    assert_announced::<lib::Point>();
    assert_announced::<lib::Named>();
    assert_announced::<lib::Left>();
    assert_announced::<lib::Right>();
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
    lib::OwnedAnnounce! { [lib::OwnedAnnounce!, lib::OwnedPlainAnnounce!, summary!] ALL }
}

#[test]
fn checks_that_own_attributes_work_through_the_machinery() {
    let rows = owned::ALL;
    assert_eq!(rows.len(), 2);
    for (row, name) in rows.iter().zip(["Owned", "OwnedPlain"]) {
        assert_eq!(row.2, format!("struct {}", name), "{:?}", row);
        // The check, and the names it owns.
        assert_eq!(
            norm(row.1),
            "::announce_test_machinery_lib::owner,attrs(field_attr)",
            "{:?}",
            row
        );
    }
    // The item is emitted without the attribute.
    let o = lib::Owned {
        x: 1,
        y: 2,
        tracked: true,
    };
    assert_eq!(o.x + o.y, 3);
    assert_eq!(lib::OwnedPlain(5).0, 5);
}

mod docs {
    use super::lib;
    lib::DocsAnnounce! { [lib::DocsAnnounce!, capture!] }
    pub mod kept {
        use super::lib;
        lib::DocsKeptAnnounce! { [lib::DocsKeptAnnounce!, capture!] }
    }
    pub mod plain {
        use super::lib;
        lib::DocsPlainAnnounce! { [lib::DocsPlainAnnounce!, capture!] }
    }
}

#[test]
fn docs_are_removed_from_the_payload_unless_asked() {
    // Default: gone, whatever way the item was announced.
    let out = norm(docs::OUT);
    assert!(
        out.ends_with("{pubstructDocs{pubx:u8,pubtracked:bool}}"),
        "{}",
        out
    );
    assert!(
        norm(docs::plain::OUT).ends_with("{pubstructDocsPlain{pubx:u8,}}"),
        "{}",
        norm(docs::plain::OUT)
    );
    // `docs = true` (a field of `announce::machinery::Options`): kept.
    let out = norm(docs::kept::OUT);
    assert!(
        out.contains("#[doc=\"DocofDocsKept.\"]pubstructDocsKept{#[doc=\"Docofx.\"]pubx:u8,"),
        "{}",
        out
    );
}

mod merged {
    use super::{lib, summary};
    lib::MergedBelow! { [lib::MergedBelow!, lib::MergedAbove!, summary!] ALL }
}

#[test]
fn machinery_announcements_merge_with_attributes() {
    // One trampoline each (a second would not compile), with both metas: the entries
    // of the announcement that expanded first come first.
    let rows = merged::ALL;
    assert_eq!(rows.len(), 2);
    for (row, name, metas) in [
        (&rows[0], "MergedBelow", "test_lib::tracked,test_lib::attr"),
        (&rows[1], "MergedAbove", "test_lib::attr,test_lib::tracked"),
    ] {
        assert_eq!(row.2, format!("struct {}", name), "{:?}", row);
        assert_eq!(norm(row.1), metas, "{:?}", row);
    }
    fn assert_announced<T: ?Sized + announce_test_machinery_lib::announce::Announce>() {}
    assert_announced::<lib::MergedBelow>();
    assert_announced::<lib::MergedAbove>();
    let m = lib::MergedAbove {
        a: 1,
        tracked: false,
    };
    assert_eq!(m.a, 1);
}
