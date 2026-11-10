//! UI tests: calls of trampolines, callbacks and transformers that are errors.

use subcrate::subcrate;

// `with_macro` carries `$` tokens. Through the derive/attribute route the
// trampoline needs the `@__announce_dollar { $ }` prefix that
// `announce::helper::redirect` adds. Called by hand without it, it must point at it.
#[subcrate(
    ::announce,
    announce_test_defs = defs,
    should_error = "invalid trampoline input",
    should_error = "this error originates in the macro `announce_test_defs::kinds::with_macro`",
    should_error = "this error originates in the macro `announce_test_defs::kinds::with_macro2`",
    should_spanned = "announce_test_defs::kinds::with_macro! { [#[derive(announce_test_defs::conflict::Parse)] N, cb!] }",
    should_spanned = "announce_test_defs::kinds::with_macro2! { [#[announce_test_defs::conflict::parse_attr], cb!] }",
    should_error = "aborting due to 2 previous errors\n"
)]
pub mod dollar_no_prefix {
    macro_rules! cb {
        ($($t:tt)*) => {};
    }

    announce_test_defs::kinds::with_macro! {
        [#[derive(announce_test_defs::conflict::Parse)] N, cb!]
    }

    announce_test_defs::kinds::with_macro2! {
        [#[announce_test_defs::conflict::parse_attr], cb!]
    }

    // A payload without `$` does not need the prefix.
    announce_test_defs::kinds::Greet! {
        [#[derive(announce_test_defs::conflict::Parse)] M, cb!]
    }
}

#[subcrate(
    ::announce,
    announce_test_defs = defs,
    should_error = "a derive/attribute continuation emits items, so it cannot be used with `(expr)`",
    should_spanned = "announce_test_defs::kinds::Greet! { (expr) [announce_test_defs::kinds::Greet!, #[derive(::announce_test_defs::conflict::Parse)] N, cb!] }",
    should_spanned = "announce_test_defs::kinds::Greet! { @announce_format \"1\" (expr) [announce_test_defs::kinds::Greet!, #[derive(::announce_test_defs::conflict::Parse)] N, cb!] }",
    should_warn = "unused macro definition: `cb`",
    should_spanned = "cb",
    should_error = "aborting due to 2 previous errors; 1 warning emitted\n"
)]
pub mod expr_position {
    macro_rules! cb {
        ($($t:tt)*) => {
            0
        };
    }

    pub fn main() {
        // The derive route emits items: not usable in expression position.
        let _ = announce_test_defs::kinds::Greet! {
            (expr) [announce_test_defs::kinds::Greet!, #[derive(::announce_test_defs::conflict::Parse)] N, cb!]
        };
        // The same with the format prefix, as a call from a helper has it.
        let _ = announce_test_defs::kinds::Greet! {
            @announce_format "1" (expr) [announce_test_defs::kinds::Greet!, #[derive(::announce_test_defs::conflict::Parse)] N, cb!]
        };
    }
}

#[subcrate(
    ::announce,
    announce_test_defs = defs,
    should_error = "invalid trampoline input",
    should_spanned = "announce_test_defs::kinds::Greet! { not a list }",
    should_error = "aborting due to 1 previous error\n"
)]
pub mod malformed {
    announce_test_defs::kinds::Greet! { not a list }
}

// An entry without its `[META]` group must be reported, not taken as REST.
#[subcrate(
    ::announce,
    announce_test_derive = derive,
    should_error = "malformed entry: expected square brackets",
    should_spanned = "{",
    should_error = "aborting due to 1 previous error\n"
)]
pub mod malformed_entry {
    announce_test_derive::summary! { [announce_test_derive::summary!] a::b! => { struct S; } NAME }
}

#[subcrate(
    ::announce,
    announce_test_defs = defs,
    should_error = "invalid trampoline input",
    should_spanned = "announce_test_defs::kinds::Greet! { [announce_test_defs::kinds::Greet!] }",
    should_error = "aborting due to 1 previous error\n"
)]
pub mod no_continuation {
    announce_test_defs::kinds::Greet! { [announce_test_defs::kinds::Greet!] }
}

