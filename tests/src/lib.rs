//! The cross-crate and UI tests of `announce`, in one package. `build.rs` runs
//! `subcrate::build::compile()`, which compiles every `#[subcrate]` module of the package
//! as a crate of its own before the tests are built.
//!
//! `announce` is made for crates that never share a compilation: the one that defines an
//! item (`#[announce]`, `$crate` and `crate::` paths in the trampoline), the proc-macro
//! crate with the callbacks, checks and transformers (`announce::helper`), and the crate
//! that calls the trampoline. Here they are `#[subcrate]` modules: each is a real crate
//! with its own `crate::`, and the test file around them is the calling crate.
//!
//! This library holds the crates that many test files share:
//!
//! - [`derive`]: proc macros of the tests (derives, attributes, callbacks, checks);
//! - [`defs`]: the definitions that the test files call. It names `derive` as
//!   `announce_test_derive`, and is not built with `cfg(test)`, while the test files are,
//!   so `cfg(test)` tells the defining crate from the calling one;
//! - [`machinery`] and [`machinery_lib`]: a library whose macros announce with
//!   `announce::machinery`;
//! - [`ui_lib`]: items announced with `strict = false`, for the UI tests of `same_context!`.
//!
//! The test files reach them as `announce_tests::defs` and so on; a subcrate of a test
//! file links them as siblings (`#[subcrate(::announce, defs)]`).
//!
//! # Adding a test
//!
//! Add `tests/<topic>.rs` (cargo finds it; nothing to list in `Cargo.toml`) and put the
//! crates in it:
//!
//! ```ignore
//! use subcrate::subcrate;
//!
//! // A proc-macro crate. `::name` links a dependency of this package, a name without
//! // `::` another subcrate. `template_quote::quote!` expands to paths in `::proc_macro2`
//! // and `::template_quote`, so both are linked beside `::syn`.
//! #[subcrate(proc_macro, ::announce, ::syn, ::proc_macro2, ::template_quote)]
//! pub mod macros {
//!     use proc_macro::TokenStream;
//!     use template_quote::quote;
//!
//!     #[proc_macro]
//!     pub fn my_callback(input: TokenStream) -> TokenStream {
//!         announce::helper::callback(input.into(), |input| { /* ... */ }).into()
//!     }
//! }
//!
//! // The defining crate.
//! #[subcrate(::announce)]
//! pub mod definer {
//!     #[announce::announce]
//!     pub struct S(pub u8);
//! }
//!
//! // The rest of the file is the calling crate.
//! definer::S! { [definer::S!, macros::my_callback!] REST }
//! ```
//!
//! A UI test is a subcrate that must fail to compile, with the messages and the
//! underlined code it must produce (`tests/ui_*.rs`):
//!
//! ```ignore
//! #[subcrate(::announce, should_error = "announce: ..", should_spanned = "Item")]
//! mod case { /* ... */ }
//! ```
//!
//! Things to know:
//!
//! - Every subcrate of the package is built by `build.rs` before any test, so one that
//!   does not compile, or a UI test that does not get its diagnostics, fails the whole
//!   package with the output of `rustc`.
//! - A subcrate re-exports the crates it links (`lib::announce` when `lib` links
//!   `::announce`). It links the same `announce` as the test file under `cargo build` and
//!   `cargo test`, but `cargo check` and clippy build other copies for the test file. So a
//!   bound on a type of a subcrate names the trait through the subcrate:
//!   `T: announce_tests::defs::announce::Announce`, not `T: announce::Announce`.
//! - A subcrate is compiled from a pretty-printed copy of its body, read from stdin:
//!   `should_spanned` compares the underlined code without whitespace, and the source
//!   lines in the messages are those of the copy. A span in another subcrate has no
//!   source to show; its label is printed as a note.
//! - `should_error = "aborting due to N previous errors\n"` pins the number of errors,
//!   and with the `\n` that there is no warning (`..; M warnings emitted\n` otherwise):
//!   trybuild compared the whole output, the other expectations only look for what they
//!   name.
//! - The UI texts are those of current compilers; `tools/msrv.sh` empties `tests/ui_*.rs`
//!   for the old ones.

use subcrate::subcrate;

#[subcrate(proc_macro, ::announce, ::syn, ::proc_macro2, ::template_quote)]
pub mod derive {
    use proc_macro::TokenStream;
    use proc_macro2::TokenStream as TokenStream2;
    use template_quote::{quote, ToTokens};

    /// announce-aware derive: redirects trampolines to `::announce_tests::defs::conflict::ParseAnnounce!`.
    #[proc_macro_derive(Parse)]
    pub fn derive_parse(input: TokenStream) -> TokenStream {
        let input: TokenStream2 = input.into();
        if let Some(out) = announce::helper::redirect(
            input,
            &syn::parse_quote!(::announce_tests::defs::conflict::ParseAnnounce),
        ) {
            return out.into();
        }
        TokenStream::new()
    }

    /// announce-aware attribute: same redirect; consumes the dummy.
    #[proc_macro_attribute]
    pub fn parse_attr(_: TokenStream, input: TokenStream) -> TokenStream {
        let input: TokenStream2 = input.into();
        match announce::helper::redirect(
            input.clone(),
            &syn::parse_quote!(::announce_tests::defs::conflict::ParseAnnounce),
        ) {
            Some(out) => out.into(),
            None => input.into(),
        }
    }

    /// Derive whose trampoline holds its item once (a large trait, `layout = single`
    /// route): redirects to `::announce_tests::defs::large::BigParseAnnounce!`.
    #[proc_macro_derive(BigParse)]
    pub fn derive_big_parse(input: TokenStream) -> TokenStream {
        let input: TokenStream2 = input.into();
        match announce::helper::redirect(
            input,
            &syn::parse_quote!(::announce_tests::defs::large::BigParseAnnounce),
        ) {
            Some(out) => out.into(),
            None => TokenStream::new(),
        }
    }

    /// The attribute form of [`derive_big_parse`]; consumes the dummy.
    #[proc_macro_attribute]
    pub fn big_parse_attr(_: TokenStream, input: TokenStream) -> TokenStream {
        let input: TokenStream2 = input.into();
        match announce::helper::redirect(
            input.clone(),
            &syn::parse_quote!(::announce_tests::defs::large::BigParseAnnounce),
        ) {
            Some(out) => out.into(),
            None => input.into(),
        }
    }

    /// Derive that redirects to a small trampoline held once
    /// (`::announce_tests::defs::large::SmallSingleParseAnnounce!`).
    #[proc_macro_derive(SmallSingleParse)]
    pub fn derive_small_single_parse(input: TokenStream) -> TokenStream {
        let input: TokenStream2 = input.into();
        match announce::helper::redirect(
            input,
            &syn::parse_quote!(::announce_tests::defs::large::SmallSingleParseAnnounce),
        ) {
            Some(out) => out.into(),
            None => TokenStream::new(),
        }
    }

    /// Callback: `summary! { [summary!] ENTRY* NAME }` emits
    /// `pub const NAME: &[(&str, &str, &str)] = &[(key, meta, item-kind+ident), ..]`
    /// in visiting order, and forwards if more continuations remain.
    #[proc_macro]
    pub fn summary(input: TokenStream) -> TokenStream {
        let input: announce::helper::Input = match syn::parse(input) {
            Ok(i) => i,
            Err(e) => return e.to_compile_error().into(),
        };
        if let Some(fwd) = input.forward() {
            return fwd.into();
        }
        let name: syn::Ident = match syn::parse2(input.rest.clone()) {
            Ok(n) => n,
            Err(e) => return e.to_compile_error().into(),
        };
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
                Ok(syn::Item::Trait(s)) => format!("trait {} ({} items)", s.ident, s.items.len()),
                Ok(syn::Item::Type(s)) => format!("type {}", s.ident),
                Ok(syn::Item::Mod(s)) => format!("mod {}", s.ident),
                Ok(_) => "other".to_string(),
                Err(e) => format!("error: {}", e),
            };
            quote! { (#key, #meta, #item) }
        });
        TokenStream::from(quote! { pub const #name: &[(&str, &str, &str)] = &[ #(#rows),* ]; })
    }

    /// Starts a trampoline from a proc macro: `start!([a!, #[derive(B)], ..] REST)`.
    /// Attribute continuations get random dummy names.
    #[proc_macro]
    pub fn start(input: TokenStream) -> TokenStream {
        struct Start(announce::helper::ContList, TokenStream2);
        impl syn::parse::Parse for Start {
            fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
                let content;
                syn::bracketed!(content in input);
                Ok(Start(content.parse()?, input.parse()?))
            }
        }
        match syn::parse::<Start>(input) {
            Ok(Start(list, rest)) => list.invoke(rest).into(),
            Err(e) => e.to_compile_error().into(),
        }
    }

    /// Definition-side check: rejects tuple structs. `no_tuple! { (ARGS), ITEM }`.
    /// It owns no attributes, and answers the attribute query all the same.
    #[proc_macro]
    pub fn no_tuple(input: TokenStream) -> TokenStream {
        announce::helper::check(input.into(), &[], |ci| match &ci.item {
            syn::Item::Struct(s) if matches!(s.fields, syn::Fields::Unnamed(_)) => {
                let msg = format!("this macro does not support tuple structs ({})", ci.args);
                Err(syn::Error::new_spanned(&s.fields, msg))
            }
            _ => Ok(TokenStream2::new()),
        })
        .into()
    }

    /// Owns the attribute `my_lib`, and rejects `#[my_lib(bad)]` anywhere in the item.
    #[proc_macro]
    pub fn lib_check(input: TokenStream) -> TokenStream {
        announce::helper::check(input.into(), &["my_lib"], |ci| {
            for attr in ci.owned_attributes() {
                if matches!(attr.parse_args::<syn::Ident>(), Ok(i) if i == "bad") {
                    return Err(syn::Error::new_spanned(
                        &attr,
                        "`#[my_lib(bad)]` is not allowed",
                    ));
                }
            }
            Ok(TokenStream2::new())
        })
        .into()
    }

    /// Owns the attributes `other` and `tag`, and rejects `#[other(forbidden)]`.
    #[proc_macro]
    pub fn other_check(input: TokenStream) -> TokenStream {
        announce::helper::check(input.into(), &["other", "tag"], |ci| {
            for attr in ci.owned_attributes() {
                if attr.path().is_ident("other")
                    && attr
                        .parse_args::<syn::Ident>()
                        .map_or(false, |i| i == "forbidden")
                {
                    return Err(syn::Error::new_spanned(
                        &attr,
                        "`#[other(forbidden)]` is not allowed",
                    ));
                }
            }
            Ok(TokenStream2::new())
        })
        .into()
    }

    /// Owns `my_lib` and records what it saw: `pub const SEEN_<ITEM>: &[&str]`, one
    /// string for each owned attribute (without spaces), in source order.
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
                    a.meta
                        .to_token_stream()
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

    /// Owns an attribute that rustc uses: always an error where it is used.
    #[proc_macro]
    pub fn doc_owner(input: TokenStream) -> TokenStream {
        announce::helper::check(input.into(), &["doc"], |_| Ok(TokenStream2::new())).into()
    }

