// `announce::same_context!` in a call requires that the call is in the module of the item. Each
// cause of an item announced with `strict = false` is an error at its tokens in the definition,
// with the call in the note; an item without a record passes anywhere.
#[macro_use]
mod m {
    pub struct UiLocal;
    pub struct UiOther;

    #[announce::announce(strict = false)]
    pub struct Item {
        pub a: UiLocal,
    }

    #[announce::announce(strict = false)]
    pub struct Second {
        pub a: UiOther,
    }

    #[announce::announce(strict = false)]
    pub struct Plain(pub u8);

    macro_rules! through_the_check {
        () => {
            crate::m::Item! { [crate::m::Item!, announce::same_context!] }
        };
    }

    // Fine: the module (also through a macro written here), a function body, and a submodule that
    // brings the names in scope.
    Item! { [Item!, announce::same_context!] }
    through_the_check!();
    pub fn in_fn() {
        Second! { [Second!, announce::same_context!] }
    }
    pub mod with_the_names {
        use super::*;

        Item! { [Item!, announce::same_context!] }

        pub fn in_fn() {
            Item! { [Item!, announce::same_context!] }
        }
    }

    // A submodule that does not: in an item and in a function body.
    mod without_the_names {
        super::Item! { [super::Item!, announce::same_context!] }

        pub fn in_fn() {
            super::Item! { [super::Item!, announce::same_context!] }
        }
    }

    // The check is an item, which cannot stand among the associated items of an `impl` or a trait
    // (the check goes beside the impl, tests/defs/src/recorded.rs).
    pub struct S;

    impl S {
        Item! { [Item!, announce::same_context!] }
    }

    pub trait T {
        Item! { [Item!, announce::same_context!] }
    }
}

// From the parent module, in the middle of a chain: an error for each item with a record, none
// for `Plain`.
m::Item! { [m::Item!, m::Second!, m::Plain!, announce::same_context!, announce_test_derive::leak_failures!] ROWS }

// From a sibling module, also through the macro of `m`.
mod sibling {
    super::m::Item! { [super::m::Item!, announce::same_context!] }

    through_the_check!();
}

// The announcements of one item are merged, and `strict = false` in any of them makes a record, in
// either order, with a derive between them and next to `strict = true`.
mod merged {
    #[derive(Clone)]
    pub struct UiLocal;

    #[announce::announce(strict = false)]
    #[announce::announce]
    pub struct A {
        pub a: UiLocal,
    }

    #[announce::announce(strict = false)]
    #[derive(Clone)]
    #[announce::announce(strict = true)]
    pub struct F {
        pub a: UiLocal,
    }

    #[announce::announce]
    #[derive(Clone)]
    #[announce::announce(strict = false)]
    pub struct G {
        pub a: UiLocal,
    }
}

merged::A! { [merged::A!, merged::F!, merged::G!, announce::same_context!] }

// An item of another crate (announce-test-ui-lib), also through its `$crate` macro: the error is
// at the type in the source of that crate, and the texts name that crate. `Plain` has no record.
announce_test_ui_lib::m::Holder! { [announce_test_ui_lib::m::Holder!, announce::same_context!] }
announce_test_ui_lib::m::Plain! { [announce_test_ui_lib::m::Plain!, announce::same_context!] }
announce_test_ui_lib::check_holder!();

fn main() {}
