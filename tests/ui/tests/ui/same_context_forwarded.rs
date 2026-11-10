// A `macro_rules!` that builds the chain and is written in the module of the item: invoked there
// it passes, and invoked anywhere else it fails like the call that it expands to, at the cause,
// with the path that the macro writes in the note.
#[macro_use]
mod m {
    pub struct UiLocal;

    #[announce::announce(strict = false)]
    pub struct Item {
        pub a: UiLocal,
    }

    macro_rules! through_the_check {
        () => {
            crate::m::Item! { [crate::m::Item!, announce::same_context!] }
        };
    }

    // In the module of the item: fine.
    through_the_check!();
}

// From the parent module, from a module of its own, and in a function body.
through_the_check!();

mod elsewhere {
    through_the_check!();

    pub fn in_a_fn() {
        through_the_check!();
    }
}

fn main() {}
