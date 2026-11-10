//! Several announcements of one item make one trampoline, one `Announce` impl and one
//! META with the entries of all of them: the first announcement puts the runner on the item,
//! the others are merged into it, and the runner makes the trampoline. The items are in
//! `announce_tests::defs::runner`, where a second trampoline or impl would not compile.

use announce_tests::defs::runner as d;
use announce_tests::derive::summary;
use subcrate::subcrate;

type Rows = &'static [(&'static str, &'static str, &'static str)];

macro_rules! capture {
    (@announce_format "1" [$($l:tt)*] $($t:tt)*) => {
        pub const OUT: &str = stringify!($($t)*);
    };
}

mod rows_a {
    use super::{d, summary};
    d::BareTwice! {
        [d::BareTwice!, d::BareTwiceEnum!, d::BareTwiceUnion!, d::RenamedTwice!,
         d::RenamedAfterPass!, d::RenamedAmongPass!, d::RenamedAroundEmit!,
         d::EmitBeforeRenamed!, d::EmitAfterRenamed!, d::Names!, summary!] ALL
    }
}

mod rows_b {
    use super::{d, summary};
    d::AttrThenDerive! {
        [d::AttrThenDerive!, d::AttrThenDeriveBare!, d::DeriveThenAttr!, d::DeriveAmong!,
         d::BareThenCheck!, d::SameCheckTwice!, d::TwoChecksAroundDerive!,
         d::OwnedAfterPass!, d::OwnersMixed!, d::TwoChecks!, d::TwoChecksEnum!, d::RecordsCfgAttr!,
         d::RunQueriedOnce!, d::RunQueriedAfterPass!, d::RunQueriedTwice!, d::RunQueriedWithDerive!,
         d::Ticked!, d::TraitTwice!, d::TraitEmit!, d::twice_mod!, d::TwiceAlias!, summary!] ALL
    }
}

/// The META of the trampoline called `key`, without spaces.
fn meta(rows: Rows, key: &str) -> String {
    let mut found = rows.iter().filter(|r| r.0 == key);
    let row = found
        .next()
        .unwrap_or_else(|| panic!("{} is not in {:?}", key, rows));
    assert!(found.next().is_none(), "{} is there twice", key);
    row.1.split_whitespace().collect()
}

// The `Announce` of the `announce` that `defs` links. Under `cargo check` and clippy the
// subcrates and this file link different builds of `announce`, so `announce::Announce`
// would be another trait there.
fn is_announce<T: announce_tests::defs::announce::Announce>() {}

fn norm(s: &str) -> String {
    s.split_whitespace().collect()
}

#[test]
fn the_attribute_with_its_own_name_twice() {
    let rows = rows_a::ALL;
    assert_eq!(rows.len(), 10);
    // The second `#[announce]` is merged into the runner that the first has put on the item.
    assert_eq!(meta(rows, "BareTwice"), "runner::b");
    assert_eq!(meta(rows, "BareTwiceEnum"), "runner::a,runner::b");
    assert_eq!(meta(rows, "BareTwiceUnion"), "runner::a,runner::b");
    is_announce::<d::BareTwice>();
    is_announce::<d::BareTwiceEnum>();
    is_announce::<d::BareTwiceUnion>();
}

#[test]
fn a_renamed_import_is_an_announcement_in_every_order() {
    let rows = rows_a::ALL;
    // The renamed attribute is merged into the runner: the entries of the announcement
    // that expands first come first.
    assert_eq!(meta(rows, "RenamedTwice"), "runner::b");
    assert_eq!(meta(rows, "RenamedAfterPass"), "runner::a,runner::b");
    assert_eq!(meta(rows, "RenamedAmongPass"), "runner::a,runner::b");
    assert_eq!(
        meta(rows, "RenamedAroundEmit"),
        "runner::a,runner::e,runner::b"
    );
    assert_eq!(
        meta(rows, "EmitBeforeRenamed"),
        "runner::e,runner::a,runner::b"
    );
    assert_eq!(
        meta(rows, "EmitAfterRenamed"),
        "runner::a,runner::b,runner::e"
    );
    // Under both names, with a derive of someone else.
    assert_eq!(meta(rows, "Names"), "runner::a,runner::b,runner::c");
    is_announce::<d::RenamedTwice>();
    is_announce::<d::RenamedAfterPass>();
    is_announce::<d::RenamedAmongPass>();
    is_announce::<d::RenamedAroundEmit>();
    is_announce::<d::EmitBeforeRenamed>();
    is_announce::<d::EmitAfterRenamed>();
    is_announce::<d::Names>();
    // The derive of someone else ran.
    let _ = format!("{:?}", d::Names(1).clone());
}

