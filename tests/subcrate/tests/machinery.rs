//! `announce::machinery::announce` called from the proc-macro crate of a library, whose users
//! do not link `announce`.
//!
//! - `machinery`: the proc-macro crate of the library. Its macros build or rewrite items and
//!   announce them with `announce::machinery::announce`, passing
//!   `crate = ::lib::announce`: the generated code reaches `announce` through the library;
//! - `lib`: the public crate of the library. It re-exports the macros and `announce`;
//! - `user_defs`: a crate that uses the library and has no `announce` dependency: it
//!   defines the items;
//! - this file is the calling crate.

use subcrate::subcrate;

#[subcrate(proc_macro, ::announce, ::proc_macro2, ::syn, ::template_quote)]
pub mod machinery {
    use announce::machinery::{self, Options};
    use proc_macro::TokenStream;
    use proc_macro2::TokenStream as TokenStream2;
    use template_quote::{quote, ToTokens};

    /// The library re-exports `announce` as `lib::announce`; the crate that defines the
    /// items has no other way to reach it.
    fn facade() -> syn::Path {
        syn::parse_quote!(::lib::announce)
    }

    /// `#[tracked(OPTIONS)]` on a struct with named fields: adds a field `pub tracked:
    /// bool` and announces the result. `OPTIONS` are announce options.
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

    /// `pair!(Left, Right)`: generates two structs and announces both. The options are
    /// written as tokens.
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

    /// A check of the library that owns the attribute `field_attr`. It is used by
    /// `#[tracked(..)]` and by `#[announce(..)]`: the machinery sends the same query as
    /// `#[announce]`.
    #[proc_macro]
    pub fn owner(input: TokenStream) -> TokenStream {
        announce::helper::check(input.into(), &["field_attr"], |_| Ok(TokenStream2::new())).into()
    }

    /// `summary! { [summary!] ENTRY* NAME }`: `pub const NAME: &[(&str, &str, &str)]` with
    /// a row `(key, meta, item kind and ident)` for each entry, in visiting order. A
    /// callback of the library for its users.
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

// `lib` re-exports `announce` as the module `announce`. A subcrate cannot `pub use` a crate
// it links (`pub use announce;` is E0365: the dependency is a private `extern crate` at the
// root of the subcrate), so the module re-exports everything the crate has. (The crate is
// linked as `ann`, because the name `announce` of the module would clash with it.)
#[subcrate(machinery, ann = ::announce)]
pub mod lib {
    pub use machinery::{owner, pair, summary, tracked};

    /// The macros pass `crate = ::lib::announce`, so the generated code reaches `announce`
    /// through here.
    #[doc(hidden)]
    pub mod announce {
        pub use crate::ann::*;
    }
}

// `user_defs` links the library only: `announce` is not among its dependencies.
#[subcrate(lib)]
pub mod user_defs {
    use lib::{pair, tracked};

    // The macro adds a field and the struct is announced as rewritten. The `cfg` is
    // evaluated here, as for `#[announce]`.
    #[tracked]
    pub struct Point {
        pub x: i32,
        #[cfg(any())]
        pub gone: i32,
        #[cfg(not(any()))]
        pub kept: i32,
    }

    // Options are passed through.
    #[tracked(macro_name = NamedAnnounce, meta = test_lib::info(1))]
    pub struct Named {
        pub a: u8,
    }

    pair!(Left, Right);

    // The attribute of the `announce` crate, through the library's re-export: this crate
    // has no `announce` dependency, so `crate = ..` names the re-export.
    #[lib::announce::announce(crate = ::lib::announce, macro_name = PlainAnnounce)]
    pub struct PlainStruct(pub u8);

    // A check that owns an attribute: the announcement removes it from the item it emits,
    // and the check sees it.
    #[tracked(::lib::owner, macro_name = OwnedAnnounce)]
    pub struct Owned {
        #[field_attr(x)]
        pub x: i32,
        pub y: i32,
    }

