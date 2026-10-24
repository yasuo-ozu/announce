//! UI tests: names that do not resolve outside the module of the item, with `strict = true`
//! (the default), and what a `repeater` cannot carry.

use subcrate::subcrate;

// With `strict = true` (the default) every cause that keeps a name from resolving outside its
// module is an error at its tokens, all of an item at once, and the item is still emitted.
// `strict = false` records the causes instead (ui_same_context.rs).
#[subcrate(
    ::announce,
    announce_test_derive = derive,
    announce_test_ui_lib = ui_lib,
    should_error = "`UiLocal` cannot be named outside its module",
    should_spanned = "UiLocal",
    should_error = "`UiBase` cannot be named outside its module",
    should_spanned = "UiBase",
    should_error = "`LEN` cannot be named outside its module",
    should_spanned = "LEN",
    should_error = "`UiWrap<'_>` cannot be carried",
    should_spanned = "UiWrap<'_>",
    should_error = "`[u8; LEN]` cannot be named outside its module",
    should_spanned = "[u8; LEN]",
    should_error = "`Option<(UiArr<{ LEN }>, T)>` cannot be named outside its module",
    should_spanned = "Option<(UiArr<{ LEN }>, T)>",
    should_error = "`UiIdx` cannot be carried",
    should_spanned = "UiIdx",
    // The causes of `mod e`.
    should_error = "`UiWrap<'a>` leaves the lifetime `'a` of the method unconstrained in the copy",
    should_spanned = "UiWrap<'a>",
    should_error = "`crate::inner::UiIdx` cannot be carried",
    should_spanned = "crate::inner::UiIdx",
    should_error = "`N` cannot be named outside its module",
    should_spanned = "N",
    should_error = "`ty!(Self)` is in the default of a parameter, and names `Self`",
    should_spanned = "Self",
    // 7 x `UiLocal`, 5 x `UiBase`, 2 x `LEN`, 3 x `UiWrap<'_>` and one each of the others.
    should_error = "aborting due to 24 previous errors\n"
)]
pub mod leak_errors {
    #[macro_export]
    macro_rules! plain_check {
        (@announce_format "1" @__announce_attrs [ $($hp:ident)? $(:: $hs:ident)* ! , $($np:ident)? $(:: $ns:ident)* ! $(, $($rest:tt)*)? ] { $($state:tt)* }) => {
            $($np)? $(:: $ns)* ! { @announce_format "1" @__announce_attrs [ $($np)? $(:: $ns)* ! $(, $($rest)*)? ] { $($state)* } }
        };
        (@announce_format "1" ($($args:tt)*), $($item:tt)*) => {};
    }
    ::announce::define_repeater!(pub Repeater);
    pub mod inner {
        #[derive(Clone)]
        pub struct UiLocal;
        pub struct UiOther;
        pub trait UiBase {}
        pub struct UiWrap<'a>(pub &'a u8);
        pub type UiIdx = usize;
        pub struct UiArr<const N: usize>(pub [u8; N]);
    }

    mod d {
        use super::inner::{UiArr, UiBase, UiIdx, UiLocal, UiOther, UiWrap};

        const LEN: usize = 4;

        #[announce::announce]
        pub struct Plain {
            pub a: UiLocal,
        }

        // With a derive, which the runner waits for; `strict = true` is the default written out.
        #[announce::announce(strict = true)]
        #[derive(Clone)]
        pub struct Runner {
            pub a: UiLocal,
        }

        // Two announcements make one error, not two.
        #[announce::announce]
        #[announce::announce(meta = my_lib::info)]
        pub struct Twice {
            pub a: UiLocal,
        }

        // Checks of other crates, and one that owns an attribute: the item is emitted without it.
        #[announce::announce(crate::plain_check, announce_test_derive::lib_check)]
        #[derive(Clone)]
        pub struct Checked {
            #[my_lib(skip)]
            pub a: UiLocal,
        }

        // Trait path, value path, a type that cannot be carried, and relative types: an array length
        // makes the type relative.
        #[announce::announce]
        pub trait Sub: UiBase {
            const M: usize = LEN;
            fn f(&self, a: UiWrap<'_>, b: UiLocal, c: [u8; LEN]);
        }

        // The right-hand side of a type alias.
        #[announce::announce]
        pub type Alias<T: UiBase> = Option<(UiArr<{ LEN }>, T)>;

        // A `repeater` carries relative types, but not trait paths or values.
        #[announce::announce(repeater = crate::Repeater)]
        pub trait WithRepeater: UiBase {
            const M: usize = LEN;
            fn f(&self, a: UiOther, c: [u8; LEN]);
        }

        // A const generic that cannot be encoded.
        #[announce::announce(repeater = crate::Repeater)]
        pub struct ConstTy<const N: UiIdx>(pub UiArr<N>);

        // A macro of a library announces the user's item: the error is at the user's tokens.
        announce_test_ui_lib::announced! {
            pub struct FromLibrary {
                pub a: UiLocal,
            }
        }

        // Fine: a `repeater` that carries everything, absolute paths.
        #[announce::announce(repeater = crate::Repeater)]
        pub struct Carried {
            pub a: UiLocal,
        }

        #[announce::announce]
        pub struct Absolute {
            pub a: crate::inner::UiLocal,
        }
    }

    pub fn main() {
        // The items are there, without the attribute that a check owns.
        let _ = d::Runner { a: inner::UiLocal }.clone();
        let _ = d::Checked { a: inner::UiLocal }.clone();
    }

    // Every cause is at its own place: not at the first node that says the same, and not one error
    // for a type written in two places. A const generic that cannot be encoded is a cause too, and
    // a value path names a const parameter only where the parameter is in scope.
    mod e {
        use super::inner::{UiArr, UiBase, UiLocal, UiWrap};

        const N: usize = 4;

        // Only `f` has a lifetime that is in its return type alone.
        #[announce::announce(repeater = crate::Repeater)]
        pub trait Lifetimes {
            fn g<'a>(&self, a: UiWrap<'a>);
            fn f<'a>(&self, a: UiWrap<'a>) -> &'a u8;
        }

        // `UiLocal` is a parameter of `f`, and a type in the return type of `g`.
        #[announce::announce]
        pub trait Shadow: UiBase {
            fn f<UiLocal>(&self, x: UiLocal);
            fn g(&self) -> UiLocal;
        }

        // The same type in two methods: two errors.
        #[announce::announce]
        pub trait Places {
            fn a(&self, x: UiWrap<'_>);
            fn b(&self, y: UiWrap<'_>);
        }

        // The trait path, and the const generic that no table carries.
        #[announce::announce(repeater = crate::Repeater)]
        pub struct Encoded<T: UiBase, const N: crate::inner::UiIdx>(pub UiArr<N>, pub T);

        // The `N` of the default is the constant, not the parameter of `f`.
        #[announce::announce(repeater = crate::Repeater)]
        pub trait Scoped {
            const M: usize = N;
            fn f<const N: usize>(&self);
        }

        macro_rules! ty {
            ($($t:tt)*) => {
                u8
            };
        }

        // The `Self` in the macro is underlined where it is written.
        #[announce::announce(repeater = crate::Repeater)]
        pub struct InMacro<T = ty!(Self)>(pub T, pub UiLocal);
    }
}

