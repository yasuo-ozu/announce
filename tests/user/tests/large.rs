//! The single-copy layout (DESIGN.md section 4.1): the definitions in
//! `announce_test_defs::large` and `large_leak` are above the size threshold,
//! so their trampolines hold the item once and take three expansions per call
//! (two through the derive route). Everything a caller sees is the same as with
//! small items; these tests run the large ones through every route and
//! position, and mix them with small ones.

use announce_test_defs::large;
use announce_test_derive::{gathered, public_fields, start, summary};

macro_rules! capture {
    ([$($l:tt)*] $($t:tt)*) => {
        pub const OUT: &str = stringify!([$($l)*] $($t)*);
    };
}

/// Whitespace-free form, so assertions do not depend on `stringify!` spacing.
fn norm(s: &str) -> String {
    s.split_whitespace().collect()
}

macro_rules! reemit {
    ([$($l:tt)*] $($p:ident)? $(:: $s:ident)* ! => [$($m:tt)*] { $($item:tt)* }) => {
        $($item)*
    };
}

macro_rules! reemit_two {
    ([$($l:tt)*]
     $($p1:ident)? $(:: $s1:ident)* ! => [$($m1:tt)*] { $($a:tt)* }
     $($p2:ident)? $(:: $s2:ident)* ! => [$($m2:tt)*] { $($b:tt)* }) => {
        $($a)*
        $($b)*
    };
}

fn kinds_of(rows: &[(&str, &str, &str)]) -> Vec<String> {
    rows.iter().map(|r| r.2.to_string()).collect()
}

mod single_large {
    ::announce_test_defs::large::BigTrait! { [::announce_test_defs::large::BigTrait!, capture!] rest tokens }
}

#[test]
fn the_wire_format_is_the_same() {
    let out = norm(single_large::OUT);
    assert!(
        out.starts_with(
            "[capture!]::announce_test_defs::large::BigTrait!=>[]{pubtraitBigTrait{fnm0(&self,x:u32)->u32;"
        ),
        "{}",
        out
    );
    assert!(
        out.ends_with("fnm29(&self,x:u32)->u32;}}resttokens"),
        "{}",
        out
    );
}

mod mixed {
    use super::summary;
    use announce_test_defs::{kinds, large};
    // Small (direct) and large (single-copy) definitions in one chain, and a
    // large one first and last.
    large::BigTrait! {
        [large::BigTrait!, kinds::Named!, large::BigStruct!, kinds::Greet!, large::BigEnum!,
         kinds::Shape!, large::BigUnion!, kinds::Alias!, large::BigAlias!, kinds::Tuple!,
         large::BigTrait!, summary!] ALL
    }
}

#[test]
fn small_and_large_definitions_in_one_chain() {
    let rows = mixed::ALL;
    assert_eq!(
        kinds_of(rows),
        [
            "trait BigTrait (30 items)",
            "struct Named",
            "struct BigStruct",
            "trait Greet (2 items)",
            "enum BigEnum",
            "enum Shape",
            "union BigUnion",
            "type Alias",
            "type BigAlias",
            "struct Tuple",
            "trait BigTrait (30 items)",
        ]
    );
    let keys: Vec<&str> = rows.iter().map(|r| r.0).collect();
    assert_eq!(keys[0], "BigTrait");
    assert_eq!(keys[10], "BigTrait");
}

mod long_chain {
    use super::summary;
    use announce_test_defs::{kinds, large};
    // 6 x (3 + 1 + 3 + 3 + 1) = 66 nested expansions, at the default limit (128).
    large::BigTrait! {
        [large::BigTrait!, kinds::Greet!, large::BigStruct!, large::BigEnum!, kinds::Shape!,
         large::BigTrait!, kinds::Greet!, large::BigStruct!, large::BigEnum!, kinds::Shape!,
         large::BigTrait!, kinds::Greet!, large::BigStruct!, large::BigEnum!, kinds::Shape!,
         large::BigTrait!, kinds::Greet!, large::BigStruct!, large::BigEnum!, kinds::Shape!,
         large::BigTrait!, kinds::Greet!, large::BigStruct!, large::BigEnum!, kinds::Shape!,
         large::BigTrait!, kinds::Greet!, large::BigStruct!, large::BigEnum!, kinds::Shape!,
         summary!] ALL
    }
}

#[test]
fn a_chain_of_thirty_definitions_fits_the_default_recursion_limit() {
    assert_eq!(long_chain::ALL.len(), 30);
    assert_eq!(long_chain::ALL[29].0, "Shape");
}

