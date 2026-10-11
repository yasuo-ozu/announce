// `allow_primitive = false`: primitives need a repeater (or cannot be used).
#[announce::announce(allow_primitive = false)]
pub struct NoPrim {
    pub a: u8,
}

// `allow_std_prelude = true` accepts prelude names, also with generic
// arguments (`Vec<u8>`): no error here.
#[announce::announce(allow_std_prelude = true)]
pub struct PreludeGeneric {
    pub v: Vec<u8>,
}

// The values are booleans.
#[announce::announce(allow_std_prelude = yes)]
pub struct BadValue;

fn main() {}
