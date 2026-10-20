// A relative trait path in a bound is recorded too, with `strict = false` (recorded-lib/src/lib.rs). But rustc
// stops at a trait it cannot find in the bounds of an item, before it checks the call: the
// error here is rustc's own, at the call, and the check of the module does not get to say why.
// Called in the module of the trait, it is fine.
use announce_test_ui_lib::m;

m::Sub! { [m::Sub!, announce_test_derive::resolved_copy!] SubFromHere }

fn main() {}