// The limits in DESIGN.md section 4.1: at the default recursion limit (128) a
// chain holds up to 127 small definitions (one expansion each), 63 small
// definitions with `$` (two) and 42 large ones (three).
mod at_the_limit {
    use super::summary;
    pub mod large {
        use super::summary;
        ::announce_test_defs::large::BigTrait! {
            [::announce_test_defs::large::BigTrait!,
         ::announce_test_defs::large::BigTrait!, ::announce_test_defs::large::BigTrait!, ::announce_test_defs::large::BigTrait!, ::announce_test_defs::large::BigTrait!, ::announce_test_defs::large::BigTrait!, ::announce_test_defs::large::BigTrait!,
         ::announce_test_defs::large::BigTrait!, ::announce_test_defs::large::BigTrait!, ::announce_test_defs::large::BigTrait!, ::announce_test_defs::large::BigTrait!, ::announce_test_defs::large::BigTrait!, ::announce_test_defs::large::BigTrait!,
         ::announce_test_defs::large::BigTrait!, ::announce_test_defs::large::BigTrait!, ::announce_test_defs::large::BigTrait!, ::announce_test_defs::large::BigTrait!, ::announce_test_defs::large::BigTrait!, ::announce_test_defs::large::BigTrait!,
         ::announce_test_defs::large::BigTrait!, ::announce_test_defs::large::BigTrait!, ::announce_test_defs::large::BigTrait!, ::announce_test_defs::large::BigTrait!, ::announce_test_defs::large::BigTrait!, ::announce_test_defs::large::BigTrait!,
         ::announce_test_defs::large::BigTrait!, ::announce_test_defs::large::BigTrait!, ::announce_test_defs::large::BigTrait!, ::announce_test_defs::large::BigTrait!, ::announce_test_defs::large::BigTrait!, ::announce_test_defs::large::BigTrait!,
         ::announce_test_defs::large::BigTrait!, ::announce_test_defs::large::BigTrait!, ::announce_test_defs::large::BigTrait!, ::announce_test_defs::large::BigTrait!, ::announce_test_defs::large::BigTrait!, ::announce_test_defs::large::BigTrait!,
         ::announce_test_defs::large::BigTrait!, ::announce_test_defs::large::BigTrait!, ::announce_test_defs::large::BigTrait!, ::announce_test_defs::large::BigTrait!, ::announce_test_defs::large::BigTrait!,
         summary!] ALL
        }
    }
    pub mod small {
        use super::summary;
        ::announce_test_defs::kinds::Greet! {
            [::announce_test_defs::kinds::Greet!,
         ::announce_test_defs::kinds::Greet!, ::announce_test_defs::kinds::Greet!, ::announce_test_defs::kinds::Greet!, ::announce_test_defs::kinds::Greet!, ::announce_test_defs::kinds::Greet!, ::announce_test_defs::kinds::Greet!,
         ::announce_test_defs::kinds::Greet!, ::announce_test_defs::kinds::Greet!, ::announce_test_defs::kinds::Greet!, ::announce_test_defs::kinds::Greet!, ::announce_test_defs::kinds::Greet!, ::announce_test_defs::kinds::Greet!,
         ::announce_test_defs::kinds::Greet!, ::announce_test_defs::kinds::Greet!, ::announce_test_defs::kinds::Greet!, ::announce_test_defs::kinds::Greet!, ::announce_test_defs::kinds::Greet!, ::announce_test_defs::kinds::Greet!,
         ::announce_test_defs::kinds::Greet!, ::announce_test_defs::kinds::Greet!, ::announce_test_defs::kinds::Greet!, ::announce_test_defs::kinds::Greet!, ::announce_test_defs::kinds::Greet!, ::announce_test_defs::kinds::Greet!,
         ::announce_test_defs::kinds::Greet!, ::announce_test_defs::kinds::Greet!, ::announce_test_defs::kinds::Greet!, ::announce_test_defs::kinds::Greet!, ::announce_test_defs::kinds::Greet!, ::announce_test_defs::kinds::Greet!,
         ::announce_test_defs::kinds::Greet!, ::announce_test_defs::kinds::Greet!, ::announce_test_defs::kinds::Greet!, ::announce_test_defs::kinds::Greet!, ::announce_test_defs::kinds::Greet!, ::announce_test_defs::kinds::Greet!,
         ::announce_test_defs::kinds::Greet!, ::announce_test_defs::kinds::Greet!, ::announce_test_defs::kinds::Greet!, ::announce_test_defs::kinds::Greet!, ::announce_test_defs::kinds::Greet!, ::announce_test_defs::kinds::Greet!,
         ::announce_test_defs::kinds::Greet!, ::announce_test_defs::kinds::Greet!, ::announce_test_defs::kinds::Greet!, ::announce_test_defs::kinds::Greet!, ::announce_test_defs::kinds::Greet!, ::announce_test_defs::kinds::Greet!,
         ::announce_test_defs::kinds::Greet!, ::announce_test_defs::kinds::Greet!, ::announce_test_defs::kinds::Greet!, ::announce_test_defs::kinds::Greet!, ::announce_test_defs::kinds::Greet!, ::announce_test_defs::kinds::Greet!,
         ::announce_test_defs::kinds::Greet!, ::announce_test_defs::kinds::Greet!, ::announce_test_defs::kinds::Greet!, ::announce_test_defs::kinds::Greet!, ::announce_test_defs::kinds::Greet!, ::announce_test_defs::kinds::Greet!,
         ::announce_test_defs::kinds::Greet!, ::announce_test_defs::kinds::Greet!, ::announce_test_defs::kinds::Greet!, ::announce_test_defs::kinds::Greet!, ::announce_test_defs::kinds::Greet!, ::announce_test_defs::kinds::Greet!,
         ::announce_test_defs::kinds::Greet!, ::announce_test_defs::kinds::Greet!, ::announce_test_defs::kinds::Greet!, ::announce_test_defs::kinds::Greet!, ::announce_test_defs::kinds::Greet!, ::announce_test_defs::kinds::Greet!,
         ::announce_test_defs::kinds::Greet!, ::announce_test_defs::kinds::Greet!, ::announce_test_defs::kinds::Greet!, ::announce_test_defs::kinds::Greet!, ::announce_test_defs::kinds::Greet!, ::announce_test_defs::kinds::Greet!,
         ::announce_test_defs::kinds::Greet!, ::announce_test_defs::kinds::Greet!, ::announce_test_defs::kinds::Greet!, ::announce_test_defs::kinds::Greet!, ::announce_test_defs::kinds::Greet!, ::announce_test_defs::kinds::Greet!,
         ::announce_test_defs::kinds::Greet!, ::announce_test_defs::kinds::Greet!, ::announce_test_defs::kinds::Greet!, ::announce_test_defs::kinds::Greet!, ::announce_test_defs::kinds::Greet!, ::announce_test_defs::kinds::Greet!,
         ::announce_test_defs::kinds::Greet!, ::announce_test_defs::kinds::Greet!, ::announce_test_defs::kinds::Greet!, ::announce_test_defs::kinds::Greet!, ::announce_test_defs::kinds::Greet!, ::announce_test_defs::kinds::Greet!,
         ::announce_test_defs::kinds::Greet!, ::announce_test_defs::kinds::Greet!, ::announce_test_defs::kinds::Greet!, ::announce_test_defs::kinds::Greet!, ::announce_test_defs::kinds::Greet!, ::announce_test_defs::kinds::Greet!,
         ::announce_test_defs::kinds::Greet!, ::announce_test_defs::kinds::Greet!, ::announce_test_defs::kinds::Greet!, ::announce_test_defs::kinds::Greet!, ::announce_test_defs::kinds::Greet!, ::announce_test_defs::kinds::Greet!,
         ::announce_test_defs::kinds::Greet!, ::announce_test_defs::kinds::Greet!, ::announce_test_defs::kinds::Greet!, ::announce_test_defs::kinds::Greet!, ::announce_test_defs::kinds::Greet!, ::announce_test_defs::kinds::Greet!,
         ::announce_test_defs::kinds::Greet!, ::announce_test_defs::kinds::Greet!, ::announce_test_defs::kinds::Greet!, ::announce_test_defs::kinds::Greet!, ::announce_test_defs::kinds::Greet!,
         summary!] ALL
        }
    }
    pub mod dollar {
        use super::summary;
        ::announce_test_defs::kinds::with_macro! {
            [::announce_test_defs::kinds::with_macro!,
         ::announce_test_defs::kinds::with_macro!, ::announce_test_defs::kinds::with_macro!, ::announce_test_defs::kinds::with_macro!, ::announce_test_defs::kinds::with_macro!, ::announce_test_defs::kinds::with_macro!, ::announce_test_defs::kinds::with_macro!,
         ::announce_test_defs::kinds::with_macro!, ::announce_test_defs::kinds::with_macro!, ::announce_test_defs::kinds::with_macro!, ::announce_test_defs::kinds::with_macro!, ::announce_test_defs::kinds::with_macro!, ::announce_test_defs::kinds::with_macro!,
         ::announce_test_defs::kinds::with_macro!, ::announce_test_defs::kinds::with_macro!, ::announce_test_defs::kinds::with_macro!, ::announce_test_defs::kinds::with_macro!, ::announce_test_defs::kinds::with_macro!, ::announce_test_defs::kinds::with_macro!,
         ::announce_test_defs::kinds::with_macro!, ::announce_test_defs::kinds::with_macro!, ::announce_test_defs::kinds::with_macro!, ::announce_test_defs::kinds::with_macro!, ::announce_test_defs::kinds::with_macro!, ::announce_test_defs::kinds::with_macro!,
         ::announce_test_defs::kinds::with_macro!, ::announce_test_defs::kinds::with_macro!, ::announce_test_defs::kinds::with_macro!, ::announce_test_defs::kinds::with_macro!, ::announce_test_defs::kinds::with_macro!, ::announce_test_defs::kinds::with_macro!,
         ::announce_test_defs::kinds::with_macro!, ::announce_test_defs::kinds::with_macro!, ::announce_test_defs::kinds::with_macro!, ::announce_test_defs::kinds::with_macro!, ::announce_test_defs::kinds::with_macro!, ::announce_test_defs::kinds::with_macro!,
         ::announce_test_defs::kinds::with_macro!, ::announce_test_defs::kinds::with_macro!, ::announce_test_defs::kinds::with_macro!, ::announce_test_defs::kinds::with_macro!, ::announce_test_defs::kinds::with_macro!, ::announce_test_defs::kinds::with_macro!,
         ::announce_test_defs::kinds::with_macro!, ::announce_test_defs::kinds::with_macro!, ::announce_test_defs::kinds::with_macro!, ::announce_test_defs::kinds::with_macro!, ::announce_test_defs::kinds::with_macro!, ::announce_test_defs::kinds::with_macro!,
         ::announce_test_defs::kinds::with_macro!, ::announce_test_defs::kinds::with_macro!, ::announce_test_defs::kinds::with_macro!, ::announce_test_defs::kinds::with_macro!, ::announce_test_defs::kinds::with_macro!, ::announce_test_defs::kinds::with_macro!,
         ::announce_test_defs::kinds::with_macro!, ::announce_test_defs::kinds::with_macro!, ::announce_test_defs::kinds::with_macro!, ::announce_test_defs::kinds::with_macro!, ::announce_test_defs::kinds::with_macro!,
         summary!] ALL
        }
    }
}

