mod m {
    pub struct Local;
}
use m::Local;

// `Local` does not resolve outside this crate, so `repeater = ..` is required.
#[announce::announce]
pub struct NeedsRepeater {
    pub a: Local,
    pub b: ::std::vec::Vec<u8>, // absolute std path: fine
    pub c: crate::m::Local, // absolute: fine
}



// Type aliases are not a type-leak target: the right-hand side must be absolute.
#[announce::announce]
pub type Rel = Option<Local>;

#[announce::announce]
pub type Abs = ::core::option::Option<crate::m::Local>;

// Std prelude names count as relative.
#[announce::announce]
pub type Prelude = Vec<u8>;
fn main() {}
