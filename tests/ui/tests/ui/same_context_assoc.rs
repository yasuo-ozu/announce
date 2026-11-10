// The check of `announce::same_context!` is an item (`const _: () = { .. };`), which cannot
// stand among the associated items of an `impl` or a `trait`: the call there fails, in the
// module of the item as well. The check goes beside the impl, at the call's item position, and the
// call in the impl has no `same_context!`; its answer is the same, as it depends on the module only
// (tests/user/tests/recorded.rs has the workaround).
mod m {
    pub struct UiLocal;

    #[announce::announce(strict = false)]
    pub struct Item {
        pub a: UiLocal,
    }

    pub struct S;

    // In the module of the item, and still an error: this is not an item position.
    impl S {
        Item! { [Item!, announce::same_context!] }
    }

    pub trait T {
        Item! { [Item!, announce::same_context!] }
    }
}

fn main() {}