    /// A check that reads the attribute query by hand and asserts that the list is
    /// headed by this very macro (`head_probe!`, whatever path it is written with):
    /// the head of the list is the macro being called. It owns `probed`, and answers
    /// with `Query::answer`, which drops the head and calls the next element.
    #[proc_macro]
    pub fn head_probe(input: TokenStream) -> TokenStream {
        use announce::helper::{CheckCall, Cont, OwnedAttrs};
        match syn::parse::<CheckCall>(input) {
            Ok(CheckCall::Query(q)) => {
                let is_me = matches!(
                    q.list.0.first(),
                    Some(Cont::Bang(p)) if p.segments.last().map_or(false, |s| s.ident == "head_probe")
                );
                if !is_me {
                    let list = q
                        .list
                        .to_token_stream()
                        .to_string()
                        .split_whitespace()
                        .collect::<Vec<_>>()
                        .join(" ");
                    let msg = format!(
                        "head_probe: the list does not start with `head_probe!`: {}",
                        list
                    );
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

    /// A check written the way checks were before the attribute query existed: it
    /// parses `CheckInput` and does not answer the query.
    #[proc_macro]
    pub fn old_style_check(input: TokenStream) -> TokenStream {
        match syn::parse::<announce::helper::CheckInput>(input) {
            Ok(_) => TokenStream::new(),
            Err(e) => e.to_compile_error().into(),
        }
    }

    /// `check_marker!(TYPE)`: best-effort `announce::Announce` check for a type.
    /// `check_marker!(crate = PATH, TYPE)` names the `announce` crate.
    #[proc_macro]
    pub fn check_marker(input: TokenStream) -> TokenStream {
        struct Marker(syn::Path, syn::Type);
        impl syn::parse::Parse for Marker {
            fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
                let mut krate = syn::parse_quote!(::announce);
                if input.peek(syn::Token![crate]) && input.peek2(syn::Token![=]) {
                    input.parse::<syn::Token![crate]>()?;
                    input.parse::<syn::Token![=]>()?;
                    krate = input.parse()?;
                    input.parse::<syn::Token![,]>()?;
                }
                Ok(Marker(krate, input.parse()?))
            }
        }
        match syn::parse::<Marker>(input) {
            Ok(Marker(krate, ty)) => announce::helper::check_type(&krate, &ty).into(),
            Err(e) => e.to_compile_error().into(),
        }
    }

    /// A derive of a library that emits a trampoline of a copy of its input, with
    /// `#[announce(emit_item = false)]`: the item it is on is not announced by it, and the
    /// copy is not emitted.
    #[proc_macro_derive(Announced)]
    pub fn derive_announced(input: TokenStream) -> TokenStream {
        let input: TokenStream2 = input.into();
        TokenStream::from(quote! { #[::announce::announce(emit_item = false)] #input })
    }

    /// Transformer: keeps only `pub` fields in the newest entry's struct and
    /// lists their names in META as `fields(..)`.
    #[proc_macro]
    pub fn public_fields(input: TokenStream) -> TokenStream {
        announce::helper::transformer(input.into(), |mut entry| {
            let mut s: syn::ItemStruct = syn::parse2(entry.item.clone())?;
            let mut names = Vec::new();
            if let syn::Fields::Named(f) = &mut s.fields {
                f.named = f
                    .named
                    .clone()
                    .into_iter()
                    .filter(|f| matches!(f.vis, syn::Visibility::Public(_)))
                    .collect();
                names = f.named.iter().map(|f| f.ident.clone().unwrap()).collect();
            }
            entry.item = quote! { #s };
            entry.meta.push(syn::parse_quote!(fields(#(#names),*)));
            Ok(entry)
        })
        .into()
    }

    struct GatherRest {
        paths: Vec<syn::Path>,
        name: syn::Ident,
    }
    impl syn::parse::Parse for GatherRest {
        fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
            let content;
            syn::parenthesized!(content in input);
            let paths = syn::punctuated::Punctuated::<syn::Path, syn::Token![,]>::parse_terminated(
                &content,
            )?;
            Ok(GatherRest {
                paths: paths.into_iter().collect(),
                name: input.parse()?,
            })
        }
    }

    /// `gathered! { [gathered!] (a::A, b::B) NAME }` fetches the listed
    /// definitions (in one go) and emits `NAME: &[&str]` with their item
    /// idents in the requested order.
    #[proc_macro]
    pub fn gathered(input: TokenStream) -> TokenStream {
        announce::helper::callback(input.into(), |input| {
            let rest: GatherRest = input.rest_as()?;
            let needed = rest.paths.iter().cloned().map(announce::helper::Cont::bang);
            let entries = match input.gather(needed) {
                announce::helper::Gather::Ready(e) => e,
                announce::helper::Gather::Fetch(ts) => return Ok(ts),
            };
            let mut idents = Vec::new();
            for e in entries {
                let ident = match e.item()? {
                    syn::Item::Struct(s) => s.ident,
                    syn::Item::Enum(s) => s.ident,
                    syn::Item::Trait(s) => s.ident,
                    syn::Item::Type(s) => s.ident,
                    other => return Err(syn::Error::new_spanned(other, "unexpected item")),
                };
                idents.push(ident.to_string());
            }
            let name = &rest.name;
            Ok(quote! { pub const #name: &[&str] = &[ #(#idents),* ]; })
        })
        .into()
    }

    /// `fetched_raw! { [fetched_raw!] (a::A, b::B) NAME }`: like `gathered`, but
    /// with the raw `Input::fetch` for what is still missing. Emits `NAME: &[&str]`
    /// with the item idents in the requested order, and `NAME_OPTIONS: &str`, the
    /// options group the callback was last called with.
    #[proc_macro]
    pub fn fetched_raw(input: TokenStream) -> TokenStream {
        announce::helper::callback(input.into(), |input| {
            let rest: GatherRest = input.rest_as()?;
            let needed: Vec<announce::helper::Cont> = rest
                .paths
                .iter()
                .cloned()
                .map(announce::helper::Cont::bang)
                .collect();
            let missing: Vec<announce::helper::Cont> = needed
                .iter()
                .filter(|c| input.entry_for(c).is_none())
                .cloned()
                .collect();
            if !missing.is_empty() {
                return Ok(input.fetch(missing));
            }
            let mut idents = Vec::new();
            for c in &needed {
                let e = input.entry_for(c).expect("checked above");
                let ident = match e.item()? {
                    syn::Item::Struct(s) => s.ident,
                    syn::Item::Enum(s) => s.ident,
                    syn::Item::Trait(s) => s.ident,
                    syn::Item::Type(s) => s.ident,
                    other => return Err(syn::Error::new_spanned(other, "unexpected item")),
                };
                idents.push(ident.to_string());
            }
            let name = &rest.name;
            let options = input
                .options
                .as_ref()
                .map(|o| o.to_string())
                .unwrap_or_default();
            let options_name = syn::Ident::new(&format!("{}_OPTIONS", name), name.span());
            Ok(quote! {
                pub const #name: &[&str] = &[ #(#idents),* ];
                pub const #options_name: &str = #options;
            })
        })
        .into()
    }

    /// Records, for every entry in visiting order, `pub const <REST>: &[(&str, usize, usize, String)]`
    /// is not possible in a const, so the rows are strings: `"names|in item|in stripped"`:
    /// the owned attribute names of the entry (`Entry::owned_attrs`, comma separated), the
    /// number of owned attributes in `Entry::item` and in `Entry::item_without_owned_attrs`.
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

    /// Reports what a callback receives next to the entries, for the tests that
    /// REST is the caller's tokens and nothing else. Emits, with fixed names:
    /// `REST: &str` (`input.rest`, printed), `ENTRIES: usize`, `OWNED: &[&str]` (the
    /// names of `Entry::owned_attrs`, comma separated, one row for each entry in
    /// visiting order) and `META: &[&str]` (the META of each entry). It forwards
    /// when more continuations remain.
    #[proc_macro]
    pub fn rest_probe(input: TokenStream) -> TokenStream {
        announce::helper::callback(input.into(), |input| {
            if let Some(fwd) = input.forward() {
                return Ok(fwd);
            }
            let rest = input.rest.to_string();
            let entries = input.entries.len();
            let mut owned = Vec::new();
            let mut metas = Vec::new();
            for e in input.entries_in_order() {
                let names: Vec<String> = e
                    .owned_attrs()
                    .names()
                    .iter()
                    .map(|n| n.to_string())
                    .collect();
                owned.push(names.join(","));
                metas.push(
                    e.meta
                        .iter()
                        .map(|m| m.to_token_stream().to_string())
                        .collect::<Vec<_>>()
                        .join(", "),
                );
            }
            Ok(quote! {
                pub const REST: &str = #rest;
                pub const ENTRIES: usize = #entries;
                pub const OWNED: &[&str] = &[ #(#owned),* ];
                pub const META: &[&str] = &[ #(#metas),* ];
            })
        })
        .into()
    }

    /// Re-emits every entry's item, rewritten with its type-leak information
    /// (`TypeLeak::apply`) when the definition has an repeater, and records the
    /// META as `pub const <REST>: &[&str]`.
    #[proc_macro]
    pub fn apply_leak(input: TokenStream) -> TokenStream {
        announce::helper::callback(input.into(), |input| {
            let name: syn::Ident = input.rest_as()?;
            let mut out = proc_macro2::TokenStream::new();
            let mut metas = Vec::new();
            for e in input.entries_in_order() {
                let mut item = e.item()?;
                if let Some(tl) = e.type_leak()? {
                    tl.apply(&mut item);
                }
                metas.push(
                    e.meta
                        .iter()
                        .map(|m| m.to_token_stream().to_string())
                        .collect::<Vec<_>>()
                        .join(", "),
                );
                out.extend(quote! { #item });
            }
            Ok(quote! { #out pub const #name: &[&str] = &[ #(#metas),* ]; })
        })
        .into()
    }

    /// Reads the `type_leak(..)` META through `Entry::type_leak` and records,
    /// for every entry in visiting order, `pub const <REST>: &[&str]` with
    /// `"-"` when the entry has no table, else `"[Type@Args,..]"`: each
    /// replaced type (type-leak's required entries) and the first argument of
    /// its repeater at the scope of the item (types and arguments without
    /// spaces, sorted by text, so the order of the table does not matter).
    #[proc_macro]
    pub fn leak_types(input: TokenStream) -> TokenStream {
        announce::helper::callback(input.into(), |input| {
            let name: syn::Ident = input.rest_as()?;
            let squash = |t: TokenStream2| t.to_string().split_whitespace().collect::<String>();
            let mut rows = Vec::new();
            for e in input.entries_in_order() {
                rows.push(match e.type_leak()? {
                    None => "-".to_string(),
                    Some(tl) => {
                        let mut pairs = Vec::new();
                        for (ix, ty) in tl.types() {
                            let args = tl.entry_args(
                                ix,
                                &announce::helper::type_leak::UseSite::identity(),
                            )?;
                            pairs.push(format!(
                                "{}@{}",
                                squash(ty.to_token_stream()),
                                squash(args.to_token_stream())
                            ));
                        }
                        pairs.sort();
                        format!("[{}]", pairs.join(","))
                    }
                });
            }
            Ok(quote! { pub const #name: &[&str] = &[ #(#rows),* ]; })
        })
        .into()
    }

    // The macro forms `callback!` and `transformer!` are kept for compatibility
    // (flat_enum 0.2 and other crates define their proc macros with them). They are thin
    // wrappers around the functions `callback` and `transformer`; these three are tests
    // that they still work.

    announce::helper::callback! {
        /// `compat_idents! { [compat_idents!] NAME }`: emits `NAME: &[&str]` with the item
        /// idents of the entries in visiting order. Defined with the macro `callback!`.
        pub fn compat_idents(input) {
            let name: syn::Ident = input.rest_as()?;
            let mut idents = Vec::new();
            for e in input.entries_in_order() {
                match e.item()? {
                    syn::Item::Struct(s) => idents.push(s.ident.to_string()),
                    other => return Err(syn::Error::new_spanned(other, "expected a struct")),
                }
            }
            Ok(quote! { pub const #name: &[&str] = &[ #(#idents),* ]; })
        }
    }

    announce::helper::transformer! {
        /// Adds `tagged` to the META of the newest entry. Defined with the macro
        /// `transformer!`; the body returns early when there is no META.
        pub fn compat_tag(entry) {
            if entry.meta.is_empty() {
                return Ok(entry);
            }
            entry.meta.push(syn::parse_quote!(tagged));
            Ok(entry)
        }
    }

    announce::helper::transformer! {
        /// Changes nothing: its `entry` is not mutated, and that must not warn.
        pub fn compat_noop(entry) {
            Ok(entry)
        }
    }

    // ---- several announcements of one item (`defs::merge`) ----

    /// An attribute macro that returns its input as it is. `#[announce]` cannot know
    /// that, so the runner waits for it (see the module `merge` of the definitions).
    #[proc_macro_attribute]
    pub fn passthrough(_: TokenStream, item: TokenStream) -> TokenStream {
        item
    }

    /// `#[emit_announce(OPTIONS)]`: a macro that announces what it produces. Emits the
    /// item with `#[::announce::announce(OPTIONS)]` as its outermost attribute.
    #[proc_macro_attribute]
    pub fn emit_announce(attr: TokenStream, item: TokenStream) -> TokenStream {
        let (attr, item): (TokenStream2, TokenStream2) = (attr.into(), item.into());
        TokenStream::from(quote! { #[::announce::announce(#attr)] #item })
    }

    /// An attribute macro of another crate that returns the item without its outer attributes,
    /// as a macro that rebuilds its input from the parsed fields does. It drops the runner that
    /// `#[announce]` has put last among the attributes, so nothing announces the item: the
    /// sentinel of the announcement fails at the definition.
    #[proc_macro_attribute]
    pub fn drop_attrs(_: TokenStream, item: TokenStream) -> TokenStream {
        use proc_macro::TokenTree;
        let mut tokens = item.into_iter().peekable();
        // The attributes are `#` and a bracketed group, one after the other.
        while matches!(tokens.peek(), Some(TokenTree::Punct(p)) if p.as_char() == '#') {
            tokens.next();
            tokens.next();
        }
        tokens.collect()
    }

    /// An attribute macro that puts a runner of another version of `announce` below the other
    /// attributes of the item, as a second `announce` crate in the build would.
    #[proc_macro_attribute]
    pub fn other_version(_: TokenStream, item: TokenStream) -> TokenStream {
        use proc_macro::TokenTree;
        let mut out = TokenStream::new();
        let mut tokens = item.into_iter().peekable();
        while matches!(tokens.peek(), Some(TokenTree::Punct(p)) if p.as_char() == '#') {
            out.extend(tokens.next());
            out.extend(tokens.next());
        }
        out.extend(TokenStream::from(quote! {
            #[::announce::__private::__announce_runner(__announce_format_version = "7")]
        }));
        out.extend(tokens);
        out
    }

    /// `#[late_announce(OPTIONS)]`: a macro that announces what it produces, wrongly: it adds
    /// `#[::announce::announce(OPTIONS)]` *below* the other attributes of the item, after
    /// the runner that `#[announce]` put there. That announcement is too late.
    #[proc_macro_attribute]
    pub fn late_announce(attr: TokenStream, item: TokenStream) -> TokenStream {
        let attr: TokenStream2 = attr.into();
        let mut item: syn::DeriveInput = match syn::parse(item) {
            Ok(i) => i,
            Err(e) => return e.to_compile_error().into(),
        };
        item.attrs
            .push(syn::parse_quote!(#[::announce::announce(#attr)]));
        TokenStream::from(quote! { #item })
    }

    /// `#[chain(N)]`: a macro that runs `N` times, each time announcing the item with
    /// `meta = merge::step(N)` and leaving `#[chain(N-1)]` to expand next. `#[chain(0)]`
    /// only returns the item. All the announcements must end up as one.
    #[proc_macro_attribute]
    pub fn chain(attr: TokenStream, item: TokenStream) -> TokenStream {
        let n: u32 = match attr.to_string().trim().parse() {
            Ok(n) => n,
            Err(_) => {
                return quote! { ::core::compile_error!("chain: expected a number"); }.into();
            }
        };
        let item: TokenStream2 = item.into();
        if n == 0 {
            return item.into();
        }
        let (next, step) = (
            proc_macro2::Literal::u32_unsuffixed(n - 1),
            proc_macro2::Literal::u32_unsuffixed(n),
        );
        TokenStream::from(quote! {
            #[::announce::announce(meta = merge::step(#step))]
            #[::announce_test_derive::chain(#next)]
            #item
        })
    }

    /// A check that leaves a mark where it runs: `tick! { (NAME), ITEM }` emits
    /// `pub const TICK_<ITEM>_<NAME>: u8 = 0;`, so a check that ran twice for one item is
    /// an error (the constant would be defined twice). It owns no attributes.
    #[proc_macro]
    pub fn tick(input: TokenStream) -> TokenStream {
        announce::helper::check(input.into(), &[], |ci| {
            let name = match &ci.item {
                syn::Item::Struct(s) => s.ident.to_string(),
                syn::Item::Enum(s) => s.ident.to_string(),
                syn::Item::Trait(s) => s.ident.to_string(),
                other => {
                    return Err(syn::Error::new_spanned(other, "tick: unsupported item"));
                }
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

    // ---- announcements are told by their flag, not by their path (`defs::paths`) ----

    /// An attribute macro of another crate that happens to be called `announce`, like
    /// `#[metrics::announce(level = 2)]`. It returns the item and a constant next to it:
    /// `#[announce(level = N)]` makes `FOREIGN_<ITEM>_<N>: u32 = N`, and any other arguments
    /// (also none, or `meta = path`, which are arguments of ours as well) make
    /// `FOREIGN_<ITEM>_ARGS: &str`, the arguments without spaces. `#[announce]` must never
    /// take it for an announcement or swallow it: not by the path
    /// `announce_test_derive::announce`, nor when it is imported and written as the bare
    /// `#[announce(level = 2)]`, `#[announce]` or `#[announce(meta = path)]`.
    #[proc_macro_attribute]
    pub fn announce(attr: TokenStream, item: TokenStream) -> TokenStream {
        let attr: TokenStream2 = attr.into();
        let item: TokenStream2 = item.into();
        let parsed: syn::DeriveInput = match syn::parse2(item.clone()) {
            Ok(i) => i,
            Err(e) => return e.to_compile_error().into(),
        };
        let name = parsed.ident.to_string().to_uppercase();
        let span = proc_macro2::Span::call_site();
        let tokens: Vec<proc_macro2::TokenTree> = attr.clone().into_iter().collect();
        let level = match tokens.as_slice() {
            [proc_macro2::TokenTree::Ident(key), proc_macro2::TokenTree::Punct(eq), proc_macro2::TokenTree::Literal(lit)]
                if key == "level" && eq.as_char() == '=' =>
            {
                syn::parse2::<syn::LitInt>(quote! { #lit }).ok()
            }
            _ => None,
        };
        let konst = match level {
            Some(level) => {
                let id =
                    syn::Ident::new(&format!("FOREIGN_{}_{}", name, level.base10_digits()), span);
                quote! { pub const #id: u32 = #level; }
            }
            None => {
                let id = syn::Ident::new(&format!("FOREIGN_{}_ARGS", name), span);
                let args: String = attr.to_string().split_whitespace().collect();
                quote! { pub const #id: &str = #args; }
            }
        };
        TokenStream::from(quote! {
            #item
            #konst
        })
    }

    thread_local! {
        /// How many times `count_query` answered the query for each item (by name).
        static QUERIES: std::cell::RefCell<std::collections::HashMap<String, usize>> =
            std::cell::RefCell::new(std::collections::HashMap::new());
    }

    /// The name of the item in the state of an attribute query
    /// (`[RANDOM; attr (OPTIONS) { ITEM }] attrs(..)*`): the identifier after the first
    /// `struct`, `enum`, `union` or `trait` in it.
    fn queried_item(state: &TokenStream2) -> String {
        use proc_macro2::TokenTree;
        fn find(ts: TokenStream2) -> Option<String> {
            let mut prev_kw = false;
            for t in ts {
                match t {
                    TokenTree::Ident(i) if prev_kw => return Some(i.to_string()),
                    TokenTree::Ident(i) => {
                        prev_kw = matches!(
                            i.to_string().as_str(),
                            "struct" | "enum" | "union" | "trait"
                        )
                    }
                    TokenTree::Group(g) => {
                        if let Some(found) = find(g.stream()) {
                            return Some(found);
                        }
                        prev_kw = false;
                    }
                    _ => prev_kw = false,
                }
            }
            None
        }
        find(state.clone()).unwrap_or_default()
    }

    /// A check that counts the queries it is asked. It answers the query of an item with
    /// an attribute name that owns the number of the answer, `queried_0` for the first
    /// query that it answers for that item, `queried_1` for the second, ..., so META lists
    /// `attrs(queried_0)` if the query was sent once. A query that is sent again
    /// for the same item shows in the names. (A thread-local counter: the compiler
    /// expands macros on one thread.)
    #[proc_macro]
    pub fn count_query(input: TokenStream) -> TokenStream {
        use announce::helper::{CheckCall, OwnedAttrs};
        match syn::parse::<CheckCall>(input) {
            Ok(CheckCall::Query(q)) => {
                let item = queried_item(&q.state);
                let n = QUERIES.with(|c| {
                    let mut c = c.borrow_mut();
                    let n = c.entry(item).or_insert(0);
                    *n += 1;
                    *n - 1
                });
                q.answer(&OwnedAttrs::from_strs(&[&format!("queried_{}", n)]))
                    .into()
            }
            Ok(CheckCall::Run(_)) => TokenStream::new(),
            Err(e) => e.to_compile_error().into(),
        }
    }

    announce::helper::callback! {
        /// Reports the format prefix of the call it got: `pub const <REST>: &str` is the
        /// version in `@announce_format "VERSION"` (`Input::format`), or `"none"` for a
        /// call without the prefix. `REST` is the name. Defined with the macro `callback!`.
        pub fn format_report(input) {
            let name: syn::Ident = input.rest_as()?;
            let format = input.format.clone().unwrap_or_else(|| "none".to_string());
            Ok(quote! { pub const #name: &str = #format; })
        }
    }

    /// The same as `format_report!`, defined with the function `announce::helper::callback`.
    #[proc_macro]
    pub fn format_report_fn(input: TokenStream) -> TokenStream {
        announce::helper::callback(input.into(), |input| {
            let name: syn::Ident = input.rest_as()?;
            let format = input.format.clone().unwrap_or_else(|| "none".to_string());
            Ok(quote! { pub const #name: &str = #format; })
        })
        .into()
    }

    /// Forwards what it got to the next element, with `announce::helper::transformer` (the
    /// entry is not changed): the call it makes has the format prefix.
    #[proc_macro]
    pub fn pass_through_fn(input: TokenStream) -> TokenStream {
        announce::helper::transformer(input.into(), Ok).into()
    }

    // ---- items whose types do not resolve outside their module (`defs::recorded`) ----

    /// `resolved_copy! { [..] NAME }`: a copy named `NAME` of the item of every entry, as
    /// `Entry::item_resolved` gives it, with its check.
    #[proc_macro]
    pub fn resolved_copy(input: TokenStream) -> TokenStream {
        announce::helper::callback(input.into(), |input| {
            let name: syn::Ident = input.rest_as()?;
            let krate: syn::Path = syn::parse_quote!(::announce);
            let mut out = TokenStream2::new();
            for e in input.entries_in_order() {
                let (mut item, check) = e.item_resolved(&krate)?;
                match &mut item {
                    syn::Item::Struct(i) => i.ident = name.clone(),
                    syn::Item::Enum(i) => i.ident = name.clone(),
                    syn::Item::Trait(i) => i.ident = name.clone(),
                    syn::Item::Type(i) => i.ident = name.clone(),
                    other => {
                        return Err(syn::Error::new_spanned(other, "expected a type or trait"))
                    }
                }
                out.extend(quote! { #item #check });
            }
            Ok(out)
        })
        .into()
    }

    /// `leak_failures! { [..] NAME }`: `pub const NAME: &[&str]` has a row for every entry:
    /// `"table"` with a `type_leak(..)`, `"-"` with neither, and with a `type_leak_error(..)` the
    /// causes as `"TOKENS@REASON|TOKENS@REASON"` (the tokens without spaces). `NAME_ERR: &[bool]` is
    /// `true` where `Entry::type_leak()` is an error.
    #[proc_macro]
    pub fn leak_failures(input: TokenStream) -> TokenStream {
        announce::helper::callback(input.into(), |input| {
            let name: syn::Ident = input.rest_as()?;
            let err_name = syn::Ident::new(&format!("{}_ERR", name), name.span());
            let (mut rows, mut errs) = (Vec::new(), Vec::new());
            for e in input.entries_in_order() {
                errs.push(e.type_leak().is_err());
                rows.push(match e.leak_failure()? {
                    Some(f) => f
                        .errors
                        .iter()
                        .map(|x| {
                            let tokens = x.tokens.to_token_stream();
                            let tokens = tokens.to_string().split_whitespace().collect::<String>();
                            format!("{}@{}", tokens, x.reason)
                        })
                        .collect::<Vec<_>>()
                        .join("|"),
                    None if e.type_leak()?.is_some() => "table".to_string(),
                    None => "-".to_string(),
                });
            }
            Ok(quote! {
                pub const #name: &[&str] = &[ #(#rows),* ];
                pub const #err_name: &[bool] = &[ #(#errs),* ];
            })
        })
        .into()
    }
}

#[subcrate(::announce, announce_test_derive = derive)]
pub mod defs {
    use announce::announce;

    pub mod carried {
        //! Types that name something of this module without being a path to it, carried by a `repeater`:
        //! a trait object, an array length and a const generic argument. Without a `repeater` the same
        //! items fail (`value_paths` and `trait_paths_relative` in `tests/ui_leak.rs`).

        use announce::announce;

        pub mod m {
            use super::announce;

            pub trait Base {
                fn n(&self) -> u8;
            }

            pub const N: usize = 3;

            pub mod inner {
                pub struct Wrap<const K: usize>(pub [u8; K]);
            }
            use inner::Wrap;

            announce::define_repeater!(pub Repeater);

            #[announce(repeater = crate::carried::m::Repeater)]
            pub struct Item {
                pub a: ::std::boxed::Box<dyn Base>,
                pub b: [u8; N],
                pub c: Wrap<{ N }>,
            }

            #[announce(repeater = crate::carried::m::Repeater)]
            pub trait Make {
                fn object(&self) -> ::std::boxed::Box<dyn Base + Send>;
                fn array(&self) -> [u8; N];
                fn wrap(&self) -> Wrap<{ N }>;
            }
        }
    }
    pub mod docs {
        //! Doc attributes are removed from the payload (and from what checks get), not
        //! from the emitted item. missing_docs is denied in this module: an item that
        //! lost its docs on the way would not build. The definer is not built with
        //! `cfg(test)`, the test files are.
        #![deny(missing_docs)]
        #![allow(unused_doc_comments, unknown_lints, clippy::mixed_attributes_style)]

        use announce::announce;

        /// Doc of Plain.
        #[doc(alias = "plain_alias")]
        #[announce]
        #[derive(Clone, Debug)]
        pub struct Plain<#[doc = "doc of T"] T> {
            /// Doc of a.
            pub a: T,
            /// Doc of b, which is hidden.
            #[doc(hidden)]
            pub b: u8,
            #[cfg_attr(not(test), doc = "cfg_attr doc, without x")]
            #[cfg_attr(test, doc = "cfg_attr doc, with x")]
            pub c: u8,
            #[cfg_attr(not(test), doc(hidden))]
            /// Doc of d.
            pub d: u8,
        }

        /// Doc of Choice.
        #[announce]
        pub enum Choice {
            /// Doc of A.
            A,
            /// Doc of B.
            B(#[doc = "doc of the tuple field"] u8),
        }

        /// Doc of Tr.
        #[announce]
        pub trait Tr {
            /// Doc of K.
            const K: u8;
            /// Doc of A.
            type A;
            /// Doc of f.
            #[doc(alias = "method_alias")]
            fn f(&self) -> u8 {
                1
            }
        }

        /// Doc of inner.
        #[announce]
        pub mod inner {
            //! Inner doc of inner.
            #![doc(alias = "inner_alias")]

            /// Doc of Nested.
            pub struct Nested {
                /// Doc of n.
                pub n: u8,
            }

            /**
             * Block doc of three.
             */
            pub fn three() -> u8 {
                3
            }
        }

        /// Doc of Kept, whose docs stay in the payload.
        #[announce(docs = true)]
        pub struct Kept {
            /// Doc of k.
            pub k: u8,
        }

        /// Doc of KeptMod.
        #[announce(docs = true)]
        pub mod kept_mod {
            //! Inner doc of kept_mod.

            /// Doc of Inside.
            pub struct Inside;
        }

        /// Doc of WithDerive.
        #[announce]
        #[derive(Debug)]
        pub struct WithDerive {
            /// Doc of a.
            pub a: u8,
        }

        /// Doc of KeptWithDerive.
        #[announce(docs = true)]
        #[derive(Debug)]
        pub struct KeptWithDerive {
            /// Doc of a.
            pub a: u8,
        }

        /// A check that records the item it receives, next to the item.
        #[macro_export]
        #[doc(hidden)]
        macro_rules! record_item {
            (@announce_format "1" @__announce_attrs [ $($hp:ident)? $(:: $hs:ident)* ! , $($np:ident)? $(:: $ns:ident)* ! $(, $($rest:tt)*)? ] { $($state:tt)* }) => {
                $($np)? $(:: $ns)* ! { @announce_format "1" @__announce_attrs [ $($np)? $(:: $ns)* ! $(, $($rest)*)? ] { $($state)* } }
            };
            (@announce_format "1" ($($args:tt)*), $($item:tt)*) => {
                /// What the check received.
                pub const CHECKED: &str = stringify!($($item)*);
            };
        }

        /// The checks get the payload: no docs by default, with docs = true the docs.
        pub mod checked {
            use super::announce;

            /// Item of a check without docs.
            pub mod plain {
                use super::announce;

                /// Doc of Checked.
                #[announce(crate::record_item)]
                pub struct Checked {
                    /// Doc of a.
                    pub a: u8,
                }
            }

            /// Item of a check with docs = true.
            pub mod kept {
                use super::announce;

                /// Doc of Checked.
                #[announce(docs = true, crate::record_item)]
                pub struct Checked {
                    /// Doc of a.
                    pub a: u8,
                }
            }
        }

        /// With owned attributes the derive emits the item (not [`#[announce]`](macro@announce)): it keeps its docs.
        #[announce(crate::rules_check)]
        pub struct Owned {
            /// Doc of a.
            #[rules(skip)]
            pub a: u8,
            /// Doc of b.
            pub b: u8,
        }
    }
    pub mod large {
        //! Items above `SINGLE_COPY_THRESHOLD` tokens: their trampoline holds the item
        //! once (`Layout::Single`). Each one is well over 300
        //! tokens; `tests/large.rs` runs them through every route and
        //! `large_errors` in `tests/ui_trampoline.rs` pins the errors only that layout has.

        use super::announce;

        // A trait with 30 required methods.
        #[announce]
        pub trait BigTrait {
            fn m0(&self, x: u32) -> u32;
            fn m1(&self, x: u32) -> u32;
            fn m2(&self, x: u32) -> u32;
            fn m3(&self, x: u32) -> u32;
            fn m4(&self, x: u32) -> u32;
            fn m5(&self, x: u32) -> u32;
            fn m6(&self, x: u32) -> u32;
            fn m7(&self, x: u32) -> u32;
            fn m8(&self, x: u32) -> u32;
            fn m9(&self, x: u32) -> u32;
            fn m10(&self, x: u32) -> u32;
            fn m11(&self, x: u32) -> u32;
            fn m12(&self, x: u32) -> u32;
            fn m13(&self, x: u32) -> u32;
            fn m14(&self, x: u32) -> u32;
            fn m15(&self, x: u32) -> u32;
            fn m16(&self, x: u32) -> u32;
            fn m17(&self, x: u32) -> u32;
            fn m18(&self, x: u32) -> u32;
            fn m19(&self, x: u32) -> u32;
            fn m20(&self, x: u32) -> u32;
            fn m21(&self, x: u32) -> u32;
            fn m22(&self, x: u32) -> u32;
            fn m23(&self, x: u32) -> u32;
            fn m24(&self, x: u32) -> u32;
            fn m25(&self, x: u32) -> u32;
            fn m26(&self, x: u32) -> u32;
            fn m27(&self, x: u32) -> u32;
            fn m28(&self, x: u32) -> u32;
            fn m29(&self, x: u32) -> u32;
        }

        // Required and provided methods (for `signature_only!`).
        #[announce]
        pub trait BigSig {
            fn required(&self) -> u32;
            fn r0(&self, x: u32) -> u32;
            fn r1(&self, x: u32) -> u32;
            fn r2(&self, x: u32) -> u32;
            fn r3(&self, x: u32) -> u32;
            fn r4(&self, x: u32) -> u32;
            fn r5(&self, x: u32) -> u32;
            fn r6(&self, x: u32) -> u32;
            fn r7(&self, x: u32) -> u32;
            fn r8(&self, x: u32) -> u32;
            fn r9(&self, x: u32) -> u32;
            fn r10(&self, x: u32) -> u32;
            fn r11(&self, x: u32) -> u32;
            fn r12(&self, x: u32) -> u32;
            fn r13(&self, x: u32) -> u32;
            fn p0(&self) -> u32 {
                0
            }
            fn p1(&self) -> u32 {
                1
            }
            fn p2(&self) -> u32 {
                2
            }
            fn p3(&self) -> u32 {
                3
            }
            fn p4(&self) -> u32 {
                4
            }
            fn p5(&self) -> u32 {
                5
            }
            fn p6(&self) -> u32 {
                6
            }
            fn p7(&self) -> u32 {
                7
            }
            fn p8(&self) -> u32 {
                8
            }
            fn p9(&self) -> u32 {
                9
            }
            fn p10(&self) -> u32 {
                10
            }
            fn p11(&self) -> u32 {
                11
            }
            fn p12(&self) -> u32 {
                12
            }
            fn p13(&self) -> u32 {
                13
            }
            fn body(&self) -> u32 {
                42
            }
        }

        #[announce]
        pub struct BigStruct {
            pub f0: u32,
            pub f1: u32,
            pub f2: u32,
            pub f3: u32,
            pub f4: u32,
            pub f5: u32,
            pub f6: u32,
            pub f7: u32,
            pub f8: u32,
            pub f9: u32,
            pub f10: u32,
            pub f11: u32,
            pub f12: u32,
            pub f13: u32,
            pub f14: u32,
            pub f15: u32,
            pub f16: u32,
            pub f17: u32,
            pub f18: u32,
            pub f19: u32,
            pub f20: u32,
            pub f21: u32,
            pub f22: u32,
            pub f23: u32,
            pub f24: u32,
            pub f25: u32,
            pub f26: u32,
            pub f27: u32,
            pub f28: u32,
            pub f29: u32,
            pub f30: u32,
            pub f31: u32,
            pub f32: u32,
            pub f33: u32,
            pub f34: u32,
            pub f35: u32,
            pub f36: u32,
            pub f37: u32,
            pub f38: u32,
            pub f39: u32,
            pub f40: u32,
            pub f41: u32,
            pub f42: u32,
            pub f43: u32,
            pub f44: u32,
            pub f45: u32,
            pub f46: u32,
            pub f47: u32,
            pub f48: u32,
            pub f49: u32,
            pub f50: u32,
            pub f51: u32,
            pub f52: u32,
            pub f53: u32,
            pub f54: u32,
            pub f55: u32,
            pub f56: u32,
            pub f57: u32,
            pub f58: u32,
            pub f59: u32,
            pub f60: u32,
            pub f61: u32,
            pub f62: u32,
            pub f63: u32,
            pub f64: u32,
            pub f65: u32,
            pub f66: u32,
            pub f67: u32,
            pub f68: u32,
            pub f69: u32,
            pub f70: u32,
            pub f71: u32,
            pub f72: u32,
            pub f73: u32,
            pub f74: u32,
            pub f75: u32,
            pub f76: u32,
            pub f77: u32,
            pub f78: u32,
            pub f79: u32,
        }

        #[announce]
        pub enum BigEnum {
            V0(u32),
            V1(u32),
            V2(u32),
            V3(u32),
            V4(u32),
            V5(u32),
            V6(u32),
            V7(u32),
            V8(u32),
            V9(u32),
            V10(u32),
            V11(u32),
            V12(u32),
            V13(u32),
            V14(u32),
            V15(u32),
            V16(u32),
            V17(u32),
            V18(u32),
            V19(u32),
            V20(u32),
            V21(u32),
            V22(u32),
            V23(u32),
            V24(u32),
            V25(u32),
            V26(u32),
            V27(u32),
            V28(u32),
            V29(u32),
            V30(u32),
            V31(u32),
            V32(u32),
            V33(u32),
            V34(u32),
            V35(u32),
            V36(u32),
            V37(u32),
            V38(u32),
            V39(u32),
            V40(u32),
            V41(u32),
            V42(u32),
            V43(u32),
            V44(u32),
            V45(u32),
            V46(u32),
            V47(u32),
            V48(u32),
            V49(u32),
            V50(u32),
            V51(u32),
            V52(u32),
            V53(u32),
            V54(u32),
            V55(u32),
            V56(u32),
            V57(u32),
            V58(u32),
            V59(u32),
            V60(u32),
            V61(u32),
            V62(u32),
            V63(u32),
            V64(u32),
            V65(u32),
            V66(u32),
            V67(u32),
            V68(u32),
            V69(u32),
        }

        #[announce]
        #[derive(Clone, Copy)]
        pub union BigUnion {
            pub i: u32,
            pub f: f32,
            pub f0: u32,
            pub f1: u32,
            pub f2: u32,
            pub f3: u32,
            pub f4: u32,
            pub f5: u32,
            pub f6: u32,
            pub f7: u32,
            pub f8: u32,
            pub f9: u32,
            pub f10: u32,
            pub f11: u32,
            pub f12: u32,
            pub f13: u32,
            pub f14: u32,
            pub f15: u32,
            pub f16: u32,
            pub f17: u32,
            pub f18: u32,
            pub f19: u32,
            pub f20: u32,
            pub f21: u32,
            pub f22: u32,
            pub f23: u32,
            pub f24: u32,
            pub f25: u32,
            pub f26: u32,
            pub f27: u32,
            pub f28: u32,
            pub f29: u32,
            pub f30: u32,
            pub f31: u32,
            pub f32: u32,
            pub f33: u32,
            pub f34: u32,
            pub f35: u32,
            pub f36: u32,
            pub f37: u32,
            pub f38: u32,
            pub f39: u32,
            pub f40: u32,
            pub f41: u32,
            pub f42: u32,
            pub f43: u32,
            pub f44: u32,
            pub f45: u32,
            pub f46: u32,
            pub f47: u32,
            pub f48: u32,
            pub f49: u32,
            pub f50: u32,
            pub f51: u32,
            pub f52: u32,
            pub f53: u32,
            pub f54: u32,
            pub f55: u32,
            pub f56: u32,
            pub f57: u32,
            pub f58: u32,
            pub f59: u32,
            pub f60: u32,
            pub f61: u32,
            pub f62: u32,
            pub f63: u32,
            pub f64: u32,
            pub f65: u32,
            pub f66: u32,
            pub f67: u32,
            pub f68: u32,
            pub f69: u32,
        }

        #[announce]
        pub type BigAlias = (
            ::std::vec::Vec<u8>,
            ::std::vec::Vec<u8>,
            ::std::vec::Vec<u8>,
            ::std::vec::Vec<u8>,
            ::std::vec::Vec<u8>,
            ::std::vec::Vec<u8>,
            ::std::vec::Vec<u8>,
            ::std::vec::Vec<u8>,
            ::std::vec::Vec<u8>,
            ::std::vec::Vec<u8>,
            ::std::vec::Vec<u8>,
            ::std::vec::Vec<u8>,
            ::std::vec::Vec<u8>,
            ::std::vec::Vec<u8>,
            ::std::vec::Vec<u8>,
            ::std::vec::Vec<u8>,
            ::std::vec::Vec<u8>,
            ::std::vec::Vec<u8>,
            ::std::vec::Vec<u8>,
            ::std::vec::Vec<u8>,
            ::std::vec::Vec<u8>,
            ::std::vec::Vec<u8>,
            ::std::vec::Vec<u8>,
            ::std::vec::Vec<u8>,
            ::std::vec::Vec<u8>,
            ::std::vec::Vec<u8>,
            ::std::vec::Vec<u8>,
            ::std::vec::Vec<u8>,
            ::std::vec::Vec<u8>,
            ::std::vec::Vec<u8>,
            ::std::vec::Vec<u8>,
            ::std::vec::Vec<u8>,
            ::std::vec::Vec<u8>,
            ::std::vec::Vec<u8>,
            ::std::vec::Vec<u8>,
            ::std::vec::Vec<u8>,
            ::std::vec::Vec<u8>,
            ::std::vec::Vec<u8>,
            ::std::vec::Vec<u8>,
            ::std::vec::Vec<u8>,
            ::std::vec::Vec<u8>,
            ::std::vec::Vec<u8>,
            ::std::vec::Vec<u8>,
            ::std::vec::Vec<u8>,
            ::std::vec::Vec<u8>,
        );

        // `$` in a large item: a module with a macro and a repetition.
        #[announce]
        pub mod big_mod {
            macro_rules! sum {
                ($($x:expr),*) => {
                    0 $(+ $x)*
                };
            }
            pub fn six() -> i32 {
                sum!(1, 2, 3)
            }
            pub fn f0() -> u32 {
                0
            }
            pub fn f1() -> u32 {
                1
            }
            pub fn f2() -> u32 {
                2
            }
            pub fn f3() -> u32 {
                3
            }
            pub fn f4() -> u32 {
                4
            }
            pub fn f5() -> u32 {
                5
            }
            pub fn f6() -> u32 {
                6
            }
            pub fn f7() -> u32 {
                7
            }
            pub fn f8() -> u32 {
                8
            }
            pub fn f9() -> u32 {
                9
            }
            pub fn f10() -> u32 {
                10
            }
            pub fn f11() -> u32 {
                11
            }
            pub fn f12() -> u32 {
                12
            }
            pub fn f13() -> u32 {
                13
            }
            pub fn f14() -> u32 {
                14
            }
            pub fn f15() -> u32 {
                15
            }
            pub fn f16() -> u32 {
                16
            }
            pub fn f17() -> u32 {
                17
            }
            pub fn f18() -> u32 {
                18
            }
            pub fn f19() -> u32 {
                19
            }
            pub fn f20() -> u32 {
                20
            }
            pub fn f21() -> u32 {
                21
            }
            pub fn f22() -> u32 {
                22
            }
            pub fn f23() -> u32 {
                23
            }
            pub fn f24() -> u32 {
                24
            }
            pub fn f25() -> u32 {
                25
            }
            pub fn f26() -> u32 {
                26
            }
            pub fn f27() -> u32 {
                27
            }
            pub fn f28() -> u32 {
                28
            }
            pub fn f29() -> u32 {
                29
            }
        }

        #[announce]
        pub mod big_mod2 {
            macro_rules! prod {
                ($($x:expr),*) => {
                    1 $(* $x)*
                };
            }
            pub fn twenty_four() -> i32 {
                prod!(2, 3, 4)
            }
            pub fn f0() -> u32 {
                0
            }
            pub fn f1() -> u32 {
                1
            }
            pub fn f2() -> u32 {
                2
            }
            pub fn f3() -> u32 {
                3
            }
            pub fn f4() -> u32 {
                4
            }
            pub fn f5() -> u32 {
                5
            }
            pub fn f6() -> u32 {
                6
            }
            pub fn f7() -> u32 {
                7
            }
            pub fn f8() -> u32 {
                8
            }
            pub fn f9() -> u32 {
                9
            }
            pub fn f10() -> u32 {
                10
            }
            pub fn f11() -> u32 {
                11
            }
            pub fn f12() -> u32 {
                12
            }
            pub fn f13() -> u32 {
                13
            }
            pub fn f14() -> u32 {
                14
            }
            pub fn f15() -> u32 {
                15
            }
            pub fn f16() -> u32 {
                16
            }
            pub fn f17() -> u32 {
                17
            }
            pub fn f18() -> u32 {
                18
            }
            pub fn f19() -> u32 {
                19
            }
            pub fn f20() -> u32 {
                20
            }
            pub fn f21() -> u32 {
                21
            }
            pub fn f22() -> u32 {
                22
            }
            pub fn f23() -> u32 {
                23
            }
            pub fn f24() -> u32 {
                24
            }
            pub fn f25() -> u32 {
                25
            }
            pub fn f26() -> u32 {
                26
            }
            pub fn f27() -> u32 {
                27
            }
            pub fn f28() -> u32 {
                28
            }
            pub fn f29() -> u32 {
                29
            }
        }

        // For `signature_only!` on a large module.
        #[announce]
        pub mod big_sig_mod {
            pub fn plain() -> u32 {
                7
            }
            pub fn opaque() -> impl ::core::fmt::Display {
                8
            }
            pub fn f0() -> u32 {
                0
            }
            pub fn f1() -> u32 {
                1
            }
            pub fn f2() -> u32 {
                2
            }
            pub fn f3() -> u32 {
                3
            }
            pub fn f4() -> u32 {
                4
            }
            pub fn f5() -> u32 {
                5
            }
            pub fn f6() -> u32 {
                6
            }
            pub fn f7() -> u32 {
                7
            }
            pub fn f8() -> u32 {
                8
            }
            pub fn f9() -> u32 {
                9
            }
            pub fn f10() -> u32 {
                10
            }
            pub fn f11() -> u32 {
                11
            }
            pub fn f12() -> u32 {
                12
            }
            pub fn f13() -> u32 {
                13
            }
            pub fn f14() -> u32 {
                14
            }
            pub fn f15() -> u32 {
                15
            }
            pub fn f16() -> u32 {
                16
            }
            pub fn f17() -> u32 {
                17
            }
            pub fn f18() -> u32 {
                18
            }
            pub fn f19() -> u32 {
                19
            }
            pub fn f20() -> u32 {
                20
            }
            pub fn f21() -> u32 {
                21
            }
            pub fn f22() -> u32 {
                22
            }
            pub fn f23() -> u32 {
                23
            }
            pub fn f24() -> u32 {
                24
            }
            pub fn f25() -> u32 {
                25
            }
            pub fn f26() -> u32 {
                26
            }
            pub fn f27() -> u32 {
                27
            }
            pub fn f28() -> u32 {
                28
            }
            pub fn f29() -> u32 {
                29
            }
        }

        // The derive and attribute routes into a large trampoline: the derive
        // `BigParse` and the attribute `big_parse_attr` redirect to
        // `BigParseAnnounce!`.
        #[announce(macro_name = BigParseAnnounce)]
        pub trait BigParse {
            fn m0(&self, x: u32) -> u32;
            fn m1(&self, x: u32) -> u32;
            fn m2(&self, x: u32) -> u32;
            fn m3(&self, x: u32) -> u32;
            fn m4(&self, x: u32) -> u32;
            fn m5(&self, x: u32) -> u32;
            fn m6(&self, x: u32) -> u32;
            fn m7(&self, x: u32) -> u32;
            fn m8(&self, x: u32) -> u32;
            fn m9(&self, x: u32) -> u32;
            fn m10(&self, x: u32) -> u32;
            fn m11(&self, x: u32) -> u32;
            fn m12(&self, x: u32) -> u32;
            fn m13(&self, x: u32) -> u32;
            fn m14(&self, x: u32) -> u32;
            fn m15(&self, x: u32) -> u32;
            fn m16(&self, x: u32) -> u32;
            fn m17(&self, x: u32) -> u32;
            fn m18(&self, x: u32) -> u32;
            fn m19(&self, x: u32) -> u32;
            fn m20(&self, x: u32) -> u32;
            fn m21(&self, x: u32) -> u32;
            fn m22(&self, x: u32) -> u32;
            fn m23(&self, x: u32) -> u32;
            fn m24(&self, x: u32) -> u32;
            fn m25(&self, x: u32) -> u32;
            fn m26(&self, x: u32) -> u32;
            fn m27(&self, x: u32) -> u32;
            fn m28(&self, x: u32) -> u32;
            fn m29(&self, x: u32) -> u32;
        }
        pub use announce_test_derive::big_parse_attr;
        pub use announce_test_derive::BigParse;

        // A large item with a check and `meta`.
        #[announce(announce_test_derive::no_tuple(big), meta = my_lib::info(big))]
        pub struct BigChecked {
            pub f0: u32,
            pub f1: u32,
            pub f2: u32,
            pub f3: u32,
            pub f4: u32,
            pub f5: u32,
            pub f6: u32,
            pub f7: u32,
            pub f8: u32,
            pub f9: u32,
            pub f10: u32,
            pub f11: u32,
            pub f12: u32,
            pub f13: u32,
            pub f14: u32,
            pub f15: u32,
            pub f16: u32,
            pub f17: u32,
            pub f18: u32,
            pub f19: u32,
            pub f20: u32,
            pub f21: u32,
            pub f22: u32,
            pub f23: u32,
            pub f24: u32,
            pub f25: u32,
            pub f26: u32,
            pub f27: u32,
            pub f28: u32,
            pub f29: u32,
            pub f30: u32,
            pub f31: u32,
            pub f32: u32,
            pub f33: u32,
            pub f34: u32,
            pub f35: u32,
            pub f36: u32,
            pub f37: u32,
            pub f38: u32,
            pub f39: u32,
            pub f40: u32,
            pub f41: u32,
            pub f42: u32,
            pub f43: u32,
            pub f44: u32,
            pub f45: u32,
            pub f46: u32,
            pub f47: u32,
            pub f48: u32,
            pub f49: u32,
            pub f50: u32,
            pub f51: u32,
            pub f52: u32,
            pub f53: u32,
            pub f54: u32,
            pub f55: u32,
            pub f56: u32,
            pub f57: u32,
            pub f58: u32,
            pub f59: u32,
            pub f60: u32,
            pub f61: u32,
            pub f62: u32,
            pub f63: u32,
            pub f64: u32,
            pub f65: u32,
            pub f66: u32,
            pub f67: u32,
            pub f68: u32,
            pub f69: u32,
            pub f70: u32,
            pub f71: u32,
            pub f72: u32,
            pub f73: u32,
            pub f74: u32,
            pub f75: u32,
            pub f76: u32,
            pub f77: u32,
            pub f78: u32,
            pub f79: u32,
        }

        // The layout can be chosen: a small item held once ..
        #[announce(layout = single)]
        pub trait SmallSingle {
            fn f(&self) -> u8;
        }

        #[announce(layout = single)]
        pub struct SmallSingleStruct<T>(pub T, pub u8);

        #[announce(layout = single)]
        pub mod small_single_mod {
            macro_rules! sum {
                ($($x:expr),*) => {
                    0 $(+ $x)*
                };
            }
            pub fn six() -> i32 {
                sum!(1, 2, 3)
            }
        }

        // .. a small derive target held once ..
        #[announce(macro_name = SmallSingleParseAnnounce, layout = single)]
        pub trait SmallSingleParse {
            fn parse(s: &str) -> Self;
        }
        pub use announce_test_derive::SmallSingleParse;

        // .. and large items copied into each arm.
        #[announce(layout = direct)]
        pub trait BigDirect {
            fn m0(&self, x: u32) -> u32;
            fn m1(&self, x: u32) -> u32;
            fn m2(&self, x: u32) -> u32;
            fn m3(&self, x: u32) -> u32;
            fn m4(&self, x: u32) -> u32;
            fn m5(&self, x: u32) -> u32;
            fn m6(&self, x: u32) -> u32;
            fn m7(&self, x: u32) -> u32;
            fn m8(&self, x: u32) -> u32;
            fn m9(&self, x: u32) -> u32;
            fn m10(&self, x: u32) -> u32;
            fn m11(&self, x: u32) -> u32;
            fn m12(&self, x: u32) -> u32;
            fn m13(&self, x: u32) -> u32;
            fn m14(&self, x: u32) -> u32;
            fn m15(&self, x: u32) -> u32;
            fn m16(&self, x: u32) -> u32;
            fn m17(&self, x: u32) -> u32;
            fn m18(&self, x: u32) -> u32;
            fn m19(&self, x: u32) -> u32;
            fn m20(&self, x: u32) -> u32;
            fn m21(&self, x: u32) -> u32;
            fn m22(&self, x: u32) -> u32;
            fn m23(&self, x: u32) -> u32;
            fn m24(&self, x: u32) -> u32;
            fn m25(&self, x: u32) -> u32;
            fn m26(&self, x: u32) -> u32;
            fn m27(&self, x: u32) -> u32;
            fn m28(&self, x: u32) -> u32;
            fn m29(&self, x: u32) -> u32;
        }

        #[announce(layout = direct)]
        pub mod big_direct_mod {
            macro_rules! sum {
                ($($x:expr),*) => {
                    0 $(+ $x)*
                };
            }
            pub fn six() -> i32 {
                sum!(1, 2, 3)
            }
            pub fn f0() -> u32 {
                0
            }
            pub fn f1() -> u32 {
                1
            }
            pub fn f2() -> u32 {
                2
            }
            pub fn f3() -> u32 {
                3
            }
            pub fn f4() -> u32 {
                4
            }
            pub fn f5() -> u32 {
                5
            }
            pub fn f6() -> u32 {
                6
            }
            pub fn f7() -> u32 {
                7
            }
            pub fn f8() -> u32 {
                8
            }
            pub fn f9() -> u32 {
                9
            }
            pub fn f10() -> u32 {
                10
            }
            pub fn f11() -> u32 {
                11
            }
            pub fn f12() -> u32 {
                12
            }
            pub fn f13() -> u32 {
                13
            }
            pub fn f14() -> u32 {
                14
            }
            pub fn f15() -> u32 {
                15
            }
            pub fn f16() -> u32 {
                16
            }
            pub fn f17() -> u32 {
                17
            }
            pub fn f18() -> u32 {
                18
            }
            pub fn f19() -> u32 {
                19
            }
            pub fn f20() -> u32 {
                20
            }
            pub fn f21() -> u32 {
                21
            }
            pub fn f22() -> u32 {
                22
            }
            pub fn f23() -> u32 {
                23
            }
            pub fn f24() -> u32 {
                24
            }
            pub fn f25() -> u32 {
                25
            }
            pub fn f26() -> u32 {
                26
            }
            pub fn f27() -> u32 {
                27
            }
            pub fn f28() -> u32 {
                28
            }
            pub fn f29() -> u32 {
                29
            }
        }

        // An item with a derive of another trait goes through the runner, which holds the
        // option, and the item is emitted as written.
        #[announce(layout = single, meta = crate::info(runner))]
        #[derive(Debug)]
        pub struct SingleWithDerive {
            pub a: u8,
            #[cfg(any())]
            pub gone: u8,
        }
    }
    pub mod large_leak {
        //! type-leak with a large item: the `type_leak(..)` META travels in the single
        //! copy of META (see `large.rs`).

        use super::announce;

        ::announce::define_repeater!(pub Repeater);

        pub mod inner {
            #[derive(Debug, PartialEq)]
            pub struct Local(pub u8);
        }
        use inner::Local;

        #[announce(repeater = crate::large_leak::Repeater)]
        pub struct BigLeaky<T> {
            pub a: Local,
            pub b: Vec<T>,
            pub f0: u32,
            pub f1: u32,
            pub f2: u32,
            pub f3: u32,
            pub f4: u32,
            pub f5: u32,
            pub f6: u32,
            pub f7: u32,
            pub f8: u32,
            pub f9: u32,
            pub f10: u32,
            pub f11: u32,
            pub f12: u32,
            pub f13: u32,
            pub f14: u32,
            pub f15: u32,
            pub f16: u32,
            pub f17: u32,
            pub f18: u32,
            pub f19: u32,
            pub f20: u32,
            pub f21: u32,
            pub f22: u32,
            pub f23: u32,
            pub f24: u32,
            pub f25: u32,
            pub f26: u32,
            pub f27: u32,
            pub f28: u32,
            pub f29: u32,
            pub f30: u32,
            pub f31: u32,
            pub f32: u32,
            pub f33: u32,
            pub f34: u32,
            pub f35: u32,
            pub f36: u32,
            pub f37: u32,
            pub f38: u32,
            pub f39: u32,
            pub f40: u32,
            pub f41: u32,
            pub f42: u32,
            pub f43: u32,
            pub f44: u32,
            pub f45: u32,
            pub f46: u32,
            pub f47: u32,
            pub f48: u32,
            pub f49: u32,
            pub f50: u32,
            pub f51: u32,
            pub f52: u32,
            pub f53: u32,
            pub f54: u32,
            pub f55: u32,
            pub f56: u32,
            pub f57: u32,
            pub f58: u32,
            pub f59: u32,
            pub f60: u32,
            pub f61: u32,
            pub f62: u32,
            pub f63: u32,
            pub f64: u32,
            pub f65: u32,
            pub f66: u32,
            pub f67: u32,
            pub f68: u32,
            pub f69: u32,
            pub f70: u32,
            pub f71: u32,
            pub f72: u32,
            pub f73: u32,
            pub f74: u32,
            pub f75: u32,
            pub f76: u32,
            pub f77: u32,
            pub f78: u32,
            pub f79: u32,
        }

        #[announce(repeater = crate::large_leak::Repeater)]
        pub trait BigLeakTrait {
            fn make(&self) -> Local;
            fn m0(&self, x: u32) -> u32;
            fn m1(&self, x: u32) -> u32;
            fn m2(&self, x: u32) -> u32;
            fn m3(&self, x: u32) -> u32;
            fn m4(&self, x: u32) -> u32;
            fn m5(&self, x: u32) -> u32;
            fn m6(&self, x: u32) -> u32;
            fn m7(&self, x: u32) -> u32;
            fn m8(&self, x: u32) -> u32;
            fn m9(&self, x: u32) -> u32;
            fn m10(&self, x: u32) -> u32;
            fn m11(&self, x: u32) -> u32;
            fn m12(&self, x: u32) -> u32;
            fn m13(&self, x: u32) -> u32;
            fn m14(&self, x: u32) -> u32;
            fn m15(&self, x: u32) -> u32;
            fn m16(&self, x: u32) -> u32;
            fn m17(&self, x: u32) -> u32;
            fn m18(&self, x: u32) -> u32;
            fn m19(&self, x: u32) -> u32;
            fn m20(&self, x: u32) -> u32;
            fn m21(&self, x: u32) -> u32;
            fn m22(&self, x: u32) -> u32;
            fn m23(&self, x: u32) -> u32;
            fn m24(&self, x: u32) -> u32;
            fn m25(&self, x: u32) -> u32;
            fn m26(&self, x: u32) -> u32;
            fn m27(&self, x: u32) -> u32;
            fn m28(&self, x: u32) -> u32;
            fn m29(&self, x: u32) -> u32;
        }
    }
    pub mod merge {
        //! Items with several announcements: `#[announce]` attributes, next to derives of other
        //! traits, and macros that announce what they produce. Each of them must make one trampoline and one
        //! `Announce` impl, whose META holds the entries of all announcements
        //! (`tests/merge.rs` reads it). Nothing here may be an error.

        #![allow(dead_code)]

        use announce::announce;
        use announce_test_derive::{
            chain, emit_announce, passthrough, record_attrs, summary, tick,
        };

        // ---- attribute and attribute -----------------------------------------------

        #[announce(meta = merge::a)]
        #[::announce::announce(meta = merge::b)]
        pub struct AttrAttr(pub u8);

        #[announce(meta = merge::a)]
        #[::announce::announce(meta = merge::b)]
        #[::announce::announce(meta = merge::c)]
        pub struct ThreeAttrs(pub u8);

        /// The attribute below is the bare `announce` (imported, so a macro).
        #[::announce::announce(meta = merge::a)]
        #[announce(meta = merge::b)]
        pub struct BareBelow(pub u8);

        #[announce]
        #[::announce::announce]
        pub struct NoOptions(pub u8);

        #[announce(meta = merge::a)]
        #[::announce::announce(meta = merge::b)]
        pub enum AttrAttrEnum {
            A,
            B(u8),
        }

        #[announce(meta = merge::a)]
        #[::announce::announce(meta = merge::b)]
        pub trait AttrAttrTrait {
            fn f(&self) -> u8;
        }

        /// Options that differ but do not conflict are all kept. Both announcements name the
        /// trampoline `Renamed` (it is one trampoline, so one name).
        #[announce(crate = ::announce, macro_name = Renamed, allow_std_prelude = true, meta = merge::a)]
        #[::announce::announce(macro_name = Renamed, layout = single, docs = true, meta = merge::b)]
        pub struct DifferentOptions(pub String);

        // ---- attributes around a derive of another trait ---------------------------

        #[::announce::announce(meta = merge::a)]
        #[derive(Debug)]
        #[announce(meta = merge::b)]
        pub struct AttrAboveDerive(pub u8);

        #[::announce::announce(meta = merge::a)]
        #[derive(Debug)]
        pub struct AttrAboveDeriveAlone(pub u8);

        #[::announce::announce(meta = merge::a)]
        #[::announce::announce(meta = merge::b)]
        #[derive(Debug)]
        #[announce(meta = merge::c)]
        pub struct TwoAttrsAboveDerive(pub u8);

        /// The derive expands first, and the attributes below it announce.
        #[derive(Debug)]
        #[announce(meta = merge::b)]
        #[::announce::announce(meta = merge::a)]
        pub struct AttrBelowDerive(pub u8);

        #[derive(Debug)]
        #[::announce::announce(meta = merge::a)]
        pub struct AttrBelowDeriveAlone(pub u8);

        /// The same, with the attributes among other derives.
        #[derive(Clone)]
        #[announce(meta = merge::b)]
        #[derive(Debug)]
        #[::announce::announce(meta = merge::a)]
        #[derive(PartialEq)]
        pub struct AttrBelowDeriveAmongOthers(pub u8);

        // ---- macros that announce what they produce --------------------------------

        #[emit_announce(meta = merge::a)]
        #[announce(meta = merge::b)]
        pub struct EmitIntoAttr(pub u8);

        #[emit_announce(meta = merge::a)]
        #[derive(Debug)]
        #[announce(meta = merge::b)]
        pub struct EmitAroundDerive(pub u8);

        /// `emit_announce` expands first, and its announcement puts the runner below the second
        /// `emit_announce` (which announces as well, into the runner).
        #[emit_announce(meta = merge::a)]
        #[emit_announce(meta = merge::b)]
        pub struct BothDefer(pub u8);

        /// An announcement whose runner waits for a macro that does not announce.
        #[emit_announce(meta = merge::a)]
        #[derive(Debug)]
        #[passthrough]
        pub struct DeferOnce(pub u8);

        /// An unknown attribute macro sits between two announcements, and announces itself.
        #[announce(meta = merge::one)]
        #[emit_announce(meta = merge::two)]
        #[::announce::announce(meta = merge::three)]
        pub struct Between(pub u8);

        /// Five macros that announce one after the other.
        #[chain(5)]
        pub struct ChainFive(pub u8);

        #[chain(3)]
        #[announce(meta = merge::a)]
        #[passthrough]
        pub struct ChainThenAttr(pub u8);

        // ---- checks run once each --------------------------------------------------

        /// Both announcements list `tick(shared)`: an exact duplicate, run once. `tick`
        /// leaves a constant, so a second run would define it twice.
        #[announce(tick(left), tick(shared), meta = merge::a)]
        #[::announce::announce(tick(shared), tick(right), meta = merge::b)]
        pub struct Ticked(pub u8);

        /// The query for the owned attributes comes before the announcement can move.
        #[emit_announce(tick(waiting))]
        #[passthrough]
        pub struct TickedWaiting(pub u8);

        #[emit_announce(tick(first))]
        #[emit_announce(tick(second))]
        pub struct TickedBothDefer(pub u8);

        #[announce(tick(first))]
        #[derive(Debug)]
        #[::announce::announce(tick(second))]
        pub struct TickedAroundDerive(pub u8);

        /// An attribute that a check owns stays below `#[announce]`: the runner is below
        /// `passthrough`, the owned attribute follows the runner, and the runner removes it.
        #[emit_announce(record_attrs)]
        #[my_lib(on_the_item)]
        #[passthrough]
        pub struct OwnedOnItem(pub u8);

        // ---- items that are not `pub` ----------------------------------------------

        mod hidden {
            use super::*;

            #[announce(meta = merge::a)]
            #[::announce::announce(meta = merge::b)]
            struct Private(pub u8);

            #[announce(meta = merge::a)]
            #[::announce::announce(meta = merge::b)]
            pub(crate) struct Crate(pub u8);

            #[emit_announce(meta = merge::a)]
            #[emit_announce(meta = merge::b)]
            pub(super) struct Super(pub u8);

            #[::announce::announce(meta = merge::a)]
            #[derive(Debug)]
            #[announce(meta = merge::b)]
            pub(in crate::merge) struct InPath(pub u8);

            Private! { [Private!, summary!] PRIVATE_ROWS }
            Crate! { [Crate!, summary!] CRATE_ROWS }
            Super! { [Super!, summary!] SUPER_ROWS }
            InPath! { [InPath!, summary!] IN_PATH_ROWS }
            pub(crate) fn rows() -> [&'static [(&'static str, &'static str, &'static str)]; 4] {
                [PRIVATE_ROWS, CRATE_ROWS, SUPER_ROWS, IN_PATH_ROWS]
            }
        }

        /// META of the items above that are not `pub`, one slice for each.
        pub fn hidden_rows() -> [&'static [(&'static str, &'static str, &'static str)]; 4] {
            hidden::rows()
        }
    }
    pub mod owned {
        //! Items with attributes of their own (`#[my_lib(..)]`, `#[other(..)]`, ..),
        //! which the checks of `#[announce]` own: they are removed from the emitted item
        //! (by the runner, or by the announcement when nothing can expand after it) and kept in
        //! the payload.

        use announce::announce;

        /// A check written with `macro_rules!`: it forwards the attribute query and adds
        /// `rules` (the one arm every check needs), and does nothing else.
        #[macro_export]
        macro_rules! rules_check {
            (@announce_format "1" @__announce_attrs [ $($hp:ident)? $(:: $hs:ident)* ! , $($np:ident)? $(:: $ns:ident)* ! $(, $($rest:tt)*)? ] { $($state:tt)* }) => {
                $($np)? $(:: $ns)* ! { @announce_format "1" @__announce_attrs [ $($np)? $(:: $ns)* ! $(, $($rest)*)? ] { $($state)* attrs(rules) } }
            };
            (@announce_format "1" ($($args:tt)*), $($item:tt)*) => {};
        }

        /// A `macro_rules!` check that owns nothing: it forwards the state unchanged.
        #[macro_export]
        macro_rules! plain_rules_check {
            (@announce_format "1" @__announce_attrs [ $($hp:ident)? $(:: $hs:ident)* ! , $($np:ident)? $(:: $ns:ident)* ! $(, $($rest:tt)*)? ] { $($state:tt)* }) => {
                $($np)? $(:: $ns)* ! { @announce_format "1" @__announce_attrs [ $($np)? $(:: $ns)* ! $(, $($rest)*)? ] { $($state)* } }
            };
            (@announce_format "1" ($($args:tt)*), $($item:tt)*) => {};
        }

        /// A `macro_rules!` check that does not write `@announce_format "1"` when it
        /// forwards the query: the next element is `finish!` (the check must be the last
        /// one), which reads a query with or without the prefix. Its patterns take the
        /// prefix as optional, so it also answers a query by hand.
        #[macro_export]
        macro_rules! unprefixed_rules_check {
            ($(@announce_format "1")? @__announce_attrs [ $($hp:ident)? $(:: $hs:ident)* ! , $($np:ident)? $(:: $ns:ident)* ! $(, $($rest:tt)*)? ] { $($state:tt)* }) => {
                $($np)? $(:: $ns)* ! { @__announce_attrs [ $($np)? $(:: $ns)* ! $(, $($rest)*)? ] { $($state)* attrs(unprefixed) } }
            };
            ($(@announce_format "1")? ($($args:tt)*), $($item:tt)*) => {};
        }

        /// A `macro_rules!` check that asserts, with its pattern, that the list of the
        /// attribute query is headed by the check itself: `crate::head_check!`, as written
        /// in `#[announce(..)]`. A list that starts with anything else matches no arm.
        #[macro_export]
        // `crate::head_check` in the pattern is the path the item below writes, on purpose.
        #[allow(clippy::crate_in_macro_def)]
        macro_rules! head_check {
            (@announce_format "1" @__announce_attrs [ crate::head_check ! , $($np:ident)? $(:: $ns:ident)* ! $(, $($rest:tt)*)? ] { $($state:tt)* }) => {
                $($np)? $(:: $ns)* ! { @announce_format "1" @__announce_attrs [ $($np)? $(:: $ns)* ! $(, $($rest)*)? ] { $($state)* attrs(head_seen) } }
            };
            (@announce_format "1" ($($args:tt)*), $($item:tt)*) => {};
        }

        /// Attributes on every kind of position of a struct. The definer is not built with `cfg(test)`.
        #[announce(announce_test_derive::record_attrs)]
        #[derive(Clone, Debug, PartialEq)]
        pub struct Fields {
            #[my_lib(skip)]
            pub a: u8,
            pub b: u8,
            #[my_lib::tag = "x"]
            #[doc = "a documented field"]
            pub c: u8,
            // `cfg_attr` is evaluated first: only the one that holds is seen.
            #[cfg_attr(not(test), my_lib(from_cfg_attr))]
            pub d: u8,
            #[cfg_attr(test, my_lib(from_cfg_attr_x))]
            pub e: u8,
            #[cfg(test)]
            #[my_lib(only_with_x)]
            pub f: u8,
        }

        /// On the item, its variants and their fields.
        #[announce(announce_test_derive::record_attrs)]
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

        /// On a trait, its generics, its items, a parameter and a statement.
        #[announce(announce_test_derive::record_attrs)]
        #[my_lib(on_trait)]
        pub trait Tr<#[my_lib(generic)] T> {
            #[my_lib(assoc_type)]
            type A;
            #[my_lib(assoc_const)]
            const C: u8;
            #[my_lib(method)]
            #[allow(clippy::let_and_return)]
            fn f(&self, #[my_lib(param)] _x: T) -> u8 {
                #[my_lib(stmt)]
                let y = 1;
                y
            }
        }

        /// On the inside of an inline `mod`, also as an inner attribute.
        #[announce(announce_test_derive::record_attrs)]
        pub mod inline {
            #![my_lib(inner_attr)]
            #[my_lib(in_mod)]
            pub struct Z {
                #[my_lib(field)]
                pub z: u8,
            }
            pub fn three() -> u8 {
                3
            }
        }

        /// Three checks that own different attributes: a proc-macro check, a second one
        /// and a `macro_rules!` check.
        #[announce(
            announce_test_derive::lib_check,
            announce_test_derive::other_check,
            crate::rules_check
        )]
        pub struct Two {
            #[my_lib(a)]
            pub a: u8,
            #[other(b)]
            pub b: u8,
            #[tag]
            pub t: u8,
            #[rules(c)]
            pub c: u8,
        }

        /// A check that owns nothing, among checks that do: the answers are merged.
        #[announce(
            crate::plain_rules_check,
            announce_test_derive::no_tuple,
            announce_test_derive::lib_check
        )]
        pub struct Mixed {
            #[my_lib(m)]
            pub m: u8,
        }

        /// The same check twice: the names are listed once.
        #[announce(announce_test_derive::lib_check, announce_test_derive::lib_check)]
        pub struct Twice {
            #[my_lib(t)]
            pub t: u8,
        }

        /// Every check finds itself at the head of the list: a proc-macro check that reads
        /// the query by hand (twice, once with an absolute path), a `macro_rules!` check
        /// that matches its own path in the pattern, and one that skips its head.
        #[announce(
            announce_test_derive::head_probe,
            crate::head_check,
            ::announce_test_derive::head_probe,
            crate::rules_check
        )]
        pub struct Heads {
            #[probed(a)]
            pub a: u8,
            #[head_seen(b)]
            pub b: u8,
            #[rules(c)]
            pub c: u8,
        }

        /// Checks that own nothing at all.
        #[announce(announce_test_derive::no_tuple, crate::plain_rules_check)]
        pub struct Nothing {
            pub n: u8,
        }

        /// Both layouts, with the item held once or copied.
        #[announce(layout = single, announce_test_derive::record_attrs)]
        pub struct SingleOwned {
            #[my_lib(s)]
            pub s: u8,
        }

        #[announce(layout = direct, announce_test_derive::record_attrs)]
        pub struct DirectOwned {
            #[my_lib(d)]
            pub d: u8,
        }

        macro_rules! big_owned {
            ($($f:ident),*) => {
                /// More than 300 tokens, so `layout = auto` holds it once.
                #[announce(announce_test_derive::record_attrs)]
                pub struct BigOwned {
                    $(
                        #[my_lib(big)]
                        pub $f: u8,
                    )*
                }
            };
        }
        big_owned!(
            f00, f01, f02, f03, f04, f05, f06, f07, f08, f09, f10, f11, f12, f13, f14, f15, f16,
            f17, f18, f19, f20, f21, f22, f23, f24, f25, f26, f27, f28, f29, f30, f31, f32, f33,
            f34, f35, f36, f37, f38, f39
        );

        /// `$` in the item, with attributes on and around the macro.
        #[announce(announce_test_derive::record_attrs)]
        pub mod with_dollar {
            #[my_lib(on_macro)]
            macro_rules! k {
                ($($x:expr),*) => { 0 $(+ $x)* };
            }
            #[my_lib(on_fn)]
            pub fn six() -> i32 {
                k!(1, 2, 3)
            }
        }

        /// A mock: its hidden copy is compiled too, and must not have the attributes.
        pub mod mocks {
            use super::announce;

            pub trait Real {
                fn f(&self) -> u8;
            }

            #[announce(mock = crate::owned::mocks::Real, announce_test_derive::record_attrs)]
            pub trait Mocked {
                #[my_lib(on_method)]
                fn f(&self) -> u8;
            }
        }

        /// The item is not emitted (`emit_item = false`): the attributes are only in the payload.
        #[announce(emit_item = false, announce_test_derive::record_attrs)]
        pub mod NotEmitted {
            #[my_lib(never_compiled)]
            pub struct NotEmitted {
                #[my_lib(field)]
                pub a: u8,
            }
        }

        /// An item that is `cfg`'d out is gone with its attributes.
        #[cfg(test)]
        #[announce(announce_test_derive::record_attrs)]
        pub struct OnlyWithX {
            #[my_lib(x)]
            pub a: u8,
        }

        /// A name that announce renames, with an owned attribute.
        #[announce(macro_name = RenamedOwned, announce_test_derive::record_attrs)]
        pub struct Renamed {
            #[my_lib(r)]
            pub r: u8,
        }

        /// `$` in the item, through a `macro_rules!` check.
        #[announce(crate::rules_check)]
        pub mod with_dollar_rules {
            #[rules(on_macro)]
            macro_rules! k {
                ($($x:expr),*) => { 1 $(* $x)* };
            }
            pub fn twenty_four() -> i32 {
                k!(2, 3, 4)
            }
        }

        /// A check that owns `rules`, on an item with a derive of another trait: the runner
        /// makes the trampoline, and the callbacks see `attrs(rules)`.
        #[announce(crate::rules_check)]
        #[derive(Debug)]
        pub struct OwnedWithDerive {
            #[rules(on_field)]
            pub a: u8,
        }

        #[doc(hidden)]
        pub use announce as __announce;

        /// A macro that defines announced items in the caller's crate, as a library would:
        /// `crate = $crate::__announce` and a check path with `$crate`.
        #[macro_export]
        macro_rules! make_owned {
            ($name:ident) => {
                #[$crate::owned::__announce::announce(crate = $crate::owned::__announce, $crate::plain_rules_check, $crate::rules_check, announce_test_derive::no_tuple)]
                pub struct $name {
                    #[rules(made)]
                    pub x: u8,
                }
            };
        }

        /// The last check forwards the query without the prefix: `finish!` takes it.
        #[announce(crate::unprefixed_rules_check)]
        pub struct Unprefixed {
            #[unprefixed(x)]
            pub a: u8,
        }
    }
    pub mod paths {
        //! The runner is told by its name (`__announce_runner`), never by the path `announce`.
        //! `tests/paths.rs` reads the META of the trampolines made here. Nothing here
        //! may be an error: a second trampoline or `Announce` impl would not compile, and a
        //! check that ran twice for one item would define its constant twice.
        //!
        //! - other crates' attributes that are called `announce` are not announcements, and keep
        //!   working next to ours;
        //! - a renamed import (`use announce::announce as ann`) is an announcement;
        //! - derives of other traits, above and below the attributes;
        //! - checks that own attributes, next to other announcements, asked once;
        //! - items that are not emitted (`emit_item = false`, `mock`);
        //! - macros that announce what they produce.

        #![allow(dead_code)]

        use announce_test_derive::{
            count_query, emit_announce, lib_check, other_check, passthrough, record_attrs, tick,
        };

        // ---- attributes of other crates that are named `announce` -------------------

        /// The attribute of another crate has the path `announce_test_derive::announce`, which
        /// ends in `announce`: `#[announce]` leaves it alone, and it makes its constant.
        #[::announce::announce(meta = paths::a)]
        #[announce_test_derive::announce(level = 2)]
        pub struct ForeignBelow(pub u8);

        #[announce_test_derive::announce(level = 3)]
        #[::announce::announce(meta = paths::a)]
        pub struct ForeignAbove(pub u8);

        /// Our two announcements, with the foreign one between them.
        #[::announce::announce(meta = paths::a)]
        #[announce_test_derive::announce(level = 4)]
        #[::announce::announce(meta = paths::b)]
        pub struct ForeignBetween(pub u8);

        /// Two foreign attributes and a doc attribute (the docs are an attribute as well).
        /// Ours is the only announcement.
        #[announce_test_derive::announce(level = 5)]
        #[::announce::announce(meta = paths::a)]
        #[announce_test_derive::announce(level = 6)]
        pub struct ForeignTwice(pub u8);

        /// An attribute with options `#[announce]` does not know, in front of ours: if it were
        /// merged into the foreign one it would be an error in the foreign macro.
        #[announce_test_derive::announce(level = 7)]
        #[::announce::announce(layout = direct, meta = paths::a)]
        #[announce_test_derive::announce(level = 8)]
        pub struct ForeignWithOptions(pub u8);

        /// The foreign attribute imported, and written as the bare `#[announce(level = N)]`,
        /// `#[announce]` or `#[announce(meta = ..)]`.
        pub mod bare {
            use announce_test_derive::announce;

            #[::announce::announce(meta = paths::a)]
            #[announce(level = 9)]
            pub struct ForeignBare(pub u8);

            /// A bare `#[announce]` with no arguments, and one with arguments that are options of
            /// ours (`meta = ..`): rustc expands the macro that the name means here, which is
            /// the foreign one. `#[announce]` neither takes it over nor deletes it.
            #[::announce::announce(meta = paths::a)]
            #[announce]
            pub struct ForeignBareNoArgs(pub u8);

            #[::announce::announce(meta = paths::a)]
            #[announce(meta = paths::foreign)]
            pub struct ForeignBareMeta(pub u8);

            #[announce]
            #[::announce::announce(meta = paths::a)]
            pub struct ForeignBareNoArgsAbove(pub u8);

            #[::announce::announce(meta = paths::a)]
            #[announce]
            #[::announce::announce(meta = paths::b)]
            pub struct ForeignBareNoArgsBetween(pub u8);

            #[announce(level = 10)]
            #[::announce::announce(meta = paths::a)]
            pub struct ForeignBareAbove(pub u8);

            #[::announce::announce(meta = paths::a)]
            #[announce(level = 11)]
            #[::announce::announce(meta = paths::b)]
            pub struct ForeignBareBetween(pub u8);
        }

        // ---- a renamed import is an announcement ------------------------------------

        pub mod renamed {
            use ::announce::announce as ann;

            #[ann]
            #[ann(meta = paths::b)]
            pub struct Twice(pub u8);

            #[ann(meta = paths::a)]
            #[ann(meta = paths::b)]
            #[ann(meta = paths::c)]
            pub struct Thrice(pub u8);

            /// With the attribute under its own name as well, and an attribute of another
            /// crate called `announce`.
            #[ann(meta = paths::a)]
            #[::announce::announce(meta = paths::b)]
            #[announce_test_derive::announce(level = 12)]
            pub struct Mixed(pub u8);
        }

        // ---- derives of other traits --------------------------------------------------
        //
        // The first attribute that expands puts the runner below the derives, and the others
        // are merged into it, wherever they stand.

        pub mod derives {
            // Imported, so that the bare `#[announce(..)]` is the attribute macro.
            #[allow(unused_imports)]
            use announce::announce;

            /// The attribute is above the derive: it puts the runner on the item, below the derive.
            #[::announce::announce(meta = paths::a)]
            #[derive(Debug)]
            pub struct AttrAbove(pub u8);

            #[::announce::announce(meta = paths::a)]
            #[derive(Debug)]
            #[announce(meta = paths::b)]
            pub struct AttrAboveBare(pub u8);

            /// The derive is above the attribute.
            #[derive(Debug)]
            #[::announce::announce(meta = paths::a)]
            pub struct DeriveAbove(pub u8);

            #[derive(Debug)]
            #[announce(meta = paths::b)]
            #[::announce::announce(meta = paths::a)]
            pub struct DeriveAboveBare(pub u8);

            /// Two bare attributes below the derive, and an attribute above it.
            #[::announce::announce(meta = paths::a)]
            #[derive(Debug)]
            #[announce(meta = paths::b1)]
            #[announce(meta = paths::b2)]
            pub struct AttrAboveTwoBare(pub u8);

            /// Two attributes and a derive, in the three places.
            #[::announce::announce(meta = paths::a)]
            #[::announce::announce(meta = paths::b)]
            #[derive(Debug)]
            #[announce(meta = paths::c)]
            pub struct TwoAttrsAbove(pub u8);

            #[derive(Debug)]
            #[announce(meta = paths::c)]
            #[::announce::announce(meta = paths::a)]
            #[::announce::announce(meta = paths::b)]
            pub struct TwoAttrsBelow(pub u8);

            #[::announce::announce(meta = paths::a)]
            #[derive(Debug)]
            #[announce(meta = paths::c)]
            #[::announce::announce(meta = paths::b)]
            pub struct DeriveBetween(pub u8);

            /// Several derives next to ours.
            #[::announce::announce(meta = paths::a)]
            #[derive(Clone, Debug)]
            pub struct OtherDerives(pub u8);
        }

        // ---- checks that own attributes ---------------------------------------------

        /// `record_attrs` owns `my_lib`, which stands between two announcements. The other
        /// announcement has a check as well, `tick`, that leaves a constant.
        #[::announce::announce(record_attrs, meta = paths::a)]
        #[my_lib(on_item)]
        #[::announce::announce(tick(second), meta = paths::b)]
        pub struct OwnedAndAnother(pub u8);

        #[::announce::announce(tick(first), meta = paths::a)]
        #[::announce::announce(record_attrs, meta = paths::b)]
        #[my_lib(on_item)]
        pub struct AnotherAndOwned(pub u8);

        /// Two checks that own different attributes. Neither stands above the announcement it
        /// belongs to when the item is compiled.
        #[::announce::announce(lib_check, meta = paths::a)]
        #[::announce::announce(other_check, meta = paths::b)]
        #[my_lib(one)]
        #[other(two)]
        #[tag(three)]
        pub struct TwoOwners(pub u8);

        /// An unknown macro between, and a macro that announces with a check of its own.
        #[::announce::announce(record_attrs, meta = paths::a)]
        #[passthrough]
        #[emit_announce(tick(emitted))]
        #[my_lib(on_item)]
        pub struct OwnedAndEmitted(pub u8);

        /// The query is sent once for each announcement. `count_query` owns the name
        /// `queried_<n>`, where `n` counts the queries it has answered for the item: `queried_0`
        /// is the first and only query of an announcement.
        #[::announce::announce(count_query)]
        #[passthrough]
        pub struct QueriedOnce(pub u8);

        #[::announce::announce(count_query)]
        pub struct QueriedOnceNoAttributes(pub u8);

        /// Two announcements, two queries: `queried_0` and `queried_1`, and no more.
        #[::announce::announce(count_query)]
        #[::announce::announce(count_query, meta = paths::b)]
        #[passthrough]
        pub struct QueriedTwice(pub u8);

        #[::announce::announce(count_query)]
        #[derive(Debug)]
        #[::announce::announce(meta = paths::b)]
        pub struct QueriedWithDerive(pub u8);

        #[emit_announce(count_query)]
        #[passthrough]
        #[emit_announce(count_query, meta = paths::b)]
        pub struct QueriedEmitted(pub u8);

        #[::announce::announce(count_query, record_attrs)]
        #[my_lib(on_item)]
        #[passthrough]
        #[::announce::announce(tick(shared), meta = paths::b)]
        pub struct QueriedAndOwned(pub u8);

        // ---- items that are not emitted ---------------------------------------------

        /// The item is not emitted: one trampoline, and no item. The runner drops it, so the
        /// macros below see it first (a derive that implements a trait for it fails: see the UI
        /// test `not_emitted_derive`).
        #[::announce::announce(emit_item = false, meta = paths::a)]
        #[passthrough]
        pub struct NotEmittedPassthrough(pub u8);

        /// The announcements below are merged into the one that is not emitted, and the other
        /// way round, and the trampoline is named once.
        #[::announce::announce(emit_item = false, macro_name = NotEmittedBelow, meta = paths::a)]
        #[::announce::announce(meta = paths::b)]
        pub struct NotEmittedBelow(pub u8);

        /// `emit_item = false` is on a later announcement of a trait here. For a struct, enum or
        /// union without generics it has to be on the first (see the UI test `merge_not_first`).
        #[::announce::announce(meta = paths::b)]
        #[::announce::announce(emit_item = false, meta = paths::a)]
        pub trait NotEmittedAbove {}

        #[::announce::announce(meta = paths::a)]
        #[passthrough]
        #[::announce::announce(emit_item = false, meta = paths::b)]
        #[passthrough]
        pub trait NotEmittedBetween {}

        /// A mock is compiled in a hidden module, and the item is not emitted.
        pub trait Real {
            fn f(&self) -> u8;
        }

        #[::announce::announce(mock = crate::paths::Real, meta = paths::a)]
        #[::announce::announce(meta = paths::b)]
        pub trait MockedBelow {
            fn f(&self) -> u8;
        }

        #[::announce::announce(meta = paths::b)]
        #[::announce::announce(mock = crate::paths::Real, meta = paths::a)]
        pub trait MockedAbove {
            fn f(&self) -> u8;
        }

        /// Not emitted, with a check that owns an attribute.
        #[::announce::announce(emit_item = false, record_attrs)]
        #[my_lib(never_compiled)]
        pub struct NotEmittedOwned(pub u8);

        /// Another announcement with a check that owns an attribute, below the one that is not
        /// emitted.
        #[::announce::announce(emit_item = false, record_attrs)]
        #[::announce::announce(other_check, meta = paths::b)]
        #[my_lib(never_compiled)]
        #[other(never_compiled)]
        pub struct NotEmittedTwoOwners(pub u8);

        // ---- macros that announce what they produce ---------------------------------

        /// The user's announcement puts the runner on the item first; the macro below it announces
        /// into the item that has the runner already.
        #[::announce::announce(meta = paths::a)]
        #[emit_announce(meta = paths::b)]
        pub struct IntoRunner(pub u8);

        /// Two macros that do not know each other announce, with a macro that does not
        /// announce between them.
        #[emit_announce(meta = paths::a)]
        #[passthrough]
        #[emit_announce(meta = paths::b)]
        pub struct TwoUnknown(pub u8);

        #[::announce::announce(meta = paths::u)]
        #[emit_announce(meta = paths::a)]
        #[passthrough]
        #[emit_announce(meta = paths::b)]
        pub struct TwoUnknownAndUser(pub u8);

        #[emit_announce(meta = paths::a)]
        #[::announce::announce(meta = paths::u)]
        #[emit_announce(meta = paths::b)]
        #[passthrough]
        pub struct UserBetweenTwoUnknown(pub u8);
    }
    pub mod recorded {
        //! Items announced with `strict = false` whose types do not resolve outside their module, and
        //! that have no `repeater`: the definition records that instead of failing. A call in the module
        //! of the item passes (here; `tests/recorded.rs` uses what the calls make), a call
        //! anywhere else is a compile error (`tests/ui_same_context.rs`).

        use announce::announce;

        pub mod m {
            use super::announce;

            pub mod inner {
                pub struct Local(pub u8);
                pub struct Other(pub u8);
                pub struct Wrap<'a>(pub &'a u8);
                pub trait Base {}
            }
            use inner::{Base, Local, Other, Wrap};

            const LEN: usize = 4;

            // In front of the item: the names of a module do not depend on the order of its items.
            Holder! { [Holder!, ::announce::same_context!, ::announce_test_derive::resolved_copy!] HolderBefore }

            #[announce(strict = false)]
            pub struct Holder {
                pub a: Local,
                pub b: u8,
            }

            #[announce(strict = false)]
            pub struct Pair {
                pub a: Local,
                pub b: Other,
            }

            #[announce(strict = false)]
            pub trait Sub: Base {
                fn local(&self) -> u8;
            }

            /// Every kind of cause in one item.
            #[announce(strict = false)]
            pub trait Many: Base {
                const M: usize = LEN;
                fn f(&self, a: Wrap<'_>, b: Local, c: [u8; LEN]) -> Other;
            }

            /// Nothing to record.
            #[announce(strict = false)]
            pub struct Plain(pub u8);

            // The copies name `Local` and `Base` as written.
            Holder! { [Holder!, ::announce_test_derive::resolved_copy!] HolderCopy }
            Pair! { [Pair!, ::announce_test_derive::resolved_copy!] PairCopy }
            Sub! { [Sub!, ::announce_test_derive::resolved_copy!] SubCopy }
            Plain! { [Plain!, ::announce_test_derive::resolved_copy!] PlainCopy }

            /// A macro of the module that builds the chain: its names are those of the module.
            macro_rules! through_the_check {
                ($item:ident, $name:ident) => {
                    $item! { [$item!, ::announce::same_context!, ::announce_test_derive::resolved_copy!] $name }
                };
            }
            through_the_check!(Holder, HolderForwarded);

            // `announce::same_context!` in the middle of a chain passes the entries on, unchanged.
            Holder! {
                [
                    Holder!,
                    Pair!,
                    Sub!,
                    Many!,
                    Plain!,
                    ::announce::same_context!,
                    ::announce_test_derive::leak_failures!
                ] CONTEXT_ROWS
            }
            // The entries that a later callback gathers have not passed it.
            Holder! {
                [Holder!, ::announce::same_context!, ::announce_test_derive::gathered!] (Pair, Sub) GATHERED
            }

            /// The item of the entry, as text.
            macro_rules! item_text {
                (@announce_format "1" $( ($($o:tt)*) )? [$($l:tt)*] $($p:ident)? $(:: $s:ident)* ! => [$($m:tt)*] { $($item:tt)* } $($r:tt)*) => {
                    stringify!($($item)*)
                };
            }

            /// In an expression the check is a block, and the callback's expression is its value.
            pub fn context_in_expr() -> &'static str {
                Holder! { (expr) [Holder!, ::announce::same_context!, item_text!] }
            }

            macro_rules! method {
                (@announce_format "1" [$($l:tt)*] $($t:tt)*) => {
                    pub fn from_the_item(&self) -> u8 {
                        9
                    }
                };
            }

            /// The check of `same_context!` is an item, which an `impl` cannot hold: the call goes beside
            /// the impl, and the call in the impl has no check (`same_context` in `tests/ui_same_context.rs`).
            pub struct Methods;

            Holder! { [Holder!, ::announce::same_context!] }

            impl Methods {
                Holder! { [Holder!, method!] }
            }

            pub fn in_fn() -> u8 {
                Holder! { [Holder!, ::announce_test_derive::resolved_copy!] HolderInFn }
                let h = HolderInFn { a: Local(5), b: 6 };
                h.a.0 + h.b
            }

            /// A submodule that brings the module's names in scope.
            pub mod child {
                use super::*;

                Holder! { [Holder!, ::announce::same_context!, ::announce_test_derive::resolved_copy!] HolderInChild }
            }
        }

        /// An item announced in a function body, checked there and in a nested block.
        pub fn in_a_fn_body() -> u8 {
            struct Local(u8);

            #[allow(dead_code)]
            #[announce(strict = false)]
            struct Item {
                a: Local,
            }

            Item! { [Item!, ::announce::same_context!, ::announce_test_derive::resolved_copy!] Outer }
            let first = Outer { a: Local(1) }.a.0;
            let second = {
                Item! { [Item!, ::announce::same_context!, ::announce_test_derive::resolved_copy!] Inner }
                Inner { a: Local(2) }.a.0
            };
            first + second
        }
    }
    pub mod runner {
        //! Several announcements of one item: the first announcement puts the runner (the hidden
        //! attribute `__announce_runner`) last among the attributes of the item, the others are
        //! merged into it, and the runner makes one trampoline and one `Announce` impl when it
        //! expands (a second trampoline would not compile). The META of the trampolines is read by
        //! `tests/runner.rs`.
        //!
        //! - the attribute with its own name, and a renamed import of it, in every order and with
        //!   macros between;
        //! - derives of other traits, above and below the attribute, and the bare attribute;
        //! - checks that own attributes (on the item, in fields, in a `cfg_attr`), two
        //!   announcements with checks of their own, the query asked once for each;
        //! - the items that are not structs, and the other macros.
        #![allow(dead_code)]

        use announce::announce;
        use announce::announce as ann;
        use announce_test_derive::{
            count_query, emit_announce, lib_check, other_check, passthrough, record_attrs, tick,
        };

        // ---- the attribute, and a renamed import of it ----------------------------------

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

        /// Both names of the attribute in one item, and a derive of someone else.
        #[ann(meta = runner::a)]
        #[derive(Clone, Debug)]
        #[announce(meta = runner::b)]
        #[::announce::announce(meta = runner::c)]
        pub struct Names(pub u8);

        // ---- derives of other traits, and the bare attribute ----------------------------------

        /// The attribute above the derive: it puts the runner below the derive.
        #[ann(meta = runner::a)]
        #[derive(Debug)]
        pub struct AttrThenDerive(pub u8);

        #[ann(meta = runner::a)]
        #[derive(Debug)]
        #[announce(meta = runner::b)]
        pub struct AttrThenDeriveBare(pub u8);

        /// The derive above the attribute: the bare attribute expands first, and the attribute
        /// below it is merged into its runner.
        #[derive(Debug)]
        #[announce(meta = runner::b)]
        #[::announce::announce(meta = runner::a)]
        pub struct DeriveThenAttr(pub u8);

        /// Two attributes, the derive and two bare attributes.
        #[::announce::announce(meta = runner::a)]
        #[derive(Debug)]
        #[announce(meta = runner::b1)]
        #[announce(meta = runner::b2)]
        #[::announce::announce(meta = runner::c)]
        pub struct DeriveAmong(pub u8);

        /// A check on the attribute below the bare one, which owns an attribute of the item: the
        /// bare attribute expands first and puts the runner below the owned attribute.
        #[derive(Debug)]
        #[announce(meta = runner::b)]
        #[::announce::announce(lib_check)]
        #[my_lib(x)]
        pub struct BareThenCheck(pub u8);

        /// The check of both is the same: it is asked once for each announcement, and listed
        /// once.
        #[derive(Debug)]
        #[announce(lib_check)]
        #[::announce::announce(lib_check)]
        #[my_lib(x)]
        pub struct SameCheckTwice(pub u8);

        /// Two announcements with a check of their own: they are asked, each once.
        #[derive(Debug)]
        #[announce(other_check, tick(first))]
        #[::announce::announce(lib_check, tick(second))]
        #[my_lib(x)]
        pub struct TwoChecksAroundDerive(pub u8);

        // ---- checks that own attributes -----------------------------------------------------

        /// Two announcements with checks of their own, and attributes of both on the item, in the
        /// fields, and between the announcements.
        #[ann(lib_check, meta = runner::a)]
        #[my_lib(on_item)]
        #[ann(other_check, meta = runner::b)]
        #[other(on_item)]
        pub struct TwoChecks {
            #[my_lib(a)]
            pub a: u8,
            #[other(b)]
            pub b: u8,
            #[tag]
            pub t: u8,
        }

        /// An attribute that a check owns, below a macro that returns its input: the runner is
        /// below the macro, the attribute follows the runner, and the runner removes it.
        #[::announce::announce(lib_check)]
        #[passthrough]
        #[my_lib(x)]
        pub struct OwnedAfterPass(pub u8);

        /// Two announcements with different checks, and their attributes mixed with other macros.
        #[::announce::announce(lib_check)]
        #[passthrough]
        #[my_lib(x)]
        #[ann(other_check)]
        #[passthrough]
        #[other(y)]
        #[tag]
        #[passthrough]
        pub struct OwnersMixed(pub u8);

        /// The names are in the runner, whatever the order of the announcements.
        #[ann(other_check, meta = runner::b)]
        #[ann(lib_check, meta = runner::a)]
        #[my_lib(on_item)]
        #[other(on_item)]
        pub enum TwoChecksEnum {
            #[my_lib(variant)]
            A,
            B(#[other(field)] u8),
        }

        /// `record_attrs` records what it sees: the attributes with their `cfg_attr` evaluated, in
        /// source order.
        #[ann(record_attrs, meta = runner::a)]
        #[cfg_attr(not(test), my_lib(outer_cfg_attr))]
        #[ann(count_query)]
        pub struct RecordsCfgAttr {
            #[my_lib(first)]
            pub a: u8,
            #[cfg_attr(not(test), my_lib(from_cfg_attr))]
            pub b: u8,
            #[cfg_attr(test, my_lib(not_here))]
            pub c: u8,
            #[cfg(test)]
            #[my_lib(not_here_either)]
            pub d: u8,
        }

        /// The query is sent once for each announcement: `count_query` owns `queried_<n>` for
        /// the n-th query that it answers for an item.
        #[ann(count_query)]
        pub struct RunQueriedOnce(pub u8);

        #[ann(count_query)]
        #[passthrough]
        pub struct RunQueriedAfterPass(pub u8);

        #[ann(count_query)]
        #[ann(count_query, meta = runner::b)]
        #[passthrough]
        pub struct RunQueriedTwice(pub u8);

        #[ann(count_query, meta = runner::a)]
        #[derive(Debug)]
        #[announce(meta = runner::b)]
        pub struct RunQueriedWithDerive(pub u8);

        /// Each check runs once (`tick` leaves a constant: a second run would define it twice).
        #[ann(tick(left), tick(shared), meta = runner::a)]
        #[ann(tick(shared), tick(right), meta = runner::b)]
        pub struct Ticked(pub u8);

        // ---- the other items -----------------------------------------------------------------

        /// A trait is announced like a struct.
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

        /// An inline module, and a type alias.
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
    pub mod wire {
        //! The definitions behind the wire format example in the docs of
        //! `announce::helper::Input`. The test that captures the chain is
        //! `tests/wire.rs`.

        use announce::announce;

        ::announce::define_repeater!(pub Repeater);

        mod inner {
            pub struct Meters(pub f64);
        }
        use inner::Meters;

        /// A point.
        ///
        /// A check (that owns the attribute `my_lib`), a `meta = ..` entry and a doc
        /// comment: the docs are not in the payload, the owned attribute is.
        #[announce(announce_test_derive::lib_check, meta = crate::info(1))]
        pub struct Point {
            /// The x coordinate.
            #[my_lib(skip)]
            pub x: u8,
            pub y: u8,
        }

        /// A length: `Meters` is private to this crate, so the definition has a
        /// `repeater` and its META gets a `type_leak(..)` element.
        #[announce(repeater = crate::wire::Repeater)]
        pub struct Length(pub Meters);
    }

    pub mod kinds {
        use super::announce;

        #[announce]
        #[derive(Clone, Debug)]
        pub struct Named {
            pub a: u32,
            #[cfg(test)]
            pub only_with_x: u32,
            #[cfg(not(test))]
            pub only_without_x: u32,
            #[cfg_attr(not(test), doc = "cfg_attr evaluated in the definer")]
            pub c: u8,
        }

        #[announce]
        pub struct Tuple<T>(pub T, pub u8);

        #[announce]
        pub enum Shape {
            Circle(f64),
            Rect {
                w: f64,
                h: f64,
            },
            #[cfg(test)]
            OnlyWithX,
        }

        #[announce]
        #[derive(Clone, Copy)]
        pub union Bits {
            pub i: u32,
            pub f: f32,
        }

        #[announce]
        pub trait Greet {
            fn hi(&self) -> &'static str;
            fn twice(&self) -> [&'static str; 2] {
                [self.hi(), self.hi()]
            }
            #[cfg(test)]
            fn only_with_x(&self);
        }

        #[announce]
        pub type Alias = ::std::vec::Vec<u8>;

        /// A module whose body holds `$` and a repetition.
        #[announce]
        pub mod with_macro {
            macro_rules! sum {
                ($($x:expr),*) => { 0 $(+ $x)* };
            }
            pub fn six() -> i32 {
                sum!(1, 2, 3)
            }
        }

        /// A second module with `$` and a repetition (chains of two).
        #[announce]
        pub mod with_macro2 {
            macro_rules! prod {
                ($($x:expr),*) => { 1 $(* $x)* };
            }
            pub fn twenty_four() -> i32 {
                prod!(2, 3, 4)
            }
        }

        #[announce]
        pub trait Sig {
            fn required(&self) -> u32;
            fn body(&self) -> u32 {
                42
            }
        }

        /// For `signature_only!`: fn bodies are replaced, except when the fn
        /// returns `impl Trait`.
        #[announce]
        pub mod sig_mod {
            pub fn plain() -> u32 {
                7
            }
            pub fn opaque() -> impl ::core::fmt::Display {
                8
            }
        }

        #[announce]
        #[cfg(test)]
        pub struct GoneWithoutX;
    }

    #[allow(dead_code)]
    pub mod vis {
        use super::announce;

        #[announce]
        pub(crate) struct CrateOnly;

        #[announce]
        struct Private;

        pub mod inner {
            use crate::announce;
            #[announce]
            pub(super) enum SuperOnly {
                A,
            }
        }

        // Same-crate calls by relative, `self::`, `super::` and `crate::` paths.
        macro_rules! count {
            (@announce_format "1" [$($l:tt)*] $($t:tt)*) => {
                pub const SAME_CRATE: &str = stringify!($($t)*);
            };
        }
        pub mod a {
            crate::vis::CrateOnly! { [crate::vis::CrateOnly!, count!] }
        }
        pub mod b {
            super::Private! { [super::Private!, count!] }
        }
        pub mod c {
            super::inner::SuperOnly! { [super::inner::SuperOnly!, count!] }
        }
        pub mod d {
            crate::kinds::Greet! { [crate::kinds::Greet!, count!] }
        }
    }

    /// The same for trampolines that hold their item once: they call themselves
    /// through the path the caller wrote, which must resolve from the expansion too.
    #[allow(dead_code)]
    pub mod vis_single {
        use super::announce;

        #[announce(layout = single)]
        pub(crate) struct CrateOnly;

        #[announce(layout = single)]
        struct Private;

        pub mod inner {
            use crate::announce;
            #[announce(layout = single)]
            pub(super) enum SuperOnly {
                A,
            }
        }

        macro_rules! count {
            (@announce_format "1" [$($l:tt)*] $($t:tt)*) => {
                pub const SAME_CRATE: &str = stringify!($($t)*);
            };
        }
        macro_rules! count_bare {
            (@announce_format "1" [$($l:tt)*] $($t:tt)*) => {
                pub const BARE: &str = stringify!($($t)*);
            };
        }
        pub mod a {
            crate::vis_single::CrateOnly! { [crate::vis_single::CrateOnly!, count!] }
        }
        pub mod b {
            super::Private! { [super::Private!, count!] }
        }
        pub mod c {
            super::inner::SuperOnly! { [super::inner::SuperOnly!, count!] }
        }
        pub mod d {
            crate::large::BigTrait! { [crate::large::BigTrait!, count!] }
        }
        pub mod e {
            crate::large::big_mod! { [crate::large::big_mod!, count!] }
        }
        // By its bare name, in the module that defines it and through a `use`.
        Private! { [Private!, count_bare!] }
        pub mod f {
            use super::Private;
            Private! { [Private!, count!] }
        }
    }

    pub mod conflict {
        use super::announce;

        /// The macro namespace name `Parse` is taken by the derive below, so the
        /// trampoline is renamed.
        #[announce(macro_name = ParseAnnounce)]
        pub trait Parse {
            fn parse(s: &str) -> Self;
        }
        pub use announce_test_derive::Parse;

        pub use announce_test_derive::parse_attr;
    }

    pub mod checks {
        use super::announce;

        #[announce(announce_test_derive::no_tuple(named only))]
        pub struct Checked {
            pub a: u8,
        }

        /// A check on an item with a derive of another trait: the item goes through the
        /// runner, which runs after the derive, and stays as written.
        #[announce(announce_test_derive::no_tuple(runner), meta = crate::info(1))]
        #[derive(Debug)]
        pub struct CheckedWithDerive {
            pub a: u8,
            #[cfg(any())]
            pub gone: u8,
        }
    }

    pub mod markers {
        use super::announce;

        #[announce]
        pub trait Simple {
            fn f(&self) -> u8;
        }

        #[announce]
        pub trait WithAssoc {
            type Out;
        }

        /// An item with a derive of another trait, and options: the item is emitted as
        /// written, the derive has run and is not in the payload.
        #[announce(meta = crate::info(1))]
        #[derive(Debug)]
        pub struct WithDerive {
            pub a: u8,
            #[cfg(any())]
            pub gone: u8,
        }

        /// The same without options.
        #[announce]
        #[derive(Debug)]
        pub enum WithDeriveEnum {
            A,
            B(u8),
        }
    }

    /// A `$crate` path inside a continuation list, as a library's
    /// `macro_rules!` would write it.
    #[macro_export]
    macro_rules! via_dollar_crate {
        () => {
            $crate::kinds::Greet! { [$crate::kinds::Greet!, $crate::dollar_crate_cb!] }
        };
    }
    #[macro_export]
    #[doc(hidden)]
    macro_rules! dollar_crate_cb {
        (@announce_format "1" [$($l:tt)*] $($t:tt)*) => {
            pub const VIA_DOLLAR_CRATE: &str = stringify!($($t)*);
        };
    }

    /// The same for a large item, which calls itself through the head path
    /// (`$crate::..` tokens must survive that).
    #[macro_export]
    macro_rules! via_dollar_crate_large {
        () => {
            $crate::large::BigTrait! { [$crate::large::BigTrait!, $crate::dollar_crate_cb!] }
        };
    }
    #[macro_export]
    macro_rules! via_dollar_crate_large_mod {
        () => {
            $crate::large::big_mod! { [$crate::large::big_mod!, $crate::dollar_crate_cb!] }
        };
    }

    pub mod metas {
        use super::announce;

        #[announce(meta = my_lib::info(version = 3), meta = my_lib::flag)]
        pub struct WithMeta;
    }

    pub mod derived {
        #[derive(announce_test_derive::Announced)]
        pub struct ByDerive {
            pub field: u8,
        }
    }

    pub mod mocks {
        use super::announce;

        pub struct Local;

        /// A mock whose copy refers to a local type (resolved via `use super::*`).
        #[announce(mock = crate::mocks::RealTrait)]
        pub trait RealTrait {
            fn f(&self) -> crate::mocks::Local;
        }
    }

    /// Paths starting with `crate::` must keep pointing at this crate when the
    /// payload is re-emitted elsewhere.
    #[announce]
    pub mod uses_crate_paths {
        pub fn make() -> crate::kinds::Alias {
            ::std::vec![1, 2, 3]
        }
        pub(crate) fn _vis_untouched() {}
    }

    pub mod transformed {
        use super::announce;

        #[announce(meta = orig(1))]
        pub struct Partly {
            pub a: u8,
            #[cfg(test)]
            pub gone: u8,
            #[allow(dead_code)]
            hidden: u8,
            pub c: u8,
        }
    }

    /// Type-leak: `Local` resolves only in this crate; with `(type_leak)` the
    /// payload refers to it through `crate::leaky::Repeater`.
    pub mod leaky {
        use super::announce;

        ::announce::define_repeater!(pub Repeater);

        pub mod inner {
            #[derive(Debug, PartialEq)]
            pub struct Local(pub u8);
        }
        use inner::Local;

        #[announce(repeater = crate::leaky::Repeater)]
        pub struct Leaky<T> {
            pub a: Local,
            pub b: Vec<T>,
            pub c: u8,
        }

        #[announce(repeater = crate::leaky::Repeater)]
        pub trait LeakTrait {
            fn make(&self) -> Local;
        }

        #[announce]
        pub struct NoRepeater;
    }

    /// Regressions: `crate::` after a single `:` and `$crate` in a payload macro.
    pub mod crate_paths2 {
        use super::announce;
        pub struct Foo(pub u8);

        #[announce]
        pub struct Field {
            pub a: crate::crate_paths2::Foo,
        }

        #[announce]
        pub mod with_dollar_crate {
            #[macro_export]
            macro_rules! __announce_test_mk_foo {
                () => {
                    $crate::crate_paths2::Foo(9)
                };
            }
            pub fn make() -> crate::crate_paths2::Foo {
                crate::crate_paths2::Foo(9)
            }
        }
    }

    /// A constant of this crate, named by `crate::ARR` inside a re-emitted payload.
    pub const ARR: [u8; 3] = [1, 2, 3];

    /// The `crate` after the `in` of a `for` loop is an
    /// ordinary path; only the `in` of `pub(in ..)` is part of a visibility.
    #[announce]
    pub mod for_in_crate {
        pub fn sum() -> u32 {
            let mut s = 0;
            for x in crate::ARR {
                s += x as u32;
            }
            s
        }
        pub(crate) fn _vis() {}
    }

    pub mod allow_opts {
        use super::announce;

        /// `String` is a prelude name: allowed with `allow_std_prelude = true`.
        #[announce(allow_std_prelude = true)]
        pub struct WithString {
            pub s: String,
            pub n: u8,
        }
    }

    /// These used to break the definer's own build.
    pub mod audit_high {
        use super::announce;

        /// Lifetime-generic trait: no `Announce` impl (it would not compile).
        #[announce]
        pub trait Life<'a> {
            fn f(&self) -> &'a str;
        }

        pub mod own {
            /// A user trait that is merely *named* `Send`.
            pub trait Send {
                type X;
            }
        }

        #[announce]
        pub trait UsesOwnSend: crate::audit_high::own::Send {}

        /// Raw identifier.
        #[announce]
        #[allow(non_camel_case_types)]
        pub struct r#match;
    }

    pub mod trait_paths_ok {
        use super::announce;
        pub trait Base {}

        #[announce]
        pub trait Sub: crate::trait_paths_ok::Base + ::core::marker::Send {}

        #[announce]
        pub trait Show {
            fn show<T: ::core::fmt::Display>(&self, t: T);
        }

        /// Bare prelude traits are accepted with `allow_std_prelude = true`.
        #[announce(allow_std_prelude = true)]
        pub trait Cl: Clone {
            fn get<T: ?Sized + Send>(&self, t: &T);
        }

        /// A const generic array length: a const parameter resolves everywhere.
        #[announce]
        pub struct Arr<const N: usize>(pub [u8; N]);

        pub mod inner {
            pub struct Local;
        }
        use inner::Local;
        ::announce::define_repeater!(pub Repeater);

        #[announce(repeater = crate::trait_paths_ok::Repeater)]
        pub struct ArrLocal<const N: usize>(pub [Local; N]);
    }

    /// Repeater tables and the analysis.
    pub mod audit3 {
        use super::announce;

        ::announce::define_repeater!(pub Repeater);
        pub trait Base {}
        pub mod inner {
            pub struct LocalG<U>(pub U);
            pub struct LocalL<'a>(pub &'a u8);
            #[derive(Debug, PartialEq)]
            pub struct Local(pub u8);
            impl From<Local> for u8 {
                fn from(l: Local) -> u8 {
                    l.0
                }
            }
        }
        use inner::{Local, LocalG, LocalL};
        pub const N: usize = 3;

        /// #1: method-level generics and lifetimes in interned types.
        #[announce(repeater = crate::audit3::Repeater)]
        pub trait MethodGenerics {
            fn f<U>(&self, x: LocalG<U>) -> U;
            fn g<'a>(&'a self, x: &'a u8) -> LocalL<'a>;
        }

        /// #2: a `Self` where-clause and a bound mentioning an interned type.
        #[announce(repeater = crate::audit3::Repeater)]
        pub trait SelfWhere<T>
        where
            Self: crate::audit3::Base,
        {
            fn h(&self, t: T) -> Local;
        }
        #[announce(repeater = crate::audit3::Repeater)]
        pub struct Bnd<T: ::core::convert::From<Local>>(pub T, pub Local);

        /// #3: a default type parameter that is an interned type.
        #[announce(repeater = crate::audit3::Repeater)]
        pub struct Dflt<T = Local>(pub T);

        /// #4: higher-ranked bounds with absolute paths need no repeater.
        #[announce]
        pub struct Hrtb<F>
        where
            F: for<'b> ::core::ops::Fn(&'b u8) -> &'b u8,
        {
            pub f: F,
            pub g: ::std::boxed::Box<dyn for<'b> ::core::ops::Fn(&'b u8) -> &'b u8>,
        }

        /// #5: absolute value paths, `Self::..` and method const generics.
        #[announce]
        pub enum Discr {
            A = crate::audit3::N as isize,
        }
        #[announce]
        pub struct ArrAbs(pub [u8; crate::audit3::N]);
        #[announce]
        pub trait ConstDefault {
            const N: usize;
            const M: usize = Self::N * 2;
            fn f<const K: usize>(&self) -> [u8; K];
        }

        /// #6: prelude generic bounds and types with `allow_std_prelude`.
        #[announce(allow_std_prelude = true)]
        pub trait PreludeBounds {
            fn it<T: DoubleEndedIterator + AsRef<str>>(&self, t: T) -> Vec<Option<u8>>;
            fn fns<F: Fn(u8) -> u8>(&self, f: F) -> Box<dyn Iterator<Item = u8>>;
        }
    }

    /// `crate = path`: an explicit path to the announce crate in generated code.
    pub mod crate_opt {
        use super::announce;

        /// A stand-in facade: `Capture` and `remove` come from announce, but
        /// `Announce` is a trait of this module, so `impl Announce for ..` can only
        /// name it when the option is honoured.
        pub mod spy {
            pub use ::announce::__private;
            pub trait Announce {}
        }

        #[announce(crate = crate::crate_opt::spy)]
        pub struct ViaSpy {
            pub a: u8,
        }

        #[announce(crate = crate::crate_opt::spy)]
        pub trait SpyTrait {
            fn f(&self) -> u8;
        }

        #[announce(crate = ::announce)]
        pub struct Explicit {
            pub a: u8,
        }
    }

    /// With a `repeater` but only absolute types, META still has `type_leak(..)`
    /// with empty lists.
    pub mod empty_table {
        use super::announce;

        ::announce::define_repeater!(pub Repeater);

        #[announce(repeater = crate::empty_table::Repeater)]
        pub struct NoLeaks {
            pub a: u8,
            pub b: ::std::vec::Vec<u8>,
            pub c: ::core::option::Option<crate::empty_table::NoLeaks2>,
        }
        pub struct NoLeaks2;
    }

    /// Repeater tables keep the bounds and const generics of the item.
    pub mod table_bounds {
        use super::announce;

        ::announce::define_repeater!(pub Repeater);

        pub mod inner {
            #[derive(Debug, PartialEq, Clone)]
            pub struct Local(pub u8);
            /// Well-formed only for `T: Clone`.
            #[derive(Debug, PartialEq)]
            pub struct Wrap<T: ::core::clone::Clone>(pub T);
            #[derive(Debug, PartialEq)]
            pub struct Buf<const N: usize>(pub [u8; N]);
            #[derive(Debug, PartialEq)]
            pub struct Lt<'a>(pub &'a u8);
        }
        use inner::{Buf, Local, Lt, Wrap};

        #[announce(repeater = crate::table_bounds::Repeater)]
        pub struct Bounded<T: ::core::clone::Clone>(pub ::std::vec::Vec<T>, pub Wrap<T>, pub Local);

        #[announce(repeater = crate::table_bounds::Repeater)]
        pub struct ArrOfLocal<const N: usize>(pub [Local; N]);

        #[announce(repeater = crate::table_bounds::Repeater)]
        pub struct ConstArg<const N: usize>(pub Buf<N>);

        #[announce(repeater = crate::table_bounds::Repeater)]
        pub struct Both<'a, T: ::core::clone::Clone, const N: usize>(
            pub Lt<'a>,
            pub Wrap<T>,
            pub Buf<N>,
        );
    }

    /// `allow_std_prelude = true` with a repeater: prelude names stay, the
    /// relative types inside them are carried.
    pub mod prelude_leak {
        use super::announce;

        ::announce::define_repeater!(pub Repeater);
        pub mod inner {
            #[derive(Debug, PartialEq, Clone)]
            pub struct Local(pub u8);
        }
        use inner::Local;

        #[announce(allow_std_prelude = true, repeater = crate::prelude_leak::Repeater)]
        pub struct Holder {
            pub a: Vec<Local>,
            pub b: Option<Box<Local>>,
            pub s: String,
        }
    }

    /// `signature_only!` on a module with `const fn`, `impl Trait` and impl blocks.
    pub mod sig_full {
        use super::announce;

        #[announce]
        pub mod m {
            pub const fn zero() -> u32 {
                0
            }
            pub fn opaque() -> impl ::core::fmt::Display {
                8
            }
            pub fn plain() -> u32 {
                7
            }
            pub struct S;
            impl S {
                pub fn method(&self) -> u32 {
                    9
                }
                pub const fn cmethod() -> u32 {
                    11
                }
                pub fn opaque_method(&self) -> impl ::core::fmt::Debug {
                    12
                }
            }
            pub mod nested {
                pub fn deep() -> u32 {
                    13
                }
            }
            pub const ZERO: u32 = zero();
        }
    }

    /// The module-wrapper pattern of the `#[announce]` docs: `emit_item = false` on a
    /// same-named module next to the real item.
    pub mod wrapper {
        use super::announce;

        pub trait Base {}
        pub struct Local(pub u8);

        #[announce]
        pub trait Gadget {
            fn f(&self) -> u8;
        }

        /// Modules are not analysed: the relative `Base` and `Local` pass, and the
        /// copy is not emitted, so it does not clash with the trait `Gadget`.
        #[announce(emit_item = false, macro_name = GadgetWrapped, meta = crate::info(wrapped))]
        pub mod Gadget {
            pub trait Gadget: Base {
                fn f(&self) -> Local;
            }
        }

        /// A wrapper whose paths all resolve in other crates.
        #[announce(emit_item = false, macro_name = ResolvedWrapped)]
        pub mod Resolved {
            pub trait Resolved: crate::wrapper::Base {
                fn f(&self) -> crate::wrapper::Local;
            }
        }
    }

    /// Used across crates: a union with a derive of other traits.
    pub mod union_with_meta {
        #[::announce::announce(meta = crate::info(union))]
        #[derive(Clone, Copy)]
        pub union Raw {
            pub i: u32,
            pub f: f32,
        }
    }

    /// Item-level `#[cfg]` that is false: no trampoline exists.
    pub mod gone {
        use super::announce;

        #[announce]
        #[cfg(any())]
        pub struct AnnounceFirst;

        #[cfg(any())]
        #[announce]
        pub struct CfgFirst;

        #[announce]
        #[cfg(not(any()))]
        pub struct Present;
    }

    /// type-leak 0.8: elided lifetimes, higher-ranked bounds and tuples with a
    /// repeater.
    pub mod leak08 {
        use super::announce;

        ::announce::define_repeater!(pub Repeater);
        pub mod inner {
            #[derive(Debug, PartialEq, Clone, Copy)]
            pub struct Local(pub u8);
        }
        use inner::Local;

        /// `&Local` with an elided lifetime in method signatures.
        #[announce(repeater = crate::leak08::Repeater)]
        pub trait Elided {
            fn get(&self, l: &Local) -> u8;
            fn pick<'a>(&'a self, l: &'a Local) -> &'a Local;
        }

        /// A higher-ranked `Fn` bound whose argument is `&Local`.
        #[announce(repeater = crate::leak08::Repeater)]
        pub struct Hr {
            pub f: ::std::boxed::Box<dyn for<'b> ::core::ops::Fn(&'b Local) -> u8>,
        }

        /// A tuple: only `Local` is carried, `u8` is a primitive.
        #[announce(repeater = crate::leak08::Repeater)]
        pub struct Tup {
            pub t: (Local, u8),
        }

        pub mod wrap {
            pub struct Wrap<'a>(pub &'a u8);
        }
        use wrap::Wrap;

        /// A method lifetime that a relative type mentions is fine when another
        /// argument keeps it as written (`&'a self`, a plain `&'a u8`). Without that,
        /// type-leak 0.8 rejects the definition (`repeater_lifetime_unconstrained` in `tests/ui_leak.rs`).
        #[announce(repeater = crate::leak08::Repeater)]
        pub trait Constrained {
            fn by_self<'a>(&'a self, a: Wrap<'a>) -> &'a u8;
            fn by_arg<'a>(&self, a: Wrap<'a>, b: &'a u8) -> &'a u8;
        }
    }

    /// The repeater of type-leak 0.8: every entry is written from the context of
    /// the scope where its type occurs, and const
    /// generics of every primitive type are carried.
    pub mod contexts {
        use super::announce;

        ::announce::define_repeater!(pub Repeater);

        pub mod inner {
            #[derive(Debug, PartialEq, Clone, Default)]
            pub struct Local(pub u8);
            /// Any `X`.
            #[derive(Debug, PartialEq, Default)]
            pub struct G<X>(pub X);
            /// Well-formed only for `X: Clone`.
            #[derive(Debug, PartialEq, Default)]
            pub struct GC<X: ::core::clone::Clone>(pub X);
            #[derive(Debug, PartialEq, Default)]
            pub struct LocalG<U>(pub U);
            #[derive(Debug, PartialEq, Default)]
            pub struct LocalC<T: ::core::clone::Clone>(pub T);
            /// A type named like a generic of the items below.
            #[derive(Debug, PartialEq, Default)]
            pub struct U;
            #[derive(Debug, PartialEq)]
            pub struct Arr<const N: usize>(pub [u8; N]);
            #[derive(Debug, PartialEq, Default)]
            pub struct W<const A: u8, const B: bool, const C: char, const D: i64>;
            #[derive(Debug, PartialEq, Default)]
            pub struct Flag<const B: bool>;
            #[derive(Debug, PartialEq)]
            pub struct Wrap<'a>(pub &'a u8);
        }
        use inner::{Arr, Flag, Local, LocalC, LocalG, Wrap, G, GC, W};

        /// A: the bound `X: Clone` of `f` must not reach the impl of `G<X>`.
        #[announce(repeater = crate::contexts::Repeater)]
        pub trait SameTextTwice {
            fn f<X: ::core::clone::Clone>(&self, x: X) -> GC<X>;
            fn g<X>(&self, x: X) -> G<X>;
        }

        /// A: `Local` is a generic only inside `f`.
        #[announce(repeater = crate::contexts::Repeater)]
        pub trait GenericNamedLikeAType {
            fn f<Local>(&self, x: Local) -> Local;
            fn g(&self) -> Local;
        }

        /// #1: `T: Into<V>` needs `V` in the impl of `LocalG<T>`.
        #[announce(repeater = crate::contexts::Repeater)]
        pub struct BoundNamesOther<T: ::core::convert::Into<V>, V>(pub LocalG<T>, pub V);

        /// #1: a method bound that names a parameter of the trait.
        #[announce(repeater = crate::contexts::Repeater)]
        pub trait MethodBoundNamesTrait<T> {
            fn f<U: ::core::convert::Into<T>>(&self, x: LocalG<U>) -> T;
        }

        /// #1: a lifetime bound `T: 'a`.
        #[announce(repeater = crate::contexts::Repeater)]
        pub struct OutlivesBound<'a, T: 'a>(pub &'a LocalG<T>);

        /// #2: a where-clause on the carried type's generics.
        #[announce(repeater = crate::contexts::Repeater)]
        pub struct WhereClause<T>(pub LocalC<T>)
        where
            T: ::core::clone::Clone;

        /// #3: `U` is a generic in `f` and a type in `g`.
        #[announce(repeater = crate::contexts::Repeater)]
        pub trait NameOnlyElsewhere {
            fn f<U>(&self, x: LocalG<U>) -> U;
            fn g(&self) -> inner::U;
        }

        /// #4: `N` is a type generic in `f` and a const generic in `g`.
        #[announce(repeater = crate::contexts::Repeater)]
        pub trait KindClash {
            fn f<N>(&self, x: LocalG<N>) -> N;
            fn g<const N: usize>(&self, x: Arr<N>) -> usize;
        }

        /// Const generics of primitive types other than `usize` in a carried type.
        #[announce(repeater = crate::contexts::Repeater)]
        pub struct ConstParams<const A: u8, const B: bool, const C: char, const D: i64> {
            pub w: W<A, B, C, D>,
            pub local: Local,
        }

        /// A trait with a `bool` const generic and a method with a `char` one.
        #[announce(repeater = crate::contexts::Repeater)]
        pub trait ConstTrait<const B: bool> {
            fn flag(&self) -> Flag<B>;
            fn pick<const C: char>(&self) -> W<0, B, C, 1>;
        }

        /// A relative type with a lifetime that a `for<..>` of the item binds.
        #[announce(repeater = crate::contexts::Repeater)]
        pub struct HigherRanked {
            pub f: for<'b> fn(Wrap<'b>) -> u8,
        }

        /// `Sized` and `?Sized` are accepted bare, in a
        /// method's generics, a `where` clause and the item's own generics, with
        /// relative types next to them.
        #[announce(repeater = crate::contexts::Repeater)]
        pub trait BareSized {
            fn len_of<U: ?Sized + ::core::convert::AsRef<[u8]>>(&self, u: &U) -> LocalG<usize>;
            fn local() -> Local
            where
                Self: Sized;
        }

        #[announce(repeater = crate::contexts::Repeater)]
        pub struct BareUnsized<T: ?Sized>(pub ::std::boxed::Box<T>, pub Local);
    }
}

// The proc-macro crate of a library that builds or rewrites items and announces them with
// `announce::machinery` (machinery.rs, ui_errors.rs). Its users link `machinery_lib` only:
// the macros pass `crate = ::machinery_lib::announce`.
#[subcrate(proc_macro, ::announce, ::syn, ::proc_macro2, ::template_quote)]
pub mod machinery {
    use announce::machinery::{self, Options};
    use proc_macro::TokenStream;
    use proc_macro2::TokenStream as TokenStream2;
    use template_quote::{quote, ToTokens};

    fn facade() -> syn::Path {
        syn::parse_quote!(::machinery_lib::announce)
    }

    /// `#[tracked(OPTIONS)]` on a struct with named fields: adds a field
    /// `pub tracked: bool` and announces the result. `OPTIONS` are announce options.
    #[proc_macro_attribute]
    pub fn tracked(attr: TokenStream, item: TokenStream) -> TokenStream {
        let mut opts: Options = match syn::parse(attr) {
            Ok(o) => o,
            Err(e) => return e.to_compile_error().into(),
        };
        opts.krate.get_or_insert_with(facade);
        let mut s: syn::ItemStruct = match syn::parse(item) {
            Ok(s) => s,
            Err(e) => return e.to_compile_error().into(),
        };
        if let syn::Fields::Named(f) = &mut s.fields {
            f.named.push(syn::parse_quote!(pub tracked: bool));
        }
        machinery::announce(opts.to_token_stream(), quote! { #s }).into()
    }

    /// `pair!(Left, Right)`: generates two structs and announces both. The
    /// attribute is written as tokens.
    #[proc_macro]
    pub fn pair(input: TokenStream) -> TokenStream {
        struct Names(syn::Ident, syn::Ident);
        impl syn::parse::Parse for Names {
            fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
                let l = input.parse()?;
                input.parse::<syn::Token![,]>()?;
                Ok(Names(l, input.parse()?))
            }
        }
        let Names(l, r) = match syn::parse(input) {
            Ok(n) => n,
            Err(e) => return e.to_compile_error().into(),
        };
        let krate = facade();
        let attr = |n: &syn::Ident| quote! { crate = #krate, meta = test_lib::pair(#n) };
        let mut out = TokenStream2::new();
        out.extend(machinery::announce(
            attr(&l),
            quote! { pub struct #l { pub value: u8 } },
        ));
        out.extend(machinery::announce(
            attr(&r),
            quote! { pub struct #r { pub twin: u8 } },
        ));
        out.into()
    }

    /// A check of the library that owns the attribute `field_attr` and rejects
    /// `#[field_attr(bad)]`. It is used by `#[tracked(..)]` and by `#[announce(..)]`: the
    /// machinery sends the same query as `#[announce]`.
    #[proc_macro]
    pub fn owner(input: TokenStream) -> TokenStream {
        announce::helper::check(input.into(), &["field_attr"], |ci| {
            for attr in ci.owned_attributes() {
                if matches!(attr.parse_args::<syn::Ident>(), Ok(i) if i == "bad") {
                    return Err(syn::Error::new_spanned(
                        &attr,
                        "`#[field_attr(bad)]` is not allowed",
                    ));
                }
            }
            Ok(TokenStream2::new())
        })
        .into()
    }
}

// The public crate of that library: the macros, and `announce` (`machinery_lib::announce`,
// as a subcrate re-exports the crates it links).
#[subcrate(machinery, ::announce)]
pub mod machinery_lib {
    pub use machinery::{owner, pair, tracked};
}

// Items whose types do not resolve outside their module, announced with `strict = false`, and
// macros of a library over them (the UI tests of `same_context!` call them from another crate).
// `ui_lib::announce` is the `announce` it links.
#[subcrate(::announce)]
pub mod ui_lib {
    pub mod m {
        use announce::announce;

        pub mod inner {
            pub struct UiLocal(pub u8);
            pub trait UiBase {}
        }
        use inner::{UiBase, UiLocal};

        #[announce(strict = false)]
        pub struct Holder {
            pub a: UiLocal,
        }

        #[announce(strict = false)]
        pub trait Sub: UiBase {}

        #[announce(strict = false)]
        pub struct Plain(pub u8);

        // Two causes in one item.
        pub mod inner2 {
            pub struct UiOther(pub u8);
        }
        use inner2::UiOther;

        #[announce(strict = false)]
        pub struct Pair {
            pub a: UiLocal,
            pub b: UiOther,
        }
    }

    /// Announces the item it is given: the item is the user's (`leak_errors.rs`).
    #[macro_export]
    macro_rules! announced {
        ($item:item) => {
            #[$crate::announce::announce(crate = $crate::announce)]
            $item
        };
    }

    /// Calls the trampoline of an item of the library through `$crate`. A proc macro cannot tell
    /// which crate that is, so the texts of the error name the crate that defined the item.
    #[macro_export]
    macro_rules! check_holder {
        () => {
            $crate::m::Holder! { [$crate::m::Holder!, $crate::announce::same_context!] }
        };
    }

    /// The same for a callback of the caller that needs a type table.
    #[macro_export]
    macro_rules! apply_holder {
        ($($callback:ident)::+, $name:ident) => {
            $crate::m::Holder! { [$crate::m::Holder!, $($callback)::+!] $name }
        };
    }
}