// Types that a repeater cannot carry. The errors are type-leak's, re-worded by
// announce: no mention of its API, `crate::..` for a path of this crate, types
// printed as written, one error for every offending type.
#[subcrate(
    ::announce,
    should_error = "`Wrap<'_>` cannot be carried",
    should_spanned = "Wrap<'_>",
    should_error = "`Other<'_>` cannot be carried",
    should_spanned = "Other<'_>",
    should_error = "`Vec<&'_ u8>` cannot be carried",
    should_spanned = "Vec<&'_ u8>",
    should_error = "`G<Self>` cannot be carried",
    should_spanned = "G<Self>",
    should_error = "`G<Self::A>` cannot be carried",
    should_spanned = "G<Self::A>",
    should_error = "`Self` has no meaning here",
    should_spanned = "Self",
    should_error = "`G<T>` is in a bound or where predicate, and a bound of its generics names a leaked type",
    should_spanned = "G<T>",
    should_error = "`two` cannot be carried",
    should_spanned = "two",
    should_error = "`Idx` cannot be carried",
    should_spanned = "Idx",
    // 2 x `Wrap<'_>` and one each of the others.
    should_error = "aborting due to 10 previous errors\n"
)]
pub mod leak_not_carried {
    ::announce::define_repeater!(pub Repeater);
    pub mod inner {
        pub struct G<T>(pub T);
        pub struct Wrap<'a>(pub &'a u8);
        pub struct Other<'a>(pub &'a u8);
        pub struct Ty;
    }
    use inner::{Other, Wrap, G};

    macro_rules! two {
        () => {
            2
        };
    }

    // An elided lifetime keeps a relative type from being carried: write it absolutely.
    #[announce::announce(repeater = crate::Repeater)]
    pub trait Elided {
        fn f(&self, a: Wrap<'_>);
    }

    // Every offending type is its own error, at its own span.
    #[announce::announce(repeater = crate::Repeater)]
    pub trait Two {
        fn f(&self, a: Wrap<'_>, b: Other<'_>);
    }

    // A prelude name is another option.
    #[announce::announce(repeater = crate::Repeater)]
    pub trait Prelude {
        fn f(&self, a: Vec<&'_ u8>);
    }

    // ... which then works.
    #[announce::announce(repeater = crate::Repeater, allow_std_prelude = true)]
    pub trait PreludeAllowed {
        fn f(&self, a: Vec<&'_ u8>);
    }

    // A relative type cannot name `Self` or `Self::A` (a struct's `Self::A` has no valid
    // form to show here: see the unit tests of announce-macros).
    #[announce::announce(repeater = crate::Repeater)]
    pub trait SelfInside {
        type A;
        fn f(&self) -> G<Self>
        where
            Self: ::core::marker::Sized;
        fn g(&self) -> G<Self::A>;
    }

    // `Self`, and `Self::Out`, are fine on their own and inside absolute types.
    #[announce::announce(repeater = crate::Repeater)]
    pub trait SelfAlone {
        type Out;
        fn f(&self) -> Self::Out;
        fn g(&self) -> ::std::boxed::Box<Self::Out>;
    }

    // `Self` means nothing in a type alias.
    #[announce::announce]
    pub type SelfAlias = Self;

    // A bound cannot name a relative type that mentions the bounded parameter.
    #[announce::announce(repeater = crate::Repeater)]
    pub struct Cycle<T: ::core::convert::From<G<T>>>(pub T);

    // A relative macro in an array length.
    #[announce::announce(repeater = crate::Repeater)]
    pub struct MacroLen(pub [u8; two!()]);

    // The type of a const parameter.
    pub type Idx = usize;
    #[announce::announce(repeater = crate::Repeater)]
    pub struct ConstTy<const N: Idx>;
}

#[subcrate(
    ::announce,
    should_error = "`crate::Idx` cannot be carried",
    should_spanned = "crate::Idx",
    should_error = "aborting due to 1 previous error\n"
)]
pub mod leak_unsupported {
    ::announce::define_repeater!(pub Repeater);
    pub mod inner {
        pub struct Arr<const N: usize>(pub [u8; N]);
        pub struct LocalL<'a>(pub &'a u8);
    }
    use inner::{Arr, LocalL};

    pub type Idx = usize;

    // A relative type that uses a const generic must have a primitive type: an
    // alias (even an absolute one) cannot be encoded in the repeater's slots.
    #[announce::announce(repeater = crate::Repeater)]
    pub struct AliasConst<const N: crate::Idx>(pub Arr<N>);

    // A const generic of an alias that no carried type uses is fine.
    #[announce::announce(repeater = crate::Repeater)]
    pub struct AliasConstUnused<const N: crate::Idx>(pub [u8; N], pub LocalL<'static>);

    // `bool`, `char`, `u8`.. and `usize` const generics are all fine.
    #[announce::announce(repeater = crate::Repeater)]
    pub struct Primitive<const B: bool, const C: char>(pub inner::Arr<3>, pub [bool; 1]);

    // A `for<..>` that holds a relative type is fine too: the impl declares the lifetime.
    #[announce::announce(repeater = crate::Repeater)]
    pub struct Hr {
        pub f: for<'b> fn(LocalL<'b>) -> u8,
    }
}

