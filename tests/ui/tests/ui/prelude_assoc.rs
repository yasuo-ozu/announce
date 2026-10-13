// A std prelude name around `Self::Out`. The names of the prelude are relative
// paths here, and a relative type cannot hold `Self::Out`. The error says that
// the name is the problem, and gives the two ways out. With and without a repeater.
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

fn main() {}
