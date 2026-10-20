// The same across crates: `Local` is private to a module of announce-test-ui-lib, which has
// no `repeater` for it (recorded-lib/src/lib.rs). Calling the trampoline from this crate
// is an error: rustc's own at the call, and the check at the cause in that crate.
use announce_test_ui_lib::m;

m::Holder! { [m::Holder!, announce_test_derive::resolved_copy!] FromHere }

// Two types.
m::Pair! { [m::Pair!, announce_test_derive::resolved_copy!] PairFromHere }

// Nothing was recorded for this one: it is fine.
m::Plain! { [m::Plain!, announce_test_derive::resolved_copy!] PlainFromHere }

fn main() {}