#[subcrate(
    ::announce,
    should_error = "`Local` cannot be named outside its module",
    should_spanned = "Local",
    should_error = "`Option<Local>` cannot be named outside its module",
    should_spanned = "Option<Local>",
    should_error = "`Vec<u8>` cannot be named outside its module",
    should_spanned = "Vec<u8>",
    should_error = "aborting due to 3 previous errors\n"
)]
pub mod repeater_required {
    mod m {
        pub struct Local;
    }
    use m::Local;

    // `Local` does not resolve outside this crate, so `repeater = ..` is required.
    #[announce::announce]
    pub struct NeedsRepeater {
        pub a: Local,
        pub b: ::std::vec::Vec<u8>, // absolute std path: fine
        pub c: crate::m::Local,     // absolute: fine
    }

    // Type aliases are not a type-leak target: the right-hand side must be absolute.
    #[announce::announce]
    pub type Rel = Option<Local>;

    #[announce::announce]
    pub type Abs = ::core::option::Option<crate::m::Local>;

    // Std prelude names count as relative.
    #[announce::announce]
    pub type Prelude = Vec<u8>;
}

// A method lifetime that only a relative type mentions: the copy would have `'a`
// in a projection only, and an implementation `fn i<'a>(&self, a: Wrap<'a>)` is
// rejected with E0195 (early- vs late-bound). type-leak 0.8 reports this at the
// definition; type-leak 0.7 accepted the definition, and the received item
// could not be implemented that way.
#[subcrate(
    ::announce,
    should_error = "`Wrap<'a>` leaves the lifetime `'a` of the method unconstrained in the copy",
    should_spanned = "Wrap<'a>",
    should_error = "aborting due to 1 previous error\n"
)]
pub mod repeater_lifetime_unconstrained {
    ::announce::define_repeater!(pub Repeater);
    pub mod inner {
        pub struct Wrap<'a>(pub &'a u8);
    }
    use inner::Wrap;

    #[announce::announce(repeater = crate::Repeater)]
    pub trait Unconstrained {
        fn i<'a>(&self, a: Wrap<'a>) -> &'a u8;
    }

    // Fine: another argument keeps `'a` as written ...
    #[announce::announce(repeater = crate::Repeater)]
    pub trait SelfKeepsIt {
        fn i<'a>(&'a self, a: Wrap<'a>) -> &'a u8;
    }

    // ... also a plain reference.
    #[announce::announce(repeater = crate::Repeater)]
    pub trait ArgKeepsIt {
        fn i<'a>(&self, a: Wrap<'a>, b: &'a u8) -> &'a u8;
    }
}

