//! Items whose types do not resolve outside their module, and that have no `repeater`, announced
//! with `strict = false`. The definition does not fail: it records that in the META
//! (`type_leak_error(..)`), which the callbacks read (`tests/user/tests/recorded.rs`). Without
//! `strict = false` the same items fail at the definition (`tests/ui/tests/ui/leak_errors.rs`).

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

    /// Nothing to record.
    #[announce(strict = false)]
    pub struct Plain(pub u8);
}
