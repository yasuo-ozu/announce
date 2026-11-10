// The same across crates: `UiLocal` is private to a module of announce-test-ui-lib, which has
// no `repeater` for it (recorded-lib/src/lib.rs). `announce::same_context!` in a call from
// this crate is an error at the type in the source of that crate, with the call in the note.
use announce_test_ui_lib::m;

m::Holder! { [m::Holder!, announce::same_context!] }

// Two types, and a callback after the check.
m::Pair! { [m::Pair!, announce::same_context!, announce_test_derive::leak_failures!] ROWS }

// A trait with a relative trait path.
m::Sub! { [m::Sub!, announce::same_context!] }

// Nothing was recorded for this one: it is fine.
m::Plain! { [m::Plain!, announce::same_context!] }

fn main() {}
