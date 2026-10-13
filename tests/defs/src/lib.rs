//! Definitions for the announce integration tests. The `x` feature is OFF here and
//! ON in `announce-test-user`, to check that cfgs are evaluated in this crate.

use announce::announce;

pub mod docs;
pub mod large;
pub mod large_leak;
pub mod owned;
pub mod wire;

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

    /// A second module with `$` and a repetition (chains of two).
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

/// The same for trampolines that hold their item once: they call themselves
/// through the path the caller wrote, which must resolve from the expansion too.
#[allow(dead_code)]
pub mod vis_single {
    use super::announce;

    #[announce(layout = single)]
    pub(crate) struct CrateOnly;

    #[announce(layout = single)]
    struct Private;

    pub mod inner {
        use crate::announce;
        #[announce(layout = single)]
        pub(super) enum SuperOnly {
            A,
        }
    }

    macro_rules! count {
        ([$($l:tt)*] $($t:tt)*) => {
            pub const SAME_CRATE: &str = stringify!($($t)*);
        };
    }
    macro_rules! count_bare {
        ([$($l:tt)*] $($t:tt)*) => {
            pub const BARE: &str = stringify!($($t)*);
        };
    }
    pub mod a {
        crate::vis_single::CrateOnly! { [crate::vis_single::CrateOnly!, count!] }
    }
    pub mod b {
        super::Private! { [super::Private!, count!] }
    }
    pub mod c {
        super::inner::SuperOnly! { [super::inner::SuperOnly!, count!] }
    }
    pub mod d {
        crate::large::BigTrait! { [crate::large::BigTrait!, count!] }
    }
    pub mod e {
        crate::large::big_mod! { [crate::large::big_mod!, count!] }
    }
    // By its bare name, in the module that defines it and through a `use`.
    Private! { [Private!, count_bare!] }
    pub mod f {
        use super::Private;
        Private! { [Private!, count!] }
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

