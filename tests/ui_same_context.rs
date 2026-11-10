//! UI tests of items announced with `strict = false`: `same_context!` and callbacks that
//! use such an item outside its module.

use subcrate::subcrate;

// `announce::same_context!` in a call requires that the call is in the module of the item. Each
// cause of an item announced with `strict = false` is an error at its tokens in the definition,
// with the call in the note; an item without a record passes anywhere.
#[subcrate(
    ::announce,
    announce_test_derive = derive,
    announce_test_ui_lib = ui_lib,
    should_error = "`const` items in this context need a name",
    should_error = "use absolute path: 'UiLocal'",
    should_error = "the trait `without_the_names::_::announce_check::InModuleOf_Item` is not implemented",
    should_error = "`super::Item` is used outside its module `super`",
    should_error = "the trait `without_the_names::in_fn::_::announce_check::InModuleOf_Item` is not implemented",
    should_error = "`m::Item` is used outside its module `m`",
    should_error = "use absolute path: 'UiOther'",
    should_error = "`m::Second` is used outside its module `m`",
    should_error = "the trait `sibling::_::announce_check::InModuleOf_Item` is not implemented",
    should_error = "`super::m::Item` is used outside its module `super::m`",
    should_error = "`crate::m::Item` is used outside its module `crate::m`",
    should_error = "`merged::A` is used outside its module `merged`",
    should_error = "`merged::F` is used outside its module `merged`",
    should_error = "`merged::G` is used outside its module `merged`",
    should_error = "`announce_test_ui_lib::m::Holder` is used outside its module `announce_test_ui_lib::m`",
    should_error = "the trait `sibling_fn::in_fn::_::announce_check::InModuleOf_Item` is not implemented",
    should_error = "`announce_test_ui_lib::m::Pair` is used outside its module `announce_test_ui_lib::m`",
    should_error = "use absolute path: 'UiBase'",
    should_error = "`announce_test_ui_lib::m::Sub` is used outside its module `announce_test_ui_lib::m`",
    should_error = "the trait `in_expr::_::announce_check::InModuleOf_Item` is not implemented",
    should_error = "`shadowed::Item` is used outside its module `shadowed`",
    // `check_holder!` calls `$crate::m::Holder!`: a subcrate is built without `CARGO_CRATE_NAME`,
    // so the texts name the path without the crate.
    should_error = "`m::Holder` is used outside its module `m`",
    should_spanned = "Item",
    should_spanned = "UiLocal",
    should_spanned = "UiOther",
    should_spanned = "Second",
    should_spanned = "through_the_check!()",
    should_spanned = "A",
    should_spanned = "F",
    should_spanned = "G",
    should_spanned = "Holder",
    should_spanned = "announce_test_ui_lib::check_holder!()",
    should_spanned = "Pair",
    should_spanned = "Sub",
    should_error = "aborting due to 20 previous errors\n",
)]
pub mod same_context {
    #[macro_use]
    mod m {
        pub struct UiLocal;
        pub struct UiOther;

        #[announce::announce(strict = false)]
        pub struct Item {
            pub a: UiLocal,
        }

        #[announce::announce(strict = false)]
        pub struct Second {
            pub a: UiOther,
        }

        #[announce::announce(strict = false)]
        pub struct Plain(pub u8);

        macro_rules! through_the_check {
            () => {
                crate::m::Item! { [crate::m::Item!, announce::same_context!] }
            };
        }

        // Fine: the module (also through a macro written here), a function body, and a submodule that
        // brings the names in scope.
        Item! { [Item!, announce::same_context!] }
        through_the_check!();
        pub fn in_fn() {
            Second! { [Second!, announce::same_context!] }
        }
        pub mod with_the_names {
            use super::*;

            Item! { [Item!, announce::same_context!] }

            pub fn in_fn() {
                Item! { [Item!, announce::same_context!] }
            }
        }

        // A submodule that does not: in an item and in a function body.
        mod without_the_names {
            super::Item! { [super::Item!, announce::same_context!] }

            pub fn in_fn() {
                super::Item! { [super::Item!, announce::same_context!] }
            }
        }

