macro_rules! cb { ($($t:tt)*) => { 0 }; }

fn main() {
    // The derive route emits items: not usable in expression position.
    let _ = announce_test_defs::kinds::Greet! {
        (expr) [announce_test_defs::kinds::Greet!, #[derive(::announce_test_defs::conflict::Parse)] N, cb!]
    };
    // The same with the format prefix, as a call from a helper has it.
    let _ = announce_test_defs::kinds::Greet! {
        @announce_format "1" (expr) [announce_test_defs::kinds::Greet!, #[derive(::announce_test_defs::conflict::Parse)] N, cb!]
    };
}