#[test]
fn derives_of_other_traits_and_the_bare_attribute() {
    let rows = rows_b::ALL;
    assert_eq!(meta(rows, "AttrThenDerive"), "runner::a");
    assert_eq!(meta(rows, "AttrThenDeriveBare"), "runner::a,runner::b");
    // The bare attribute expands first, the attribute below it is merged into its runner.
    assert_eq!(meta(rows, "DeriveThenAttr"), "runner::b,runner::a");
    assert_eq!(
        meta(rows, "DeriveAmong"),
        "runner::a,runner::b1,runner::b2,runner::c"
    );
    is_announce::<d::AttrThenDerive>();
    is_announce::<d::AttrThenDeriveBare>();
    is_announce::<d::DeriveThenAttr>();
    is_announce::<d::DeriveAmong>();
    // The derives ran.
    let _ = format!("{:?}", d::DeriveThenAttr(1));
}

#[test]
fn a_bare_attribute_and_an_attribute_with_a_check() {
    // `#[derive(Debug)] #[announce(meta = ..)] #[::announce::announce(lib_check)]
    // #[my_lib(x)]`: the bare attribute expands first and puts the runner below the
    // attribute with the check, which owns `my_lib`.
    let rows = rows_b::ALL;
    let m = meta(rows, "BareThenCheck");
    assert!(
        m.contains("lib_check") && m.contains("runner::b") && m.contains("attrs(my_lib)"),
        "{}",
        m
    );
    is_announce::<d::BareThenCheck>();
    let _ = d::BareThenCheck(0);
    // The same check twice: asked once for each announcement, listed once.
    let m = meta(rows, "SameCheckTwice");
    assert_eq!(m.matches("lib_check").count(), 1, "{}", m);
    assert!(m.contains("attrs(my_lib)"), "{}", m);
    is_announce::<d::SameCheckTwice>();
    // Two checks of two announcements; each runs once.
    let m = meta(rows, "TwoChecksAroundDerive");
    for name in [
        "lib_check",
        "other_check",
        "tick(first)",
        "tick(second)",
        "my_lib",
        "other",
        "tag",
    ] {
        assert!(m.contains(name), "{}: {}", name, m);
    }
    assert_eq!(d::TICK_TWOCHECKSAROUNDDERIVE_FIRST, 0);
    assert_eq!(d::TICK_TWOCHECKSAROUNDDERIVE_SECOND, 0);
    is_announce::<d::TwoChecksAroundDerive>();
}

mod owned_after_pass {
    use super::d;
    d::OwnedAfterPass! { [d::OwnedAfterPass!, d::OwnersMixed!, capture!] REST }
}

mod two_checks {
    use super::d;
    d::TwoChecks! { [d::TwoChecks!, capture!] REST }
    pub mod e {
        use super::d;
        d::TwoChecksEnum! { [d::TwoChecksEnum!, capture!] REST }
    }
    pub mod r {
        use super::d;
        d::RecordsCfgAttr! { [d::RecordsCfgAttr!, capture!] REST }
    }
}