        // The check is an item, which cannot stand among the associated items of an `impl` or a trait
        // (the check goes beside the impl, `defs::recorded` in src/lib.rs).
        pub struct S;

        impl S {
            Item! { [Item!, announce::same_context!] }
        }

        pub trait T {
            Item! { [Item!, announce::same_context!] }
        }
    }

    // From the parent module, in the middle of a chain: an error for each item with a record, none
    // for `Plain`.
    m::Item! { [m::Item!, m::Second!, m::Plain!, announce::same_context!, announce_test_derive::leak_failures!] ROWS }

    // From a sibling module, also through the macro of `m`.
    mod sibling {
        super::m::Item! { [super::m::Item!, announce::same_context!] }

        through_the_check!();
    }

    // The announcements of one item are merged, and `strict = false` in any of them makes a record, in
    // either order, with a derive between them and next to `strict = true`.
    mod merged {
        #[derive(Clone)]
        pub struct UiLocal;

        #[announce::announce(strict = false)]
        #[announce::announce]
        pub struct A {
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
    }

    merged::A! { [merged::A!, merged::F!, merged::G!, announce::same_context!] }

    // An item of another crate (`ui_lib`), also through its `$crate` macro: the error is at the type
    // in the source of that crate (a note here: a subcrate has no source file to show), and the
    // texts name the crate as the caller wrote it; through `$crate`, without it. `Plain` has no
    // record.
    announce_test_ui_lib::m::Holder! { [announce_test_ui_lib::m::Holder!, announce::same_context!] }
    announce_test_ui_lib::m::Plain! { [announce_test_ui_lib::m::Plain!, announce::same_context!] }
    announce_test_ui_lib::check_holder!();

    // The forwarding macro of `m` in a function body outside the module, and from the parent module.
    mod sibling_fn {
        pub fn in_fn() {
            through_the_check!();
        }
    }

    through_the_check!();

    // An item of another crate with two causes and a callback after the check, and one with a relative
    // trait path.
    announce_test_ui_lib::m::Pair! { [announce_test_ui_lib::m::Pair!, announce::same_context!, announce_test_derive::leak_failures!] PAIR_ROWS }
    announce_test_ui_lib::m::Sub! { [announce_test_ui_lib::m::Sub!, announce::same_context!] }

    // In expression position (`(expr)`) the check is a block in front of the callback's value, and
    // fails at the cause as well; the passing case is `defs::recorded` in src/lib.rs.
    macro_rules! expr_cb {
        ($($t:tt)*) => {
            0
        };
    }

    fn in_expr() -> u8 {
        m::Item! { (expr) [m::Item!, announce::same_context!, expr_cb!] }
    }

    // A module with an item named `diagnostic`: the attributes of the check do not see it. The call in
    // the module passes, also from a submodule that imports it, and the one outside fails like any
    // other.
    mod shadowed {
        pub struct UiLocal;
        pub mod diagnostic {}

        #[announce::announce(strict = false)]
        pub struct Item {
            pub a: UiLocal,
        }

        Item! { [Item!, announce::same_context!] }

        mod with_the_names {
            use super::*;

            Item! { [Item!, announce::same_context!] }
        }
    }

