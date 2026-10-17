//! A `#![no_std]` crate that defines announced items. `announce` is used with
//! `default-features = false`, so nothing of proc-macro2 or syn is built for the
//! target: `cargo build -p announce-test-nostd --target x86_64-unknown-none`
//! must work (the `check` job of CI does it).
//!
//! `tests/nostd.rs` calls the trampolines from a crate with std.
#![no_std]
// The derive that the author writes is deprecated; `Derived` below is a test of it.
#![allow(deprecated)]

use announce::announce;

#[announce]
pub struct Named {
    pub a: u32,
    pub b: u8,
}

#[announce]
pub struct Tuple<T>(pub T, pub u8);

#[announce]
pub enum Shape {
    Circle(u32),
    Rect { w: u32, h: u32 },
}

#[announce]
#[derive(Clone, Copy)]
pub union Bits {
    pub i: u32,
    pub f: f32,
}

#[announce]
pub trait Greet {
    fn hi(&self) -> &'static str;
    fn twice(&self) -> [&'static str; 2] {
        [self.hi(), self.hi()]
    }
}

#[announce]
pub type Alias = [u8; 4];

#[announce]
pub mod inner {
    pub fn answer() -> u32 {
        42
    }
}

#[derive(announce::Announce)]
pub struct Derived {
    pub x: i32,
}

/// The trampoline has another name than the item.
#[announce(macro_name = RenamedTrampoline)]
pub struct Renamed;

/// `meta` is carried to the callback.
#[announce(meta = nostd_info(version = 1))]
pub struct WithMeta;

/// The `repeater` option works as well: the generated code needs only the
/// hidden `announce::__private` and `announce::Announce` here.
pub mod leaky {
    announce::define_repeater!(pub Repeater);

    pub mod inner {
        pub struct Local(pub u8);
    }
    use inner::Local;

    #[announce::announce(repeater = crate::leaky::Repeater)]
    pub struct Leaky {
        pub a: Local,
        pub c: u8,
    }
}

/// The `Announce` trait is there without the feature.
pub fn is_announced<T: ?Sized + announce::Announce>() {}

impl Greet for Named {
    fn hi(&self) -> &'static str {
        "named"
    }
}