#[test]
fn two_announcements_with_checks_that_own_attributes() {
    let rows = rows_b::ALL;
    // The names of both, in one `attrs(..)`, and both checks.
    for key in ["TwoChecks", "TwoChecksEnum"] {
        let m = meta(rows, key);
        assert_eq!(m.matches("attrs(").count(), 1, "{}", m);
        for name in ["my_lib", "other", "tag", "lib_check", "other_check"] {
            assert!(m.contains(name), "{}: {}", name, m);
        }
    }
    // The items compile without the attributes.
    let _ = d::TwoChecks { a: 1, b: 2, t: 3 };
    let _ = d::TwoChecksEnum::B(1);
    is_announce::<d::TwoChecks>();
    is_announce::<d::TwoChecksEnum>();
    // The payload keeps them, where they were.
    let out = norm(two_checks::OUT);
    assert!(
        out.contains("#[my_lib(on_item)]") && out.contains("#[other(on_item)]"),
        "{}",
        out
    );
    assert!(
        out.contains("#[my_lib(a)]puba:u8,#[other(b)]pubb:u8,#[tag]pubt:u8"),
        "{}",
        out
    );
    assert!(out.ends_with("REST"), "{}", out);
    let out = norm(two_checks::e::OUT);
    assert!(
        out.contains("#[my_lib(variant)]A,B(#[other(field)]u8)"),
        "{}",
        out
    );
    // The docs and the derives are not in it.
    assert!(
        !out.contains("#[derive") && !out.contains("#[doc"),
        "{}",
        out
    );
}

#[test]
fn cfg_attr_and_cfg_are_evaluated_for_the_owned_attributes() {
    // `record_attrs` saw the attributes with `cfg_attr` evaluated in the defining crate,
    // in source order, the outer one first (the definer is not built with `cfg(test)`).
    assert_eq!(
        d::SEEN_RECORDSCFGATTR,
        [
            "my_lib(outer_cfg_attr)",
            "my_lib(first)",
            "my_lib(from_cfg_attr)"
        ]
    );
    let out = norm(two_checks::r::OUT);
    assert!(out.contains("#[my_lib(outer_cfg_attr)]"), "{}", out);
    assert!(out.contains("#[my_lib(first)]puba:u8"), "{}", out);
    assert!(out.contains("#[my_lib(from_cfg_attr)]pubb:u8"), "{}", out);
    assert!(
        !out.contains("not_here") && !out.contains("#[cfg"),
        "{}",
        out
    );
    let _ = d::RecordsCfgAttr { a: 1, b: 2, c: 3 };
    is_announce::<d::RecordsCfgAttr>();
}

/// The numbers `queried_<n>` that META lists for `key`.
fn queried(key: &str) -> Vec<String> {
    let m = meta(rows_b::ALL, key);
    let mut out: Vec<String> = m
        .split(|c: char| !(c.is_alphanumeric() || c == '_'))
        .filter(|w| w.starts_with("queried_"))
        .map(|w| w.to_string())
        .collect();
    out.sort();
    out
}

#[test]
fn the_query_is_sent_once_for_each_announcement() {
    // `count_query` owns `queried_<n>` for the n-th query it answers for an item, so a
    // query that was sent twice would list `queried_1`.
    assert_eq!(queried("RunQueriedOnce"), ["queried_0"]);
    assert_eq!(queried("RunQueriedAfterPass"), ["queried_0"]);
    assert_eq!(queried("RunQueriedWithDerive"), ["queried_0"]);
    // Two announcements, two queries: the names of both are there, and no third.
    assert_eq!(queried("RunQueriedTwice"), ["queried_0", "queried_1"]);
    // The query of the one that records the attributes is also one query.
    let m = meta(rows_b::ALL, "RecordsCfgAttr");
    assert_eq!(m.matches("count_query").count(), 1, "{}", m);
    assert_eq!(queried("RecordsCfgAttr"), ["queried_0"]);
    // Each check runs once, whatever the number of announcements that list it.
    assert_eq!(d::TICK_TICKED_LEFT, 0);
    assert_eq!(d::TICK_TICKED_SHARED, 0);
    assert_eq!(d::TICK_TICKED_RIGHT, 0);
    let m = meta(rows_b::ALL, "Ticked");
    assert_eq!(m.matches("tick(shared)").count(), 1, "{}", m);
    is_announce::<d::Ticked>();
}

