//! Items announced with `strict = false` whose types do not resolve outside their module
//! (`tests/defs/src/recorded.rs`): the calls in the module of the item compile and make the copies
//! used here; the calls from anywhere else are compile-fail tests (`tests/ui/tests/ui`).

use announce_test_defs::recorded as r;
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
fn copies_made_in_front_of_the_item_in_a_macro_a_function_body_and_a_submodule_work() {
    let local = || m::inner::Local(7);
    assert_eq!(m::HolderBefore { a: local(), b: 1 }.a.0, 7);
    assert_eq!(m::HolderForwarded { a: local(), b: 2 }.b, 2);
    assert_eq!(m::child::HolderInChild { a: local(), b: 3 }.b, 3);
    assert_eq!(m::in_fn(), 11);
    assert_eq!(r::in_a_fn_body(), 3);
}

mod rows {
    use announce_test_defs::recorded::m;

    // What the callbacks read: a record for the items, a table for `Leaky`, neither for `Plain`.
    m::Holder! {
        [m::Holder!, m::Pair!, m::Sub!, m::Many!, m::Plain!, announce_test_defs::leaky::Leaky!, announce_test_derive::leak_failures!] ROWS
    }
}

fn failure(row: &str) -> Vec<&str> {
    row.split('|').collect()
}

#[test]
fn the_meta_records_every_cause_of_an_item() {
    let rows = rows::ROWS;
    assert_eq!(rows.len(), 6);
    assert_eq!(failure(rows[0]), ["Local@without `repeater`"]);
    // Both types of a pair, once each, in the order of the fields.
    assert_eq!(
        failure(rows[1]),
        ["Local@without `repeater`", "Other@without `repeater`"]
    );
    assert_eq!(
        failure(rows[2]),
        ["Base@with a relative trait path in a bound"]
    );
    // Every kind of cause, not only the first that the analysis finds.
    assert_eq!(
        failure(rows[3]),
        [
            "Base@with a relative trait path in a bound",
            "LEN@with a relative value path",
            "Wrap<'_>@with a type that cannot be carried",
            "Local@without `repeater`",
            "Other@without `repeater`",
            "[u8;LEN]@without `repeater`",
        ]
    );
    // No record, and a table, for the others.
    assert_eq!((rows[4], rows[5]), ("-", "table"));
    // `Entry::type_leak()` is an error exactly for the records.
    assert_eq!(rows::ROWS_ERR, [true, true, true, true, false, false]);
}

// A table rewrites the item (`TypeLeak::apply`) and needs no check: the copy resolves anywhere.
mod table {
    announce_test_defs::leaky::Leaky! {
        [announce_test_defs::leaky::Leaky!, announce_test_derive::resolved_copy!] LeakyCopy
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

#[test]
fn same_context_passes_the_entries_on() {
    assert_eq!(m::CONTEXT_ROWS, &rows::ROWS[..5]);
    assert_eq!(m::CONTEXT_ROWS_ERR, &rows::ROWS_ERR[..5]);
    // A later callback gathers more definitions after the check.
    assert_eq!(m::GATHERED, ["Pair", "Sub"]);
    // In an expression.
    let text: String = m::context_in_expr().split_whitespace().collect();
    assert!(text.starts_with("pubstructHolder{"), "{}", text);
}

#[test]
fn the_check_goes_beside_an_impl() {
    assert_eq!(m::Methods.from_the_item(), 9);
}
