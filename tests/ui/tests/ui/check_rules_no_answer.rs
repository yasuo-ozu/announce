// A `macro_rules!` check without the `@__announce_attrs` arm: rustc reports the
// call, and `S` is not emitted until the check answers.
macro_rules! check {
    (@announce_format "1" ($($args:tt)*), $($item:tt)*) => {};
}

#[announce::announce(check)]
pub struct S {
    pub a: u8,
}

fn main() {}
