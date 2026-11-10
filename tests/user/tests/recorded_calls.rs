//! The calls of items announced with `strict = false` that pass (`tests/defs/src/recorded_calls.rs`):
//! everything in the module of the item. They compile there; here is what they made. The calls
//! that fail are compile-fail tests (`tests/ui/tests/ui/same_context_*.rs`).

use announce_test_defs::recorded_calls as c;

#[test]
fn a_call_in_front_of_the_item_and_one_behind_it_make_the_same_copy() {
    let before = c::order::BeforeCopy {
        a: c::order::Local(1),
    };
    let after = c::order::AfterCopy {
        a: c::order::Local(2),
    };
    assert_eq!((before.a.0, after.a.0), (1, 2));
}

#[test]
fn an_item_announced_in_a_function_body_is_checked_in_that_body_and_in_its_blocks() {
    assert_eq!(c::in_a_fn_body(), 3);
}

#[test]
fn a_macro_of_the_module_that_builds_the_chain_passes_in_the_module() {
    assert_eq!(c::forwarded::made_by_the_macro(), 12);
}

#[test]
fn announcements_that_merge_make_a_record_when_one_says_so() {
    // `Plain`, `Reversed`, `Runner`, `RunnerReversed`, `Explicit` and `ExplicitReversed`, in the
    // order of the chain: each has the record of its one type, and `type_leak()` fails for it.
    assert_eq!(c::merged::ROWS, ["Local@without `repeater`"; 6]);
    assert_eq!(c::merged::ROWS_ERR, [true; 6]);
}