#[test]
fn chains_up_to_the_documented_lengths_fit_the_default_limit() {
    assert_eq!(at_the_limit::large::ALL.len(), 42);
    assert_eq!(at_the_limit::small::ALL.len(), 120);
    assert_eq!(at_the_limit::dollar::ALL.len(), 60);
}

mod dollar {
    use super::summary;
    use announce_test_defs::{kinds, large};
    // Two large `$` payloads.
    pub mod two {
        use super::*;
        large::big_mod! { [large::big_mod!, large::big_mod2!, reemit_two!] }
    }
    // Small and large `$` payloads in one chain, with a derive-free callback.
    pub mod mixed {
        use super::*;
        large::big_mod! {
            [large::big_mod!, kinds::with_macro!, large::big_mod2!, kinds::with_macro2!, summary!] S
        }
    }
    // `REST` holding `$(..)*` reaches the callback unchanged.
    pub mod rest {
        use super::*;
        large::big_mod! {
            [large::big_mod!, capture!]
            macro_rules! in_rest { ($($x:tt)*) => { $($x)* }; }
        }
    }
}

#[test]
fn dollar_payloads_of_large_items() {
    assert_eq!(dollar::two::big_mod::six(), 6);
    assert_eq!(dollar::two::big_mod2::twenty_four(), 24);
    assert_eq!(dollar::two::big_mod::f29(), 29);
    assert_eq!(
        kinds_of(dollar::mixed::S),
        [
            "mod big_mod",
            "mod with_macro",
            "mod big_mod2",
            "mod with_macro2"
        ]
    );
    let rest = norm(dollar::rest::OUT);
    assert!(
        rest.ends_with("macro_rules!in_rest{($($x:tt)*)=>{$($x)*};}"),
        "{}",
        rest
    );
    assert!(
        rest.contains("macro_rules!sum{($($x:expr),*)=>{0$(+$x)*};}"),
        "{}",
        rest
    );
}

