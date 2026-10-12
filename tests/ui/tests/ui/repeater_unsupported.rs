::announce::define_repeater!(pub Repeater);

// `repeater` works on a struct, enum, union and trait only.
#[announce::announce(repeater = crate::Repeater)]
pub type Alias = ::core::option::Option<u8>;

#[announce::announce(repeater = crate::Repeater)]
pub mod m {
    pub struct S;
}

fn main() {}
