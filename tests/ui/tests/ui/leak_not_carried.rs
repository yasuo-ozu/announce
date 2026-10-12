// Types that a repeater cannot carry. The errors are type-leak's, re-worded by
// announce: no mention of its API, `crate::..` for a path of this crate, types
// printed as written, one error for every offending type.
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

fn main() {}