mod derived_single {
    ::announce_test_defs::large::DerivedSingle! { [::announce_test_defs::large::DerivedSingle!, capture!] }
}

#[test]
fn the_derive_takes_the_layout_option() {
    fn is_announced<T: ?Sized + announce::Announce>() {}
    is_announced::<announce_test_defs::large::DerivedSingle>();
    let out = norm(derived_single::OUT);
    assert!(
        out.contains("=>[$crate::info(derived)]{")
            && out.ends_with("pubstructDerivedSingle{puba:u8,}}"),
        "{}",
        out
    );
}

#[test]
fn private_and_restricted_trampolines_that_hold_their_item_once() {
    use announce_test_defs::vis_single as v;
    assert!(norm(v::a::SAME_CRATE).contains("structCrateOnly"));
    assert!(norm(v::b::SAME_CRATE).contains("structPrivate"));
    assert!(norm(v::c::SAME_CRATE).contains("enumSuperOnly"));
    assert!(norm(v::d::SAME_CRATE).contains("traitBigTrait"));
    assert!(norm(v::e::SAME_CRATE).contains("modbig_mod"));
    assert!(norm(v::BARE).contains("structPrivate"));
    assert!(norm(v::f::SAME_CRATE).contains("structPrivate"));
}

mod dollar_crate {
    pub mod plain {
        announce_test_defs::via_dollar_crate_large!();
    }
    pub mod with_dollar {
        announce_test_defs::via_dollar_crate_large_mod!();
    }
}

