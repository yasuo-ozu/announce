// `strict = false` turns the errors of leak_errors.rs into a record: the definitions below
// are the same items, and none of them fails. The call decides: in the module of the items it is
// fine (a function body and a submodule too), and `announce::same_context!` anywhere else is an
// error at the type in the definition, which says why, with the call in the note.
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

    #[announce::announce(strict = false)]
    pub struct Plain {
        pub a: UiLocal,
    }

    #[announce::announce(strict = false)]
    #[derive(Clone)]
    pub struct Runner {
        pub a: UiLocal,
    }

    #[announce::announce(strict = false)]
    pub struct Two {
        pub a: UiLocal,
        pub b: UiOther,
    }

    #[announce::announce(strict = false)]
    pub trait Sub: UiBase {
        fn f(&self);
    }

    #[announce::announce(strict = false)]
    pub type Alias = Option<UiLocal>;

    #[announce::announce(strict = false, repeater = crate::Repeater)]
    pub trait WithRepeater: UiBase {
        fn f(&self, a: UiOther);
    }

    #[announce::announce(strict = false, repeater = crate::Repeater)]
    pub trait Elided {
        fn f(&self, a: UiWrap<'_>);
    }

    #[announce::announce(strict = false, repeater = crate::Repeater)]
    pub struct ConstTy<const N: crate::inner::UiIdx>(pub UiArr<N>);

    // In the module: fine.
    Plain! { [Plain!, announce::same_context!] }
    Two! { [Two!, announce::same_context!] }
    Alias! { [Alias!, announce::same_context!] }
    ConstTy! { [ConstTy!, announce::same_context!] }
    pub fn in_fn() {
        Runner! { [Runner!, announce::same_context!] }
    }
}

// From outside the module: one error for each cause of each item that has a record, at the
// cause in the definition, with the call in the note.
d::Plain! { [d::Plain!, announce::same_context!] }
d::Runner! { [d::Runner!, announce::same_context!] }
d::Two! { [d::Two!, announce::same_context!] }
d::Alias! { [d::Alias!, announce::same_context!] }
d::ConstTy! { [d::ConstTy!, announce::same_context!] }

fn main() {}