#[test]
fn other_items_are_announced_in_the_same_way() {
    let rows = rows_b::ALL;
    assert_eq!(meta(rows, "TraitTwice"), "runner::a,runner::b");
    assert_eq!(meta(rows, "TraitEmit"), "runner::a,runner::e");
    assert_eq!(meta(rows, "twice_mod"), "runner::a,runner::b");
    assert_eq!(meta(rows, "TwiceAlias"), "runner::a,runner::b");
    assert_eq!(d::twice_mod::f(), 1);
    let _: d::TwiceAlias = [1, 2];
}

#[test]
fn the_trampoline_has_the_arm_of_the_sentinel() {
    // The sentinel calls the trampoline with `@__announce_exists`, which expands to nothing,
    // whatever the layout and the payload: a small item, a large one (held once), one with
    // `$` in it, a module, and a trampoline that is renamed.
    d::BareTwice! { @__announce_exists }
    announce_tests::defs::large::BigTrait! { @__announce_exists }
    announce_tests::defs::large::big_mod! { @__announce_exists }
    announce_tests::defs::kinds::with_macro! { @__announce_exists }
    announce_tests::defs::conflict::ParseAnnounce! { @__announce_exists }
}

#[test]
#[allow(dead_code)]
fn the_sentinel_works_in_the_body_of_a_function() {
    use announce_tests::derive::passthrough;
    // The sentinel is a sibling of the item, also in a block: a struct, a generic struct
    // (the call of its trampoline), a trait and a type alias.
    #[::announce::announce]
    #[passthrough]
    struct Plain(u8);
    #[::announce::announce]
    #[passthrough]
    struct Generic<T>(T);
    #[::announce::announce]
    #[passthrough]
    trait Tr {
        fn f(&self) -> u8;
    }
    #[::announce::announce(macro_name = AliasAnnounce)]
    #[passthrough]
    type Alias = [u8; 2];
    #[::announce::announce(emit_item = false, macro_name = GoneAnnounce)]
    #[passthrough]
    struct Gone(u8);
    #[cfg(any())]
    #[::announce::announce]
    #[passthrough]
    struct CfgdOut(u8);
    #[::announce::announce]
    #[cfg(any())]
    #[passthrough]
    struct CfgdOutBelow(u8);
    macro_rules! count {
        (@announce_format "1" [$($l:tt)*] $($t:tt)*) => {
            const N: &str = stringify!($($t)*);
        };
    }
    Plain! { [Plain!, count!] }
    assert!(norm(N).contains("structPlain"), "{}", N);
    let _ = (Plain(0), Generic(0u8), Alias::default());
    fn is<T: ?Sized + announce::Announce>() {}
    is::<Plain>();
}

#[test]
fn owned_attributes_below_other_macros() {
    // The items compile: the attributes are the checks', and the runner removed them.
    let _ = (d::OwnedAfterPass(0), d::OwnersMixed(0));
    is_announce::<d::OwnedAfterPass>();
    is_announce::<d::OwnersMixed>();
    let rows = rows_b::ALL;
    assert!(meta(rows, "OwnedAfterPass").contains("attrs(my_lib)"));
    let m = meta(rows, "OwnersMixed");
    assert_eq!(m.matches("attrs(").count(), 1, "{}", m);
    for name in ["my_lib", "other", "tag", "lib_check", "other_check"] {
        assert!(m.contains(name), "{}: {}", name, m);
    }
    // The payloads keep the attributes, where they were.
    let out = norm(owned_after_pass::OUT);
    assert!(
        out.contains("#[my_lib(x)]pubstructOwnedAfterPass(pubu8);"),
        "{}",
        out
    );
}

// ---- A crate of macros and a definer of this file ----

