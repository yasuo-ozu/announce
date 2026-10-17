// A call with `@announce_format "2"`, a version of the wire format that this announce
// does not read: an item announced, or a macro written, with another version of
// announce. The parsers of announce say so in one error that names both versions.

// A proc-macro callback (`announce::helper::Input`).
announce_test_derive::format_report! { @announce_format "2" [announce_test_derive::format_report!] A }

// A proc-macro check, called by the definition (`announce::helper::check`).
announce_test_derive::no_tuple! { @announce_format "2" (), struct S { a: u8 } }

// The end of the attribute query (`finish!`).
announce::__private::finish! {
    @announce_format "2" @__announce_attrs [announce::__private::finish!] { [1u64; attr () { struct T; }] }
}

// A trampoline is a `macro_rules!`: it cannot compare versions, and its error says
// which one it takes.
announce_test_defs::kinds::Greet! {
    @announce_format "2" [announce_test_defs::kinds::Greet!, announce_test_derive::format_report!] B
}

fn main() {}