    #[lib::announce::announce(crate = ::lib::announce, macro_name = OwnedPlainAnnounce, ::lib::owner)]
    pub struct OwnedPlain(#[field_attr(z)] pub u8);

    // Doc attributes: removed from the payload by default, `docs = true` keeps them.
    /// Doc of Docs.
    #[tracked(macro_name = DocsAnnounce)]
    pub struct Docs {
        /// Doc of x.
        pub x: u8,
    }

    /// Doc of DocsKept.
    #[tracked(macro_name = DocsKeptAnnounce, docs = true)]
    pub struct DocsKept {
        /// Doc of x.
        pub x: u8,
    }

    // `tracked` calls `announce::machinery::announce`, which does what `#[announce]` does:
    // the item comes back with the runner, which holds the options, as its last attribute.
    // The other announcement expands next and merges its options into the runner.
    #[tracked(meta = test_lib::tracked)]
    #[lib::announce::announce(crate = ::lib::announce, meta = test_lib::attr)]
    pub struct MergedBelow {
        pub a: u8,
    }

    // Here the attribute expands first and puts the runner on the item; `tracked` then
    // announces as well, and its options are merged into the runner.
    #[lib::announce::announce(crate = ::lib::announce, meta = test_lib::attr)]
    #[tracked(meta = test_lib::tracked)]
    pub struct MergedAbove {
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

mod point {
    use crate::user_defs;
    user_defs::Point! { [user_defs::Point!, capture!] }
}

#[test]
fn the_rewritten_item_is_announced() {
    let out = norm(point::OUT);
    assert!(out.contains("pubstructPoint"), "{}", out);
    assert!(out.contains("pubtracked:bool"), "{}", out);
    // `cfg` is evaluated at the definition.
    assert!(out.contains("pubkept:i32"), "{}", out);
    assert!(!out.contains("gone"), "{}", out);
    assert!(!out.contains("#[cfg"), "{}", out);
}

mod chain {
    use crate::{lib::summary, user_defs as u};
    u::Point! {
        [u::Point!, u::NamedAnnounce!, u::Left!, u::Right!, u::PlainAnnounce!, summary!] ALL
    }
}

#[test]
fn definitions_made_by_the_macros_of_the_library_chain_into_one_callback() {
    let rows: Vec<(&str, &str, &str)> = chain::ALL.to_vec();
    let items: Vec<&str> = rows.iter().map(|r| r.2).collect();
    assert_eq!(
        items,
        [
            "struct Point",
            "struct Named",
            "struct Left",
            "struct Right",
            "struct PlainStruct"
        ]
    );
    let metas: Vec<String> = rows.iter().map(|r| norm(r.1)).collect();
    assert_eq!(metas[0], "");
    assert_eq!(metas[1], "test_lib::info(1)");
    assert_eq!(metas[2], "test_lib::pair(Left)");
    assert_eq!(metas[3], "test_lib::pair(Right)");
    assert_eq!(metas[4], "");
}

#[test]
fn the_announce_impl_resolves_through_the_facade() {
    // `lib::announce` is the crate that the library links, and `user_defs` names it
    // through the library.
    fn assert_announced<T: ?Sized + lib::announce::Announce>() {}
    assert_announced::<user_defs::Point>();
    assert_announced::<user_defs::Named>();
    assert_announced::<user_defs::Left>();
    assert_announced::<user_defs::Right>();
    assert_announced::<user_defs::PlainStruct>();
    // The item is emitted as written (and rewritten) by the macro.
    let p = user_defs::Point {
        x: 1,
        kept: 2,
        tracked: true,
    };
    assert_eq!((p.x, p.kept, p.tracked), (1, 2, true));
}

mod owned {
    use crate::{lib::summary, user_defs as u};
    u::OwnedAnnounce! { [u::OwnedAnnounce!, u::OwnedPlainAnnounce!, summary!] ALL }
}

#[test]
fn checks_that_own_attributes_work_through_the_machinery() {
    let rows = owned::ALL;
    assert_eq!(rows.len(), 2);
    for (row, name) in rows.iter().zip(["Owned", "OwnedPlain"]) {
        assert_eq!(row.2, format!("struct {}", name), "{:?}", row);
        // The check, and the names it owns.
        assert_eq!(norm(row.1), "::lib::owner,attrs(field_attr)", "{:?}", row);
    }
    // The item is emitted without the attribute.
    let o = user_defs::Owned {
        x: 1,
        y: 2,
        tracked: true,
    };
    assert_eq!(o.x + o.y, 3);
    assert_eq!(user_defs::OwnedPlain(5).0, 5);
}

mod docs {
    use crate::user_defs as u;
    u::DocsAnnounce! { [u::DocsAnnounce!, capture!] }
    pub mod kept {
        use crate::user_defs as u;
        u::DocsKeptAnnounce! { [u::DocsKeptAnnounce!, capture!] }
    }
}

#[test]
fn docs_are_removed_from_the_payload_unless_asked() {
    // Default: gone.
    let out = norm(docs::OUT);
    assert!(
        out.ends_with("{pubstructDocs{pubx:u8,pubtracked:bool}}"),
        "{}",
        out
    );
    // `docs = true` (a field of `announce::machinery::Options`): kept.
    let out = norm(docs::kept::OUT);
    assert!(
        out.contains("#[doc=\"DocofDocsKept.\"]pubstructDocsKept{#[doc=\"Docofx.\"]pubx:u8,"),
        "{}",
        out
    );
}

mod merged {
    use crate::{lib::summary, user_defs as u};
    u::MergedBelow! { [u::MergedBelow!, u::MergedAbove!, summary!] ALL }
}

#[test]
fn machinery_announcements_merge_with_attributes() {
    // One trampoline each (a second would not compile), with both metas: the entries of
    // the announcement that expanded first come first.
    let rows = merged::ALL;
    assert_eq!(rows.len(), 2);
    for (row, name, metas) in [
        (&rows[0], "MergedBelow", "test_lib::tracked,test_lib::attr"),
        (&rows[1], "MergedAbove", "test_lib::attr,test_lib::tracked"),
    ] {
        assert_eq!(row.2, format!("struct {}", name), "{:?}", row);
        assert_eq!(norm(row.1), metas, "{:?}", row);
    }
    fn assert_announced<T: ?Sized + lib::announce::Announce>() {}
    assert_announced::<user_defs::MergedBelow>();
    assert_announced::<user_defs::MergedAbove>();
    let m = user_defs::MergedAbove {
        a: 1,
        tracked: false,
    };
    assert_eq!(m.a, 1);
}
