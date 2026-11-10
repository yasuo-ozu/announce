// A relative trait path in a bound is recorded too, but rustc stops at a trait it cannot find in
// the bounds of an item before it checks the call: the error is its own, at the call.
use announce_test_ui_lib::m;

m::Sub! { [m::Sub!, announce_test_derive::resolved_copy!] SubFromHere }

fn main() {}