    shadowed::Item! { [shadowed::Item!, announce::same_context!] }
}

// One error for each cause that `announce::same_context!` finds in a call outside the module, at
// the whole type or path in the definition, with the way to fix it.
#[subcrate(
    ::announce,
    should_error = "use absolute path: 'UiBase'",
    should_error = "`m::Many` is used outside its module `m`",
    should_error = "use absolute path: 'LEN'",
    should_error = "use absolute path: 'UiWrap<'_>'",
    should_error = "`m::InBound` is used outside its module `m`",
    should_error = "use absolute path: 'UiAssoc'",
    should_error = "`m::Objects` is used outside its module `m`",
    should_error = "use absolute path: 'UiAssoc::X'",
    should_error = "use absolute path: 'Vec<&'_ u8>'",
    should_error = "`m::Prelude` is used outside its module `m`",
    should_error = "use absolute path: 'UiIdx'",
    should_error = "`m::ConstTy` is used outside its module `m`",
    should_error = "use absolute path: 'Option<u8>'",
    should_error = "`m::Alias` is used outside its module `m`",
    should_error = "use absolute path: 'UiLocal'",
    should_error = "`m::Item` is used outside its module `m`",
    should_error = "use absolute path: 'UiOther'",
    should_error = "use absolute path: 'UiUnique'",
    should_error = "`m::Behind` is used outside its module `m`",
    should_error = "use absolute path: 'UiRaw'",
    should_error = "`m::Generic` is used outside its module `m`",
    should_error = "use absolute path: 'UiTwice'",
    should_error = "`m::Twice` is used outside its module `m`",
    should_error = "`m2::InSignature` is used outside its module `m2`",
    should_error = "`m2::InWhere` is used outside its module `m2`",
    should_error = "`m2::InMethod` is used outside its module `m2`",
    should_error = "`m2::Elided` is used outside its module `m2`",
    should_error = "use absolute path: 'UiG<Self::A>'",
    should_error = "`m2::SelfInside` is used outside its module `m2`",
    should_error = "`UiWrap<'a>` leaves the lifetime `'a` of the method unconstrained in the copy",
    should_error = "`m3::Lifetimes` is used outside its module `m3`",
    should_error = "`m3::Shadow` is used outside its module `m3`",
    should_error = "`m3::Encoded` is used outside its module `m3`",
    should_error = "const parameter of a non-primitive type: 'crate::inner::UiIdx'",
    should_error = "use absolute path: 'N'",
    should_error = "`m3::Scoped` is used outside its module `m3`",
    should_spanned = "UiBase",
    should_spanned = "Many",
    should_spanned = "LEN",
    should_spanned = "UiWrap<'_>",
    should_spanned = "[u8; LEN]",
    should_spanned = "InBound",
    should_spanned = "UiAssoc",
    should_spanned = "Objects",
    should_spanned = "UiAssoc>::X",
    should_spanned = "Vec<&'_ u8>",
    should_spanned = "Prelude",
    should_spanned = "UiIdx",
    should_spanned = "ConstTy",
    should_spanned = "Option<u8>",
    should_spanned = "Alias",
    should_spanned = "UiLocal",
    should_spanned = "Item",
    should_spanned = "UiOther",
    should_spanned = "&'static mut UiUnique",
    should_spanned = "Behind",
    should_spanned = "UiRaw",
    should_spanned = "Generic",
    should_spanned = "&'static UiTwice",
    should_spanned = "Twice",
    should_spanned = "InSignature",
    should_spanned = "InWhere",
    should_spanned = "InMethod",
    should_spanned = "Elided",
    should_spanned = "UiG<Self::A>",
    should_spanned = "SelfInside",
    should_spanned = "UiWrap<'a>",
    should_spanned = "Lifetimes",
    should_spanned = "Shadow",
    should_spanned = "Encoded",
    should_spanned = "crate::inner::UiIdx",
    should_spanned = "N",
    should_spanned = "Scoped",
    should_error = "aborting due to 29 previous errors\n"
)]
pub mod same_context_causes {
    ::announce::define_repeater!(pub Repeater);
    pub mod inner {
        pub struct UiLocal;
        pub struct UiOther;
        pub struct UiWrap<'a>(pub &'a u8);
        pub struct UiArr<const N: usize>(pub [u8; N]);
        pub type UiIdx = usize;
    }

    mod m {
        use super::inner::{UiArr, UiIdx, UiLocal, UiOther, UiWrap};

        pub trait UiBase {}
        pub trait UiAssoc {
            type X;
        }
        pub struct UiRaw(pub u8);
        pub struct UiUnique(pub u8);
        const LEN: usize = 4;

        // Every kind of cause in one item: trait path, value path, a type that cannot be carried, and
        // a relative type (an array length makes it relative).
        #[announce::announce(strict = false)]
        pub trait Many: UiBase {
            const M: usize = LEN;
            fn f(&self, a: UiWrap<'_>, c: [u8; LEN]);
        }

        // A relative trait path in a bound of a parameter.
        #[announce::announce(strict = false)]
        pub struct InBound<T: UiBase> {
            pub t: T,
        }

        // What a repeater cannot carry: the trait of a trait object under a reference, a qualified
        // path, a prelude name with an elided lifetime, a const generic that is not primitive.
        #[announce::announce(strict = false, repeater = crate::Repeater)]
        pub trait Objects: UiAssoc {
            fn f(&self, a: &dyn UiBase) -> <Self as UiAssoc>::X;
        }

        #[announce::announce(strict = false, repeater = crate::Repeater)]
        pub trait Prelude {
            fn f(&self, a: Vec<&'_ u8>);
        }

        #[announce::announce(strict = false, repeater = crate::Repeater)]
        pub struct ConstTy<const N: UiIdx>(pub UiArr<N>);

        #[announce::announce(strict = false)]
        pub type Alias = Option<u8>;

        // A type written in a tuple, an array and a field is one cause, at its first occurrence;
        // a second type is another. A type behind a reference or a pointer is underlined whole.
        #[announce::announce(strict = false)]
        pub struct Item {
            pub a: (UiLocal, u8),
            pub b: [UiLocal; 2],
            pub c: UiLocal,
            pub d: UiOther,
        }

        #[announce::announce(strict = false)]
        pub struct Behind {
            pub a: *const UiRaw,
            pub b: &'static mut UiUnique,
        }

        // The parameters of an item are not causes.
        #[announce::announce(strict = false)]
        pub struct Generic<T> {
            pub a: (UiOther, T),
            pub b: [UiOther; 2],
            pub c: T,
        }

        // `&'static UiTwice` and `UiTwice` are one cause, at the first.
        pub struct UiTwice(pub u8);

        #[announce::announce(strict = false)]
        pub struct Twice {
            pub a: &'static UiTwice,
            pub b: UiTwice,
        }

        // Fine inside the module.
        Prelude! { [Prelude!, announce::same_context!] }
    }

    m::Many! {
        [
            m::Many!,
            m::InBound!,
            m::Objects!,
            m::Prelude!,
            m::ConstTy!,
            m::Alias!,
            m::Item!,
            m::Behind!,
            m::Generic!,
            m::Twice!,
            announce::same_context!
        ]
    }

    // More causes, in a second module and a second call, after the first: a where clause, the bound of
    // a method's parameter, a relative type as an argument and as the return type, and relative types
    // that a repeater cannot carry.
    pub mod inner2 {
        pub struct UiG<T>(pub T);
    }

    mod m2 {
        use super::inner::{UiLocal, UiOther, UiWrap};
        use super::inner2::UiG;
        use super::m::UiBase;

        #[announce::announce(strict = false)]
        pub trait InSignature {
            fn f(&self, a: UiLocal) -> UiOther;
        }

        #[announce::announce(strict = false)]
        pub struct InWhere<T>
        where
            T: UiBase,
        {
            pub t: T,
        }

        #[announce::announce(strict = false)]
        pub trait InMethod {
            fn f<U: UiBase>(&self, u: U);
        }

        // A relative type with an elided lifetime, and `Self::A` inside a relative type.
        #[announce::announce(strict = false, repeater = crate::Repeater)]
        pub trait Elided {
            fn f(&self, a: UiWrap<'_>);
        }

        #[announce::announce(strict = false, repeater = crate::Repeater)]
        pub trait SelfInside {
            type A;
            fn g(&self) -> UiG<Self::A>;
        }
    }

    m2::InSignature! {
        [
            m2::InSignature!,
            m2::InWhere!,
            m2::InMethod!,
            m2::Elided!,
            m2::SelfInside!,
            announce::same_context!
        ]
    }

    // A third call: every cause at its own place, with the others. Only `f` of `Lifetimes` has a
    // lifetime that is in its return type alone; `Shadow` has `UiLocal` as a parameter of one method
    // and as the return type of another; a const generic that cannot be encoded is reported with the
    // trait path of `Encoded`; and the `N` of the default of `Scoped` is the constant, not the
    // parameter of `f`.
    mod m3 {
        use super::inner::{UiArr, UiLocal, UiWrap};
        use super::m::UiBase;

        const N: usize = 4;

        #[announce::announce(strict = false, repeater = crate::Repeater)]
        pub trait Lifetimes {
            fn g<'a>(&self, a: UiWrap<'a>);
            fn f<'a>(&self, a: UiWrap<'a>) -> &'a u8;
        }

        #[announce::announce(strict = false)]
        pub trait Shadow: UiBase {
            fn f<UiLocal>(&self, x: UiLocal);
            fn g(&self) -> UiLocal;
        }

        #[announce::announce(strict = false, repeater = crate::Repeater)]
        pub struct Encoded<T: UiBase, const N: crate::inner::UiIdx>(pub UiArr<N>, pub T);

        #[announce::announce(strict = false, repeater = crate::Repeater)]
        pub trait Scoped {
            const M: usize = N;
            fn f<const N: usize>(&self);
        }
    }

    m3::Lifetimes! {
        [
            m3::Lifetimes!,
            m3::Shadow!,
            m3::Encoded!,
            m3::Scoped!,
            announce::same_context!
        ]
    }
}

