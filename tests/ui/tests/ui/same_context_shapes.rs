// Several causes in one item. A type that is written in a tuple, in an array and as a field
// is one cause, at its first occurrence: the error is at the type there, and a second relative
// type is another error. The error is at the name of the type and names it (`UiShared`, not
// `&'static UiShared`), not at the `&` or `*const` that stands before it in a field.
mod m {
    pub struct UiLocal(pub u8);
    pub struct UiOther(pub u8);
    pub struct UiShared(pub u8);
    pub struct UiRaw(pub u8);
    pub struct UiUnique(pub u8);

    #[announce::announce(strict = false)]
    pub struct Item {
        pub a: (UiLocal, u8),
        pub b: [UiLocal; 2],
        pub c: UiLocal,
        pub d: UiOther,
    }

    // A type behind a reference or a raw pointer: the error is at the name of the type.
    #[announce::announce(strict = false)]
    pub struct Behind {
        pub a: &'static UiShared,
        pub b: *const UiRaw,
        pub c: &'static mut UiUnique,
    }

    // A generic item: its own parameters are not causes.
    #[announce::announce(strict = false)]
    pub struct Generic<T> {
        pub a: (UiOther, T),
        pub b: [UiOther; 2],
        pub c: T,
    }
}

m::Item! { [m::Item!, announce::same_context!] }
m::Behind! { [m::Behind!, announce::same_context!] }
m::Generic! { [m::Generic!, announce::same_context!] }

fn main() {}