// A large item's trampoline holds the item once. The
// calls that are errors for every trampoline say the same; the one that only
// that layout has is a derive or attribute head written by hand, without the
// path `announce::helper::redirect` adds.
#[subcrate(
    ::announce,
    announce_test_defs = defs,
    should_error = "invalid trampoline input",
    should_error = "this error originates in the macro `announce_test_defs::large::BigTrait`",
    should_error = "this error originates in the macro `announce_test_defs::large::big_mod`",
    should_spanned = "announce_test_defs::large::BigTrait! { [#[derive(announce_test_defs::conflict::Parse)] N, cb!] }",
    should_spanned = "announce_test_defs::large::big_mod! { [#[announce_test_defs::conflict::parse_attr], cb!] }",
    should_spanned = "announce_test_defs::large::BigTrait! { [announce_test_defs::large::BigTrait!] }",
    should_spanned = "announce_test_defs::large::big_mod! { not a list }",
    should_error = "a derive/attribute continuation emits items, so it cannot be used with `(expr)`",
    should_spanned = "announce_test_defs::large::BigStruct! { (expr) [announce_test_defs::large::BigStruct!, #[derive(::announce_test_defs::large::BigParse)] N, cb!] }",
    should_warn = "unused macro definition: `cb`",
    should_spanned = "cb",
    should_error = "aborting due to 5 previous errors; 1 warning emitted\n"
)]
pub mod large_errors {
    macro_rules! cb {
        ($($t:tt)*) => {};
    }

    // Without `redirect`, a large trampoline cannot call itself.
    announce_test_defs::large::BigTrait! {
        [#[derive(announce_test_defs::conflict::Parse)] N, cb!]
    }
    announce_test_defs::large::big_mod! {
        [#[announce_test_defs::conflict::parse_attr], cb!]
    }

    // No continuation, and not a list.
    announce_test_defs::large::BigTrait! { [announce_test_defs::large::BigTrait!] }
    announce_test_defs::large::big_mod! { not a list }

    // The derive route emits items: not usable in expression position.
    pub fn main() {
        let _ = announce_test_defs::large::BigStruct! {
            (expr) [announce_test_defs::large::BigStruct!, #[derive(::announce_test_defs::large::BigParse)] N, cb!]
        };
    }
}

// A call with `@announce_format "2"`, a version of the wire format that this announce
// does not read: an item announced, or a macro written, with another version of
// announce. The parsers of announce say so in one error that names both versions.
#[subcrate(
    ::announce,
    announce_test_defs = defs,
    announce_test_derive = derive,
    should_error = "incompatible wire format",
    should_spanned = "\"2\"",
    should_error = "invalid trampoline input",
    should_error = "this error originates in the macro `announce_test_defs::kinds::Greet`",
    should_error = "this error originates in the macro `announce_test_defs::kinds::with_macro`",
    should_error = "this error originates in the macro `announce_test_defs::large::BigTrait`",
    should_error = "this error originates in the macro `announce_test_defs::large::big_mod`",
    should_spanned = "announce_test_defs::kinds::Greet! { @announce_format \"2\" [announce_test_defs::kinds::Greet!, announce_test_derive::format_report!] B }",
    should_spanned = "announce_test_defs::kinds::with_macro! { @announce_format \"2\" [announce_test_defs::kinds::with_macro!, announce_test_derive::format_report!] C }",
    should_spanned = "announce_test_defs::large::BigTrait! { @announce_format \"2\" [announce_test_defs::large::BigTrait!, announce_test_derive::format_report!] D }",
    should_spanned = "announce_test_defs::large::big_mod! { @announce_format \"2\" @__announce_self { announce_test_defs::large::big_mod } @__announce_dollar { $ } (my_opt) [announce_test_defs::large::big_mod!, announce_test_derive::format_report!] E }",
    should_error = "aborting due to 7 previous errors\n"
)]
pub mod format_mismatch {
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

