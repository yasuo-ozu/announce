//! UI tests: names that do not resolve outside the module of the item, with `strict = true`
//! (the default), and what a `repeater` cannot carry. The errors of type-leak are not the
//! definition's: it compiles, and the consumer that applies the table reports them
//! (`leak_at_the_consumer`).

use subcrate::subcrate;

// With `strict = true` (the default) every relative type, and every type that has no impl, is an
// error at its tokens, all of an item at once, and the item is still emitted. `strict = false`
// records the causes instead (ui_same_context.rs). A trait path, a value path and a type that type-leak
// cannot leak are no cause here: they are errors of type-leak, in the table (`leak_at_the_consumer`).
#[subcrate(
    ::announce,
    announce_test_derive = derive,
    announce_test_ui_lib = ui_lib,
    should_error = "`UiLocal` cannot be named outside its module",
    should_spanned = "UiLocal",
    should_error = "`[u8; LEN]` cannot be named outside its module",
    should_spanned = "[u8; LEN]",
    should_error = "`Option<(UiArr<{ LEN }>, T)>` cannot be named outside its module",
    should_spanned = "Option<(UiArr<{ LEN }>, T)>",
    should_error = "`UiIdx` cannot be carried",
    should_spanned = "UiIdx",
    // The causes of `mod e`.
    should_error = "`crate::inner::UiIdx` cannot be carried",
    should_spanned = "crate::inner::UiIdx",
    // 7 x `UiLocal` and one each of the others.
    should_error = "aborting due to 11 previous errors\n"
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

        // The relative types: an array length makes the type relative. The trait path, the value path
        // and `UiWrap<'_>` are errors of type-leak, which this definition does not report.
        #[announce::announce]
        pub trait Sub: UiBase {
            const M: usize = LEN;
            fn f(&self, a: UiWrap<'_>, b: UiLocal, c: [u8; LEN]);
        }

        // The right-hand side of a type alias.
        #[announce::announce]
        pub type Alias<T: UiBase> = Option<(UiArr<{ LEN }>, T)>;

        // A `repeater` carries the relative types: fine here. The trait path and the value are
        // errors of type-leak, which the consumer reports.
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
    // for a type written in two places. A const generic that cannot be encoded is a cause too. The
    // errors of type-leak are no cause: the definitions compile.
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

        // The const generic that no table carries (and the trait path, which is type-leak's).
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

        // The macro call with the `Self` in it is underlined where it is written.
        #[announce::announce(repeater = crate::Repeater)]
        pub struct InMacro<T = ty!(Self)>(pub T, pub UiLocal);
    }
}

