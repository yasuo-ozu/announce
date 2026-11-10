// An item that a macro of a library announces (`#[$crate::announce::announce(crate =
// $crate::announce)]`) is the user's: the default check fails at the user's tokens, where the
// type is written, and the macro of the library is in the note about where the error comes from.
mod user {
    pub struct UiLocal(pub u8);

    announce_test_ui_lib::announced! {
        pub struct Item {
            pub a: UiLocal,
        }
    }
}

fn main() {}
