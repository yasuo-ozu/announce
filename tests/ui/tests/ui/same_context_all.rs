// The record of an item has every cause (leak_errors_all.rs is the same item without
// `strict = false`), and the check of a call outside the module says each of them.
mod m {
    pub struct UiLocal;
    pub struct UiOther;
    pub trait UiBase {}
    pub struct UiWrap<'a>(pub &'a u8);
    const LEN: usize = 4;

    #[announce::announce(strict = false)]
    pub trait Many: UiBase {
        const M: usize = LEN;
        fn f(&self, a: UiWrap<'_>, b: UiLocal, c: [u8; LEN]) -> UiOther;
    }
}

m::Many! { [m::Many!, announce::same_context!] }

// The trait of a trait object under a reference and the trait of a qualified path cannot be
// carried: the check says each of them, next to the relative type.
mod o {
    pub struct UiLocal;
    pub trait UiBase {}
    pub trait UiAssoc {
        type X;
    }

    #[announce::announce(strict = false)]
    pub trait Objects: crate::o::UiAssoc {
        fn f(&self, a: &dyn UiBase, b: UiLocal) -> <Self as UiAssoc>::X;
    }
}

o::Objects! { [o::Objects!, announce::same_context!] }

fn main() {}