// Types that a repeater cannot carry: a relative type that names `Self`. The impls are for `()`,
// which `Self` is not. Everything else that type-leak cannot leak is its own error, in the table,
// which the consumer reports (`leak_at_the_consumer`): these definitions compile.
#[subcrate(
    ::announce,
    should_error = "`G<Self>` cannot be carried",
    should_spanned = "G<Self>",
    should_error = "`G<Self::A>` cannot be carried",
    should_spanned = "G<Self::A>",
    should_error = "aborting due to 2 previous errors\n"
)]
pub mod leak_not_carried {
    ::announce::define_repeater!(pub Repeater);
    pub mod inner {
        pub struct G<T>(pub T);
        pub struct Wrap<'a>(pub &'a u8);
    }
    use inner::{Wrap, G};

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

    // What type-leak cannot leak is no error here: the part is copied as written, and the
    // definition compiles. `Self` means nothing in a type alias, an elided lifetime keeps a
    // relative type from being leaked, a prelude name is another option, a bound cannot name a
    // relative type that mentions the bounded parameter, a relative macro in an array length, and
    // the type of a const parameter.
    pub type Idx = usize;
    macro_rules! two {
        () => {
            2
        };
    }

    #[announce::announce(repeater = crate::Repeater)]
    pub trait Elided {
        fn f(&self, a: Wrap<'_>);
    }

    #[announce::announce(repeater = crate::Repeater)]
    pub trait Two {
        fn f(&self, a: Wrap<'_>, b: inner::Wrap<'_>);
    }

    #[announce::announce(repeater = crate::Repeater)]
    pub trait Prelude {
        fn f(&self, a: Vec<&'_ u8>);
    }

    // ... which then works.
    #[announce::announce(repeater = crate::Repeater, allow_std_prelude = true)]
    pub trait PreludeAllowed {
        fn f(&self, a: Vec<&'_ u8>);
    }

    #[announce::announce]
    pub type SelfAlias = Self;

    #[announce::announce(repeater = crate::Repeater)]
    pub struct Cycle<T: ::core::convert::From<G<T>>>(pub T);

    #[announce::announce(repeater = crate::Repeater)]
    pub struct MacroLen(pub [u8; two!()]);

    #[announce::announce(repeater = crate::Repeater)]
    pub struct ConstTy<const N: Idx>;
}

// A callback copies a primitive name as written (`u8`: `allow_primitive`, or the type of a const
// parameter), so it means the primitive wherever the callback writes it. The definition checks
// that its module has not shadowed the name (type-leak's `Leaker::assertions()`, which the fast
// path writes too): `struct u8;` there is an error at the definition, at the `#[announce]` of each
// item that copies it (rustc says that the primitive `u8` is not `shadows::u8`). Where nothing
// shadows it, the same items compile.
#[subcrate(
    ::announce,
    should_error = "mismatched types",
    should_error = "one `u8` is a primitive defined by the language",
    // The fast path, the analysis with a repeater, and a record.
    should_spanned = "#[announce :: announce ]",
    should_spanned = "#[announce :: announce (repeater = crate :: Repeater ) ]",
    should_spanned = "#[announce :: announce (strict = false ) ]",
    should_error = "aborting due to 3 previous errors\n"
)]
pub mod shadowed_primitive {
    ::announce::define_repeater!(pub Repeater);
    pub mod inner {
        pub struct Local;
    }

    mod shadows {
        use super::inner::Local;

        #[allow(non_camel_case_types)]
        pub struct u8;

        // The fast path: nothing but primitives and absolute paths.
        #[announce::announce]
        pub struct Fast {
            pub a: u8,
        }

        // The analysis: a repeater carries `Local`, and the primitive stays as written.
        #[announce::announce(repeater = crate::Repeater)]
        pub struct Full {
            pub a: u8,
            pub b: Local,
        }

        // The relative type, with `strict = false`: a record, and the check all the same.
        #[announce::announce(strict = false)]
        pub struct Recorded {
            pub a: Local,
            pub b: u8,
        }

        // Without a primitive there is nothing to check.
        #[announce::announce]
        pub struct Nothing {
            pub a: ::std::string::String,
        }
    }

    mod fine {
        use super::inner::Local;

        #[announce::announce]
        pub struct Fast {
            pub a: u8,
        }

        #[announce::announce(repeater = crate::Repeater)]
        pub struct Full {
            pub a: u8,
            pub b: Local,
        }

        #[announce::announce(strict = false)]
        pub struct Recorded {
            pub a: Local,
            pub b: u8,
        }
    }
}

