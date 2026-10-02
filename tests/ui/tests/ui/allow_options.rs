// `allow_primitive = false`: primitives need a repeater (or cannot be used).
#[announce::announce(allow_primitive = false)]
pub struct NoPrim {
    pub a: u8,
}

// `allow_std_prelude = true` accepts prelude names without generic arguments
// only (type-leak 0.7); `Vec<u8>` is still reported.
#[announce::announce(allow_std_prelude = true)]
pub struct PreludeGeneric {
    pub v: Vec<u8>,
}

// The values are booleans.
#[announce::announce(allow_std_prelude = yes)]
pub struct BadValue;

fn main() {}
