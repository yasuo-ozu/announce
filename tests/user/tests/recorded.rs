//! Definitions whose types do not resolve outside their module and have no `repeater`
//! (`tests/defs/src/recorded.rs`): with `strict = false` the definition records that instead
//! of failing, and the callbacks read the record.

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
    // A relative trait path is recorded as that; the type `Local` is not looked at.
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
