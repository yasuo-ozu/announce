//! Items announced with `strict = false` whose types do not resolve outside their module, and
//! that have no `repeater`: the definition records that instead of failing. A call in the module
//! of the item passes (here; `tests/user/tests/recorded.rs` uses what the calls make), a call
//! anywhere else is a compile error (`tests/ui/tests/ui/same_context*.rs`, `recorded_*.rs`).

use announce::announce;

pub mod m {
    use super::announce;

    pub mod inner {
        pub struct Local(pub u8);
        pub struct Other(pub u8);
        pub struct Wrap<'a>(pub &'a u8);
        pub trait Base {}
    }
    use inner::{Base, Local, Other, Wrap};

    const LEN: usize = 4;

    // In front of the item: the names of a module do not depend on the order of its items.
    Holder! { [Holder!, ::announce::same_context!, ::announce_test_derive::resolved_copy!] HolderBefore }

    #[announce(strict = false)]
    pub struct Holder {
        pub a: Local,
        pub b: u8,
    }

    #[announce(strict = false)]
    pub struct Pair {
        pub a: Local,
        pub b: Other,
    }

    #[announce(strict = false)]
    pub trait Sub: Base {
        fn local(&self) -> u8;
    }

    /// Every kind of cause in one item.
    #[announce(strict = false)]
    pub trait Many: Base {
        const M: usize = LEN;
        fn f(&self, a: Wrap<'_>, b: Local, c: [u8; LEN]) -> Other;
    }

    /// Nothing to record.
    #[announce(strict = false)]
    pub struct Plain(pub u8);

    // The copies name `Local` and `Base` as written.
    Holder! { [Holder!, ::announce_test_derive::resolved_copy!] HolderCopy }
    Pair! { [Pair!, ::announce_test_derive::resolved_copy!] PairCopy }
    Sub! { [Sub!, ::announce_test_derive::resolved_copy!] SubCopy }
    Plain! { [Plain!, ::announce_test_derive::resolved_copy!] PlainCopy }

    /// A macro of the module that builds the chain: its names are those of the module.
    macro_rules! through_the_check {
        ($item:ident, $name:ident) => {
            $item! { [$item!, ::announce::same_context!, ::announce_test_derive::resolved_copy!] $name }
        };
    }
    through_the_check!(Holder, HolderForwarded);

    // `announce::same_context!` in the middle of a chain passes the entries on, unchanged.
    Holder! {
        [
            Holder!,
            Pair!,
            Sub!,
            Many!,
            Plain!,
            ::announce::same_context!,
            ::announce_test_derive::leak_failures!
        ] CONTEXT_ROWS
    }
    // The entries that a later callback gathers have not passed it.
    Holder! {
        [Holder!, ::announce::same_context!, ::announce_test_derive::gathered!] (Pair, Sub) GATHERED
    }

    /// The item of the entry, as text.
    macro_rules! item_text {
        (@announce_format "1" $( ($($o:tt)*) )? [$($l:tt)*] $($p:ident)? $(:: $s:ident)* ! => [$($m:tt)*] { $($item:tt)* } $($r:tt)*) => {
            stringify!($($item)*)
        };
    }

    /// In an expression the check is a block, and the callback's expression is its value.
    pub fn context_in_expr() -> &'static str {
        Holder! { (expr) [Holder!, ::announce::same_context!, item_text!] }
    }

    macro_rules! method {
        (@announce_format "1" [$($l:tt)*] $($t:tt)*) => {
            pub fn from_the_item(&self) -> u8 {
                9
            }
        };
    }

    /// The check of `same_context!` is an item, which an `impl` cannot hold: the call goes beside
    /// the impl, and the call in the impl has no check (`tests/ui/tests/ui/same_context.rs`).
    pub struct Methods;

    Holder! { [Holder!, ::announce::same_context!] }

    impl Methods {
        Holder! { [Holder!, method!] }
    }

    pub fn in_fn() -> u8 {
        Holder! { [Holder!, ::announce_test_derive::resolved_copy!] HolderInFn }
        let h = HolderInFn { a: Local(5), b: 6 };
        h.a.0 + h.b
    }

    /// A submodule that brings the module's names in scope.
    pub mod child {
        use super::*;

        Holder! { [Holder!, ::announce::same_context!, ::announce_test_derive::resolved_copy!] HolderInChild }
    }
}

/// An item announced in a function body, checked there and in a nested block.
pub fn in_a_fn_body() -> u8 {
    struct Local(u8);

    #[allow(dead_code)]
    #[announce(strict = false)]
    struct Item {
        a: Local,
    }

    Item! { [Item!, ::announce::same_context!, ::announce_test_derive::resolved_copy!] Outer }
    let first = Outer { a: Local(1) }.a.0;
    let second = {
        Item! { [Item!, ::announce::same_context!, ::announce_test_derive::resolved_copy!] Inner }
        Inner { a: Local(2) }.a.0
    };
    first + second
}