// Several announcements of one item, from several crates, make one trampoline, one
// `Announce` impl and one META with the entries of all of them.
//
// The first announcement puts the runner (the hidden attribute `__announce_runner`) last
// among the attributes of the item, the others are merged into it, and the runner makes
// the trampoline once every other macro has run. A second trampoline or `Announce` impl
// would not compile in `definer`; a check that ran twice would define its constant twice.
//
// - `macros`: a proc-macro crate with attribute macros that stand between the
//   announcements (one returns its input, one announces what it produces), two checks
//   that own attributes, and the callback;
// - `definer`: the defining crate, with a renamed import of the attribute
//   (`use announce::announce as ann`);
// - this file is the calling crate.
//
// The macro that drops the runner is a UI test: `dropped_runner` in `tests/ui_runner.rs`.

#[subcrate(proc_macro, ::announce, ::syn, ::proc_macro2, ::template_quote)]
pub mod macros {
    use proc_macro::TokenStream;
    use template_quote::{quote, ToTokens};

    /// An attribute macro that returns its input as it is. `#[announce]` cannot know that,
    /// so the runner waits for it.
    #[proc_macro_attribute]
    pub fn passthrough(_: TokenStream, item: TokenStream) -> TokenStream {
        item
    }

    /// `#[emit_announce(OPTIONS)]`: a macro that announces what it produces. It emits the
    /// item with `#[::announce::announce(OPTIONS)]` as its outermost attribute.
    #[proc_macro_attribute]
    pub fn emit_announce(attr: TokenStream, item: TokenStream) -> TokenStream {
        let (attr, item): (proc_macro2::TokenStream, proc_macro2::TokenStream) =
            (attr.into(), item.into());
        TokenStream::from(quote! { #[::announce::announce(#attr)] #item })
    }

    /// Owns the attribute `my_lib`.
    #[proc_macro]
    pub fn lib_check(input: TokenStream) -> TokenStream {
        announce::helper::check(input.into(), &["my_lib"], |_| {
            Ok(proc_macro2::TokenStream::new())
        })
        .into()
    }

    /// Owns the attributes `other` and `tag`.
    #[proc_macro]
    pub fn other_check(input: TokenStream) -> TokenStream {
        announce::helper::check(input.into(), &["other", "tag"], |_| {
            Ok(proc_macro2::TokenStream::new())
        })
        .into()
    }

    /// Leaves a mark where it runs: `tick! { (NAME), ITEM }` emits
    /// `pub const TICK_<ITEM>_<NAME>: u8 = 0;`. It owns no attributes.
    #[proc_macro]
    pub fn tick(input: TokenStream) -> TokenStream {
        announce::helper::check(input.into(), &[], |ci| {
            let name = match &ci.item {
                syn::Item::Struct(s) => s.ident.to_string(),
                other => return Err(syn::Error::new_spanned(other, "tick: a struct only")),
            };
            let arg: String = ci.args.to_string().split_whitespace().collect();
            let id = syn::Ident::new(
                &format!("TICK_{}_{}", name.to_uppercase(), arg.to_uppercase()),
                proc_macro2::Span::call_site(),
            );
            Ok(quote! { pub const #id: u8 = 0; })
        })
        .into()
    }

    /// `summary! { [summary!] ENTRY* NAME }`: `pub const NAME: &[(&str, &str, &str)]` with
    /// a row `(key, meta, item kind and ident)` for each entry, in visiting order.
    #[proc_macro]
    pub fn summary(input: TokenStream) -> TokenStream {
        announce::helper::callback(input.into(), |input| {
            if let Some(fwd) = input.forward() {
                return Ok(fwd);
            }
            let name: syn::Ident = input.rest_as()?;
            let rows = input.entries_in_order().map(|e| {
                let key = e.key.path().segments.last().unwrap().ident.to_string();
                let meta = e
                    .meta
                    .iter()
                    .map(|m| m.to_token_stream().to_string())
                    .collect::<Vec<_>>()
                    .join(", ");
                let item = match e.item() {
                    Ok(syn::Item::Struct(s)) => format!("struct {}", s.ident),
                    Ok(syn::Item::Enum(s)) => format!("enum {}", s.ident),
                    Ok(syn::Item::Union(s)) => format!("union {}", s.ident),
                    Ok(syn::Item::Trait(s)) => format!("trait {}", s.ident),
                    Ok(syn::Item::Type(s)) => format!("type {}", s.ident),
                    Ok(syn::Item::Mod(s)) => format!("mod {}", s.ident),
                    Ok(_) => "other".to_string(),
                    Err(e) => format!("error: {}", e),
                };
                quote! { (#key, #meta, #item) }
            });
            Ok(quote! { pub const #name: &[(&str, &str, &str)] = &[ #(#rows),* ]; })
        })
        .into()
    }
}

#[subcrate(::announce, macros)]
pub mod definer {
    #![allow(dead_code)]

    use announce::announce;
    use announce::announce as ann;
    use macros::{emit_announce, passthrough};

    // ---- the attribute, and a renamed import of it ----

    #[announce]
    #[announce(meta = runner::b)]
    pub struct BareTwice(pub u8);

    #[announce(meta = runner::a)]
    #[announce(meta = runner::b)]
    pub enum BareTwiceEnum {
        A,
        B(u8),
    }

    #[announce(meta = runner::a)]
    #[announce(meta = runner::b)]
    pub union BareTwiceUnion {
        pub a: u8,
        pub b: i8,
    }

    #[ann]
    #[ann(meta = runner::b)]
    pub struct RenamedTwice(pub u8);

    /// Macros between the announcements, of the two kinds: one that returns its input, and
    /// one that announces what it produces.
    #[ann(meta = runner::a)]
    #[passthrough]
    #[ann(meta = runner::b)]
    pub struct RenamedAfterPass(pub u8);

    #[passthrough]
    #[ann(meta = runner::a)]
    #[passthrough]
    #[ann(meta = runner::b)]
    #[passthrough]
    pub struct RenamedAmongPass(pub u8);

    #[ann(meta = runner::a)]
    #[emit_announce(meta = runner::e)]
    #[ann(meta = runner::b)]
    pub struct RenamedAroundEmit(pub u8);

    #[emit_announce(meta = runner::e)]
    #[ann(meta = runner::a)]
    #[passthrough]
    #[ann(meta = runner::b)]
    pub struct EmitBeforeRenamed(pub u8);

    #[ann(meta = runner::a)]
    #[passthrough]
    #[ann(meta = runner::b)]
    #[emit_announce(meta = runner::e)]
    pub struct EmitAfterRenamed(pub u8);

    /// Both names of the attribute in one item, the full path, and a derive of someone
    /// else.
    #[ann(meta = runner::a)]
    #[derive(Clone, Debug)]
    #[announce(meta = runner::b)]
    #[::announce::announce(meta = runner::c)]
    pub struct Names(pub u8);

    // ---- derives of other traits ----

    /// The attribute above the derive: it puts the runner below the derive.
    #[ann(meta = runner::a)]
    #[derive(Debug)]
    pub struct AttrThenDerive(pub u8);

    /// The derive above the attribute: the bare attribute expands first, and the attribute
    /// below it is merged into its runner.
    #[derive(Debug)]
    #[announce(meta = runner::b)]
    #[::announce::announce(meta = runner::a)]
    pub struct DeriveThenAttr(pub u8);

    // ---- checks that own attributes ----

    /// Two announcements with checks of their own, and attributes of both on the item, in
    /// the fields and between the announcements.
    #[ann(macros::lib_check, meta = runner::a)]
    #[my_lib(on_item)]
    #[ann(macros::other_check, meta = runner::b)]
    #[other(on_item)]
    pub struct TwoChecks {
        #[my_lib(a)]
        pub a: u8,
        #[other(b)]
        pub b: u8,
        #[tag]
        pub t: u8,
    }

    /// The names are in the runner whatever the order of the announcements; the item is a
    /// different kind.
    #[ann(macros::other_check, meta = runner::b)]
    #[passthrough]
    #[ann(macros::lib_check, meta = runner::a)]
    #[my_lib(on_item)]
    #[other(on_item)]
    pub enum TwoChecksEnum {
        #[my_lib(variant)]
        A,
        B(#[other(field)] u8),
    }

    /// Each check runs once (`tick` leaves a constant: a second run would define it twice).
    #[ann(macros::tick(left), macros::tick(shared), meta = runner::a)]
    #[ann(macros::tick(shared), macros::tick(right), meta = runner::b)]
    pub struct Ticked(pub u8);

    // ---- the other items ----

    #[ann(meta = runner::a)]
    #[passthrough]
    #[ann(meta = runner::b)]
    pub trait TraitTwice {
        fn f(&self) -> u8;
    }

    #[ann(meta = runner::a)]
    #[emit_announce(meta = runner::e)]
    pub trait TraitEmit {
        fn f(&self) -> u8;
    }

    #[ann(meta = runner::a)]
    #[passthrough]
    #[ann(meta = runner::b)]
    pub mod twice_mod {
        pub fn f() -> u8 {
            1
        }
    }

    #[ann(meta = runner::a)]
    #[passthrough]
    #[ann(meta = runner::b)]
    pub type TwiceAlias = [u8; 2];
}

mod crates {
    use super::*;

    /// Fails to compile unless `T` is announced. The trait is that of the `announce` that
    /// `definer` links: under `cargo check` and clippy, another build than this file links.
    fn is_announced<T: ?Sized + definer::announce::Announce>() {}

    type Rows = &'static [(&'static str, &'static str, &'static str)];

    mod rows_a {
        use crate::{definer as d, macros::summary};
        d::BareTwice! {
            [d::BareTwice!, d::BareTwiceEnum!, d::BareTwiceUnion!, d::RenamedTwice!,
             d::RenamedAfterPass!, d::RenamedAmongPass!, d::RenamedAroundEmit!,
             d::EmitBeforeRenamed!, d::EmitAfterRenamed!, d::Names!, summary!] ALL
        }
    }

    mod rows_b {
        use crate::{definer as d, macros::summary};
        d::AttrThenDerive! {
            [d::AttrThenDerive!, d::DeriveThenAttr!, d::TwoChecks!, d::TwoChecksEnum!, d::Ticked!,
             d::TraitTwice!, d::TraitEmit!, d::twice_mod!, d::TwiceAlias!, summary!] ALL
        }
    }

    /// The META of the trampoline called `key`, without spaces.
    fn meta(rows: Rows, key: &str) -> String {
        let mut found = rows.iter().filter(|r| r.0 == key);
        let row = found
            .next()
            .unwrap_or_else(|| panic!("{} is not in {:?}", key, rows));
        assert!(found.next().is_none(), "{} is there twice", key);
        row.1.split_whitespace().collect()
    }

    #[test]
    fn the_attribute_with_its_own_name_twice_is_one_announcement() {
        let rows = rows_a::ALL;
        assert_eq!(rows.len(), 10);
        // The second `#[announce]` is merged into the runner that the first has put on the item.
        assert_eq!(meta(rows, "BareTwice"), "runner::b");
        assert_eq!(meta(rows, "BareTwiceEnum"), "runner::a,runner::b");
        assert_eq!(meta(rows, "BareTwiceUnion"), "runner::a,runner::b");
        is_announced::<definer::BareTwice>();
        is_announced::<definer::BareTwiceEnum>();
        is_announced::<definer::BareTwiceUnion>();
    }

    #[test]
    fn a_renamed_import_is_an_announcement_in_every_order() {
        let rows = rows_a::ALL;
        // The renamed attribute is merged into the runner: the entries of the announcement
        // that expanded first come first.
        assert_eq!(meta(rows, "RenamedTwice"), "runner::b");
        assert_eq!(meta(rows, "RenamedAfterPass"), "runner::a,runner::b");
        assert_eq!(meta(rows, "RenamedAmongPass"), "runner::a,runner::b");
        assert_eq!(
            meta(rows, "RenamedAroundEmit"),
            "runner::a,runner::e,runner::b"
        );
        assert_eq!(
            meta(rows, "EmitBeforeRenamed"),
            "runner::e,runner::a,runner::b"
        );
        assert_eq!(
            meta(rows, "EmitAfterRenamed"),
            "runner::a,runner::b,runner::e"
        );
        // Under both names and the full path, with a derive of someone else.
        assert_eq!(meta(rows, "Names"), "runner::a,runner::b,runner::c");
        is_announced::<definer::RenamedTwice>();
        is_announced::<definer::RenamedAfterPass>();
        is_announced::<definer::RenamedAmongPass>();
        is_announced::<definer::RenamedAroundEmit>();
        is_announced::<definer::EmitBeforeRenamed>();
        is_announced::<definer::EmitAfterRenamed>();
        is_announced::<definer::Names>();
        // The derive of someone else ran.
        let _ = format!("{:?}", definer::Names(1).clone());
    }

    #[test]
    fn derives_of_other_traits_do_not_stop_the_announcements_from_merging() {
        let rows = rows_b::ALL;
        assert_eq!(meta(rows, "AttrThenDerive"), "runner::a");
        // The bare attribute expands first, the attribute below it is merged into its runner.
        assert_eq!(meta(rows, "DeriveThenAttr"), "runner::b,runner::a");
        is_announced::<definer::AttrThenDerive>();
        is_announced::<definer::DeriveThenAttr>();
        let _ = format!("{:?}", definer::DeriveThenAttr(1));
    }

    #[test]
    fn checks_of_several_announcements_are_united_and_asked_once() {
        let rows = rows_b::ALL;
        // The checks of both announcements, the names that each owns, and the META of both.
        for key in ["TwoChecks", "TwoChecksEnum"] {
            let m = meta(rows, key);
            assert!(
                m.contains("macros::lib_check") && m.contains("macros::other_check"),
                "{}: {}",
                key,
                m
            );
            assert!(m.contains("runner::a") && m.contains("runner::b"), "{}", m);
            for name in ["my_lib", "other", "tag"] {
                assert!(m.contains(name), "{}: {}", key, m);
            }
        }
        // The items compile without the attributes that the checks own.
        let _ = definer::TwoChecks { a: 1, b: 2, t: 3 };
        let _ = (definer::TwoChecksEnum::A, definer::TwoChecksEnum::B(1));
        is_announced::<definer::TwoChecks>();
        is_announced::<definer::TwoChecksEnum>();
        // `shared` is a check of both announcements: it is listed once, and ran once.
        let m = meta(rows, "Ticked");
        assert_eq!(m.matches("macros::tick(shared)").count(), 1, "{}", m);
        assert!(
            m.contains("macros::tick(left)") && m.contains("macros::tick(right)"),
            "{}",
            m
        );
        assert_eq!(
            (
                definer::TICK_TICKED_LEFT,
                definer::TICK_TICKED_SHARED,
                definer::TICK_TICKED_RIGHT
            ),
            (0, 0, 0)
        );
    }

    #[test]
    fn the_other_kinds_of_item_merge_as_well() {
        let rows = rows_b::ALL;
        assert_eq!(meta(rows, "TraitTwice"), "runner::a,runner::b");
        assert_eq!(meta(rows, "TraitEmit"), "runner::a,runner::e");
        assert_eq!(meta(rows, "twice_mod"), "runner::a,runner::b");
        assert_eq!(meta(rows, "TwiceAlias"), "runner::a,runner::b");
        is_announced::<dyn definer::TraitTwice>();
        is_announced::<dyn definer::TraitEmit>();
        assert_eq!(definer::twice_mod::f(), 1);
        let _: definer::TwiceAlias = [1, 2];
    }
}
