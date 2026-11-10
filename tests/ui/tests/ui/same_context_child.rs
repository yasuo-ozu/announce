// A child module is not the module of the item: it sees the names of the item only when it
// brings them in scope with `use super::*`. One that does not fails, in its items and in its
// function bodies; one that does passes.
mod m {
    pub struct UiLocal;

    #[announce::announce(strict = false)]
    pub struct Item {
        pub a: UiLocal,
    }

    mod without_the_names {
        super::Item! { [super::Item!, announce::same_context!] }

        pub fn in_a_fn() {
            super::Item! { [super::Item!, announce::same_context!] }
        }
    }

    mod with_the_names {
        use super::*;

        Item! { [Item!, announce::same_context!] }

        pub fn in_a_fn() {
            Item! { [Item!, announce::same_context!] }
        }
    }
}

fn main() {}
