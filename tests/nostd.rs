//! A `#![no_std]` definer crate: the `#[announce]` expansions compile without `std` in
//! scope (`build.rs` compiles the subcrate, so one that needs `std` fails the package), and
//! their trampolines are called from this crate, which has std. The callbacks are plain
//! `macro_rules!`.
//!
//! That `announce` itself builds without its default feature, for a target without std, is
//! built by the CI job (`cargo build -p announce --no-default-features --target
//! x86_64-unknown-none`), not here.

use subcrate::subcrate;

#[subcrate(::announce)]
pub mod definer {
    #![no_std]

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

    /// A derive next to `#[announce]`: the item goes through the runner.
    #[announce]
    #[derive(Clone, Debug)]
    pub struct WithDerive {
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

    /// A relative type, announced with `strict = false`: recorded, and the check passes in the module.
    pub mod recorded {
        pub mod inner {
            pub struct Local(pub u8);
        }
        use inner::Local;

        #[announce::announce(strict = false)]
        pub struct Holder {
            pub a: Local,
        }

        Holder! { [Holder!, announce::same_context!] }
    }

    /// The `Announce` trait is there without the feature.
    pub fn is_announced<T: ?Sized + announce::Announce>() {}

    impl Greet for Named {
        fn hi(&self) -> &'static str {
            "named"
        }
    }
}

macro_rules! capture {
    (@announce_format "1" [$($l:tt)*] $($t:tt)*) => {
        pub const OUT: &str = stringify!([$($l)*] $($t)*);
    };
}

fn norm(s: &str) -> String {
    s.split_whitespace().collect()
}

mod named {
    definer::Named! { [definer::Named!, capture!] rest }
}

#[test]
fn a_struct_is_passed_to_the_callback() {
    let out = norm(named::OUT);
    assert!(
        out.starts_with("[capture!]definer::Named!=>[]{pubstructNamed{"),
        "{}",
        out
    );
    assert!(out.ends_with("rest"), "{}", out);
}

macro_rules! kind {
    (@announce_format "1" [$($l:tt)*] $($p:ident)::+ ! => [$($m:tt)*] { $($item:tt)* } $($rest:tt)*) => {
        pub const KIND: &str = stringify!($($item)*);
    };
}

macro_rules! kinds {
    ($($name:ident => $needle:expr),* $(,)?) => {
        $(
            #[allow(non_snake_case)]
            mod $name {
                definer::$name! { [definer::$name!, kind!] }
                pub fn check() {
                    let item: String = super::norm(KIND);
                    assert!(item.contains($needle), "{}: {}", stringify!($name), item);
                }
            }
        )*
        #[test]
        fn every_kind_is_passed() {
            $($name::check();)*
        }
    };
}

kinds! {
    Tuple => "structTuple<T>(pubT,pubu8)",
    Shape => "enumShape",
    Bits => "unionBits",
    Greet => "traitGreet",
    Alias => "typeAlias=[u8;4]",
    inner => "modinner",
    WithDerive => "structWithDerive",
    WithMeta => "structWithMeta",
}

mod renamed {
    definer::RenamedTrampoline! { [definer::RenamedTrampoline!, capture!] }
}

#[test]
fn macro_name_renames_the_trampoline() {
    assert!(
        norm(renamed::OUT).contains("structRenamed"),
        "{}",
        renamed::OUT
    );
}

mod meta {
    definer::WithMeta! { [definer::WithMeta!, capture!] }
}

#[test]
fn meta_is_carried() {
    assert!(
        norm(meta::OUT).contains("[nostd_info(version=1)]"),
        "{}",
        meta::OUT
    );
}

mod leaky {
    definer::leaky::Leaky! { [definer::leaky::Leaky!, capture!] }
}

#[test]
fn the_repeater_option_works() {
    let out = norm(leaky::OUT);
    assert!(
        out.contains("type_leak(repeater=$crate::leaky::Repeater"),
        "{}",
        out
    );
}

mod recorded {
    definer::recorded::Holder! { [definer::recorded::Holder!, capture!] }
}

#[test]
fn a_relative_type_without_a_repeater_is_recorded() {
    let out = norm(recorded::OUT);
    assert!(!out.contains("type_leak("), "{}", out);
    assert!(
        out.contains("type_leak_error(id=") && out.contains("errors=[(Local)"),
        "{}",
        out
    );
}

#[test]
fn announce_impls_are_there() {
    definer::is_announced::<definer::Named>();
    definer::is_announced::<definer::Shape>();
    definer::is_announced::<definer::WithDerive>();
    definer::is_announced::<dyn definer::Greet>();
    assert_eq!(definer::inner::answer(), 42);
}
