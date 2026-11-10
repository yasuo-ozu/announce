// A macro of the definer's library calls the trampoline of an item of that library through
// `$crate` (recorded-lib/src/lib.rs). A proc macro cannot tell which crate a `$crate` is, so the
// texts name the crate that defined the item, as Cargo names it: `announce_test_ui_lib::m::Holder`,
// and not `$crate::m::Holder`. The check of the module is an error here, and so is a callback that
// needs a type table.
announce_test_ui_lib::check_holder!();

announce_test_ui_lib::apply_pair!(announce_test_derive::apply_leak, FromTheLibrary);

fn main() {}