// `Entry::item_resolved` (the callback `resolved_copy!`) uses an item announced with
// `strict = false` as written, so a call in the module of the item is fine and any other is an
// error: rustc's own at the names that do not resolve, and the check of the item at the cause.
#[subcrate(
    ::announce,
    announce_test_derive = derive,
    announce_test_ui_lib = ui_lib,
    should_error = "cannot find type `UiLocal` in this scope",
    should_error = "this error originates in the macro `m::Item`",
    should_error = "this error originates in the macro `super::m::Item`",
    should_error = "this error originates in the macro `announce_test_ui_lib::m::Holder`",
    should_error = "use absolute path: 'UiLocal'",
    should_error = "`m::Item` is used outside its module `m`",
    should_error = "the trait `sibling::_::announce_check::InModuleOf_Item` is not implemented",
    should_error = "`super::m::Item` is used outside its module `super::m`",
    should_error = "`announce_test_ui_lib::m::Holder` is used outside its module `announce_test_ui_lib::m`",
    // rustc suggests imports; the one from `ui_lib` is not suggested on stable.
    should_spanned = "UiLocal",
    should_spanned = "m::Item! { [m::Item!, announce_test_derive::resolved_copy!] FromParent }",
    should_spanned = "super::m::Item! { [super::m::Item!, announce_test_derive::resolved_copy!] FromSibling }",
    should_spanned = "announce_test_ui_lib::m::Holder! { [announce_test_ui_lib::m::Holder!, announce_test_derive::resolved_copy!] FromHere }",
    should_spanned = "Item",
    should_spanned = "Holder",
    should_error = "aborting due to 6 previous errors\n",
)]
pub mod recorded_outside {
    mod m {
        pub struct UiLocal;

