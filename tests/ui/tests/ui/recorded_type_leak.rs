// A callback that needs a type table (`apply_leak!` reads `Entry::type_leak()?`) gets an error
// for an item announced with `strict = false`, wherever it is called (a proc macro cannot tell):
// a line for each cause, with the whole path of the trampoline of the call underlined, also for an
// item of another crate and through its `$crate` macro.
::announce::define_repeater!(pub Repeater);
pub mod inner {
    pub struct UiLocal;
    pub struct UiOther;
    pub trait UiBase {}
    pub type UiIdx = usize;
    pub struct UiArr<const N: usize>(pub [u8; N]);
    pub struct UiWrap<'a>(pub &'a u8);
}

mod m {
    use super::inner::{UiArr, UiBase, UiLocal, UiOther, UiWrap};

    #[announce::announce(strict = false)]
    pub struct Item {
        pub a: UiLocal,
    }

    #[announce::announce(strict = false)]
    pub struct Pair {
        pub a: UiLocal,
        pub b: UiOther,
    }

    #[announce::announce(strict = false)]
    pub trait Sub: UiBase {}

    // What a table cannot carry is not about scope.
    #[announce::announce(strict = false, repeater = crate::Repeater)]
    pub struct ConstTy<const N: crate::inner::UiIdx>(pub UiArr<N>);

    #[announce::announce(strict = false, repeater = crate::Repeater)]
    pub trait Elided {
        fn f(&self, a: UiWrap<'_>);
    }

    // In the module of the item: the same error.
    Item! { [Item!, announce_test_derive::apply_leak!] InModule }
}

m::Pair! { [m::Pair!, announce_test_derive::apply_leak!] PairFromParent }
m::Sub! { [m::Sub!, announce_test_derive::apply_leak!] SubFromParent }
m::ConstTy! { [m::ConstTy!, announce_test_derive::apply_leak!] ConstFromParent }
m::Elided! { [m::Elided!, announce_test_derive::apply_leak!] ElidedFromParent }
announce_test_ui_lib::m::Holder! { [announce_test_ui_lib::m::Holder!, announce_test_derive::apply_leak!] FromAnotherCrate }
announce_test_ui_lib::apply_holder!(announce_test_derive::apply_leak, FromTheLibrary);

fn main() {}
