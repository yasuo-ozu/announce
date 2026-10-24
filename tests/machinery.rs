//! Items made by the macros of a library that announces them with `announce::machinery`
//! (`announce_tests::machinery`, re-exported with `announce` by `machinery_lib`): the crate
//! that defines them (`user_defs`) links the library only, and this file calls them.

use announce_tests::derive::summary;
use announce_tests::machinery_lib;
use subcrate::subcrate;
use user_defs as lib;

// No `announce` among the links: the macros and `crate = ..` name the re-export.
#[subcrate(machinery_lib)]
pub mod user_defs {
    use machinery_lib::{pair, tracked};

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
    #[machinery_lib::announce::announce(crate = ::machinery_lib::announce, macro_name = PlainAnnounce)]
    pub struct PlainStruct(pub u8);

    // A check that owns an attribute. The announcement removes it from the item it emits,
    // and the check sees it.
    #[tracked(::machinery_lib::owner, macro_name = OwnedAnnounce)]
    pub struct Owned {
        #[field_attr(x)]
        pub x: i32,
        pub y: i32,
    }

    #[machinery_lib::announce::announce(crate = ::machinery_lib::announce, macro_name = OwnedPlainAnnounce, ::machinery_lib::owner)]
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
    #[machinery_lib::announce::announce(crate = ::machinery_lib::announce, macro_name = DocsPlainAnnounce)]
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
    #[machinery_lib::announce::announce(crate = ::machinery_lib::announce, meta = test_lib::attr)]
    pub struct MergedBelow {
        pub a: u8,
    }

    // Here the attribute expands first and puts the runner on the item; `tracked` then
    // announces as well, and its options are merged into the runner.
    #[machinery_lib::announce::announce(crate = ::machinery_lib::announce, meta = test_lib::attr)]
    #[tracked(meta = test_lib::tracked)]
    pub struct MergedAbove {
        pub a: u8,
    }
}

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
    fn assert_announced<T: ?Sized + machinery_lib::announce::Announce>() {}
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
            "::machinery_lib::owner,attrs(field_attr)",
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
    fn assert_announced<T: ?Sized + machinery_lib::announce::Announce>() {}
    assert_announced::<lib::MergedBelow>();
    assert_announced::<lib::MergedAbove>();
    let m = lib::MergedAbove {
        a: 1,
        tracked: false,
    };
    assert_eq!(m.a, 1);
}
