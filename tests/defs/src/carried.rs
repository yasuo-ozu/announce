//! Types that name something of this module without being a path to it, carried by a `repeater`:
//! a trait object, an array length and a const generic argument. Without a `repeater` the same
//! items fail (`tests/ui/tests/ui/value_paths.rs`, `trait_paths_relative.rs`).

use announce::announce;

pub mod m {
    use super::announce;

    pub trait Base {
        fn n(&self) -> u8;
    }

    pub const N: usize = 3;

    pub mod inner {
        pub struct Wrap<const K: usize>(pub [u8; K]);
    }
    use inner::Wrap;

    announce::define_repeater!(pub Repeater);

    #[announce(repeater = crate::carried::m::Repeater)]
    pub struct Item {
        pub a: ::std::boxed::Box<dyn Base>,
        pub b: [u8; N],
        pub c: Wrap<{ N }>,
    }

    #[announce(repeater = crate::carried::m::Repeater)]
    pub trait Make {
        fn object(&self) -> ::std::boxed::Box<dyn Base + Send>;
        fn array(&self) -> [u8; N];
        fn wrap(&self) -> Wrap<{ N }>;
    }
}