// The errors of type-leak are the consumer's. Each definition compiles (the part that type-leak
// cannot leak is copied as written, and travels in the table as an error); a callback that applies
// the table (`apply_leak!` calls `TypeLeak::try_apply`) reports each at its tokens in the
// definition, in type-leak's own words. With a `repeater` the table also has the impls, and
// without one it has the errors only.
#[subcrate(
    ::announce,
    announce_test_derive = derive,
    // A relative trait bound, a relative value path, a relative lifetime `'_`, a prelude name
    // (a trait in a bound, a value) without `allow_std_prelude`, and a method lifetime that
    // only a relative type mentions.
    should_error = "use absolute path: 'UiBase'",
    should_spanned = "UiBase",
    should_error = "use absolute path: 'UiWrap<'_>'",
    should_spanned = "UiWrap<'_>",
    should_error = "use absolute path: 'LEN'",
    should_spanned = "LEN",
    should_error = "use absolute path: 'Clone'",
    should_spanned = "Clone",
    should_error = "use absolute path: 'None'",
    should_spanned = "None",
    should_error = "`UiWrap<'a>` leaves the lifetime `'a` of the method unconstrained in the copy",
    should_spanned = "UiWrap<'a>",
    should_error = "aborting due to 6 previous errors\n"
)]
pub mod leak_at_the_consumer {
    ::announce::define_repeater!(pub Repeater);
    pub mod inner {
        pub trait UiBase {}
        pub struct UiWrap<'a>(pub &'a u8);
        pub struct UiOther;
    }

    pub mod d {
        use super::inner::{UiBase, UiOther, UiWrap};

        const LEN: usize = 4;

        // Without a repeater: the table has the error only.
        #[announce::announce]
        pub trait RelativeTrait: UiBase {
            fn f(&self) -> u8;
        }

        #[announce::announce]
        pub trait RelativeValue {
            const M: usize = LEN;
        }

        #[announce::announce]
        pub struct PreludeBound<T: Clone>(pub T);

        #[announce::announce]
        pub trait PreludeValue {
            const X: ::core::option::Option<u8> = None;
        }

        // With a repeater: the table has the impls of `UiOther`, and the error.
        #[announce::announce(repeater = crate::Repeater)]
        pub trait Elided {
            fn f(&self, a: UiWrap<'_>, b: UiOther);
        }

        #[announce::announce(repeater = crate::Repeater)]
        pub trait Unconstrained {
            fn i<'a>(&self, a: UiWrap<'a>) -> &'a u8;
        }

        // Fine: another argument keeps `'a` as written ...
        #[announce::announce(repeater = crate::Repeater)]
        pub trait SelfKeepsIt {
            fn i<'a>(&'a self, a: UiWrap<'a>) -> &'a u8;
        }

        // ... also a plain reference, and the ways out of a prelude name.
        #[announce::announce(repeater = crate::Repeater)]
        pub trait ArgKeepsIt {
            fn i<'a>(&self, a: UiWrap<'a>, b: &'a u8) -> &'a u8;
        }

        #[announce::announce(allow_std_prelude = true)]
        pub struct PreludeAllowed<T: Clone>(pub T);

        #[announce::announce]
        pub trait PreludeAbsolute {
            const X: ::core::option::Option<u8> = ::core::option::Option::None;
        }
    }

    // Each consumer is in a module of its own, where the relative names are not in scope. The ones
    // that are fine emit their items (`apply_leak!` applies the table to them).
    mod a {
        super::d::RelativeTrait! { [super::d::RelativeTrait!, announce_test_derive::apply_leak!] Rows }
    }
    mod b {
        super::d::RelativeValue! { [super::d::RelativeValue!, announce_test_derive::apply_leak!] Rows }
    }
    mod c {
        super::d::PreludeBound! { [super::d::PreludeBound!, announce_test_derive::apply_leak!] Rows }
    }
    mod e {
        super::d::PreludeValue! { [super::d::PreludeValue!, announce_test_derive::apply_leak!] Rows }
    }
    mod f {
        super::d::Elided! { [super::d::Elided!, announce_test_derive::apply_leak!] Rows }
    }
    mod g {
        super::d::Unconstrained! { [super::d::Unconstrained!, announce_test_derive::apply_leak!] Rows }
    }
    mod fine {
        super::d::SelfKeepsIt! { [super::d::SelfKeepsIt!, announce_test_derive::apply_leak!] A }
        super::d::ArgKeepsIt! { [super::d::ArgKeepsIt!, announce_test_derive::apply_leak!] B }
        super::d::PreludeAllowed! { [super::d::PreludeAllowed!, announce_test_derive::apply_leak!] C }
        super::d::PreludeAbsolute! { [super::d::PreludeAbsolute!, announce_test_derive::apply_leak!] D }
    }
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

// A std prelude name around `Self::Out`. The names of the prelude are relative
// paths here, so `Option<Self::Out>` is a relative type that names `Self`: without a repeater
// it is relative, with one it cannot be carried. The ways out are to accept prelude names, or
// to write the absolute path.
#[subcrate(
    ::announce,
    should_error = "`Option<Self::Out>` cannot be named outside its module",
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
