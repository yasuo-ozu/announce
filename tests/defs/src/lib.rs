//! Definitions for the announce integration tests. The `x` feature is OFF here and
//! ON in `announce-test-user`, to check that cfgs are evaluated in this crate.

use announce::announce;

pub mod kinds {
    use super::announce;

    #[announce]
    #[derive(Clone, Debug)]
    pub struct Named {
        pub a: u32,
        #[cfg(feature = "x")]
        pub only_with_x: u32,
        #[cfg(not(feature = "x"))]
        pub only_without_x: u32,
        #[cfg_attr(not(feature = "x"), doc = "cfg_attr evaluated in the definer")]
        pub c: u8,
    }

    #[announce]
    pub struct Tuple<T>(pub T, pub u8);

    #[announce]
    pub enum Shape {
        Circle(f64),
        Rect {
            w: f64,
            h: f64,
        },
        #[cfg(feature = "x")]
        OnlyWithX,
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
        #[cfg(feature = "x")]
        fn only_with_x(&self);
    }

    #[announce]
    pub type Alias = ::std::vec::Vec<u8>;

    /// A module whose body holds `$` and a repetition.
    #[announce]
    pub mod with_macro {
        macro_rules! sum {
            ($($x:expr),*) => { 0 $(+ $x)* };
        }
        pub fn six() -> i32 {
            sum!(1, 2, 3)
        }
    }

    /// A second module with `$` and a repetition (AUDIT #3: chains of two).
    #[announce]
    pub mod with_macro2 {
        macro_rules! prod {
            ($($x:expr),*) => { 1 $(* $x)* };
        }
        pub fn twenty_four() -> i32 {
            prod!(2, 3, 4)
        }
    }

    #[announce]
    pub trait Sig {
        fn required(&self) -> u32;
        fn body(&self) -> u32 {
            42
        }
    }

    /// For `signature_only!`: fn bodies are replaced, except when the fn
    /// returns `impl Trait`.
    #[announce]
    pub mod sig_mod {
        pub fn plain() -> u32 {
            7
        }
        pub fn opaque() -> impl ::core::fmt::Display {
            8
        }
    }

    #[announce]
    #[cfg(feature = "x")]
    pub struct GoneWithoutX;
}

#[allow(dead_code)]
pub mod vis {
    use super::announce;

    #[announce]
    pub(crate) struct CrateOnly;

    #[announce]
    struct Private;

    pub mod inner {
        use crate::announce;
        #[announce]
        pub(super) enum SuperOnly {
            A,
        }
    }

    // Same-crate calls by relative, `self::`, `super::` and `crate::` paths.
    macro_rules! count {
        ([$($l:tt)*] $($t:tt)*) => {
            pub const SAME_CRATE: &str = stringify!($($t)*);
        };
    }
    pub mod a {
        crate::vis::CrateOnly! { [crate::vis::CrateOnly!, count!] }
    }
    pub mod b {
        super::Private! { [super::Private!, count!] }
    }
    pub mod c {
        super::inner::SuperOnly! { [super::inner::SuperOnly!, count!] }
    }
    pub mod d {
        crate::kinds::Greet! { [crate::kinds::Greet!, count!] }
    }
}

pub mod conflict {
    use super::announce;

    /// The macro namespace name `Parse` is taken by the derive below, so the
    /// trampoline is renamed.
    #[announce(macro_name = ParseAnnounce)]
    pub trait Parse {
        fn parse(s: &str) -> Self;
    }
    pub use announce_test_derive::Parse;

    pub use announce_test_derive::parse_attr;
}

pub mod checks {
    use super::announce;

    #[announce(announce_test_derive::no_tuple(named only))]
    pub struct Checked {
        pub a: u8,
    }
}

pub mod markers {
    use super::announce;

    #[announce]
    pub trait Simple {
        fn f(&self) -> u8;
    }

    #[announce]
    pub trait WithAssoc {
        type Out;
    }