        #[announce::announce(strict = false)]
        pub struct Item {
            pub a: UiLocal,
        }

        Item! { [Item!, announce_test_derive::resolved_copy!] InModule }
        pub fn in_fn() {
            Item! { [Item!, announce_test_derive::resolved_copy!] InFn }
        }
    }

    // From the parent module, a sibling module and another crate.
    m::Item! { [m::Item!, announce_test_derive::resolved_copy!] FromParent }

    mod sibling {
        super::m::Item! { [super::m::Item!, announce_test_derive::resolved_copy!] FromSibling }
    }

    announce_test_ui_lib::m::Holder! { [announce_test_ui_lib::m::Holder!, announce_test_derive::resolved_copy!] FromHere }
    announce_test_ui_lib::m::Plain! { [announce_test_ui_lib::m::Plain!, announce_test_derive::resolved_copy!] PlainFromHere }
}

// A relative trait path in a bound is recorded too, but rustc stops at a trait it cannot find in
// the bounds of an item before it checks the call: the error is its own, at the call.
#[subcrate(
    ::announce,
    announce_test_derive = derive,
    announce_test_ui_lib = ui_lib,
    should_error = "cannot find trait `UiBase` in this scope",
    should_spanned = "m::Sub! { [m::Sub!, announce_test_derive::resolved_copy!] SubFromHere }",
    should_error = "aborting due to 1 previous error\n",
)]
pub mod recorded_trait_path {
    use announce_test_ui_lib::m;

