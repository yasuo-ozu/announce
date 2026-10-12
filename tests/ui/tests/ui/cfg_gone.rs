// An item-level `#[cfg]` that is false in the defining crate removes the item, and
// with it the trampoline: there is no `Gone!` to call (`cfg_gone` in tests/defs).
// The names come in through globs, so that the error is the plain "cannot find
// macro": a path call would add rustc's notes about configured-out items, and the
// snapshot would test rustc's wording.
#[allow(unused_imports)]
use announce_test_defs::gone::*;
#[allow(unused_imports)]
use announce_test_defs::kinds::*;

macro_rules! cb { ($($t:tt)*) => {}; }

// `#[announce]` first, then a false `#[cfg]`.
AnnounceFirst! { [announce_test_defs::gone::AnnounceFirst!, cb!] }

// A false `#[cfg]` before `#[announce]`: the attribute macro never runs.
CfgFirst! { [announce_test_defs::gone::CfgFirst!, cb!] }

// The `cfg(feature = "x")` of tests/defs is off there, whatever this crate enables.
GoneWithoutX! { [announce_test_defs::kinds::GoneWithoutX!, cb!] }

// A true `#[cfg]` leaves the trampoline: the call below resolves (and is the only
// one of the four that does; `tests/user` expands it).
Present! { [announce_test_defs::gone::Present!, cb!] }

fn main() {}
