// A macro of the definer's library calls the trampoline of an item of that library through
// `$crate` (recorded-lib/src/lib.rs), with a callback that needs a type table. A proc macro cannot
// tell which crate a `$crate` is, so the texts name the crate that defined the item, as Cargo
// names it: `announce_test_ui_lib::m::Pair`, and not `$crate::m::Pair`.
announce_test_ui_lib::apply_pair!(announce_test_derive::apply_leak, FromTheLibrary);

fn main() {}