    // The same for the other layouts (`$` in the item; the item held once), with and without
    // the other prefixes of a call. Only a version that is given is wrong: the same calls
    // without the prefix are fine (format.rs).
    announce_test_defs::kinds::with_macro! {
        @announce_format "2" [announce_test_defs::kinds::with_macro!, announce_test_derive::format_report!] C
    }
    announce_test_defs::large::BigTrait! {
        @announce_format "2" [announce_test_defs::large::BigTrait!, announce_test_derive::format_report!] D
    }
    announce_test_defs::large::big_mod! {
        @announce_format "2" @__announce_self { announce_test_defs::large::big_mod } @__announce_dollar { $ }
        (my_opt) [announce_test_defs::large::big_mod!, announce_test_derive::format_report!] E
    }
}

// A plain macro that is not a trampoline: it forwards its input to the next
// list element and adds no entry. The raw `Input::fetch` fetched it once and
// gets no definition back, so it must stop with the same error as `gather`
// instead of fetching until the recursion limit.
#[subcrate(
    ::announce,
    announce_test_defs = defs,
    announce_test_derive = derive,
    should_error = "`forwards_only!` was fetched but did not provide a definition",
    should_spanned = "$n)? $(:: $ns)* ! { @announce_format \"1\" ($($o)*) [$($n)? $(:: $ns)* ! $($tail)*] $($r)* }",
    should_spanned = "fetched_raw! { [fetched_raw!] (forwards_only) F }",
    should_spanned = "fetched_raw! { [fetched_raw!] (::announce_test_defs::kinds::Shape, forwards_only) G }",
    should_error = "aborting due to 2 previous errors\n"
)]
pub mod fetch_not_trampoline {
    use announce_test_derive::fetched_raw;

    macro_rules! forwards_only {
        (@announce_format "1" ($($o:tt)*)
         [$($h:ident)? $(:: $hs:ident)* ! , $($n:ident)? $(:: $ns:ident)* ! $($tail:tt)*]
         $($r:tt)*) => {
            $($n)? $(:: $ns)* ! { @announce_format "1" ($($o)*) [$($n)? $(:: $ns)* ! $($tail)*] $($r)* }
        };
    }

    fetched_raw! { [fetched_raw!] (forwards_only) F }

    // The same with a fetch that follows a real one.
    fetched_raw! { [fetched_raw!] (::announce_test_defs::kinds::Shape, forwards_only) G }
}

// A plain macro that is not a trampoline: it forwards its input to the next
// list element and adds no entry. `gather` fetched it once and gets no
// definition back, so it must stop with an error instead of fetching forever.
#[subcrate(
    ::announce,
    announce_test_derive = derive,
    should_error = "`forwards_only!` was fetched but did not provide a definition",
    should_spanned = "$n)? $(:: $ns)* ! { @announce_format \"1\" ($($o)*) [$($n)? $(:: $ns)* ! $($tail)*] $($r)* }",
    should_spanned = "gathered! { [gathered!] (forwards_only) G }",
    should_error = "aborting due to 1 previous error\n"
)]
pub mod gather_not_trampoline {
    use announce_test_derive::gathered;

    macro_rules! forwards_only {
        (@announce_format "1" ($($o:tt)*)
         [$($h:ident)? $(:: $hs:ident)* ! , $($n:ident)? $(:: $ns:ident)* ! $($tail:tt)*]
         $($r:tt)*) => {
            $($n)? $(:: $ns)* ! { @announce_format "1" ($($o)*) [$($n)? $(:: $ns)* ! $($tail)*] $($r)* }
        };
    }

    gathered! { [gathered!] (forwards_only) G }
}

// Transformer misuse: `gather` asked for `::announce::signature_only`, which is
// not a definition, so the transformer reports that it comes before any
// definition. (The "fetched but did not provide a definition" guard is in
// `gather_not_trampoline`.)
#[subcrate(
    ::announce,
    announce_test_derive = derive,
    should_error = "a transformer must come after the definition it transforms",
    should_spanned = "gathered! { [gathered!] (::announce::signature_only) G }",
    should_error = "aborting due to 1 previous error\n"
)]
pub mod gather_transformer {
    use announce_test_derive::gathered;

    gathered! { [gathered!] (::announce::signature_only) G }
}

#[subcrate(
    ::announce,
    announce_test_defs = defs,
    should_error = "a transformer cannot be the last element of the continuation list",
    should_spanned = "announce_test_defs::kinds::Greet! { [announce_test_defs::kinds::Greet!, ::announce::signature_only!] }",
    should_error = "`signature_only` is not a definition-side option",
    should_spanned = "signature_only",
    should_error = "aborting due to 2 previous errors\n"
)]
pub mod transformer_misuse {
    // A transformer needs a continuation after it.
    announce_test_defs::kinds::Greet! { [announce_test_defs::kinds::Greet!, ::announce::signature_only!] }

    // Transformers are chosen by the caller, not on the definition.
    #[announce::announce(signature_only = true)]
    pub trait T {}
}
