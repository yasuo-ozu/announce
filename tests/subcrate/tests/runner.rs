//! Several announcements of one item, from several crates, make one trampoline, one
//! `Announce` impl and one META with the entries of all of them.
//!
//! The first announcement puts the runner (the hidden attribute `__announce_runner`) last
//! among the attributes of the item, the others are merged into it, and the runner makes
//! the trampoline once every other macro has run. A second trampoline or `Announce` impl
//! would not compile in `defs`; a check that ran twice would define its constant twice.
//!
//! - `macros`: a proc-macro crate with attribute macros that stand between the
//!   announcements (one returns its input, one announces what it produces), two checks
//!   that own attributes, and the callback;
//! - `defs`: the defining crate, with a renamed import of the attribute
//!   (`use announce::announce as ann`);
//! - this file is the calling crate.
//!
//! The macro that drops the runner is a compile-fail case, which a subcrate cannot be:
//! see `tests/ui/tests/ui/dropped_runner.rs`.

use subcrate::subcrate;

#[subcrate(proc_macro, ::announce, ::proc_macro2, ::syn, ::template_quote)]
pub mod macros {
    use proc_macro::TokenStream;
    use template_quote::quote;

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
                    .map(|m| template_quote::ToTokens::to_token_stream(m).to_string())
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
pub mod defs {
    #![allow(dead_code)]

    use announce::announce;
    use announce::announce as ann;
    use macros::{emit_announce, passthrough};

    /// Fails to compile here, unless `T` is announced.
    pub fn is_announced<T: ?Sized + announce::Announce>() {}

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

type Rows = &'static [(&'static str, &'static str, &'static str)];

mod rows_a {
    use crate::{defs as d, macros::summary};
    d::BareTwice! {
        [d::BareTwice!, d::BareTwiceEnum!, d::BareTwiceUnion!, d::RenamedTwice!,
         d::RenamedAfterPass!, d::RenamedAmongPass!, d::RenamedAroundEmit!,
         d::EmitBeforeRenamed!, d::EmitAfterRenamed!, d::Names!, summary!] ALL
    }
}

mod rows_b {
    use crate::{defs as d, macros::summary};
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
    defs::is_announced::<defs::BareTwice>();
    defs::is_announced::<defs::BareTwiceEnum>();
    defs::is_announced::<defs::BareTwiceUnion>();
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
    defs::is_announced::<defs::RenamedTwice>();
    defs::is_announced::<defs::RenamedAfterPass>();
    defs::is_announced::<defs::RenamedAmongPass>();
    defs::is_announced::<defs::RenamedAroundEmit>();
    defs::is_announced::<defs::EmitBeforeRenamed>();
    defs::is_announced::<defs::EmitAfterRenamed>();
    defs::is_announced::<defs::Names>();
    // The derive of someone else ran.
    let _ = format!("{:?}", defs::Names(1).clone());
}

#[test]
fn derives_of_other_traits_do_not_stop_the_announcements_from_merging() {
    let rows = rows_b::ALL;
    assert_eq!(meta(rows, "AttrThenDerive"), "runner::a");
    // The bare attribute expands first, the attribute below it is merged into its runner.
    assert_eq!(meta(rows, "DeriveThenAttr"), "runner::b,runner::a");
    defs::is_announced::<defs::AttrThenDerive>();
    defs::is_announced::<defs::DeriveThenAttr>();
    let _ = format!("{:?}", defs::DeriveThenAttr(1));
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
    let _ = defs::TwoChecks { a: 1, b: 2, t: 3 };
    let _ = (defs::TwoChecksEnum::A, defs::TwoChecksEnum::B(1));
    defs::is_announced::<defs::TwoChecks>();
    defs::is_announced::<defs::TwoChecksEnum>();
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
            defs::TICK_TICKED_LEFT,
            defs::TICK_TICKED_SHARED,
            defs::TICK_TICKED_RIGHT
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
    defs::is_announced::<dyn defs::TraitTwice>();
    defs::is_announced::<dyn defs::TraitEmit>();
    assert_eq!(defs::twice_mod::f(), 1);
    let _: defs::TwiceAlias = [1, 2];
}