#[test]
fn dollar_crate_in_the_list_of_a_large_item() {
    assert!(norm(dollar_crate::plain::VIA_DOLLAR_CRATE).contains("pubtraitBigTrait"));
    assert!(norm(dollar_crate::with_dollar::VIA_DOLLAR_CRATE).contains("pubmodbig_mod"));
}

mod derive_route {
    use super::{start, summary};
    use announce_test_defs::{conflict, kinds, large};

    // A small definition, then a derive into a large trampoline (named dummy).
    pub mod named {
        use super::*;
        kinds::Greet! { [kinds::Greet!, #[derive(large::BigParse)] N1, summary!] S }
    }
    // The same, with an anonymous dummy (the fallback name of Greet).
    pub mod anon {
        use super::*;
        kinds::Bits! { [kinds::Bits!, #[derive(large::BigParse)], summary!] S }
    }
    // The route starts with the derive: head = derive, next = bang. Two
    // unnamed routes in one module get gensym names.
    pub mod twice {
        use super::*;
        start!([#[derive(large::BigParse)], summary!] ONE);
        start!([#[derive(large::BigParse)], summary!] TWO);
    }
    // Head = derive of a large trampoline, next = derive (named by the
    // helper) of a small one: the self prefix is ignored by the direct layout.
    pub mod to_small {
        use super::*;
        start!([#[derive(large::BigParse)], #[derive(conflict::Parse)], summary!] S);
    }
    // Head = derive of a large trampoline, next = anonymous derive.
    pub mod to_anon {
        use super::*;
        kinds::Greet! {
            [kinds::Greet!, #[derive(large::BigParse)] N1, #[derive(conflict::Parse)], summary!] S
        }
    }
    // Head = derive of a large trampoline, next = another large one (derive).
    pub mod to_large {
        use super::*;
        start!([#[derive(large::BigParse)], #[derive(large::BigParse)], large::BigStruct!, summary!] S);
    }
    // A large bang head into a derive of a small and of a large one.
    pub mod from_large {
        use super::*;
        large::BigTrait! {
            [large::BigTrait!, #[derive(conflict::Parse)] A1, #[derive(large::BigParse)], summary!] S
        }
    }
    // The attribute form consumes the dummy.
    pub mod attr {
        use super::*;
        kinds::Shape! { [kinds::Shape!, #[large::big_parse_attr], summary!] S }
        pub mod after_large {
            use super::*;
            large::BigStruct! { [large::BigStruct!, #[large::big_parse_attr], summary!] S }
        }
    }
    // A small trampoline held once (`layout = single`) as the derive target.
    pub mod small_single {
        use super::*;
        kinds::Greet! { [kinds::Greet!, #[derive(large::SmallSingleParse)] N1, summary!] S }
    }
}

#[test]
fn derive_and_attribute_routes_into_large_trampolines() {
    use derive_route::*;
    assert_eq!(
        kinds_of(named::S),
        ["trait Greet (2 items)", "trait BigParse (30 items)"]
    );
    assert_eq!(named::S[1].0, "BigParse");
    assert_eq!(
        kinds_of(anon::S),
        ["union Bits", "trait BigParse (30 items)"]
    );
    assert_eq!(twice::ONE.len(), 1);
    assert_eq!(twice::TWO.len(), 1);
    assert_eq!(twice::ONE[0].2, "trait BigParse (30 items)");
    assert_eq!(
        kinds_of(to_small::S),
        ["trait BigParse (30 items)", "trait Parse (1 items)"]
    );
    assert_eq!(
        kinds_of(to_anon::S),
        [
            "trait Greet (2 items)",
            "trait BigParse (30 items)",
            "trait Parse (1 items)"
        ]
    );
    assert_eq!(
        kinds_of(to_large::S),
        [
            "trait BigParse (30 items)",
            "trait BigParse (30 items)",
            "struct BigStruct"
        ]
    );
    assert_eq!(
        kinds_of(from_large::S),
        [
            "trait BigTrait (30 items)",
            "trait Parse (1 items)",
            "trait BigParse (30 items)"
        ]
    );
    assert_eq!(
        kinds_of(attr::S),
        ["enum Shape", "trait BigParse (30 items)"]
    );
    assert_eq!(attr::S[1].0, "big_parse_attr");
    assert_eq!(
        kinds_of(attr::after_large::S),
        ["struct BigStruct", "trait BigParse (30 items)"]
    );
    assert_eq!(
        kinds_of(small_single::S),
        ["trait Greet (2 items)", "trait SmallSingleParse (1 items)"]
    );
}

// Positions: expression (with and without `(expr)`), type, pattern, statement.
macro_rules! expr_cb {
    ($( ($($o:tt)*) )? [$($l:tt)*] $($p:ident)? $(:: $s:ident)* ! => [$($m:tt)*] { $($item:tt)* } $($r:tt)*) => {
        stringify!($($item)*)
    };
}
macro_rules! expr_two {
    ($( ($($o:tt)*) )? [$($l:tt)*]
     $($p1:ident)? $(:: $s1:ident)* ! => [$($m1:tt)*] { $($a:tt)* }
     $($p2:ident)? $(:: $s2:ident)* ! => [$($m2:tt)*] { $($b:tt)* }) => {
        [stringify!($($a)*), stringify!($($b)*)]
    };
}
macro_rules! ty_cb {
    ($($t:tt)*) => {
        u32
    };
}
macro_rules! opts_cb {
    (($($o:tt)*) [$($l:tt)*] $($r:tt)*) => { stringify!($($o)*) };
}
macro_rules! struct_pat {
    ([$($l:tt)*] $($p:ident)? $(:: $s:ident)* ! => [$($m:tt)*]
     { $(#[$($a:tt)*])* $v:vis struct $name:ident $($body:tt)* } $($r:tt)*) => {
        $name { .. }
    };
}
macro_rules! first_variant_pat {
    ([$($l:tt)*] $($p:ident)? $(:: $s:ident)* ! => [$($m:tt)*]
     { $(#[$($a:tt)*])* $v:vis enum $name:ident { $first:ident $($body:tt)* } } $($r:tt)*) => {
        $name::$first(..)
    };
}
macro_rules! let_cb {
    ([$($l:tt)*] $($p:ident)? $(:: $s:ident)* ! => [$($m:tt)*] { $($item:tt)* } $bound:ident) => {
        let $bound = stringify!($($item)*);
    };
}

type FromTypePosition = large::BigTrait! { [large::BigTrait!, ty_cb!] };
type DollarInTypePosition = large::big_mod! { [large::big_mod!, ty_cb!] };

#[test]
fn expression_and_type_positions() {
    let t: &str = large::BigTrait! { [large::BigTrait!, expr_cb!] };
    assert!(
        norm(t).starts_with("pubtraitBigTrait{fnm0(&self,x:u32)->u32;"),
        "{}",
        t
    );
    // A `$` payload, with and without `(expr)` (the option has no effect).
    for m in [
        large::big_mod! { [large::big_mod!, expr_cb!] },
        large::big_mod! { (expr) [large::big_mod!, expr_cb!] },
    ] {
        assert!(
            norm(m).contains("macro_rules!sum{($($x:expr),*)=>{0$(+$x)*};}"),
            "{}",
            m
        );
    }
    let both = large::big_mod! {
        (expr) [large::big_mod!, large::big_mod2!, expr_two!]
    };
    // Newest first.
    assert!(norm(both[0]).contains("modbig_mod2{"), "{}", both[0]);
    assert!(norm(both[1]).contains("modbig_mod{"), "{}", both[1]);
    let o: &str = large::big_mod! {
        (expr, my_opt(1)) [large::big_mod!, large::BigTrait!, opts_cb!]
    };
    assert_eq!(norm(o), "expr,my_opt(1)");
    let x: FromTypePosition = 7;
    assert_eq!(x, 7u32);
    let y: DollarInTypePosition = 8;
    assert_eq!(y, 8u32);
}

#[test]
fn pattern_and_statement_positions() {
    use announce_test_defs::large::{BigEnum, BigStruct};
    // Only the names matter: all-zero bits are a valid `BigStruct`.
    let s: BigStruct = unsafe { std::mem::zeroed() };
    assert!(matches!(
        s,
        ::announce_test_defs::large::BigStruct! { [::announce_test_defs::large::BigStruct!, struct_pat!] }
    ));
    // A `let` is a pattern position too.
    let ::announce_test_defs::large::BigStruct! { [::announce_test_defs::large::BigStruct!, struct_pat!] } =
        s;
    let classify = |e: &BigEnum| match e {
        ::announce_test_defs::large::BigEnum! { [::announce_test_defs::large::BigEnum!, first_variant_pat!] } => {
            "first"
        }
        _ => "other",
    };
    assert_eq!(classify(&BigEnum::V0(1)), "first");
    assert_eq!(classify(&BigEnum::V69(1)), "other");
    // A `let` produced by the callback, and an item re-emitted into a body.
    large::BigTrait! { [large::BigTrait!, let_cb!] bound }
    assert!(norm(bound).contains("pubtraitBigTrait"), "{}", bound);
    large::BigUnion! { [large::BigUnion!, reemit!] }
    let u = BigUnion { f0: 7 };
    assert_eq!(unsafe { u.f0 }, 7);
}

mod transformed {
    use super::{public_fields, summary};
    use announce_test_defs::large;
    pub mod sig {
        use super::*;
        large::BigSig! { [large::BigSig!, ::announce::signature_only!, capture!] }
    }
    pub mod module {
        use super::*;
        large::big_sig_mod! { [large::big_sig_mod!, ::announce::signature_only!, reemit!] }
    }
    pub mod fields {
        use super::*;
        large::BigStruct! { [large::BigStruct!, public_fields!, summary!] S }
    }
    pub mod chain {
        use super::*;
        large::BigSig! {
            [large::BigSig!, ::announce::signature_only!, large::BigStruct!, public_fields!, summary!] S
        }
    }
}

#[test]
fn transformers_see_the_large_entry() {
    let out = norm(transformed::sig::OUT);
    assert!(out.contains("fnrequired(&self)->u32;"), "{}", out);
    assert!(
        out.contains("fnbody(&self)->u32{::core::unimplemented!()}"),
        "{}",
        out
    );
    assert!(
        out.contains("fnp13(&self)->u32{::core::unimplemented!()}"),
        "{}",
        out
    );
    assert!(!out.contains("42"), "{}", out);
    // The `impl Trait` fn keeps its body, the others panic.
    assert_eq!(transformed::module::big_sig_mod::opaque().to_string(), "8");
    assert!(std::panic::catch_unwind(transformed::module::big_sig_mod::plain).is_err());
    assert!(std::panic::catch_unwind(transformed::module::big_sig_mod::f3).is_err());
    // `public_fields!` rewrote the payload and added `fields(..)` to META.
    let meta = norm(transformed::fields::S[0].1);
    assert!(meta.starts_with("fields(f0,f1,f2,"), "{}", meta);
    assert!(meta.ends_with("f78,f79)"), "{}", meta);
    // Two transformers in a chain, on a trait and a struct.
    assert_eq!(
        kinds_of(transformed::chain::S),
        ["trait BigSig (30 items)", "struct BigStruct"]
    );
}

mod checked {
    use super::summary;
    use announce_test_defs::large;
    large::BigChecked! { [large::BigChecked!, summary!] CHECKED }
}

#[test]
fn checks_and_meta_travel_with_the_large_item() {
    assert_eq!(
        norm(checked::CHECKED[0].1),
        "announce_test_derive::no_tuple(big),my_lib::info(big)"
    );
    assert_eq!(checked::CHECKED[0].2, "struct BigChecked");
}

mod gather {
    use super::gathered;
    gathered! {
        [gathered!]
        (::announce_test_defs::large::BigTrait, ::announce_test_defs::kinds::Shape,
         ::announce_test_defs::large::BigStruct, ::announce_test_defs::large::BigAlias,
         ::announce_test_defs::large::BigEnum) G
    }
    // Some of them are already there: only the others are fetched.
    pub mod partly {
        use super::gathered;
        ::announce_test_defs::large::BigEnum! {
            [::announce_test_defs::large::BigEnum!, gathered!]
            (::announce_test_defs::large::BigTrait, ::announce_test_defs::large::BigEnum) G
        }
    }
}

#[test]
fn gather_fetches_large_definitions() {
    assert_eq!(
        gather::G,
        ["BigTrait", "Shape", "BigStruct", "BigAlias", "BigEnum"]
    );
    assert_eq!(gather::partly::G, ["BigTrait", "BigEnum"]);
}

mod layouts {
    use super::summary;
    use announce_test_defs::large;
    // A small item held once, and a large item copied into each arm.
    pub mod chain {
        use super::*;
        large::SmallSingle! {
            [large::SmallSingle!, large::BigDirect!, large::SmallSingleStruct!,
             large::small_single_mod!, large::big_direct_mod!, large::BigTrait!, summary!] S
        }
    }
    pub mod emit {
        use super::*;
        large::small_single_mod! { [large::small_single_mod!, large::big_direct_mod!, reemit_two!] }
    }
    pub mod single {
        use super::*;
        large::SmallSingleStruct! { [large::SmallSingleStruct!, reemit!] }
    }
    pub mod expr {
        use super::*;
        pub fn run() -> &'static str {
            large::small_single_mod! { (expr) [large::small_single_mod!, expr_cb!] }
        }
    }
}

#[test]
fn layout_option_chooses_the_trampoline() {
    assert_eq!(
        kinds_of(layouts::chain::S),
        [
            "trait SmallSingle (1 items)",
            "trait BigDirect (30 items)",
            "struct SmallSingleStruct",
            "mod small_single_mod",
            "mod big_direct_mod",
            "trait BigTrait (30 items)",
        ]
    );
    assert_eq!(layouts::emit::small_single_mod::six(), 6);
    assert_eq!(layouts::emit::big_direct_mod::six(), 6);
    assert_eq!(layouts::emit::big_direct_mod::f29(), 29);
    let t = layouts::single::SmallSingleStruct("x", 1);
    assert_eq!((t.0, t.1), ("x", 1));
    assert!(norm(layouts::expr::run()).contains("macro_rules!sum"));
}

// type-leak: the `type_leak(..)` META is part of the single copy of META.
mod leaked {
    pub mod s {
        ::announce_test_defs::large_leak::BigLeaky! { [::announce_test_defs::large_leak::BigLeaky!, ::announce_test_derive::apply_leak!] M }
    }
    pub mod t {
        ::announce_test_defs::large_leak::BigLeakTrait! { [::announce_test_defs::large_leak::BigLeakTrait!, ::announce_test_derive::apply_leak!] M }
    }
    pub mod ty {
        ::announce_test_defs::large_leak::BigLeaky! { [::announce_test_defs::large_leak::BigLeaky!, ::announce_test_derive::leak_types!] S }
    }
    pub mod ty_trait {
        ::announce_test_defs::large_leak::BigLeakTrait! { [::announce_test_defs::large_leak::BigLeakTrait!, ::announce_test_derive::leak_types!] T }
    }
    // The original payload and META.
    pub mod raw {
        ::announce_test_defs::large_leak::BigLeaky! { [::announce_test_defs::large_leak::BigLeaky!, capture!] }
    }
    // A chain: a plain large item, then a leaky one, then a small one.
    pub mod chain {
        ::announce_test_defs::large::BigStruct! {
            [::announce_test_defs::large::BigStruct!, ::announce_test_defs::large_leak::BigLeaky!,
             ::announce_test_defs::kinds::Shape!, ::announce_test_derive::apply_leak!] M
        }
    }
}

macro_rules! impl_methods {
    ($($m:ident)*) => { $(fn $m(&self, x: u32) -> u32 { x })* };
}

#[test]
fn type_leak_with_a_large_item() {
    use announce_test_defs::large_leak::inner::Local;
    // `Local` is not in scope in `leaked::s`; `TypeLeak::apply` resolves it.
    fn local_of(b: &leaked::s::BigLeaky<u8>) -> &Local {
        &b.a
    }
    fn vec_of(b: &leaked::s::BigLeaky<u8>) -> &Vec<u8> {
        &b.b
    }
    let _ = (local_of, vec_of);
    assert!(std::mem::size_of::<leaked::s::BigLeaky<u8>>() >= 80 * 4);
    struct X;
    impl leaked::t::BigLeakTrait for X {
        fn make(&self) -> Local {
            Local(5)
        }
        impl_methods!(m0 m1 m2 m3 m4 m5 m6 m7 m8 m9 m10 m11 m12 m13 m14 m15 m16 m17 m18 m19
                      m20 m21 m22 m23 m24 m25 m26 m27 m28 m29);
    }
    assert_eq!(leaked::t::BigLeakTrait::make(&X), Local(5));
    assert_eq!(leaked::t::BigLeakTrait::m29(&X, 3), 3);
    // The META carries type-leak's `Referrer`.
    let m = norm(leaked::s::M[0]);
    assert!(m.contains("referrer=[\"type-leak-format-"), "{}", m);
    assert!(m.contains("(Local)") && m.contains("<T>;(Vec<T>)"), "{}", m);
    let raw = norm(leaked::raw::OUT);
    assert!(
        raw.contains("type_leak(repeater=$crate::large_leak::Repeater,id="),
        "{}",
        raw
    );
    assert!(
        raw.contains("{pubstructBigLeaky<T>{puba:Local,pubb:Vec<T>,pubf0:u32,"),
        "{}",
        raw
    );
    // The types the callback replaces, by `Entry::type_leak`.
    assert_eq!(leaked::ty::S, ["[Local@(),Vec<T>@(T,)]"]);
    assert_eq!(leaked::ty_trait::T, ["[Local@()]"]);
    // In a chain with other large and small items, each keeps its own META.
    assert_eq!(leaked::chain::M.len(), 3);
    let metas: Vec<String> = leaked::chain::M.iter().map(|m| norm(m)).collect();
    assert_eq!(metas[0], "");
    assert!(
        metas[1].starts_with("type_leak(repeater=$crate::large_leak::Repeater,id="),
        "{:?}",
        metas
    );
    assert_eq!(metas[2], "");
}
