//! Definition-side checks written with `announce::helper::check`, in a crate of their own.
//!
//! - `macros`: a proc-macro crate with the checks and a callback;
//! - `defs`: the defining crate; `#[announce(macros::check)]` runs the checks when `defs` is
//!   compiled, so what they emit is part of `defs`;
//! - this file is the calling crate, which gets the item, the check and the attribute
//!   names the check owns in the META of the trampoline.
//!
//! A check that owns attributes is asked first (the attribute query), and the
//! attributes it owns are removed from the item that `defs` emits and kept in the payload.

use subcrate::subcrate;

#[subcrate(proc_macro, ::announce, ::proc_macro2, ::syn, ::template_quote)]
pub mod macros {
    use announce::helper::{CheckCall, Cont, OwnedAttrs};
    use proc_macro::TokenStream;
    use template_quote::quote;

    /// Definition-side check that owns no attribute: rejects tuple structs.
    /// `no_tuple! { (ARGS), ITEM }`. It answers the attribute query all the same.
    #[proc_macro]
    pub fn no_tuple(input: TokenStream) -> TokenStream {
        announce::helper::check(input.into(), &[], |ci| match &ci.item {
            syn::Item::Struct(s) if matches!(s.fields, syn::Fields::Unnamed(_)) => {
                let msg = format!("this macro does not support tuple structs ({})", ci.args);
                Err(syn::Error::new_spanned(&s.fields, msg))
            }
            _ => Ok(proc_macro2::TokenStream::new()),
        })
        .into()
    }

