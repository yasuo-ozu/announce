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
        fn f(&self, a: UiWrap<'_>, b: UiLocal, c: [u8; LEN]) -> UiOther;
    }
}

m::Many! { [m::Many!, announce::same_context!] }

fn main() {}
