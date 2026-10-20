// The announcements of one item are merged, and the item is strict unless an announcement says
// `strict = false`, in whatever order: `A` and `B` are recorded, and `C`, which no announcement
// relaxes, is an error at the definition (one error, not one for each announcement).
mod m {
    #[derive(Clone)]
    pub struct UiLocal;

    #[announce::announce(strict = false)]
    #[announce::announce]
    pub struct A {
        pub a: UiLocal,
    }

    #[announce::announce]
    #[announce::announce(strict = false)]
    pub struct B {
        pub a: UiLocal,
    }

    // With a derive of another trait between them.
    #[announce::announce(strict = false)]
    #[derive(Clone)]
    #[announce::announce]
    pub struct D {
        pub a: UiLocal,
    }

    #[announce::announce]
    #[announce::announce(meta = my_lib::info)]
    pub struct C {
        pub a: UiLocal,
    }

    // `strict = true` is written in one of them, and `strict = false` in the other.
    #[announce::announce(strict = true)]
    #[announce::announce(strict = false)]
    pub struct E {
        pub a: UiLocal,
    }

    #[announce::announce(strict = false)]
    #[derive(Clone)]
    #[announce::announce(strict = true)]
    pub struct F {
        pub a: UiLocal,
    }

    #[announce::announce]
    #[derive(Clone)]
    #[announce::announce(strict = false)]
    pub struct G {
        pub a: UiLocal,
    }

    // The recorded ones pass in the module.
    A! { [A!, announce::same_context!] }
    B! { [B!, announce::same_context!] }
    D! { [D!, announce::same_context!] }
    E! { [E!, announce::same_context!] }
    F! { [F!, announce::same_context!] }
    G! { [G!, announce::same_context!] }
}

// ... and fail at the call from outside.
m::A! { [m::A!, announce::same_context!] }
m::B! { [m::B!, announce::same_context!] }
m::D! { [m::D!, announce::same_context!] }
m::E! { [m::E!, announce::same_context!] }
m::F! { [m::F!, announce::same_context!] }
m::G! { [m::G!, announce::same_context!] }

fn main() {}
