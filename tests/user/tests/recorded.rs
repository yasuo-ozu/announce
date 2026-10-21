//! Definitions whose types do not resolve outside their module and have no `repeater`
//! (`tests/defs/src/recorded.rs`): the definition records that instead of failing, and
//! `Entry::item_resolved` (the callback `resolved_copy!`) makes the call in the module of
//! the item the only one that compiles. The copies made in that module are used here;
//! the calls from anywhere else are compile-fail tests (`tests/ui/tests/ui/recorded_*.rs`).

use announce_test_defs::recorded::m;

#[test]
fn copies_made_in_the_module_of_the_items_work() {
    let h = m::HolderCopy {
        a: m::inner::Local(1),
        b: 2,
    };
    assert_eq!((h.a.0, h.b), (1, 2));
    let p = m::PairCopy {
        a: m::inner::Local(3),
        b: m::inner::Other(4),
    };
    assert_eq!((p.a.0, p.b.0), (3, 4));

    struct X;
    impl m::inner::Base for X {}
    impl m::SubCopy for X {
        fn local(&self) -> u8 {
            9
        }
    }
    assert_eq!(m::SubCopy::local(&X), 9);

    // An item with nothing recorded is copied as it is, with no check.
    assert_eq!(m::PlainCopy(5).0, 5);
}

#[test]
fn copies_made_in_a_function_body_and_a_submodule_work() {
    assert_eq!(m::in_fn(), 11);
    let h = m::child::make();
    assert_eq!((h.a.0, h.b), (7, 8));
}

// What the callbacks read: a record for the three items, a table for `Leaky`, and neither for
// `Plain` and for a struct that never had a type to carry.
mod meta {
    pub mod rows {
        ::announce_test_defs::recorded::m::Holder! {
            [
                ::announce_test_defs::recorded::m::Holder!,
                ::announce_test_defs::recorded::m::Pair!,
                ::announce_test_defs::recorded::m::Sub!,
                ::announce_test_defs::recorded::m::Plain!,
                ::announce_test_defs::leaky::Leaky!,
                ::announce_test_derive::leak_failures!
            ] ROWS
        }
    }
}

/// The errors of a row of `leak_failures!`.
fn failure(row: &str) -> Vec<&str> {
    row.split('|').collect()
}

#[test]
fn the_meta_records_what_does_not_resolve() {
    let (rows, errs) = (meta::rows::ROWS, meta::rows::ROWS_ERR);
    assert_eq!(rows.len(), 5);

    let errors = failure(rows[0]);
    assert_eq!(errors, ["Local@without `repeater`"]);
    // Both types of a pair, once each, in the order of the fields.
    assert_eq!(
        failure(rows[1]),
        ["Local@without `repeater`", "Other@without `repeater`"]
    );
    // A relative trait path is recorded as that.
    assert_eq!(
        failure(rows[2]),
        ["Base@with a relative trait path in a bound"]
    );

    // No record, and a table, for the others.
    assert_eq!(rows[3], "-");
    assert_eq!(rows[4], "table");
    // `Entry::type_leak()` is an error exactly for the records.
    assert_eq!(errs, [true, true, true, false, false]);
}

// An item with every kind of cause has all of them in its record, not only the first kind that
// the analysis finds.
mod many {
    pub mod rows {
        ::announce_test_defs::recorded::m::Many! {
            [
                ::announce_test_defs::recorded::m::Many!,
                ::announce_test_derive::leak_failures!
            ] ROWS
        }
    }
}

#[test]
fn the_record_has_every_cause_of_an_item() {
    assert_eq!(many::rows::ROWS.len(), 1);
    assert_eq!(
        failure(many::rows::ROWS[0]),
        [
            "Base@with a relative trait path in a bound",
            "LEN@with a relative value path",
            "Wrap<'_>@with a type that cannot be carried",
            "Local@without `repeater`",
            "Other@without `repeater`",
            "[u8;LEN]@without `repeater`",
        ]
    );
    assert_eq!(many::rows::ROWS_ERR, [true]);
}

#[test]
fn a_record_names_the_crate_of_the_definer() {
    // The crate that `#[announce]` compiled, as Cargo names it, for the texts of a call that is
    // written through `$crate` (`tests/ui/tests/ui/recorded_dollar_crate.rs`). Only a record
    // has it.
    assert_eq!(
        meta::rows::ROWS_KRATE,
        [
            "announce_test_defs",
            "announce_test_defs",
            "announce_test_defs",
            "",
            ""
        ]
    );
}

