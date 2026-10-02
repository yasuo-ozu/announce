// A transformer needs a continuation after it.
announce_test_defs::kinds::Greet! { [announce_test_defs::kinds::Greet!, ::announce::signature_only!] }

// Transformers are chosen by the caller, not on the definition.
#[announce::announce(signature_only = true)]
pub trait T {}

fn main() {}