#[subcrate(
    ::announce,
    should_error = "`repeater` must be a trait of this crate",
    should_spanned = "::core::marker::PhantomData",
    should_error = "`[u8; N]` cannot be named outside its module",
    should_spanned = "[u8; N]",
    // The message quotes the import as subcrate's copy spaces it (`inner :: Local`): the span
    // pins it.
    should_warn = "unused import: `inner",
    should_spanned = "inner::Local",
    should_error = "aborting due to 2 previous errors; 1 warning emitted\n"
)]
pub mod audit3_errors {
    pub mod inner {
        pub struct Local;
    }
    use inner::Local;
    pub const N: usize = 3;

    // #7: the repeater must be local (`crate::..`).
    #[announce::announce(repeater = ::core::marker::PhantomData)]
    pub struct NonLocal(pub Local);

    // #5: a relative array length makes the type relative: it needs a repeater or an absolute path.
    #[announce::announce]
    pub struct RelLen(pub [u8; N]);
}

#[subcrate(
    ::announce,
    should_error = "`Base` cannot be named outside its module",
    should_spanned = "Base",
    should_error = "`std::fmt::Display` cannot be named outside its module",
    should_spanned = "std::fmt::Display",
    should_error = "`Clone` cannot be named outside its module",
    should_spanned = "Clone",
    should_error = "`marker::Sized` cannot be named outside its module",
    should_spanned = "marker::Sized",
    should_error = "aborting due to 4 previous errors\n"
)]
pub mod trait_paths {
    pub trait Base {}

    // Relative trait paths cannot be covered by a repeater: they must be absolute.
    #[announce::announce]
    pub trait Sub: Base {}

    // `std::` without the leading `::` is relative.
    #[announce::announce]
    pub trait Show {
        fn show<T: std::fmt::Display>(&self, t: T);
    }

    // `Sized` and `?Sized` may be written bare (no error for this one)...
    #[announce::announce]
    pub struct Unsized<T: ?Sized>(pub ::std::boxed::Box<T>);

    #[announce::announce]
    pub trait SizedBounds: Sized {
        fn f(&self) -> u8
        where
            Self: Sized;
        fn g<U: ?Sized + Sized>(&self, u: &U);
    }

    // ... but every other prelude trait needs `allow_std_prelude = true`.
    #[announce::announce]
    pub struct Cloned<T: Clone>(pub T);

    // A path that only ends in `Sized` is not the prelude's.
    #[announce::announce]
    pub struct Qualified<T: marker::Sized>(pub T);
}

