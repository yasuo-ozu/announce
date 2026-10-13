//! Items with attributes owned by checks (`#[my_lib(..)]` inside the item).
//! The definitions are in `announce_test_defs::owned`; the feature `x` is off
//! there and on here.

use announce_test_defs::owned as d;
use announce_test_derive::{owned_report, rest_probe, summary};

macro_rules! capture {
    ([$($l:tt)*] $($t:tt)*) => {
        pub const OUT: &str = stringify!($($t)*);
    };
}

fn norm(s: &str) -> String {
    s.split_whitespace().collect()
}

#[test]
fn the_items_compile_without_the_attributes() {
    let f = d::Fields {
        a: 1,
        b: 2,
        c: 3,
        d: 4,
        e: 5,
    };
    assert_eq!(f.clone(), f);
    assert_eq!(d::Choice::B(1), d::Choice::B(1));
    assert_eq!(<Impl as d::Tr<u8>>::C, 1);
    assert_eq!(d::Tr::f(&Impl, 5), 7);
    assert_eq!(d::inline::three(), 3);
    assert_eq!(d::inline::Z { z: 1 }.z, 1);
    assert_eq!(d::with_dollar::six(), 6);
    let _ = (
        d::Two {
            a: 1,
            b: 2,
            t: 3,
            c: 4,
        },
        d::Mixed { m: 1 },
        d::Twice { t: 1 },
        d::Nothing { n: 1 },
    );
    let _ = (d::SingleOwned { s: 1 }, d::DirectOwned { d: 1 });
    assert_eq!(
        d::BigOwned {
            f00: 0,
            f01: 0,
            f02: 0,
            f03: 0,
            f04: 0,
            f05: 0,
            f06: 0,
            f07: 0,
            f08: 0,
            f09: 0,
            f10: 0,
            f11: 0,
            f12: 0,
            f13: 0,
            f14: 0,
            f15: 0,
            f16: 0,
            f17: 0,
            f18: 0,
            f19: 0,
            f20: 0,
            f21: 0,
            f22: 0,
            f23: 0,
            f24: 0,
            f25: 0,
            f26: 0,
            f27: 0,
            f28: 0,
            f29: 0,
            f30: 0,
            f31: 0,
            f32: 0,
            f33: 0,
            f34: 0,
            f35: 0,
            f36: 0,
            f37: 0,
            f38: 0,
            f39: 0
        }
        .f39,
        0
    );
}

struct Impl;
impl d::Tr<u8> for Impl {
    type A = u8;
    const C: u8 = 1;
    fn f(&self, x: u8) -> u8 {
        x + 2
    }
}

#[test]
fn the_check_sees_the_attributes_in_source_order() {
    // The checks saw the item with `cfg_attr` evaluated in the defining crate.
    assert_eq!(
        d::SEEN_FIELDS,
        ["my_lib(skip)", "my_lib::tag=\"x\"", "my_lib(from_cfg_attr)"]
    );
    assert_eq!(
        d::SEEN_CHOICE,
        [
            "my_lib(on_enum)",
            "my_lib(first)",
            "my_lib(skip)",
            "my_lib(inner)",
            "my_lib(named)"
        ]
    );
    assert_eq!(
        d::SEEN_TR,
        [
            "my_lib(on_trait)",
            "my_lib(generic)",
            "my_lib(assoc_type)",
            "my_lib(assoc_const)",
            "my_lib(method)",
            "my_lib(param)",
            "my_lib(stmt)"
        ]
    );
    assert_eq!(
        d::SEEN_INLINE,
        ["my_lib(inner_attr)", "my_lib(in_mod)", "my_lib(field)"]
    );
    assert_eq!(d::SEEN_SINGLEOWNED, ["my_lib(s)"]);
    assert_eq!(d::SEEN_DIRECTOWNED, ["my_lib(d)"]);
    assert_eq!(d::SEEN_BIGOWNED.len(), 40);
    assert_eq!(d::SEEN_WITH_DOLLAR, ["my_lib(on_macro)", "my_lib(on_fn)"]);
    assert_eq!(d::mocks::SEEN_MOCKED, ["my_lib(on_method)"]);
    assert_eq!(
        d::SEEN_NOTEMITTED,
        ["my_lib(never_compiled)", "my_lib(field)"]
    );
    assert_eq!(d::SEEN_RENAMED, ["my_lib(r)"]);
}