    /// Owns `my_lib` and records what it saw: `pub const SEEN_<ITEM>: &[&str]` in the
    /// defining crate, one string for each owned attribute (without spaces), in source
    /// order.
    #[proc_macro]
    pub fn record_attrs(input: TokenStream) -> TokenStream {
        announce::helper::check(input.into(), &["my_lib"], |ci| {
            let name = match &ci.item {
                syn::Item::Struct(s) => s.ident.to_string(),
                syn::Item::Enum(s) => s.ident.to_string(),
                syn::Item::Trait(s) => s.ident.to_string(),
                syn::Item::Mod(s) => s.ident.to_string(),
                other => {
                    return Err(syn::Error::new_spanned(
                        other,
                        "record_attrs: unsupported item",
                    ))
                }
            };
            let seen: Vec<String> = ci
                .owned_attributes()
                .iter()
                .map(|a| {
                    template_quote::ToTokens::to_token_stream(&a.meta)
                        .to_string()
                        .split_whitespace()
                        .collect()
                })
                .collect();
            let id = syn::Ident::new(
                &format!("SEEN_{}", name.to_uppercase()),
                proc_macro2::Span::call_site(),
            );
            Ok(quote! { pub const #id: &[&str] = &[ #(#seen),* ]; })
        })
        .into()
    }

    /// Owns `other` and `tag`, and leaves a mark where it runs: `pub const TICK_<ITEM>: u8`,
    /// so a check that ran twice for one item would define the constant twice.
    #[proc_macro]
    pub fn tick(input: TokenStream) -> TokenStream {
        announce::helper::check(input.into(), &["other", "tag"], |ci| {
            let name = match &ci.item {
                syn::Item::Struct(s) => s.ident.to_string(),
                other => return Err(syn::Error::new_spanned(other, "tick: a struct only")),
            };
            let id = syn::Ident::new(
                &format!("TICK_{}", name.to_uppercase()),
                proc_macro2::Span::call_site(),
            );
            Ok(quote! { pub const #id: u8 = 0; })
        })
        .into()
    }

    /// A check that reads the attribute query by hand and asserts that the list is
    /// headed by this very macro, whatever path it is written with. It owns `probed`, and
    /// answers with `Query::answer`, which drops the head and calls the next element.
    #[proc_macro]
    pub fn head_probe(input: TokenStream) -> TokenStream {
        match syn::parse::<CheckCall>(input) {
            Ok(CheckCall::Query(q)) => {
                let is_me = matches!(
                    q.list.0.first(),
                    Some(Cont::Bang(p)) if p.segments.last().map_or(false, |s| s.ident == "head_probe")
                );
                if !is_me {
                    let msg = "head_probe: the list does not start with `head_probe!`";
                    return syn::Error::new(proc_macro2::Span::call_site(), msg)
                        .to_compile_error()
                        .into();
                }
                q.answer(&OwnedAttrs::from_strs(&["probed"])).into()
            }
            Ok(CheckCall::Run(_)) => TokenStream::new(),
            Err(e) => e.to_compile_error().into(),
        }
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
                    Ok(syn::Item::Trait(s)) => format!("trait {}", s.ident),
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

    /// `owned_report! { [owned_report!] ENTRY* NAME }`: for each entry in visiting order a
    /// string `"names|in item|in stripped"`: the owned attribute names of the entry
    /// (`Entry::owned_attrs`), and the number of owned attributes in `Entry::item` and in
    /// `Entry::item_without_owned_attrs`.
    #[proc_macro]
    pub fn owned_report(input: TokenStream) -> TokenStream {
        announce::helper::callback(input.into(), |input| {
            let name: syn::Ident = input.rest_as()?;
            let mut rows = Vec::new();
            for e in input.entries_in_order() {
                let owned = e.owned_attrs();
                let names: Vec<String> = owned.names().iter().map(|n| n.to_string()).collect();
                let item = e.item()?;
                let stripped = e.item_without_owned_attrs()?;
                rows.push(format!(
                    "{}|{}|{}",
                    names.join(","),
                    owned.find(&item).len(),
                    owned.find(&stripped).len()
                ));
            }
            Ok(quote! { pub const #name: &[&str] = &[ #(#rows),* ]; })
        })
        .into()
    }
}

#[subcrate(::announce, macros)]
pub mod defs {
    use announce::announce;

    /// Attributes in every kind of position of a struct. `cfg_attr` is evaluated before
    /// the check looks, and the definer is not built with `cfg(test)`.
    #[announce(macros::record_attrs)]
    #[derive(Clone, Debug, PartialEq)]
    pub struct Fields {
        #[my_lib(skip)]
        pub a: u8,
        pub b: u8,
        #[my_lib::tag = "x"]
        #[doc = "a documented field"]
        pub c: u8,
        #[cfg_attr(not(test), my_lib(from_cfg_attr))]
        pub d: u8,
        #[cfg_attr(test, my_lib(from_cfg_attr_test))]
        pub e: u8,
    }

    /// On the item, its variants and their fields.
    #[announce(macros::record_attrs)]
    #[my_lib(on_enum)]
    #[derive(Debug, PartialEq)]
    pub enum Choice {
        #[my_lib(first)]
        A,
        #[my_lib(skip)]
        B(#[my_lib(inner)] u8),
        C {
            #[my_lib(named)]
            f: u8,
        },
    }

    /// On a trait, its generics, its items and a parameter.
    #[announce(macros::record_attrs)]
    #[my_lib(on_trait)]
    pub trait Tr<#[my_lib(generic)] T> {
        #[my_lib(assoc_type)]
        type A;
        #[my_lib(method)]
        fn f(&self, #[my_lib(param)] _x: T) -> u8;
    }

    /// Checks that own different attributes, among checks that own none: the answers are
    /// merged, and each check runs once.
    #[announce(
        macros::no_tuple,
        macros::record_attrs,
        macros::tick,
        macros::head_probe
    )]
    pub struct Several {
        #[my_lib(m)]
        pub m: u8,
        #[other(o)]
        pub o: u8,
        #[tag]
        pub t: u8,
        #[probed(p)]
        pub p: u8,
    }

    /// A check with arguments, which is listed in the META as written.
    #[announce(macros::no_tuple(named only))]
    pub struct Checked {
        pub a: u8,
    }
}

macro_rules! capture {
    ($(@announce_format "1")? [$($l:tt)*] $($t:tt)*) => {
        pub const OUT: &str = stringify!($($t)*);
    };
}

fn norm(s: &str) -> String {
    s.split_whitespace().collect()
}

struct Impl;
impl defs::Tr<u8> for Impl {
    type A = u8;
    fn f(&self, x: u8) -> u8 {
        x + 2
    }
}

#[test]
fn the_items_are_emitted_without_the_attributes_the_checks_own() {
    // `#[my_lib(..)]` is no attribute of rustc: it compiles because `defs` removes it
    // from the item it emits.
    let f = defs::Fields {
        a: 1,
        b: 2,
        c: 3,
        d: 4,
        e: 5,
    };
    assert_eq!(f.clone(), f);
    assert_eq!(defs::Choice::B(1), defs::Choice::B(1));
    assert_eq!(defs::Choice::C { f: 1 }, defs::Choice::C { f: 1 });
    assert_eq!(defs::Tr::f(&Impl, 5), 7);
    let _ = defs::Several {
        m: 1,
        o: 2,
        t: 3,
        p: 4,
    };
    assert_eq!(defs::Checked { a: 1 }.a, 1);
}

#[test]
fn the_check_runs_in_the_defining_crate_and_sees_the_attributes_in_source_order() {
    // The check saw the item with `cfg_attr` evaluated by `defs`, which is not built
    // with `cfg(test)`, and emitted the constants into `defs`.
    assert_eq!(
        defs::SEEN_FIELDS,
        ["my_lib(skip)", "my_lib::tag=\"x\"", "my_lib(from_cfg_attr)"]
    );
    assert_eq!(
        defs::SEEN_CHOICE,
        [
            "my_lib(on_enum)",
            "my_lib(first)",
            "my_lib(skip)",
            "my_lib(inner)",
            "my_lib(named)"
        ]
    );
    assert_eq!(
        defs::SEEN_TR,
        [
            "my_lib(on_trait)",
            "my_lib(generic)",
            "my_lib(assoc_type)",
            "my_lib(method)",
            "my_lib(param)"
        ]
    );
    // Several checks: each ran once (a second `tick` would define its constant twice),
    // and each saw only its own attributes.
    assert_eq!(defs::SEEN_SEVERAL, ["my_lib(m)"]);
    assert_eq!(defs::TICK_SEVERAL, 0);
}

mod fields {
    use crate::defs;
    defs::Fields! { [defs::Fields!, capture!] REST }
}

#[test]
fn the_payload_keeps_the_attributes_and_meta_lists_the_check_and_its_names() {
    let out = norm(fields::OUT);
    assert!(
        out.starts_with("defs::Fields!=>[macros::record_attrs,attrs(my_lib)]{pubstructFields{"),
        "{}",
        out
    );
    // The docs are not in the payload (`docs = true` keeps them), nor `#[derive]` that
    // has run.
    assert!(
        !out.contains("#[doc=") && !out.contains("documentedfield"),
        "{}",
        out
    );
    assert!(!out.contains("#[derive"), "{}", out);
    assert!(out.contains("#[my_lib(skip)]puba:u8"), "{}", out);
    assert!(out.contains("#[my_lib::tag=\"x\"]"), "{}", out);
    // `cfg_attr` is evaluated and gone.
    assert!(out.contains("#[my_lib(from_cfg_attr)]pubd:u8"), "{}", out);
    assert!(!out.contains("from_cfg_attr_test"), "{}", out);
    assert!(!out.contains("#[cfg_attr"), "{}", out);
    assert!(out.contains(",pube:u8,"), "{}", out);
    assert!(out.ends_with("REST"), "{}", out);
}

mod summaries {
    use crate::{defs, macros::summary};
    defs::Fields! { [defs::Fields!, defs::Several!, defs::Checked!, summary!] ALL }
}

#[test]
fn the_meta_of_each_entry_has_its_checks_and_the_owned_names() {
    let metas: Vec<String> = summaries::ALL.iter().map(|r| norm(r.1)).collect();
    assert_eq!(
        metas,
        [
            "macros::record_attrs,attrs(my_lib)",
            // The checks in the order of the announcement; the names of all of them.
            "macros::no_tuple,macros::record_attrs,macros::tick,macros::head_probe,\
             attrs(my_lib,other,tag,probed)",
            "macros::no_tuple(namedonly)"
        ]
    );
}

mod report {
    use crate::{defs, macros::owned_report};
    defs::Fields! { [defs::Fields!, defs::Choice!, defs::Tr!, defs::Several!, owned_report!] ALL }
}

#[test]
fn the_entry_reads_the_owned_names_and_strips_them() {
    // names | owned attributes in the payload | after `item_without_owned_attrs`
    assert_eq!(
        report::ALL,
        [
            "my_lib|3|0",
            "my_lib|5|0",
            "my_lib|5|0",
            "my_lib,other,tag,probed|4|0"
        ]
    );
}