// A table rewrites the item (`TypeLeak::apply`) and needs no check: the copy resolves
// anywhere, here outside the module that defines the types.
mod table {
    ::announce_test_defs::leaky::Leaky! {
        [::announce_test_defs::leaky::Leaky!, ::announce_test_derive::resolved_copy!] LeakyCopy
    }
}

#[test]
fn an_item_with_a_table_resolves_anywhere() {
    let v = table::LeakyCopy::<u8> {
        a: announce_test_defs::leaky::inner::Local(1),
        b: vec![2],
        c: 3,
    };
    assert_eq!((v.a.0, v.b, v.c), (1, vec![2], 3));
}

// `announce::same_context!` in the module of the items (tests/defs/src/recorded.rs): it
// passes the entries on to the next callback, with and without a record, and checks nothing
// that it does not need to.
#[test]
fn same_context_passes_the_entries_on() {
    let rows = m::CONTEXT_ROWS;
    assert_eq!(rows.len(), 3);
    assert_eq!(failure(rows[0]), ["Local@without `repeater`"]);
    assert_eq!(
        failure(rows[1]),
        ["Local@without `repeater`", "Other@without `repeater`"]
    );
    assert_eq!(rows[2], "-");
    assert_eq!(m::CONTEXT_ROWS_ERR, [true, true, false]);

    // In a function body, and in a submodule that brings the module's names in scope.
    assert_eq!(m::context_in_fn(), 1);
    assert_eq!(m::child::CHILD_ROWS.len(), 1);
    // In an expression: the check is a block, and the callback's expression is its value.
    assert!(
        m::context_in_expr()
            .split_whitespace()
            .collect::<String>()
            .starts_with("pubstructHolder{"),
        "{}",
        m::context_in_expr()
    );
    // A later callback gathers more definitions after the check.
    assert_eq!(m::GATHERED, ["Pair", "Sub"]);
}

// The check of `same_context!` is an item, so it cannot stand inside an `impl`: the definitions
// call it at module level, next to the impl, and call the callback in the impl without it
// (tests/defs/src/recorded.rs; `tests/ui/tests/ui/same_context_assoc.rs` is the error).
#[test]
fn the_check_goes_beside_an_impl() {
    assert_eq!(m::Methods.from_the_item(), 9);
    assert_eq!(m::Named::name(&m::Methods), "Methods");
}

// What the record carries for each cause: the message, why and what to write instead, which a
// call says in its errors. A relative type has the texts of announce, and what a repeater
// cannot carry has type-leak's own.
mod texts {
    pub mod rows {
        ::announce_test_defs::recorded::m::Holder! {
            [
                ::announce_test_defs::recorded::m::Holder!,
                ::announce_test_defs::recorded::m::Sub!,
                ::announce_test_defs::recorded::m::Elided!,
                ::announce_test_derive::leak_failures!
            ] ROWS
        }
    }
}

#[test]
fn the_record_carries_what_a_call_says_about_each_cause() {
    let (rows, texts) = (texts::rows::ROWS, texts::rows::ROWS_TEXTS);
    assert_eq!(rows.len(), 3);
    // A relative type: the message, no why, and what to do, with the item named.
    assert_eq!(
        texts[0],
        "use absolute path: 'Local'~~write `Local` with an absolute path (e.g. `crate::Local`), \
         or add `repeater = ..` to the `#[announce]` of `Holder`"
    );
    // A relative trait path.
    assert_eq!(
        texts[1],
        "use absolute path: 'Base'~~write `Base` with an absolute path (e.g. `crate::Base`)"
    );
    // A refusal of type-leak: its message, the cause that stops the type from being carried, and
    // what to write instead.
    assert_eq!(rows[2], "Wrap<'_>@with a type that cannot be carried");
    let (message, rest) = texts[2].split_once('~').unwrap();
    let (notes, help) = rest.split_once('~').unwrap();
    assert_eq!(message, "use absolute path: 'Wrap<'_>'");
    assert!(notes.contains("'_"), "{}", notes);
    assert!(help.contains("absolute path"), "{}", help);
}