    /// A check on the derive form: the derive sends the same query, and the
    /// item stays as written.
    #[derive(::announce::Announce, Debug)]
    #[announce(announce_test_derive::no_tuple(derived), meta = crate::info(1))]
    pub struct DerivedChecked {
        pub a: u8,
        #[cfg(any())]
        pub gone: u8,
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

/// The same for a large item, which calls itself through the head path
/// (`$crate::..` tokens must survive that).
#[macro_export]
macro_rules! via_dollar_crate_large {
    () => {
        $crate::large::BigTrait! { [$crate::large::BigTrait!, $crate::dollar_crate_cb!] }
    };
}
#[macro_export]
macro_rules! via_dollar_crate_large_mod {
    () => {
        $crate::large::big_mod! { [$crate::large::big_mod!, $crate::dollar_crate_cb!] }
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

    ::announce::define_repeater!(pub Repeater);

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

/// Regressions: `crate::` after a single `:` and `$crate` in a payload macro.
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

/// A constant of this crate, named by `crate::ARR` inside a re-emitted payload.
pub const ARR: [u8; 3] = [1, 2, 3];

/// The `crate` after the `in` of a `for` loop is an
/// ordinary path; only the `in` of `pub(in ..)` is part of a visibility.
#[announce]
pub mod for_in_crate {
    pub fn sum() -> u32 {
        let mut s = 0;
        for x in crate::ARR {
            s += x as u32;
        }
        s
    }
    pub(crate) fn _vis() {}
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

/// These used to break the definer's own build.
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

    /// A const generic array length: a const parameter resolves everywhere.
    #[announce]
    pub struct Arr<const N: usize>(pub [u8; N]);

    pub mod inner {
        pub struct Local;
    }
    use inner::Local;
    ::announce::define_repeater!(pub Repeater);

    #[announce(repeater = crate::trait_paths_ok::Repeater)]
    pub struct ArrLocal<const N: usize>(pub [Local; N]);
}

/// Repeater tables and the analysis.
pub mod audit3 {
    use super::announce;

    ::announce::define_repeater!(pub Repeater);
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

/// `crate = path`: an explicit path to the announce crate in generated code.
pub mod crate_opt {
    use super::announce;

    /// A stand-in facade: `Capture` and `remove` come from announce, but
    /// `Announce` is a trait of this module, so `impl Announce for ..` can only
    /// name it when the option is honoured.
    pub mod spy {
        pub use ::announce::__private;
        pub trait Announce {}
    }

    #[announce(crate = crate::crate_opt::spy)]
    pub struct ViaSpy {
        pub a: u8,
    }

    #[announce(crate = crate::crate_opt::spy)]
    pub trait SpyTrait {
        fn f(&self) -> u8;
    }

    #[announce(crate = ::announce)]
    pub struct Explicit {
        pub a: u8,
    }
}

/// With a `repeater` but only absolute types, META still has `type_leak(..)`
/// with empty lists.
pub mod empty_table {
    use super::announce;

    ::announce::define_repeater!(pub Repeater);

    #[announce(repeater = crate::empty_table::Repeater)]
    pub struct NoLeaks {
        pub a: u8,
        pub b: ::std::vec::Vec<u8>,
        pub c: ::core::option::Option<crate::empty_table::NoLeaks2>,
    }
    pub struct NoLeaks2;
}

/// Repeater tables keep the bounds and const generics of the item.
pub mod table_bounds {
    use super::announce;

    ::announce::define_repeater!(pub Repeater);

    pub mod inner {
        #[derive(Debug, PartialEq, Clone)]
        pub struct Local(pub u8);
        /// Well-formed only for `T: Clone`.
        #[derive(Debug, PartialEq)]
        pub struct Wrap<T: ::core::clone::Clone>(pub T);
        #[derive(Debug, PartialEq)]
        pub struct Buf<const N: usize>(pub [u8; N]);
        #[derive(Debug, PartialEq)]
        pub struct Lt<'a>(pub &'a u8);
    }
    use inner::{Buf, Local, Lt, Wrap};

    #[announce(repeater = crate::table_bounds::Repeater)]
    pub struct Bounded<T: ::core::clone::Clone>(pub ::std::vec::Vec<T>, pub Wrap<T>, pub Local);

    #[announce(repeater = crate::table_bounds::Repeater)]
    pub struct ArrOfLocal<const N: usize>(pub [Local; N]);

    #[announce(repeater = crate::table_bounds::Repeater)]
    pub struct ConstArg<const N: usize>(pub Buf<N>);

    #[announce(repeater = crate::table_bounds::Repeater)]
    pub struct Both<'a, T: ::core::clone::Clone, const N: usize>(
        pub Lt<'a>,
        pub Wrap<T>,
        pub Buf<N>,
    );
}

/// `allow_std_prelude = true` with a repeater: prelude names stay, the
/// relative types inside them are carried.
pub mod prelude_leak {
    use super::announce;

    ::announce::define_repeater!(pub Repeater);
    pub mod inner {
        #[derive(Debug, PartialEq, Clone)]
        pub struct Local(pub u8);
    }
    use inner::Local;

    #[announce(allow_std_prelude = true, repeater = crate::prelude_leak::Repeater)]
    pub struct Holder {
        pub a: Vec<Local>,
        pub b: Option<Box<Local>>,
        pub s: String,
    }
}

/// `signature_only!` on a module with `const fn`, `impl Trait` and impl blocks.
pub mod sig_full {
    use super::announce;

    #[announce]
    pub mod m {
        pub const fn zero() -> u32 {
            0
        }
        pub fn opaque() -> impl ::core::fmt::Display {
            8
        }
        pub fn plain() -> u32 {
            7
        }
        pub struct S;
        impl S {
            pub fn method(&self) -> u32 {
                9
            }
            pub const fn cmethod() -> u32 {
                11
            }
            pub fn opaque_method(&self) -> impl ::core::fmt::Debug {
                12
            }
        }
        pub mod nested {
            pub fn deep() -> u32 {
                13
            }
        }
        pub const ZERO: u32 = zero();
    }
}

/// The module-wrapper pattern of the `#[announce]` docs: `emit_item = false` on a
/// same-named module next to the real item.
pub mod wrapper {
    use super::announce;

    pub trait Base {}
    pub struct Local(pub u8);

    #[announce]
    pub trait Gadget {
        fn f(&self) -> u8;
    }

    /// Modules are not analysed: the relative `Base` and `Local` pass, and the
    /// copy is not emitted, so it does not clash with the trait `Gadget`.
    #[announce(emit_item = false, macro_name = GadgetWrapped, meta = crate::info(wrapped))]
    pub mod Gadget {
        pub trait Gadget: Base {
            fn f(&self) -> Local;
        }
    }

    /// A wrapper whose paths all resolve in other crates.
    #[announce(emit_item = false, macro_name = ResolvedWrapped)]
    pub mod Resolved {
        pub trait Resolved: crate::wrapper::Base {
            fn f(&self) -> crate::wrapper::Local;
        }
    }
}

/// Used across crates: a derived union.
pub mod derived_union {
    #[derive(::announce::Announce, Clone, Copy)]
    #[announce(meta = crate::info(union))]
    pub union Raw {
        pub i: u32,
        pub f: f32,
    }
}

/// Item-level `#[cfg]` that is false: no trampoline exists.
pub mod gone {
    use super::announce;

    #[announce]
    #[cfg(any())]
    pub struct AnnounceFirst;

    #[cfg(any())]
    #[announce]
    pub struct CfgFirst;

    #[announce]
    #[cfg(not(any()))]
    pub struct Present;
}

/// type-leak 0.8: elided lifetimes, higher-ranked bounds and tuples with a
/// repeater.
pub mod leak08 {
    use super::announce;

    ::announce::define_repeater!(pub Repeater);
    pub mod inner {
        #[derive(Debug, PartialEq, Clone, Copy)]
        pub struct Local(pub u8);
    }
    use inner::Local;

    /// `&Local` with an elided lifetime in method signatures.
    #[announce(repeater = crate::leak08::Repeater)]
    pub trait Elided {
        fn get(&self, l: &Local) -> u8;
        fn pick<'a>(&'a self, l: &'a Local) -> &'a Local;
    }

    /// A higher-ranked `Fn` bound whose argument is `&Local`.
    #[announce(repeater = crate::leak08::Repeater)]
    pub struct Hr {
        pub f: ::std::boxed::Box<dyn for<'b> ::core::ops::Fn(&'b Local) -> u8>,
    }

    /// A tuple: only `Local` is carried, `u8` is a primitive.
    #[announce(repeater = crate::leak08::Repeater)]
    pub struct Tup {
        pub t: (Local, u8),
    }

    pub mod wrap {
        pub struct Wrap<'a>(pub &'a u8);
    }
    use wrap::Wrap;

