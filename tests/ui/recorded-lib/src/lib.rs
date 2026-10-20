//! Items whose types do not resolve outside their module, and that have no `repeater`: with
//! `strict = false` the definition records that. `tests/ui/tests/ui/recorded_type_leak.rs`
//! calls them from another crate.

pub mod m {
    use announce::announce;

    pub mod inner {
        pub struct UiLocal(pub u8);
        pub struct UiOther(pub u8);
        pub trait UiBase {}
    }
    use inner::{UiBase, UiLocal, UiOther};

    #[announce(strict = false)]
    pub struct Holder {
        pub a: UiLocal,
    }

    #[announce(strict = false)]
    pub struct Pair {
        pub a: UiLocal,
        pub b: UiOther,
    }

    #[announce(strict = false)]
    pub trait Sub: UiBase {
        fn local(&self) -> u8;
    }

    #[announce(strict = false)]
    pub struct Plain(pub u8);
}

pub use announce;

/// A macro of a library that announces the item it is given. The item is the user's, so the
/// error of the default check is at the user's tokens (`tests/ui/tests/ui/leak_errors_lib_macro.rs`).
#[macro_export]
macro_rules! announced {
    ($item:item) => {
        #[$crate::announce::announce(crate = $crate::announce)]
        $item
    };
}

/// A macro of the library that calls the trampoline of an item of the library through `$crate`,
/// with a callback of the caller that needs a type table, which is an error wherever it is
/// called. A proc macro cannot tell which crate that `$crate` is, so the texts of the error name
/// the crate that defined the item (`tests/ui/tests/ui/recorded_dollar_crate.rs`).
#[macro_export]
macro_rules! apply_pair {
    ($($callback:ident)::+, $name:ident) => {
        $crate::m::Pair! { [$crate::m::Pair!, $($callback)::+!] $name }
    };
}