    /// The derive form: the item is emitted as written, options go in the
    /// `#[announce(..)]` helper (with the `announce` attribute macro also in scope).
    #[derive(::announce::Announce, Debug)]
    #[announce(meta = crate::info(1))]
    pub struct Derived {
        pub a: u8,
        #[cfg(any())]
        pub gone: u8,
    }

    /// Derive without options.
    #[derive(::announce::Announce)]
    pub enum DerivedEnum {
        A,
        B(u8),
    }
}

/// A `$crate` path inside a continuation list, as a library's
/// `macro_rules!` would write it.
#[macro_export]
macro_rules! via_dollar_crate {
    () => {
        $crate::kinds::Greet! { [$crate::kinds::Greet!, $crate::dollar_crate_cb!] }
    };
}
#[macro_export]
#[doc(hidden)]
macro_rules! dollar_crate_cb {
    ([$($l:tt)*] $($t:tt)*) => {
        pub const VIA_DOLLAR_CRATE: &str = stringify!($($t)*);
    };
}

pub mod metas {
    use super::announce;

    #[announce(meta = my_lib::info(version = 3), meta = my_lib::flag)]
    pub struct WithMeta;
}

pub mod derived {
    #[derive(announce_test_derive::Announced)]
    pub struct ByDerive {
        pub field: u8,
    }
}

pub mod mocks {
    use super::announce;

    pub struct Local;

    /// A mock whose copy refers to a local type (resolved via `use super::*`).
    #[announce(mock = crate::mocks::RealTrait)]
    pub trait RealTrait {
        fn f(&self) -> crate::mocks::Local;
    }
}

/// Paths starting with `crate::` must keep pointing at this crate when the
/// payload is re-emitted elsewhere.
#[announce]
pub mod uses_crate_paths {
    pub fn make() -> crate::kinds::Alias {
        ::std::vec![1, 2, 3]
    }
    pub(crate) fn _vis_untouched() {}
}

pub mod transformed {
    use super::announce;

    #[announce(meta = orig(1))]
    pub struct Partly {
        pub a: u8,
        #[cfg(feature = "x")]
        pub gone: u8,
        #[allow(dead_code)]
        hidden: u8,
        pub c: u8,
    }
}

/// Type-leak: `Local` resolves only in this crate; with `(type_leak)` the
/// payload refers to it through `crate::leaky::Repeater`.
pub mod leaky {
    use super::announce;

    pub trait Repeater<const ID: u64, const I: usize, G: ?Sized> {
        type Type: ?Sized;
    }

    pub mod inner {
        #[derive(Debug, PartialEq)]
        pub struct Local(pub u8);
    }
    use inner::Local;

    #[announce(repeater = crate::leaky::Repeater)]
    pub struct Leaky<T> {
        pub a: Local,
        pub b: Vec<T>,
        pub c: u8,
    }

    #[announce(repeater = crate::leaky::Repeater)]
    pub trait LeakTrait {
        fn make(&self) -> Local;
    }

    #[announce]
    pub struct NoRepeater;
}

/// AUDIT #1/#2 regressions: `crate::` after a single `:` and `$crate` in a
/// payload macro.
pub mod crate_paths2 {
    use super::announce;
    pub struct Foo(pub u8);

    #[announce]
    pub struct Field {
        pub a: crate::crate_paths2::Foo,
    }

    #[announce]
    pub mod with_dollar_crate {
        #[macro_export]
        macro_rules! __announce_test_mk_foo {
            () => {
                $crate::crate_paths2::Foo(9)
            };
        }
        pub fn make() -> crate::crate_paths2::Foo {
            crate::crate_paths2::Foo(9)
        }
    }
}

pub mod allow_opts {
    use super::announce;

    /// `String` is a prelude name: allowed with `allow_std_prelude = true`.
    #[announce(allow_std_prelude = true)]
    pub struct WithString {
        pub s: String,
        pub n: u8,
    }
}

/// AUDIT #4 and #5: these used to break the definer's own build.
pub mod audit_high {
    use super::announce;

