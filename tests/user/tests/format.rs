//! The format prefix `@announce_format "1"`: every call that announce makes to the
//! next element of a chain starts with it, so that a macro of yours can detect that
//! announce called it. A call written by hand needs no prefix.
//!
//! The documented form is in `announce::helper::Input` (the wire format).

use announce_test_derive::{format_report, public_fields, start};

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
    direct => ::announce_test_defs::kinds::Greet;
    // `layout = direct`, with `$` in the item.
    direct_dollar => ::announce_test_defs::kinds::with_macro;
    // `layout = single`: the large item is held once.
    single => ::announce_test_defs::large::BigTrait;
    // `layout = single`, with `$` in the item.
    single_dollar => ::announce_test_defs::large::big_mod;
    // `layout = single`, chosen for a small item.
    small_single => ::announce_test_defs::large::SmallSingle;
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
    direct_hand: ::announce_test_defs::kinds::Greet! {
        [::announce_test_defs::kinds::Greet!, format_report!] FORMAT
    };
    direct_dollar_hand: ::announce_test_defs::kinds::with_macro! {
        [::announce_test_defs::kinds::with_macro!, format_report!] FORMAT
    };
    single_hand: ::announce_test_defs::large::BigTrait! {
        [::announce_test_defs::large::BigTrait!, format_report!] FORMAT
    };
    single_dollar_hand: ::announce_test_defs::large::big_mod! {
        [::announce_test_defs::large::big_mod!, format_report!] FORMAT
    };
    // By hand, with the prefix (as a helper writes it).
    direct_prefixed: ::announce_test_defs::kinds::Greet! {
        @announce_format "1" [::announce_test_defs::kinds::Greet!, format_report!] FORMAT
    };
    direct_dollar_prefixed: ::announce_test_defs::kinds::with_macro! {
        @announce_format "1" [::announce_test_defs::kinds::with_macro!, format_report!] FORMAT
    };
    single_prefixed: ::announce_test_defs::large::BigTrait! {
        @announce_format "1" [::announce_test_defs::large::BigTrait!, format_report!] FORMAT
    };
    single_dollar_prefixed: ::announce_test_defs::large::big_mod! {
        @announce_format "1" [::announce_test_defs::large::big_mod!, format_report!] FORMAT
    };
    // With the options group: the prefix comes before it.
    options_prefixed: ::announce_test_defs::kinds::Greet! {
        @announce_format "1" (my_opt) [::announce_test_defs::kinds::Greet!, format_report!] FORMAT
    };
    // Two trampolines: the second takes the call of the first, with the prefix.
    two_hops: ::announce_test_defs::kinds::Greet! {
        [::announce_test_defs::kinds::Greet!, ::announce_test_defs::large::BigTrait!, format_report!] FORMAT
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

#[test]
fn a_proc_macro_callback_called_by_hand_sees_no_format() {
    mod hand {
        use announce_test_derive::format_report;
        format_report! { [format_report!] FORMAT }
    }
    mod prefixed {
        use announce_test_derive::format_report;
        format_report! { @announce_format "1" [format_report!] FORMAT }
    }
    assert_eq!(hand::FORMAT, "none");
    assert_eq!(prefixed::FORMAT, "1");
}

mod derive_and_attribute_routes {
    use announce_test_derive::format_report;
    // Derive head: the dummy enum carries the call, and `redirect` writes it again
    // for the trampoline `ParseAnnounce!`, which calls the callback.
    ::announce_test_defs::kinds::Greet! {
        [::announce_test_defs::kinds::Greet!, #[derive(::announce_test_defs::conflict::Parse)] Named1, format_report!] DERIVE
    }
    // The same for a trampoline that holds its item once.
    ::announce_test_defs::large::BigTrait! {
        [::announce_test_defs::large::BigTrait!, #[derive(::announce_test_defs::large::SmallSingleParse)] Named2, format_report!] SINGLE
    }
    pub mod attr {
        use announce_test_derive::format_report;
        ::announce_test_defs::kinds::Shape! {
            [::announce_test_defs::kinds::Shape!, #[::announce_test_defs::conflict::parse_attr], format_report!] ATTR
        }
    }
    // Started by a proc macro (`ContList::invoke`) with a derive head.
    pub mod started {
        use announce_test_derive::{format_report, start};
        start!([#[derive(::announce_test_defs::conflict::Parse)], format_report!] STARTED);
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
    start!([::announce_test_defs::kinds::Greet!, format_report!] STARTED);
    // `Input::forward_transformed`, from a transformer.
    pub mod transformed {
        use super::*;
        ::announce_test_defs::transformed::Partly! {
            [::announce_test_defs::transformed::Partly!, public_fields!, format_report!] FORMAT
        }
    }
    // The function forms `callback` and `transformer`, and the macro forms that wrap them
    // (`format_report!`, `compat_noop!`).
    pub mod functions {
        use announce_test_derive::{compat_noop, format_report_fn, pass_through_fn};
        ::announce_test_defs::kinds::Greet! {
            [::announce_test_defs::kinds::Greet!, pass_through_fn!, format_report_fn!] FN_FN
        }
        ::announce_test_defs::kinds::Greet! {
            [::announce_test_defs::kinds::Greet!, compat_noop!, format_report_fn!] MACRO_FN
        }
        ::announce_test_defs::kinds::Greet! {
            [::announce_test_defs::kinds::Greet!, pass_through_fn!, super::super::format_report!] FN_MACRO
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
    ::announce_test_defs::kinds::Greet! { @announce_format "1" @__announce_exists }
    ::announce_test_defs::kinds::Greet! { @__announce_exists }
    ::announce_test_defs::kinds::with_macro! { @announce_format "1" @__announce_exists }
    ::announce_test_defs::kinds::with_macro! { @__announce_exists }
    ::announce_test_defs::large::BigTrait! { @announce_format "1" @__announce_exists }
    ::announce_test_defs::large::BigTrait! { @__announce_exists }
    ::announce_test_defs::large::big_mod! { @announce_format "1" @__announce_exists }
    ::announce_test_defs::large::big_mod! { @__announce_exists }
}