    /// A method lifetime that a relative type mentions is fine when another
    /// argument keeps it as written (`&'a self`, a plain `&'a u8`). Without that,
    /// type-leak 0.8 rejects the definition (`tests/ui/repeater_lifetime_unconstrained.rs`).
    #[announce(repeater = crate::leak08::Repeater)]
    pub trait Constrained {
        fn by_self<'a>(&'a self, a: Wrap<'a>) -> &'a u8;
        fn by_arg<'a>(&self, a: Wrap<'a>, b: &'a u8) -> &'a u8;
    }
}

/// The repeater of type-leak 0.8: every entry is written from the context of
/// the scope where its type occurs, and const
/// generics of every primitive type are carried.
pub mod contexts {
    use super::announce;

    ::announce::define_repeater!(pub Repeater);

    pub mod inner {
        #[derive(Debug, PartialEq, Clone, Default)]
        pub struct Local(pub u8);
        /// Any `X`.
        #[derive(Debug, PartialEq, Default)]
        pub struct G<X>(pub X);
        /// Well-formed only for `X: Clone`.
        #[derive(Debug, PartialEq, Default)]
        pub struct GC<X: ::core::clone::Clone>(pub X);
        #[derive(Debug, PartialEq, Default)]
        pub struct LocalG<U>(pub U);
        #[derive(Debug, PartialEq, Default)]
        pub struct LocalC<T: ::core::clone::Clone>(pub T);
        /// A type named like a generic of the items below.
        #[derive(Debug, PartialEq, Default)]
        pub struct U;
        #[derive(Debug, PartialEq)]
        pub struct Arr<const N: usize>(pub [u8; N]);
        #[derive(Debug, PartialEq, Default)]
        pub struct W<const A: u8, const B: bool, const C: char, const D: i64>;
        #[derive(Debug, PartialEq, Default)]
        pub struct Flag<const B: bool>;
        #[derive(Debug, PartialEq)]
        pub struct Wrap<'a>(pub &'a u8);
    }
    use inner::{Arr, Flag, Local, LocalC, LocalG, Wrap, G, GC, W};

