// A definition whose types do not resolve outside its module is an error, unless it is
// announced with `strict = false`: then it records that, and the callback decides.
// `resolved_copy!` (`Entry::item_resolved`) uses the item as written, so a call in the module
// of the item is fine, and any other call is an error: rustc's own, at the names that do not
// resolve, and the check of the item, at the cause, which says why.
mod m {
    pub struct UiLocal;
    pub struct UiOther;

    #[announce::announce(strict = false)]
    pub struct Item {
        pub a: UiLocal,
    }

    #[announce::announce(strict = false)]
    pub struct Pair {
        pub a: UiLocal,
        pub b: UiOther,
    }

    // In the module of the items: fine, also in a function body.
    Item! { [Item!, announce_test_derive::resolved_copy!] InModule }
    pub fn in_fn() {
        Pair! { [Pair!, announce_test_derive::resolved_copy!] InFn }
    }
}

// From the parent module.
m::Item! { [m::Item!, announce_test_derive::resolved_copy!] FromParent }

// From a sibling module, for two types.
mod sibling {
    super::m::Pair! { [super::m::Pair!, announce_test_derive::resolved_copy!] FromSibling }
}

fn main() {}
