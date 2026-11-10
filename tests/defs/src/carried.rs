//! Types that name something of this module without being a path to it, announced with a
//! `repeater`: a trait object (`dyn Base`), an array length (`[u8; N]`) and a const generic
//! argument (`Wrap<{ N }>`). The repeater carries the whole type, which resolves here, so
//! another module or crate uses the item through `tests/user/tests/carried.rs`. Without a
//! `repeater` the same items fail at the definition (`tests/ui/tests/ui/value_paths.rs`,
//! `trait_paths_relative.rs`).

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
