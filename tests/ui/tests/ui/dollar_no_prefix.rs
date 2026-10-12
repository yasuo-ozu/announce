// `with_macro` carries `$` tokens. Through the derive/attribute route the
// trampoline needs the `@__announce_dollar { $ }` prefix that
// `announce_helper::redirect` adds. Called by hand without it, it must point at it.
macro_rules! cb { ($($t:tt)*) => {}; }

announce_test_defs::kinds::with_macro! {
    [#[derive(announce_test_defs::conflict::Parse)] N, cb!]
}

announce_test_defs::kinds::with_macro2! {
    [#[announce_test_defs::conflict::parse_attr], cb!]
}

// A payload without `$` does not need the prefix.
announce_test_defs::kinds::Greet! {
    [#[derive(announce_test_defs::conflict::Parse)] M, cb!]
}

fn main() {}
