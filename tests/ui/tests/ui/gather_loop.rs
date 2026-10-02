use announce_test_derive::gathered;

// `gather` asked for something that is not a trampoline: a clear error, not
// an endless fetch loop.
gathered! { [gathered!] (::announce::signature_only) G }

fn main() {}