mod fields {
    use super::d;
    d::Fields! { [d::Fields!, capture!] REST }
}

#[test]
fn the_payload_keeps_the_attributes_and_meta_lists_them() {
    let out = norm(fields::OUT);
    assert!(
        out.starts_with(
            "d::Fields!=>[announce_test_derive::record_attrs,attrs(my_lib)]{#[derive(Clone,Debug,PartialEq)]"
        ),
        "{}",
        out
    );
    // The docs (on the item and on `c`) are not in the payload.
    assert!(
        !out.contains("#[doc=") && !out.contains("documentedfield"),
        "{}",
        out
    );
    assert!(
        out.contains("#[derive(Clone,Debug,PartialEq)]pubstructFields{"),
        "{}",
        out
    );
    assert!(out.contains("#[my_lib(skip)]puba:u8"), "{}", out);
    assert!(out.contains("#[my_lib::tag=\"x\"]"), "{}", out);
    // `cfg_attr` and `cfg` are evaluated and gone.
    assert!(out.contains("#[my_lib(from_cfg_attr)]pubd:u8"), "{}", out);
    assert!(
        !out.contains("from_cfg_attr_x") && !out.contains("only_with_x"),
        "{}",
        out
    );
    assert!(!out.contains("#[cfg"), "{}", out);
    assert!(out.ends_with("REST"), "{}", out);
}

mod report {
    use super::{d, owned_report};
    d::Fields! { [d::Fields!, d::Choice!, d::Tr!, d::inline!, d::Two!, d::Mixed!, d::Twice!, d::Nothing!, owned_report!] ALL }
}

#[test]
fn the_entry_reads_the_owned_names_and_strips_them() {
    // names | owned attributes in the payload | after `item_without_owned_attrs`
    assert_eq!(
        report::ALL,
        [
            "my_lib|3|0",
            "my_lib|5|0",
            "my_lib|7|0",
            "my_lib|3|0",
            "my_lib,other,tag,rules|4|0",
            "my_lib|1|0",
            "my_lib|1|0",
            "|0|0",
        ]
    );
}

mod meta {
    use super::{d, summary};
    d::Two! { [d::Two!, d::Mixed!, d::Twice!, d::Nothing!, d::SingleOwned!, d::DirectOwned!, d::BigOwned!, d::with_dollar!, d::mocks::Mocked!, d::NotEmitted!, d::RenamedOwned!, summary!] ALL }
}

#[test]
fn meta_lists_the_attrs_after_the_checks() {
    let metas: Vec<String> = meta::ALL.iter().map(|r| norm(r.1)).collect();
    assert_eq!(
        metas[0],
        "announce_test_derive::lib_check,announce_test_derive::other_check,$crate::rules_check,attrs(my_lib,other,tag,rules)"
    );
    // A check that owns nothing adds nothing; the same check twice, one name.
    assert_eq!(
        metas[1],
        "$crate::plain_rules_check,announce_test_derive::no_tuple,announce_test_derive::lib_check,attrs(my_lib)"
    );
    assert_eq!(
        metas[2],
        "announce_test_derive::lib_check,announce_test_derive::lib_check,attrs(my_lib)"
    );
    // No attribute is owned: no `attrs(..)`.
    assert_eq!(
        metas[3],
        "announce_test_derive::no_tuple,$crate::plain_rules_check"
    );
    assert_eq!(metas[4], "announce_test_derive::record_attrs,attrs(my_lib)");
    assert_eq!(metas[5], "announce_test_derive::record_attrs,attrs(my_lib)");
    assert_eq!(metas[6], "announce_test_derive::record_attrs,attrs(my_lib)");
    assert_eq!(metas[7], "announce_test_derive::record_attrs,attrs(my_lib)");
    assert_eq!(
        metas[8],
        "announce_test_derive::record_attrs,attrs(my_lib),mock($crate::owned::mocks::Real)"
    );
}

