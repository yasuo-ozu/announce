use announce_test_derive::gathered;

// Transformer misuse: `gather` asked for `::announce::signature_only`, which is
// not a definition, so the transformer reports that it comes before any
// definition. (The "fetched but did not provide a definition" guard is in
// `gather_not_trampoline.rs`.)
gathered! { [gathered!] (::announce::signature_only) G }

fn main() {}
