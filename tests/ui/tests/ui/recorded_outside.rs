// `Entry::item_resolved` (the callback `resolved_copy!`) uses an item announced with
// `strict = false` as written, so a call in the module of the item is fine and any other is an
// error: rustc's own at the names that do not resolve, and the check of the item at the cause.
mod m {
    pub struct UiLocal;

    #[announce::announce(strict = false)]
    pub struct Item {
        pub a: UiLocal,
    }

    Item! { [Item!, announce_test_derive::resolved_copy!] InModule }
    pub fn in_fn() {
        Item! { [Item!, announce_test_derive::resolved_copy!] InFn }
    }
}

// From the parent module, a sibling module and another crate.
m::Item! { [m::Item!, announce_test_derive::resolved_copy!] FromParent }

mod sibling {
    super::m::Item! { [super::m::Item!, announce_test_derive::resolved_copy!] FromSibling }
}

announce_test_ui_lib::m::Holder! { [announce_test_ui_lib::m::Holder!, announce_test_derive::resolved_copy!] FromHere }
announce_test_ui_lib::m::Plain! { [announce_test_ui_lib::m::Plain!, announce_test_derive::resolved_copy!] PlainFromHere }

fn main() {}