mod chain {
    use super::{d, owned_report};
    // A chain with a large and a single-copy item, held once, with attributes.
    d::BigOwned! { [d::BigOwned!, d::SingleOwned!, d::Fields!, d::with_dollar!, d::DirectOwned!, owned_report!] ALL }
}

#[test]
fn chains_of_large_and_single_copy_items_keep_the_attributes() {
    assert_eq!(
        chain::ALL,
        [
            "my_lib|40|0",
            "my_lib|1|0",
            "my_lib|3|0",
            "my_lib|2|0",
            "my_lib|1|0"
        ]
    );
}

mod renamed {
    use super::d;
    d::RenamedOwned! { [d::RenamedOwned!, capture!] }
}

#[test]
fn a_renamed_trampoline_carries_the_attributes() {
    let out = norm(renamed::OUT);
    assert!(out.contains("#[my_lib(r)]pubr:u8"), "{}", out);
}

announce_test_defs::make_owned!(Made);

#[test]
fn a_library_macro_defines_items_with_dollar_crate_check_paths() {
    assert_eq!(Made { x: 1 }.x, 1);
    mod m {
        use super::Made;
        Made! { [Made!, capture!] }
    }
    let out = norm(m::OUT);
    assert!(out.contains("#[rules(made)]pubx:u8"), "{}", out);
    assert!(out.contains("attrs(rules)"), "{}", out);
}

#[test]
fn an_item_in_a_function_body_goes_through_the_query() {
    #[announce::announce(announce_test_derive::lib_check, announce_test_derive::other_check)]
    #[derive(Debug)]
    pub struct Local {
        #[my_lib(a)]
        #[other(b)]
        pub x: u8,
    }
    macro_rules! here {
        ([$($l:tt)*] $($t:tt)*) => {
            const HERE: &str = stringify!($($t)*);
        };
    }
    Local! { [Local!, here!] }
    let local = Local { x: 3 };
    assert_eq!(
        (local.x, format!("{:?}", local)),
        (3, "Local { x: 3 }".to_string())
    );
    let out = norm(HERE);
    assert!(out.contains("attrs(my_lib,other,tag)"), "{}", out);
    assert!(out.contains("#[my_lib(a)]#[other(b)]pubx:u8"), "{}", out);
}

mod heads {
    use super::{d, owned_report};
    d::Heads! { [d::Heads!, owned_report!] ALL }
}

#[test]
fn every_check_sees_itself_as_the_head_of_the_list() {
    // `Heads` compiles only if each of its four checks found its own path at the head
    // of the query's list (`head_probe!` twice, `crate::head_check!` by its pattern),
    // and forwarded the list headed by the next one. Each check added its names.
    let _ = d::Heads { a: 1, b: 2, c: 3 };
    assert_eq!(heads::ALL, ["probed,head_seen,rules|3|0"]);
}

#[test]
fn macro_rules_checks_skip_their_head_in_every_path_form() {
    // Absolute paths in the head position and in the next position, `macro_rules!`
    // checks next to each other and before a proc-macro check, then `finish!`.
    #[announce::announce(
        ::announce_test_defs::plain_rules_check,
        ::announce_test_defs::rules_check,
        ::announce_test_derive::head_probe,
        announce_test_defs::rules_check
    )]
    pub struct Abs {
        #[rules(a)]
        pub a: u8,
        #[probed]
        pub b: u8,
    }
    macro_rules! here {
        ([$($l:tt)*] $($t:tt)*) => {
            const HERE: &str = stringify!($($t)*);
        };
    }
    Abs! { [Abs!, here!] }
    let abs = Abs { a: 1, b: 2 };
    assert_eq!((abs.a, abs.b), (1, 2));
    let out = norm(HERE);
    assert!(out.contains("attrs(rules,probed)"), "{}", out);
    assert!(
        out.contains("#[rules(a)]puba:u8,#[probed]pubb:u8"),
        "{}",
        out
    );
    // META lists the checks as they were written.
    assert!(out.contains("::announce_test_defs::plain_rules_check,::announce_test_defs::rules_check,::announce_test_derive::head_probe,announce_test_defs::rules_check"), "{}", out);
}

