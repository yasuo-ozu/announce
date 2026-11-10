// `announce::same_context!` in the chain of a trampoline call requires that the call is in the
// module of the items (the type context of their definitions): in the module, in a function
// body and in a submodule that does `use super::*`. Anywhere else it is an error at the type in
// the definition, which says why, and the call is in the note. Nothing of the item is emitted.
mod m {
    pub struct UiLocal;
    pub struct UiOther;

    #[announce::announce(strict = false)]
    pub struct Item {
        pub a: UiLocal,
    }

    #[announce::announce(strict = false)]
    pub struct Pair {
        pub a: UiLocal,
        pub b: UiOther,
    }

    // Nothing was recorded for this one: it passes everywhere.
    #[announce::announce(strict = false)]
    pub struct Plain(pub u8);

    // Fine: in the module, in a function body, and in a submodule that brings its names in scope.
    Item! { [Item!, announce::same_context!] }
    pub fn in_fn() {
        Pair! { [Pair!, announce::same_context!] }
    }
    pub mod child {
        use super::*;

        Item! { [Item!, announce::same_context!] }
    }
}

// From the parent module.
m::Item! { [m::Item!, announce::same_context!] }

// From a sibling module, for two types.
mod sibling {
    super::m::Pair! { [super::m::Pair!, announce::same_context!] }
}

// In the middle of a longer chain, for several definitions: one error for each of those with a
// record, and none for `Plain`.
m::Item! { [m::Item!, m::Pair!, m::Plain!, announce::same_context!, announce_test_derive::leak_failures!] ROWS }

// A module of its own that does not `use super::*`.
mod apart {
    pub fn call() {
        crate::m::Item! { [crate::m::Item!, announce::same_context!] }
    }
}

fn main() {}
