// An item that has several things that cannot be named outside its module gets one error for
// each of them, all at once: a relative trait path, a relative value path, a type that cannot be
// carried and a relative type are found together (see `leak_errors.rs` for one cause each).
::announce::define_repeater!(pub Repeater);
pub mod inner {
    pub struct UiLocal;
    pub struct UiOther;
    pub trait UiBase {}
    pub struct UiWrap<'a>(pub &'a u8);
    pub struct UiArr<const N: usize>(pub [u8; N]);
}

mod d {
    use super::inner::{UiArr, UiBase, UiLocal, UiOther, UiWrap};

    const LEN: usize = 4;

    // A relative trait path does not hide the relative type.
    #[announce::announce]
    pub struct Mixed<T: UiBase> {
        pub a: UiLocal,
        pub t: T,
    }

    // Trait path, refused type, relative type and value path.
    #[announce::announce]
    pub trait Sub: UiBase {
        fn f(&self, a: UiWrap<'_>, b: UiLocal, c: [u8; LEN]);
    }

    // The right-hand side of a type alias.
    #[announce::announce]
    pub type Alias<T: UiBase> = Option<(UiArr<{ LEN }>, T)>;

    // With a `repeater` the relative type is carried: it is no error, the others still are.
    #[announce::announce(repeater = crate::Repeater)]
    pub trait WithRepeater: UiBase {
        fn f(&self, a: UiOther, b: UiWrap<'_>, c: [u8; LEN]);
    }

    // A relative type that has a cause in it is reported as written.
    #[announce::announce]
    pub struct Inside {
        pub a: UiArr<{ LEN }>,
        pub b: ::std::boxed::Box<dyn UiBase>,
    }
}

fn main() {}
