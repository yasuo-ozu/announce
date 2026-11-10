// What a repeater cannot carry is a cause of its own: type-leak refuses the type, and the error
// of the call has type-leak's message, why, and what to write instead, at the type in the
// definition. The call is in the note about the bound. A std prelude name says to write its
// absolute path; `Self` in a relative type says why it cannot be carried.
::announce::define_repeater!(pub Repeater);
pub mod inner {
    pub struct UiWrap<'a>(pub &'a u8);
    pub struct UiG<T>(pub T);
}

mod d {
    use super::inner::{UiG, UiWrap};

    // An elided lifetime keeps a relative type from being carried.
    #[announce::announce(strict = false, repeater = crate::Repeater)]
    pub trait Elided {
        fn f(&self, a: UiWrap<'_>);
    }

    // A prelude name is another cause, with the absolute path to write.
    #[announce::announce(strict = false, repeater = crate::Repeater)]
    pub trait Prelude {
        fn f(&self, a: Vec<&'_ u8>);
    }

    // A relative type cannot name `Self::A`.
    #[announce::announce(strict = false, repeater = crate::Repeater)]
    pub trait SelfInside {
        type A;
        fn g(&self) -> UiG<Self::A>;
    }

    // A prelude name that no table is made of.
    #[announce::announce(strict = false)]
    pub type Alias = Option<u8>;

    // Fine inside the module.
    Elided! { [Elided!, announce::same_context!] }
    Prelude! { [Prelude!, announce::same_context!] }
}

d::Elided! { [d::Elided!, announce::same_context!] }
d::Prelude! { [d::Prelude!, announce::same_context!] }
d::SelfInside! { [d::SelfInside!, announce::same_context!] }
d::Alias! { [d::Alias!, announce::same_context!] }

fn main() {}