    m::Sub! { [m::Sub!, announce_test_derive::resolved_copy!] SubFromHere }
}

// A callback that needs a type table (`apply_leak!` reads `Entry::type_leak()?`) gets an error
// for an item announced with `strict = false`, wherever it is called (a proc macro cannot tell):
// a line for each cause, with the whole path of the trampoline of the call underlined, also for an
// item of another crate and through its `$crate` macro.
#[subcrate(
    ::announce,
    announce_test_derive = derive,
    announce_test_ui_lib = ui_lib,
    should_error = "this macro cannot use `announce_test_ui_lib::m::Holder`: `UiLocal` cannot be named outside its module",
    should_error = "this macro cannot use `m::Elided`: `UiWrap<'_>` cannot be carried",
    should_error = "this macro cannot use `m::ConstTy`: `crate::inner::UiIdx` cannot be carried",
    should_error = "this macro cannot use `m::Sub`: `UiBase` cannot be named outside its module",
    should_error = "this macro cannot use `m::Pair`: `UiLocal` cannot be named outside its module",
    should_error = "this macro cannot use `m::Pair`: `UiOther` cannot be named outside its module",
    should_error = "this macro cannot use `Item`: `UiLocal` cannot be named outside its module",
    // `apply_holder!` calls `$crate::m::Holder!`: a subcrate is built without `CARGO_CRATE_NAME`,
    // so the text names the path without the crate.
    should_error = "this macro cannot use `m::Holder`: `UiLocal` cannot be named outside its module",
    should_spanned = "announce_test_ui_lib::m::Holder",
    should_spanned = "announce_test_ui_lib::apply_holder!(announce_test_derive::apply_leak, FromTheLibrary)",
    should_spanned = "m::Elided",
    should_spanned = "m::ConstTy",
    should_spanned = "m::Sub",
    should_spanned = "m::Pair",
    should_spanned = "Item",
    should_error = "aborting due to 8 previous errors\n",
)]
pub mod recorded_type_leak {
    ::announce::define_repeater!(pub Repeater);
    pub mod inner {
        pub struct UiLocal;
        pub struct UiOther;
        pub trait UiBase {}
        pub type UiIdx = usize;
        pub struct UiArr<const N: usize>(pub [u8; N]);
        pub struct UiWrap<'a>(pub &'a u8);
    }

    mod m {
        use super::inner::{UiArr, UiBase, UiLocal, UiOther, UiWrap};

        #[announce::announce(strict = false)]
        pub struct Item {
            pub a: UiLocal,
        }

        #[announce::announce(strict = false)]
        pub struct Pair {
            pub a: UiLocal,
            pub b: UiOther,
        }

        #[announce::announce(strict = false)]
        pub trait Sub: UiBase {}

        // What a table cannot carry is not about scope.
        #[announce::announce(strict = false, repeater = crate::Repeater)]
        pub struct ConstTy<const N: crate::inner::UiIdx>(pub UiArr<N>);

        #[announce::announce(strict = false, repeater = crate::Repeater)]
        pub trait Elided {
            fn f(&self, a: UiWrap<'_>);
        }

        // In the module of the item: the same error.
        Item! { [Item!, announce_test_derive::apply_leak!] InModule }
    }

    m::Pair! { [m::Pair!, announce_test_derive::apply_leak!] PairFromParent }
    m::Sub! { [m::Sub!, announce_test_derive::apply_leak!] SubFromParent }
    m::ConstTy! { [m::ConstTy!, announce_test_derive::apply_leak!] ConstFromParent }
    m::Elided! { [m::Elided!, announce_test_derive::apply_leak!] ElidedFromParent }
    announce_test_ui_lib::m::Holder! { [announce_test_ui_lib::m::Holder!, announce_test_derive::apply_leak!] FromAnotherCrate }
    announce_test_ui_lib::apply_holder!(announce_test_derive::apply_leak, FromTheLibrary);
}
