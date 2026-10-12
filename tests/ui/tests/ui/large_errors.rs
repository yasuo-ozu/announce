// A large item's trampoline holds the item once (DESIGN.md section 4.1). The
// calls that are errors for every trampoline say the same; the one that only
// that layout has is a derive or attribute head written by hand, without the
// path `announce_helper::redirect` adds.
macro_rules! cb { ($($t:tt)*) => {}; }

// Without `redirect`, a large trampoline cannot call itself.
announce_test_defs::large::BigTrait! {
    [#[derive(announce_test_defs::conflict::Parse)] N, cb!]
}
announce_test_defs::large::big_mod! {
    [#[announce_test_defs::conflict::parse_attr], cb!]
}

// No continuation, and not a list.
announce_test_defs::large::BigTrait! { [announce_test_defs::large::BigTrait!] }
announce_test_defs::large::big_mod! { not a list }

// The derive route emits items: not usable in expression position.
fn main() {
    let _ = announce_test_defs::large::BigStruct! {
        (expr) [announce_test_defs::large::BigStruct!, #[derive(::announce_test_defs::large::BigParse)] N, cb!]
    };
}
