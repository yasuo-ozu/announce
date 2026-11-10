//! The format prefix `@announce_format "1"`: every call that announce makes to the
//! next element of a chain starts with it, so that a macro of yours can detect that
//! announce called it. A call written by hand needs no prefix.
//!
//! The documented form is in `announce::helper::Input` (the wire format).

use announce_tests::derive::{format_report, public_fields, start};

/// A `macro_rules!` callback that detects announce by the prefix, as the docs show.
macro_rules! detect {
    // Called by announce: the prefix, then `[LIST] ENTRY* REST`.
    (@announce_format "1" [$($list:tt)*] $($rest:tt)*) => {
        pub const CALLER: &str = "announce";
    };
    // Called by hand.
    ($($call:tt)*) => {
        pub const CALLER: &str = "hand";
    };
}

/// A trampoline of each layout calls `detect!` with the prefix. `$(:: $seg:ident)+`
/// is the path of the trampoline.
macro_rules! called_by {
    ($($name:ident => $(:: $seg:ident)+;)*) => {$(
        mod $name {
            $(:: $seg)+ ! { [$(:: $seg)+ !, detect!] }
            pub fn caller() -> &'static str {
                CALLER
            }
        }
    )*};
}

called_by! {
    // `layout = direct`, small.
    direct => ::announce_tests::defs::kinds::Greet;
    // `layout = direct`, with `$` in the item.
    direct_dollar => ::announce_tests::defs::kinds::with_macro;
    // `layout = single`: the large item is held once.
    single => ::announce_tests::defs::large::BigTrait;
    // `layout = single`, with `$` in the item.
    single_dollar => ::announce_tests::defs::large::big_mod;
    // `layout = single`, chosen for a small item.
    small_single => ::announce_tests::defs::large::SmallSingle;
}

#[test]
fn a_trampoline_calls_the_callback_with_the_prefix() {
    assert_eq!(direct::caller(), "announce");
    assert_eq!(direct_dollar::caller(), "announce");
    assert_eq!(single::caller(), "announce");
    assert_eq!(single_dollar::caller(), "announce");
    assert_eq!(small_single::caller(), "announce");
}

#[test]
fn a_callback_called_by_hand_has_no_prefix() {
    mod hand {
        detect! { [detect!] }
    }
    assert_eq!(hand::CALLER, "hand");
}

/// Reports the format of the call that reaches `format_report!`: the callback is
/// called by a trampoline, which writes the prefix whether it got one or not.
macro_rules! report {
    ($($name:ident: $(:: $seg:ident)+ ! { $($call:tt)* };)*) => {$(
        mod $name {
            #[allow(unused_imports)]
            use super::format_report;
            $(:: $seg)+ ! { $($call)* }
        }
    )*};
}

report! {
    // By hand, without the prefix: every layout takes it, and passes the prefix on.
    direct_hand: ::announce_tests::defs::kinds::Greet! {
        [::announce_tests::defs::kinds::Greet!, format_report!] FORMAT
    };
    direct_dollar_hand: ::announce_tests::defs::kinds::with_macro! {
        [::announce_tests::defs::kinds::with_macro!, format_report!] FORMAT
    };
    single_hand: ::announce_tests::defs::large::BigTrait! {
        [::announce_tests::defs::large::BigTrait!, format_report!] FORMAT
    };
    single_dollar_hand: ::announce_tests::defs::large::big_mod! {
        [::announce_tests::defs::large::big_mod!, format_report!] FORMAT
    };
    // By hand, with the prefix (as a helper writes it).
    direct_prefixed: ::announce_tests::defs::kinds::Greet! {
        @announce_format "1" [::announce_tests::defs::kinds::Greet!, format_report!] FORMAT
    };
    direct_dollar_prefixed: ::announce_tests::defs::kinds::with_macro! {
        @announce_format "1" [::announce_tests::defs::kinds::with_macro!, format_report!] FORMAT
    };
    single_prefixed: ::announce_tests::defs::large::BigTrait! {
        @announce_format "1" [::announce_tests::defs::large::BigTrait!, format_report!] FORMAT
    };
    single_dollar_prefixed: ::announce_tests::defs::large::big_mod! {
        @announce_format "1" [::announce_tests::defs::large::big_mod!, format_report!] FORMAT
    };
    // With the options group: the prefix comes before it.
    options_prefixed: ::announce_tests::defs::kinds::Greet! {
        @announce_format "1" (my_opt) [::announce_tests::defs::kinds::Greet!, format_report!] FORMAT
    };
    // Two trampolines: the second takes the call of the first, with the prefix.
    two_hops: ::announce_tests::defs::kinds::Greet! {
        [::announce_tests::defs::kinds::Greet!, ::announce_tests::defs::large::BigTrait!, format_report!] FORMAT
    };
}

