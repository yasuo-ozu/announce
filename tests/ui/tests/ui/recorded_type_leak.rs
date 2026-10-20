// A callback that needs a type table (`apply_leak!` reads `Entry::type_leak()?`) gets an error
// for an item that is recorded (announced with `strict = false`). A proc macro cannot tell where
// it is called, so the error is the same in the module of the item and anywhere else: a plain
// line for each cause, that says what the item names and what to do, at the path of the
// trampoline in the call, also for an item of another crate. A callback that can use the item
// as written reads `Entry::leak_failure` instead.
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

    // In the module of the items: the same error.
    Item! { [Item!, announce_test_derive::apply_leak!] InModule }
    pub fn in_fn() {
        Pair! { [Pair!, announce_test_derive::apply_leak!] InFn }
    }
}

// From the parent module, for one type, two and a trait path.
m::Item! { [m::Item!, announce_test_derive::apply_leak!] FromParent }
m::Pair! { [m::Pair!, announce_test_derive::apply_leak!] PairFromParent }
m::Sub! { [m::Sub!, announce_test_derive::apply_leak!] SubFromParent }

// A const generic that is not of a primitive type, and a type that cannot be carried.
m::ConstTy! { [m::ConstTy!, announce_test_derive::apply_leak!] ConstFromParent }
m::Elided! { [m::Elided!, announce_test_derive::apply_leak!] ElidedFromParent }

// An item of another crate: the error is at the call here, not in the source of the crate.
announce_test_ui_lib::m::Holder! { [announce_test_ui_lib::m::Holder!, announce_test_derive::apply_leak!] FromAnotherCrate }

fn main() {}
