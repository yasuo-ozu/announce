//! Items whose types do not resolve outside their module, and that have no `repeater`, announced
//! with `strict = false`. The definition does not fail: it records that in the META
//! (`type_leak_error(..)`), and puts a marker next to the item. The callback `resolved_copy!`
//! uses the item as written, so a call in the module of the item works (here, and in
//! `tests/user/tests/recorded.rs`), and a call anywhere else is a compile error
//! (`tests/ui/tests/ui/recorded_*.rs`). Without `strict = false` the same items fail at the
//! definition (`tests/ui/tests/ui/leak_errors.rs`).

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

    announce::define_repeater!(pub Repeater);

    /// A relative type.
    #[announce(strict = false)]
    pub struct Holder {
        pub a: Local,
        pub b: u8,
    }

    /// Two relative types.
    #[announce(strict = false)]
    pub struct Pair {
        pub a: Local,
        pub b: Other,
    }

    /// A relative trait path, which a repeater would not carry either.
    #[announce(strict = false)]
    pub trait Sub: Base {
        fn local(&self) -> u8;
    }

    /// A type that a repeater cannot carry: type-leak refuses it, and its texts are in the
    /// record.
    #[announce(strict = false, repeater = crate::recorded::m::Repeater)]
    pub trait Elided {
        fn f(&self, a: Wrap<'_>) -> u8;
    }

    const LEN: usize = 4;

    /// Every kind of cause in one item: a relative trait path, a relative value path, a type that
    /// type-leak refuses and relative types. They are all in the record.
    #[announce(strict = false)]
    pub trait Many: Base {
        fn f(&self, a: Wrap<'_>, b: Local, c: [u8; LEN]) -> Other;
    }

    /// Nothing to record.
    #[announce(strict = false)]
    pub struct Plain(pub u8);

    // In the module of the items: the copies name `Local` and `Base` as written.
    Holder! { [Holder!, ::announce_test_derive::resolved_copy!] HolderCopy }
    Pair! { [Pair!, ::announce_test_derive::resolved_copy!] PairCopy }
    Sub! { [Sub!, ::announce_test_derive::resolved_copy!] SubCopy }
    Plain! { [Plain!, ::announce_test_derive::resolved_copy!] PlainCopy }

    // `announce::same_context!` is the caller's way to require this context without a
    // callback of its own. In the middle of a chain it passes the entries on, unchanged: the
    // rows of `leak_failures!` are those of the three definitions, in the order of the chain
    // (`Plain` has no record and no check).
    Holder! {
        [
            Holder!,
            Pair!,
            Plain!,
            ::announce::same_context!,
            ::announce_test_derive::leak_failures!
        ] CONTEXT_ROWS
    }
    // The last element of a chain: nothing follows it.
    Sub! { [Sub!, ::announce::same_context!] }
    // The entries that a later callback gathers have not passed `same_context!`.
    Holder! {
        [Holder!, ::announce::same_context!, ::announce_test_derive::gathered!] (Pair, Sub) GATHERED
    }

    /// The item of the entry, as text: a callback for an expression.
    macro_rules! item_text {
        (@announce_format "1" $( ($($o:tt)*) )? [$($l:tt)*] $($p:ident)? $(:: $s:ident)* ! => [$($m:tt)*] { $($item:tt)* } $($r:tt)*) => {
            stringify!($($item)*)
        };
    }

    /// `same_context!` in the chain of an expression: its checks are statements of a block.
    pub fn context_in_expr() -> &'static str {
        Holder! { (expr) [Holder!, ::announce::same_context!, item_text!] }
    }

    /// `same_context!` in a function body of the module.
    pub fn context_in_fn() -> usize {
        Holder! { [Holder!, ::announce::same_context!, ::announce_test_derive::leak_failures!] ROWS }
        ROWS.len()
    }

    /// A callback of the plain kind that makes a method.
    macro_rules! method {
        (@announce_format "1" [$($l:tt)*] $($t:tt)*) => {
            pub fn from_the_item(&self) -> u8 {
                9
            }
        };
    }

    pub trait Named {
        fn name(&self) -> &'static str;
    }

    macro_rules! named {
        (@announce_format "1" [$($l:tt)*] $($t:tt)*) => {
            fn name(&self) -> &'static str {
                "Methods"
            }
        };
    }

    /// An `impl` takes no `const _`, which is what the check of `same_context!` is. The check is
    /// a call of its own beside the impl, at module level, and the call in the impl has no
    /// check: it gives the same answer, as it depends on the module only. A trait impl is the
    /// same. (`tests/ui/tests/ui/same_context_assoc.rs` is the error for the check inside.)
    pub struct Methods;

    Holder! { [Holder!, ::announce::same_context!] }

    impl Methods {
        Holder! { [Holder!, method!] }
    }

    impl Named for Methods {
        Holder! { [Holder!, named!] }
    }

    /// In a function body of the module.
    pub fn in_fn() -> u8 {
        Holder! { [Holder!, ::announce_test_derive::resolved_copy!] HolderInFn }
        let h = HolderInFn { a: Local(5), b: 6 };
        h.a.0 + h.b
    }

    /// In a submodule that brings the module's names in scope.
    pub mod child {
        use super::*;

        Holder! { [Holder!, ::announce_test_derive::resolved_copy!] HolderInChild }

        pub fn make() -> HolderInChild {
            HolderInChild { a: Local(7), b: 8 }
        }

        // `same_context!` is fine here as well.
        Holder! { [Holder!, ::announce::same_context!, ::announce_test_derive::leak_failures!] CHILD_ROWS }
    }
}