    /// Lifetime-generic trait: no `Announce` impl (it would not compile).
    #[announce]
    pub trait Life<'a> {
        fn f(&self) -> &'a str;
    }

    pub mod own {
        /// A user trait that is merely *named* `Send`.
        pub trait Send {
            type X;
        }
    }

    #[announce]
    pub trait UsesOwnSend: crate::audit_high::own::Send {}

    /// Raw identifier.
    #[announce]
    #[allow(non_camel_case_types)]
    pub struct r#match;
}

pub mod trait_paths_ok {
    use super::announce;
    pub trait Base {}

    #[announce]
    pub trait Sub: crate::trait_paths_ok::Base + ::core::marker::Send {}

    #[announce]
    pub trait Show {
        fn show<T: ::core::fmt::Display>(&self, t: T);
    }

    /// Bare prelude traits are accepted with `allow_std_prelude = true`.
    #[announce(allow_std_prelude = true)]
    pub trait Cl: Clone {
        fn get<T: ?Sized + Send>(&self, t: &T);
    }

    /// A const generic array length (type-leak cannot analyse it itself).
    #[announce]
    pub struct Arr<const N: usize>(pub [u8; N]);

    pub mod inner {
        pub struct Local;
    }
    use inner::Local;
    pub trait Repeater<const ID: u64, const I: usize, G: ?Sized> {
        type Type: ?Sized;
    }

    #[announce(repeater = crate::trait_paths_ok::Repeater)]
    pub struct ArrLocal<const N: usize>(pub [Local; N]);
}

/// Audit 3 #1-#6: repeater tables and the analysis.
pub mod audit3 {
    use super::announce;

    pub trait Repeater<const ID: u64, const I: usize, G: ?Sized> {
        type Type: ?Sized;
    }
    pub trait Base {}
    pub mod inner {
        pub struct LocalG<U>(pub U);
        pub struct LocalL<'a>(pub &'a u8);
        #[derive(Debug, PartialEq)]
        pub struct Local(pub u8);
        impl From<Local> for u8 {
            fn from(l: Local) -> u8 {
                l.0
            }
        }
    }
    use inner::{Local, LocalG, LocalL};
    pub const N: usize = 3;

    /// #1: method-level generics and lifetimes in interned types.
    #[announce(repeater = crate::audit3::Repeater)]
    pub trait MethodGenerics {
        fn f<U>(&self, x: LocalG<U>) -> U;
        fn g<'a>(&'a self, x: &'a u8) -> LocalL<'a>;
    }

    /// #2: a `Self` where-clause and a bound mentioning an interned type.
    #[announce(repeater = crate::audit3::Repeater)]
    pub trait SelfWhere<T>
    where
        Self: crate::audit3::Base,
    {
        fn h(&self, t: T) -> Local;
    }
    #[announce(repeater = crate::audit3::Repeater)]
    pub struct Bnd<T: ::core::convert::From<Local>>(pub T, pub Local);

    /// #3: a default type parameter that is an interned type.
    #[announce(repeater = crate::audit3::Repeater)]
    pub struct Dflt<T = Local>(pub T);

    /// #4: higher-ranked bounds with absolute paths need no repeater.
    #[announce]
    pub struct Hrtb<F>
    where
        F: for<'b> ::core::ops::Fn(&'b u8) -> &'b u8,
    {
        pub f: F,
        pub g: ::std::boxed::Box<dyn for<'b> ::core::ops::Fn(&'b u8) -> &'b u8>,
    }

    /// #5: absolute value paths, `Self::..` and method const generics.
    #[announce]
    pub enum Discr {
        A = crate::audit3::N as isize,
    }
    #[announce]
    pub struct ArrAbs(pub [u8; crate::audit3::N]);
    #[announce]
    pub trait ConstDefault {
        const N: usize;
        const M: usize = Self::N * 2;
        fn f<const K: usize>(&self) -> [u8; K];
    }

    /// #6: prelude generic bounds and types with `allow_std_prelude`.
    #[announce(allow_std_prelude = true)]
    pub trait PreludeBounds {
        fn it<T: DoubleEndedIterator + AsRef<str>>(&self, t: T) -> Vec<Option<u8>>;
        fn fns<F: Fn(u8) -> u8>(&self, f: F) -> Box<dyn Iterator<Item = u8>>;
    }
}