#[subcrate(
    ::announce,
    should_error = "`Rel` cannot be named outside its module",
    should_spanned = "Rel",
    should_error = "`dyn Rel` cannot be named outside its module",
    should_spanned = "dyn Rel",
    // 4 x `Rel` (the `where` clause, `impl Rel` in argument and return position, the bound
    // next to a repeater) and `dyn Rel`.
    should_error = "aborting due to 5 previous errors\n"
)]
pub mod trait_paths_relative {
    pub trait Rel {}
    pub struct Local;

    // A relative trait path in a `where` clause: a repeater only carries types.
    #[announce::announce]
    pub struct WhereRel<T>
    where
        T: Rel,
    {
        pub t: T,
    }

    // ... in `impl Trait` argument position ...
    #[announce::announce]
    pub trait ImplArg {
        fn f(&self, x: impl Rel);
    }

    // ... in `impl Trait` return position ...
    #[announce::announce]
    pub trait ImplRet {
        fn f(&self) -> impl Rel;
    }

    // ... and in `dyn`, a relative type (a `repeater` carries it: `defs::carried` in src/lib.rs).
    #[announce::announce]
    pub trait DynRel {
        fn g(&self) -> ::std::boxed::Box<dyn Rel>;
    }

    // ... and a repeater does not help, whatever else it carries.
    ::announce::define_repeater!(pub Repeater);
    #[announce::announce(repeater = crate::Repeater)]
    pub struct WithRepeater<T: Rel>(pub T, pub Local);
}

#[subcrate(
    ::announce,
    should_error = "`N` cannot be named outside its module",
    should_spanned = "N",
    should_error = "`crate::Buf<{ N }>` cannot be named outside its module",
    should_spanned = "crate::Buf<{ N }>",
    // 4 x `N` (the discriminant, the default of the associated const, the default of the
    // const parameter, the const argument in a bound) and `crate::Buf<{ N }>`.
    should_error = "aborting due to 5 previous errors\n"
)]
pub mod value_paths {
    pub const N: usize = 3;
    pub struct Buf<const M: usize>;

    // A relative constant in an enum discriminant.
    #[announce::announce]
    pub enum Discr {
        A = N as isize,
    }

    // ... in the default of an associated const of a trait ...
    #[announce::announce]
    pub trait ConstDefault {
        const M: usize = N;
    }

    // ... in a const generic argument ...
    #[announce::announce]
    pub struct ConstArg(pub crate::Buf<{ N }>);

    // ... and in the default of a const parameter.
    #[announce::announce]
    pub struct ConstParamDefault<const K: usize = N>;

    // In a bound a repeater does not help (in a type it carries the constant with it).
    pub trait Tr<const M: usize> {}
    ::announce::define_repeater!(pub Repeater);
    #[announce::announce(repeater = crate::Repeater)]
    pub struct ConstArgInBound<T: crate::Tr<{ N }>>(pub T);

    // Absolute paths, `Self::..` and const parameters are fine.
    #[announce::announce]
    pub enum Fine {
        A = crate::N as isize,
    }
    #[announce::announce]
    pub trait FineTrait {
        const A: usize;
        const B: usize = Self::A + 1;
        fn f<const K: usize>(&self) -> [u8; K];
    }
}

// A std prelude name around `Self::Out`. The names of the prelude are relative
// paths here, and a relative type cannot hold `Self::Out`. The error says that
// the name is the problem, and gives the two ways out. With and without a repeater.
#[subcrate(
    ::announce,
    should_error = "`Option<Self::Out>` cannot be carried",
    should_spanned = "Option<Self::Out>",
    // `Plain` and `WithRepeater`.
    should_error = "aborting due to 2 previous errors\n"
)]
pub mod prelude_assoc {
    ::announce::define_repeater!(pub Repeater);

    #[announce::announce]
    pub trait Plain {
        type Out;
        fn f(&self) -> Option<Self::Out>;
    }

    #[announce::announce(repeater = crate::Repeater)]
    pub trait WithRepeater {
        type Out;
        fn f(&self) -> Option<Self::Out>;
    }

    // The ways out: accept prelude names, or write the absolute path.
    #[announce::announce(allow_std_prelude = true)]
    pub trait Allowed {
        type Out;
        fn f(&self) -> Option<Self::Out>;
    }

    #[announce::announce]
    pub trait Absolute {
        type Out;
        fn f(&self) -> ::core::option::Option<Self::Out>;
    }
}
