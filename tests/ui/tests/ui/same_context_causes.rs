// One error for each cause that `announce::same_context!` finds in a call outside the module, at
// the whole type or path in the definition, with the way to fix it.
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

fn main() {}

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
