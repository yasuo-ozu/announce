// With `strict = true` (the default) the definition is checked: nothing here calls a trampoline, and
// each type that cannot be named outside its module is an error at its tokens, with no trampoline.
// With `strict = false` the same items are recorded, and the call checks them (`leak_not_strict.rs`).
::announce::define_repeater!(pub Repeater);
pub mod inner {
    #[derive(Clone)]
    pub struct UiLocal;
    pub struct UiOther;
    pub trait UiBase {}
    pub struct UiWrap<'a>(pub &'a u8);
    pub type UiIdx = usize;
    pub struct UiArr<const N: usize>(pub [u8; N]);
}

mod d {
    use super::inner::{UiArr, UiBase, UiLocal, UiOther, UiWrap};

    // A relative type without a `repeater`. No other attribute: no runner.
    #[announce::announce]
    pub struct Plain {
        pub a: UiLocal,
    }

    // The same behind a derive, which the runner waits for.
    #[announce::announce]
    #[derive(Clone)]
    pub struct Runner {
        pub a: UiLocal,
    }

    // One error for each thing.
    #[announce::announce]
    pub struct Two {
        pub a: UiLocal,
        pub b: UiOther,
    }

    // A relative trait path: a `repeater` would not help, and the message does not suggest one.
    #[announce::announce]
    pub trait Sub: UiBase {
        fn f(&self);
    }

    // The right-hand side of a type alias.
    #[announce::announce]
    pub type Alias = Option<UiLocal>;

    // With a `repeater`, which carries the type but not a trait path.
    #[announce::announce(repeater = crate::Repeater)]
    pub trait WithRepeater: UiBase {
        fn f(&self, a: UiOther);
    }

    // A type that a repeater cannot carry.
    #[announce::announce(repeater = crate::Repeater)]
    pub trait Elided {
        fn f(&self, a: UiWrap<'_>);
    }

    // A const generic that cannot be encoded.
    #[announce::announce(repeater = crate::Repeater)]
    pub struct ConstTy<const N: crate::inner::UiIdx>(pub UiArr<N>);

    // With a repeater that carries everything: fine.
    #[announce::announce(repeater = crate::Repeater)]
    pub struct Carried {
        pub a: UiLocal,
    }

    // Absolute paths: fine.
    #[announce::announce]
    pub struct Absolute {
        pub a: crate::inner::UiLocal,
    }
}

fn main() {}
