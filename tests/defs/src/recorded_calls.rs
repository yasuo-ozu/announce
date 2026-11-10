//! Where the call of an item announced with `strict = false` passes: the check of
//! `announce::same_context!` and `Entry::item_resolved` (the callback `resolved_copy!`) accept
//! every call in the module of the item, wherever in it. `tests/user/tests/recorded_calls.rs`
//! uses what the calls make. The calls that fail are in `tests/ui/tests/ui/same_context_*.rs`.

use announce::announce;

/// A call in front of the item and behind it.
pub mod order {
    use super::announce;

    pub struct Local(pub u8);

    // In front of the item: the names of a module do not depend on the order of its items.
    Early! { [Early!, ::announce::same_context!, ::announce_test_derive::resolved_copy!] BeforeCopy }

    #[announce(strict = false)]
    pub struct Early {
        pub a: Local,
    }

    Early! { [Early!, ::announce::same_context!, ::announce_test_derive::resolved_copy!] AfterCopy }
}

/// An item announced in the body of a function, checked there and in a nested block. The
/// copies are what the callback made of the item as written, so they are used at run time.
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

/// A `macro_rules!` of the module that builds the chain, invoked in the module: the names are
/// those of the module wherever the macro is written, so the check passes.
pub mod forwarded {
    use super::announce;

    pub struct Local(pub u8);
    pub struct Other(pub u8);

    #[announce(strict = false)]
    pub struct Item {
        pub a: Local,
    }

    #[announce(strict = false)]
    pub struct Second {
        pub a: Local,
        pub b: Other,
    }

    /// The chain of the check, then a callback, for the item that is named.
    macro_rules! through_the_check {
        ($item:ident, ($($callback:tt)*), $name:ident) => {
            $item! { [$item!, ::announce::same_context!, $($callback)* !] $name }
        };
    }

    /// The check alone, for several items.
    macro_rules! check_all {
        ($($item:ident),*) => {
            $( $item! { [$item!, ::announce::same_context!] } )*
        };
    }

    through_the_check!(Item, (::announce_test_derive::resolved_copy), ItemCopy);
    through_the_check!(Second, (::announce_test_derive::resolved_copy), SecondCopy);
    check_all!(Item, Second);

    pub fn made_by_the_macro() -> u8 {
        let item = ItemCopy { a: Local(3) };
        let second = SecondCopy {
            a: Local(4),
            b: Other(5),
        };
        item.a.0 + second.a.0 + second.b.0
    }
}

/// The announcements of one item are merged, and the item is recorded when one of them says
/// `strict = false`: in either order, with a derive (the runner) or without, and next to
/// `strict = true`.
pub mod merged {
    use super::announce;

    #[derive(Clone)]
    pub struct Local(pub u8);

    #[announce(strict = false)]
    #[announce]
    pub struct Plain {
        pub a: Local,
    }

    #[announce]
    #[announce(strict = false)]
    pub struct Reversed {
        pub a: Local,
    }

    #[announce(strict = false)]
    #[derive(Clone)]
    #[announce]
    pub struct Runner {
        pub a: Local,
    }

    #[announce]
    #[derive(Clone)]
    #[announce(strict = false)]
    pub struct RunnerReversed {
        pub a: Local,
    }

    #[announce(strict = true)]
    #[announce(strict = false)]
    pub struct Explicit {
        pub a: Local,
    }

    #[announce(strict = false)]
    #[derive(Clone)]
    #[announce(strict = true)]
    pub struct ExplicitReversed {
        pub a: Local,
    }

    // In the module of the items: the check passes for every one of them, and the rows have a
    // record for each (the order is that of the chain).
    Plain! {
        [
            Plain!,
            Reversed!,
            Runner!,
            RunnerReversed!,
            Explicit!,
            ExplicitReversed!,
            ::announce::same_context!,
            ::announce_test_derive::leak_failures!
        ] ROWS
    }
}
