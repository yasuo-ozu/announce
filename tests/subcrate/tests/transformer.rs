//! Transformers: macros in the middle of the continuation list that rewrite the newest
//! definition and add to its META before the callback sees it.
//!
//! - `transformers`: a proc-macro crate with transformers written with
//!   `announce::helper::transformer`;
//! - `defs`: the defining crate, with plain `#[announce]`s: the definition does not know
//!   which transformer a caller will choose;
//! - this file is the calling crate, which puts the transformers in the list.

use subcrate::subcrate;

#[subcrate(proc_macro, ::announce, ::proc_macro2, ::syn, ::template_quote)]
pub mod transformers {
    use proc_macro::TokenStream;
    use template_quote::quote;

    /// Keeps only the `pub` fields of the struct of the newest entry and lists their names
    /// in its META as `fields(..)`.
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

    /// Adds `tagged` to the META of the newest entry, if it has a META at all.
    #[proc_macro]
    pub fn tag(input: TokenStream) -> TokenStream {
        announce::helper::transformer(input.into(), |mut entry| {
            if !entry.meta.is_empty() {
                entry.meta.push(syn::parse_quote!(tagged));
            }
            Ok(entry)
        })
        .into()
    }
}

#[subcrate(::announce)]
pub mod defs {
    use announce::announce;

    #[announce(meta = orig(1))]
    pub struct Partly {
        pub a: u8,
        #[allow(dead_code)]
        hidden: u8,
        pub c: u8,
    }

    #[announce]
    pub struct Plain {
        pub a: u8,
    }

    /// For `::announce::signature_only!`: fn bodies are replaced, except when the fn
    /// returns `impl Trait`.
    #[announce]
    pub trait Sig {
        fn required(&self) -> u32;
        fn body(&self) -> u32 {
            42
        }
    }

    #[announce]
    pub mod sig_mod {
        pub fn plain() -> u32 {
            7
        }
        pub fn opaque() -> impl ::core::fmt::Display {
            8
        }
    }
}

macro_rules! capture {
    ($(@announce_format "1")? [$($l:tt)*] $($t:tt)*) => {
        pub const OUT: &str = stringify!($($t)*);
    };
}

macro_rules! reemit {
    ($(@announce_format "1")? [$($l:tt)*] $($p:ident)? $(:: $s:ident)* ! => [$($m:tt)*] { $($item:tt)* }) => {
        $($item)*
    };
}

fn norm(s: &str) -> String {
    s.split_whitespace().collect()
}

mod transformed {
    use crate::transformers::public_fields;
    // The caller chooses the transformer.
    ::defs::Partly! { [::defs::Partly!, public_fields!, capture!] }
    // Without it the full definition arrives.
    pub mod untouched {
        ::defs::Partly! { [::defs::Partly!, capture!] }
    }
}

#[test]
fn a_transformer_of_another_crate_rewrites_the_payload_and_adds_to_the_meta() {
    assert!(norm(transformed::untouched::OUT).contains("hidden:u8"));
    assert!(norm(transformed::untouched::OUT).contains("=>[orig(1)]{"));
    let out = norm(transformed::OUT);
    assert!(
        out.contains("=>[orig(1),fields(a,c)]{pubstructPartly{puba:u8,pubc:u8}}"),
        "{}",
        out
    );
}

mod chained {
    use crate::transformers::{public_fields, tag};
    // Transformers apply in the order of the list, one after another, to the newest entry.
    ::defs::Partly! { [::defs::Partly!, public_fields!, tag!, capture!] }
    pub mod no_meta {
        use crate::transformers::tag;
        // `tag!` does nothing for an entry without META.
        ::defs::Plain! { [::defs::Plain!, tag!, capture!] }
    }
}

#[test]
fn transformers_chain_in_list_order() {
    let out = norm(chained::OUT);
    assert!(out.contains("=>[orig(1),fields(a,c),tagged]{"), "{}", out);
    let out = norm(chained::no_meta::OUT);
    assert!(out.contains("=>[]{pubstructPlain{puba:u8,}}"), "{}", out);
}

mod gathering_ends {
    use crate::transformers::{public_fields, tag};
    // One list with two definitions: a transformer applies to the entry before it.
    ::defs::Partly! { [::defs::Partly!, public_fields!, ::defs::Plain!, tag!, capture!] }
}

#[test]
fn a_transformer_rewrites_the_entry_that_was_added_last() {
    let out = norm(gathering_ends::OUT);
    assert!(
        out.contains("=>[orig(1),fields(a,c)]{pubstructPartly{puba:u8,pubc:u8}}"),
        "{}",
        out
    );
    // `Plain` has no META, so `tag!` leaves it alone.
    assert!(out.contains("=>[]{pubstructPlain{puba:u8,}}"), "{}", out);
}

mod sig {
    // The built-in transformer of `announce`, called by the caller with its own path.
    ::defs::Sig! { [::defs::Sig!, ::announce::signature_only!, capture!] }
    pub mod module {
        ::defs::sig_mod! { [::defs::sig_mod!, ::announce::signature_only!, reemit!] }
    }
}

#[test]
fn signature_only_strips_the_bodies_of_a_definition_from_another_crate() {
    let out = norm(sig::OUT);
    assert!(out.contains("fnrequired(&self)->u32;"), "{}", out);
    assert!(
        out.contains("fnbody(&self)->u32{::core::unimplemented!()}"),
        "{}",
        out
    );
    assert!(!out.contains("42"), "{}", out);
    // Re-emitted module: the placeholder panics, the `impl Trait` fn keeps its body.
    assert_eq!(sig::module::sig_mod::opaque().to_string(), "8");
    assert!(std::panic::catch_unwind(sig::module::sig_mod::plain).is_err());
}