mod dollar_rules {
    use super::{d, owned_report};
    d::with_dollar_rules! { [d::with_dollar_rules!, owned_report!] ALL }
}

#[test]
fn an_item_with_dollar_passes_through_a_macro_rules_check() {
    assert_eq!(dollar_rules::ALL, ["rules|1|0"]);
    assert_eq!(d::with_dollar_rules::twenty_four(), 24);
}

mod derived_check {
    use super::{owned_report, summary};
    announce_test_defs::checks::DerivedChecked! { [announce_test_defs::checks::DerivedChecked!, summary!] ONE }
    announce_test_defs::checks::DerivedChecked! { [announce_test_defs::checks::DerivedChecked!, owned_report!] TWO }
}

#[test]
fn the_derive_form_runs_its_checks_through_the_query() {
    let row = derived_check::ONE[0];
    assert_eq!(row.2, "struct DerivedChecked");
    // The check is in META, and no attribute is owned.
    assert_eq!(
        norm(row.1),
        "announce_test_derive::no_tuple(derived),$crate::info(1)"
    );
    assert_eq!(derived_check::TWO, ["|0|0"]);
    // The item is the user's, as written, with the `Announce` impl.
    fn assert_announced<T: announce::Announce>() {}
    assert_announced::<announce_test_defs::checks::DerivedChecked>();
}

// REST: the tokens after the entries are the caller's. The protocol puts the owned
// names in META (`attrs(..)`, read by `Entry::owned_attrs`) and never in REST, however
// much REST looks like the protocol.

/// The REST that the tests below pass, written once: `rest!(stringify)` is its
/// text, and `rest!(call_def d::Name; [d::Name!, rest_probe!])` calls a
/// definition with it. It has the words of the query and of META (`attrs(..)`,
/// `@__announce_attrs`), a list, a group, a `#[my_lib(..)]` attribute and an
/// entry that is not at the head, so it is not one.
macro_rules! rest {
    ($cb:ident $($pre:tt)*) => {
        $cb! {
            $($pre)*
            attrs(my_lib) @__announce_attrs [x!] { .. } #[my_lib(skip)]
            [rest_cb!, ::rest_last!] ghost! => [attrs(ghost)] { struct Ghost; } @__announce_self { a::b }
        }
    };
}

macro_rules! call_def {
    ($($seg:ident)::+ ; [$($list:tt)*] $($rest:tt)*) => {
        $($seg)::+ ! { [$($list)*] $($rest)* }
    };
}

/// The text of the REST that `rest!` passes.
fn passed() -> String {
    norm(rest!(stringify))
}

#[test]
fn rest_is_what_the_caller_passed_with_owned_attributes() {
    mod nontrivial {
        use super::{d, rest_probe};
        rest!(call_def d::Fields; [d::Fields!, rest_probe!]);
    }
    mod empty {
        use super::{d, rest_probe};
        d::Fields! { [d::Fields!, rest_probe!] }
    }
    mod plain {
        use super::{d, rest_probe};
        d::Fields! { [d::Fields!, rest_probe!] just some words }
    }
    // The owned names are in META, and `Entry::owned_attrs` reads them there.
    assert_eq!(nontrivial::OWNED, ["my_lib"]);
    assert_eq!(nontrivial::ENTRIES, 1);
    assert_eq!(
        norm(nontrivial::META[0]),
        "announce_test_derive::record_attrs,attrs(my_lib)"
    );
    // REST is exactly the caller's tokens: nothing added, nothing taken.
    assert_eq!(norm(nontrivial::REST), passed());
    assert!(!passed().is_empty());
    // Without REST there is none, although the entry owns `my_lib`: the names are
    // not in REST. The entry at the head of REST is not looked for in REST.
    assert_eq!(empty::OWNED, ["my_lib"]);
    assert_eq!(empty::ENTRIES, 1);
    assert_eq!(empty::REST, "");
    assert_eq!(norm(empty::META[0]), norm(nontrivial::META[0]));
    assert_eq!(norm(plain::REST), "justsomewords");
    assert_eq!(plain::OWNED, ["my_lib"]);
}