#[test]
fn trampolines_take_a_call_with_or_without_the_prefix_and_pass_it_on() {
    assert_eq!(direct_hand::FORMAT, "1");
    assert_eq!(direct_dollar_hand::FORMAT, "1");
    assert_eq!(single_hand::FORMAT, "1");
    assert_eq!(single_dollar_hand::FORMAT, "1");
    assert_eq!(direct_prefixed::FORMAT, "1");
    assert_eq!(direct_dollar_prefixed::FORMAT, "1");
    assert_eq!(single_prefixed::FORMAT, "1");
    assert_eq!(single_dollar_prefixed::FORMAT, "1");
    assert_eq!(options_prefixed::FORMAT, "1");
    assert_eq!(two_hops::FORMAT, "1");
}

// A call by hand that has no prefix, in the shapes a trampoline is reached by: with the
// options group; with the prefixes that `redirect` writes (`@__announce_self { PATH }` and
// `@__announce_dollar { $ }`: a trampoline ignores them or needs them); and with both. Nothing
// is given, so no version is read: every layout takes the call, and writes the prefix of
// this version in what it calls next.
report! {
    direct_options_hand: ::announce_tests::defs::kinds::Greet! {
        (my_opt) [::announce_tests::defs::kinds::Greet!, format_report!] FORMAT
    };
    direct_prefixes_hand: ::announce_tests::defs::kinds::Greet! {
        @__announce_self { ::announce_tests::defs::kinds::Greet } @__announce_dollar { $ }
        [::announce_tests::defs::kinds::Greet!, format_report!] FORMAT
    };
    direct_both_hand: ::announce_tests::defs::kinds::Greet! {
        @__announce_self { ::announce_tests::defs::kinds::Greet } @__announce_dollar { $ }
        (my_opt) [::announce_tests::defs::kinds::Greet!, format_report!] FORMAT
    };
    direct_dollar_options_hand: ::announce_tests::defs::kinds::with_macro! {
        (my_opt) [::announce_tests::defs::kinds::with_macro!, format_report!] FORMAT
    };
    direct_dollar_prefixes_hand: ::announce_tests::defs::kinds::with_macro! {
        @__announce_self { ::announce_tests::defs::kinds::with_macro } @__announce_dollar { $ }
        [::announce_tests::defs::kinds::with_macro!, format_report!] FORMAT
    };
    direct_dollar_both_hand: ::announce_tests::defs::kinds::with_macro! {
        @__announce_self { ::announce_tests::defs::kinds::with_macro } @__announce_dollar { $ }
        (my_opt) [::announce_tests::defs::kinds::with_macro!, format_report!] FORMAT
    };
    single_options_hand: ::announce_tests::defs::large::BigTrait! {
        (my_opt) [::announce_tests::defs::large::BigTrait!, format_report!] FORMAT
    };
    single_prefixes_hand: ::announce_tests::defs::large::BigTrait! {
        @__announce_self { ::announce_tests::defs::large::BigTrait } @__announce_dollar { $ }
        [::announce_tests::defs::large::BigTrait!, format_report!] FORMAT
    };
    single_both_hand: ::announce_tests::defs::large::BigTrait! {
        @__announce_self { ::announce_tests::defs::large::BigTrait } @__announce_dollar { $ }
        (my_opt) [::announce_tests::defs::large::BigTrait!, format_report!] FORMAT
    };
    single_dollar_options_hand: ::announce_tests::defs::large::big_mod! {
        (my_opt) [::announce_tests::defs::large::big_mod!, format_report!] FORMAT
    };
    single_dollar_prefixes_hand: ::announce_tests::defs::large::big_mod! {
        @__announce_self { ::announce_tests::defs::large::big_mod } @__announce_dollar { $ }
        [::announce_tests::defs::large::big_mod!, format_report!] FORMAT
    };
    single_dollar_both_hand: ::announce_tests::defs::large::big_mod! {
        @__announce_self { ::announce_tests::defs::large::big_mod } @__announce_dollar { $ }
        (my_opt) [::announce_tests::defs::large::big_mod!, format_report!] FORMAT
    };
    // The head is a derive or an attribute: the call that `redirect` makes to the
    // trampoline of a derive or attribute macro, but for the prefix.
    derive_head_hand: ::announce_tests::defs::conflict::ParseAnnounce! {
        @__announce_self { ::announce_tests::defs::conflict::ParseAnnounce } @__announce_dollar { $ }
        [#[derive(::announce_tests::defs::conflict::Parse)] Named, format_report!] FORMAT
    };
    attribute_head_hand: ::announce_tests::defs::conflict::ParseAnnounce! {
        @__announce_self { ::announce_tests::defs::conflict::ParseAnnounce } @__announce_dollar { $ }
        [#[::announce_tests::defs::conflict::parse_attr], format_report!] FORMAT
    };
    derive_head_single_hand: ::announce_tests::defs::large::BigParseAnnounce! {
        @__announce_self { ::announce_tests::defs::large::BigParseAnnounce } @__announce_dollar { $ }
        [#[derive(::announce_tests::defs::large::BigParse)] Named, format_report!] FORMAT
    };
    derive_head_small_single_hand: ::announce_tests::defs::large::SmallSingleParseAnnounce! {
        @__announce_self { ::announce_tests::defs::large::SmallSingleParseAnnounce } @__announce_dollar { $ }
        [#[derive(::announce_tests::defs::large::SmallSingleParse)] Named, format_report!] FORMAT
    };
}

#[test]
fn trampolines_take_a_call_without_the_prefix_in_every_shape() {
    assert_eq!(direct_options_hand::FORMAT, "1");
    assert_eq!(direct_prefixes_hand::FORMAT, "1");
    assert_eq!(direct_both_hand::FORMAT, "1");
    assert_eq!(direct_dollar_options_hand::FORMAT, "1");
    assert_eq!(direct_dollar_prefixes_hand::FORMAT, "1");
    assert_eq!(direct_dollar_both_hand::FORMAT, "1");
    assert_eq!(single_options_hand::FORMAT, "1");
    assert_eq!(single_prefixes_hand::FORMAT, "1");
    assert_eq!(single_both_hand::FORMAT, "1");
    assert_eq!(single_dollar_options_hand::FORMAT, "1");
    assert_eq!(single_dollar_prefixes_hand::FORMAT, "1");
    assert_eq!(single_dollar_both_hand::FORMAT, "1");
    assert_eq!(derive_head_hand::FORMAT, "1");
    assert_eq!(attribute_head_hand::FORMAT, "1");
    assert_eq!(derive_head_single_hand::FORMAT, "1");
    assert_eq!(derive_head_small_single_hand::FORMAT, "1");
}

/// The dummy enum of a derive or attribute continuation, made by hand: its payload has no
/// prefix, so `redirect` has no version to keep and writes the one of this crate. The
/// trampoline it calls takes it, and the callback after it sees the prefix.
mod hand_made_dummies {
    use announce_tests::derive::format_report;

    /// `#[ATTR] enum NAME { __AnnouncePayload = (stringify!{ PAYLOAD }, 0).1 }`, the way a
    /// trampoline writes it, but for the prefix.
    macro_rules! dummy {
        (#[$($attr:tt)*] $name:ident { $($payload:tt)* }) => {
            #[$($attr)*]
            #[allow(dead_code, non_camel_case_types)]
            enum $name {
                __AnnouncePayload = (stringify! { $($payload)* }, 0).1,
            }
        };
    }

    dummy! {
        #[derive(::announce_tests::defs::conflict::Parse)] Direct {
            [#[derive(::announce_tests::defs::conflict::Parse)] Direct, format_report!] DIRECT
        }
    }
    dummy! {
        #[derive(::announce_tests::defs::large::BigParse)] Single {
            [#[derive(::announce_tests::defs::large::BigParse)] Single, format_report!] SINGLE
        }
    }
    dummy! {
        #[::announce_tests::defs::conflict::parse_attr] Attribute {
            [#[::announce_tests::defs::conflict::parse_attr] Attribute, format_report!] ATTRIBUTE
        }
    }
    dummy! {
        #[::announce_tests::defs::large::big_parse_attr] AttributeSingle {
            [#[::announce_tests::defs::large::big_parse_attr] AttributeSingle, format_report!]
            ATTRIBUTE_SINGLE
        }
    }
}

#[test]
fn a_dummy_made_by_hand_without_the_prefix_gets_the_prefix_of_this_version() {
    use hand_made_dummies as h;
    assert_eq!(h::DIRECT, "1");
    assert_eq!(h::SINGLE, "1");
    assert_eq!(h::ATTRIBUTE, "1");
    assert_eq!(h::ATTRIBUTE_SINGLE, "1");
}

#[test]
fn a_proc_macro_callback_called_by_hand_sees_no_format() {
    mod hand {
        use announce_tests::derive::format_report;
        format_report! { [format_report!] FORMAT }
    }
    mod prefixed {
        use announce_tests::derive::format_report;
        format_report! { @announce_format "1" [format_report!] FORMAT }
    }
    assert_eq!(hand::FORMAT, "none");
    assert_eq!(prefixed::FORMAT, "1");
}

mod derive_and_attribute_routes {
    use announce_tests::derive::format_report;
    // Derive head: the dummy enum carries the call, and `redirect` writes it again
    // for the trampoline `ParseAnnounce!`, which calls the callback.
    ::announce_tests::defs::kinds::Greet! {
        [::announce_tests::defs::kinds::Greet!, #[derive(::announce_tests::defs::conflict::Parse)] Named1, format_report!] DERIVE
    }
    // The same for a trampoline that holds its item once.
    ::announce_tests::defs::large::BigTrait! {
        [::announce_tests::defs::large::BigTrait!, #[derive(::announce_tests::defs::large::SmallSingleParse)] Named2, format_report!] SINGLE
    }
    pub mod attr {
        use announce_tests::derive::format_report;
        ::announce_tests::defs::kinds::Shape! {
            [::announce_tests::defs::kinds::Shape!, #[::announce_tests::defs::conflict::parse_attr], format_report!] ATTR
        }
    }
    // Started by a proc macro (`ContList::invoke`) with a derive head.
    pub mod started {
        use announce_tests::derive::{format_report, start};
        start!([#[derive(::announce_tests::defs::conflict::Parse)], format_report!] STARTED);
    }
    pub use attr::ATTR;
    pub use started::STARTED;
}

#[test]
fn the_derive_and_attribute_routes_keep_the_prefix() {
    use derive_and_attribute_routes as r;
    assert_eq!(r::DERIVE, "1");
    assert_eq!(r::SINGLE, "1");
    assert_eq!(r::ATTR, "1");
    assert_eq!(r::STARTED, "1");
}

mod helpers {
    use super::*;
    // `ContList::invoke`, from a proc macro.
    start!([::announce_tests::defs::kinds::Greet!, format_report!] STARTED);
    // `Input::forward_transformed`, from a transformer.
    pub mod transformed {
        use super::*;
        ::announce_tests::defs::transformed::Partly! {
            [::announce_tests::defs::transformed::Partly!, public_fields!, format_report!] FORMAT
        }
    }
    // The function forms `callback` and `transformer`, and the macro forms that wrap them
    // (`format_report!`, `compat_noop!`).
    pub mod functions {
        use announce_tests::derive::{compat_noop, format_report_fn, pass_through_fn};
        ::announce_tests::defs::kinds::Greet! {
            [::announce_tests::defs::kinds::Greet!, pass_through_fn!, format_report_fn!] FN_FN
        }
        ::announce_tests::defs::kinds::Greet! {
            [::announce_tests::defs::kinds::Greet!, compat_noop!, format_report_fn!] MACRO_FN
        }
        ::announce_tests::defs::kinds::Greet! {
            [::announce_tests::defs::kinds::Greet!, pass_through_fn!, super::super::format_report!] FN_MACRO
        }
    }
}

#[test]
fn proc_macro_helpers_write_the_prefix() {
    assert_eq!(helpers::STARTED, "1");
    assert_eq!(helpers::transformed::FORMAT, "1");
    use helpers::functions as f;
    assert_eq!((f::FN_FN, f::MACRO_FN, f::FN_MACRO), ("1", "1", "1"));
}

/// The sentinel of the runner calls the trampoline: `NAME! { @announce_format "1"
/// @__announce_exists }`. The first arm of every layout expands that to nothing, and takes
/// the call without the prefix as well.
mod sentinel_calls {
    ::announce_tests::defs::kinds::Greet! { @announce_format "1" @__announce_exists }
    ::announce_tests::defs::kinds::Greet! { @__announce_exists }
    ::announce_tests::defs::kinds::with_macro! { @announce_format "1" @__announce_exists }
    ::announce_tests::defs::kinds::with_macro! { @__announce_exists }
    ::announce_tests::defs::large::BigTrait! { @announce_format "1" @__announce_exists }
    ::announce_tests::defs::large::BigTrait! { @__announce_exists }
    ::announce_tests::defs::large::big_mod! { @announce_format "1" @__announce_exists }
    ::announce_tests::defs::large::big_mod! { @__announce_exists }
}
