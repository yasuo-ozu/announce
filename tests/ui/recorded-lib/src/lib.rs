//! Items whose types do not resolve outside their module, announced with `strict = false`
//! (`tests/ui/tests/ui/same_context.rs`, `recorded_outside.rs` and `recorded_type_leak.rs` use
//! them from another crate), and macros of a library over them.

pub mod m {
    use announce::announce;

    pub mod inner {
        pub struct UiLocal(pub u8);
        pub trait UiBase {}
    }
    use inner::{UiBase, UiLocal};

    #[announce(strict = false)]
    pub struct Holder {
        pub a: UiLocal,
    }

    #[announce(strict = false)]
    pub trait Sub: UiBase {}

    #[announce(strict = false)]
    pub struct Plain(pub u8);

    // Two causes in one item.
    pub mod inner2 {
        pub struct UiOther(pub u8);
    }
    use inner2::UiOther;

    #[announce(strict = false)]
    pub struct Pair {
        pub a: UiLocal,
        pub b: UiOther,
    }
}

pub use announce;

/// Announces the item it is given: the item is the user's (`leak_errors.rs`).
#[macro_export]
macro_rules! announced {
    ($item:item) => {
        #[$crate::announce::announce(crate = $crate::announce)]
        $item
    };
}

/// Calls the trampoline of an item of the library through `$crate`. A proc macro cannot tell
/// which crate that is, so the texts of the error name the crate that defined the item.
#[macro_export]
macro_rules! check_holder {
    () => {
        $crate::m::Holder! { [$crate::m::Holder!, $crate::announce::same_context!] }
    };
}

/// The same for a callback of the caller that needs a type table.
#[macro_export]
macro_rules! apply_holder {
    ($($callback:ident)::+, $name:ident) => {
        $crate::m::Holder! { [$crate::m::Holder!, $($callback)::+!] $name }
    };
}