    /// A: the bound `X: Clone` of `f` must not reach the impl of `G<X>`.
    #[announce(repeater = crate::contexts::Repeater)]
    pub trait SameTextTwice {
        fn f<X: ::core::clone::Clone>(&self, x: X) -> GC<X>;
        fn g<X>(&self, x: X) -> G<X>;
    }

    /// A: `Local` is a generic only inside `f`.
    #[announce(repeater = crate::contexts::Repeater)]
    pub trait GenericNamedLikeAType {
        fn f<Local>(&self, x: Local) -> Local;
        fn g(&self) -> Local;
    }

    /// #1: `T: Into<V>` needs `V` in the impl of `LocalG<T>`.
    #[announce(repeater = crate::contexts::Repeater)]
    pub struct BoundNamesOther<T: ::core::convert::Into<V>, V>(pub LocalG<T>, pub V);

    /// #1: a method bound that names a parameter of the trait.
    #[announce(repeater = crate::contexts::Repeater)]
    pub trait MethodBoundNamesTrait<T> {
        fn f<U: ::core::convert::Into<T>>(&self, x: LocalG<U>) -> T;
    }

    /// #1: a lifetime bound `T: 'a`.
    #[announce(repeater = crate::contexts::Repeater)]
    pub struct OutlivesBound<'a, T: 'a>(pub &'a LocalG<T>);

    /// #2: a where-clause on the carried type's generics.
    #[announce(repeater = crate::contexts::Repeater)]
    pub struct WhereClause<T>(pub LocalC<T>)
    where
        T: ::core::clone::Clone;

    /// #3: `U` is a generic in `f` and a type in `g`.
    #[announce(repeater = crate::contexts::Repeater)]
    pub trait NameOnlyElsewhere {
        fn f<U>(&self, x: LocalG<U>) -> U;
        fn g(&self) -> inner::U;
    }

    /// #4: `N` is a type generic in `f` and a const generic in `g`.
    #[announce(repeater = crate::contexts::Repeater)]
    pub trait KindClash {
        fn f<N>(&self, x: LocalG<N>) -> N;
        fn g<const N: usize>(&self, x: Arr<N>) -> usize;
    }

    /// Const generics of primitive types other than `usize` in a carried type.
    #[announce(repeater = crate::contexts::Repeater)]
    pub struct ConstParams<const A: u8, const B: bool, const C: char, const D: i64> {
        pub w: W<A, B, C, D>,
        pub local: Local,
    }

    /// A trait with a `bool` const generic and a method with a `char` one.
    #[announce(repeater = crate::contexts::Repeater)]
    pub trait ConstTrait<const B: bool> {
        fn flag(&self) -> Flag<B>;
        fn pick<const C: char>(&self) -> W<0, B, C, 1>;
    }

    /// A relative type with a lifetime that a `for<..>` of the item binds.
    #[announce(repeater = crate::contexts::Repeater)]
    pub struct HigherRanked {
        pub f: for<'b> fn(Wrap<'b>) -> u8,
    }

    /// `Sized` and `?Sized` are accepted bare, in a
    /// method's generics, a `where` clause and the item's own generics, with
    /// relative types next to them.
    #[announce(repeater = crate::contexts::Repeater)]
    pub trait BareSized {
        fn len_of<U: ?Sized + ::core::convert::AsRef<[u8]>>(&self, u: &U) -> LocalG<usize>;
        fn local() -> Local
        where
            Self: Sized;
    }

    #[announce(repeater = crate::contexts::Repeater)]
    pub struct BareUnsized<T: ?Sized>(pub ::std::boxed::Box<T>, pub Local);
}