#[test]
fn rest_is_unchanged_through_a_chain_with_and_without_owned_attributes() {
    mod owned_first {
        use super::{d, rest_probe};
        rest!(call_def d::Two; [d::Two!, d::Nothing!, rest_probe!]);
    }
    mod owned_last {
        use super::{d, rest_probe};
        rest!(call_def d::Nothing; [d::Nothing!, d::Two!, rest_probe!]);
    }
    mod sandwich {
        use super::{d, rest_probe};
        rest!(call_def d::Fields; [d::Fields!, d::Nothing!, d::Two!, rest_probe!]);
    }
    for (rest, entries, owned) in [
        (owned_first::REST, owned_first::ENTRIES, owned_first::OWNED),
        (owned_last::REST, owned_last::ENTRIES, owned_last::OWNED),
    ] {
        assert_eq!(norm(rest), passed());
        assert_eq!(entries, 2);
        // One row for each entry, in the order they were visited.
        assert_eq!(owned.len(), 2);
    }
    assert_eq!(owned_first::OWNED, ["my_lib,other,tag,rules", ""]);
    assert_eq!(owned_last::OWNED, ["", "my_lib,other,tag,rules"]);
    assert_eq!(
        norm(owned_first::META[1]),
        "announce_test_derive::no_tuple,$crate::plain_rules_check"
    );
    // The entry with no owned attribute between two that have.
    assert_eq!(norm(sandwich::REST), passed());
    assert_eq!(sandwich::ENTRIES, 3);
    assert_eq!(sandwich::OWNED, ["my_lib", "", "my_lib,other,tag,rules"]);
}

#[test]
fn rest_is_unchanged_for_large_items_with_owned_attributes() {
    mod big {
        use super::{d, rest_probe};
        rest!(call_def d::BigOwned; [d::BigOwned!, rest_probe!]);
    }
    mod layouts {
        use super::{d, rest_probe};
        rest!(call_def d::BigOwned; [d::BigOwned!, d::SingleOwned!, d::DirectOwned!, d::Nothing!, rest_probe!]);
    }
    mod big_empty {
        use super::{d, rest_probe};
        d::BigOwned! { [d::BigOwned!, rest_probe!] }
    }
    assert_eq!(norm(big::REST), passed());
    assert_eq!(big::ENTRIES, 1);
    assert_eq!(big::OWNED, ["my_lib"]);
    assert_eq!(big_empty::REST, "");
    assert_eq!(big_empty::OWNED, ["my_lib"]);
    // The held-once item, the single and the direct layout, and one with no
    // owned attributes, in one chain.
    assert_eq!(norm(layouts::REST), passed());
    assert_eq!(layouts::ENTRIES, 4);
    assert_eq!(layouts::OWNED, ["my_lib", "my_lib", "my_lib", ""]);
}

#[test]
fn rest_is_unchanged_on_the_derive_route_with_a_check() {
    mod derived {
        use super::{d, rest_probe};
        rest!(call_def d::DerivedOwned; [d::DerivedOwned!, rest_probe!]);
    }
    mod mixed {
        use super::{d, rest_probe};
        rest!(call_def d::DerivedOwned; [d::DerivedOwned!, announce_test_defs::checks::DerivedChecked!, d::Fields!, rest_probe!]);
    }
    mod derived_empty {
        use super::{d, rest_probe};
        d::DerivedOwned! { [d::DerivedOwned!, rest_probe!] }
    }
    // The derive sends the same query: the check's names are in META, and the item
    // still has the attribute (a derive removes nothing).
    assert_eq!(norm(derived::REST), passed());
    assert_eq!(derived::ENTRIES, 1);
    assert_eq!(derived::OWNED, ["rules"]);
    assert_eq!(norm(derived::META[0]), "$crate::rules_check,attrs(rules)");
    assert_eq!(derived_empty::REST, "");
    assert_eq!(derived_empty::OWNED, ["rules"]);
    assert_eq!(d::DerivedOwned { a: 1 }.a, 1);
    // A derive with owned names, one without, and the attribute form, in one chain.
    assert_eq!(norm(mixed::REST), passed());
    assert_eq!(mixed::ENTRIES, 3);
    assert_eq!(mixed::OWNED, ["rules", "", "my_lib"]);
}
