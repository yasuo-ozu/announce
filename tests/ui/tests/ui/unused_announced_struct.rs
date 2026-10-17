// The sentinel of an announced struct, enum or union (`#[derive]` makes the runner, so the
// announcement emits one) is not a use of the type: a private item that nothing uses is
// reported as unused, as it would be without `#[announce]`. A sentinel that names the type in
// the body of a function would make the lint blame the field (`field `0` is never read`)
// instead of the struct.
#![deny(dead_code)]

#[::announce::announce]
#[derive(Clone)]
struct S(u8);

#[::announce::announce]
#[derive(Clone)]
enum E {
    A,
}

fn main() {}
