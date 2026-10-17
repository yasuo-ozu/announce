//! A trampoline called across real crate boundaries, and a chain of trampolines
//! delivered to a callback written with `announce::helper::callback`.
//!
//! - `callbacks`: a proc-macro crate with the callbacks (`#[subcrate(proc_macro, ..)]`);
//! - `defs`: the defining crate, with the `#[announce]`d items;
//! - this file is the calling crate.
//!
//! The test macros print what they receive as strings, so the assertions read the
//! payloads and the META exactly as the calling crate gets them.

use subcrate::subcrate;

#[subcrate(proc_macro, ::announce, ::proc_macro2, ::syn, ::template_quote)]
pub mod callbacks {
    use announce::helper::{Cont, Gather};
    use proc_macro::TokenStream;
    use template_quote::quote;

    /// `summary! { [summary!] ENTRY* NAME }` emits
    /// `pub const NAME: &[(&str, &str, &str)] = &[(key, meta, item kind and ident), ..]`
    /// in visiting order. It forwards to the next continuation if there is one.
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
                    Ok(syn::Item::Trait(s)) => {
                        format!("trait {} ({} items)", s.ident, s.items.len())
                    }
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

    /// An announce-aware derive: when a trampoline is called with `#[derive(path::Parse)]` as
    /// its continuation, the derive gets the payload in a dummy item and redirects it to the
    /// trampoline of the trait, which is renamed because this derive has the name `Parse`.
    #[proc_macro_derive(Parse)]
    pub fn derive_parse(input: TokenStream) -> TokenStream {
        let input: proc_macro2::TokenStream = input.into();
        match announce::helper::redirect(input, &syn::parse_quote!(::defs::conflict::ParseAnnounce))
        {
            Some(out) => out.into(),
            None => TokenStream::new(),
        }
    }

    /// The attribute form of [`derive_parse`], which consumes the dummy.
    #[proc_macro_attribute]
    pub fn parse_attr(_: TokenStream, input: TokenStream) -> TokenStream {
        let input: proc_macro2::TokenStream = input.into();
        match announce::helper::redirect(
            input.clone(),
            &syn::parse_quote!(::defs::conflict::ParseAnnounce),
        ) {
            Some(out) => out.into(),
            None => input.into(),
        }
    }

    /// Starts a trampoline from a proc macro: `start!([a!, #[derive(B)], ..] REST)`.
    /// Continuations that are attributes get random dummy names.
    #[proc_macro]
    pub fn start(input: TokenStream) -> TokenStream {
        struct Start(announce::helper::ContList, proc_macro2::TokenStream);
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

    /// `gathered! { [gathered!] (a::A, b::B) NAME }`: asks for the listed definitions, in
    /// one go, and emits `NAME: &[&str]` with their item idents in the requested order.
    #[proc_macro]
    pub fn gathered(input: TokenStream) -> TokenStream {
        struct Rest {
            paths: Vec<syn::Path>,
            name: syn::Ident,
        }
        impl syn::parse::Parse for Rest {
            fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
                let content;
                syn::parenthesized!(content in input);
                let paths =
                    syn::punctuated::Punctuated::<syn::Path, syn::Token![,]>::parse_terminated(
                        &content,
                    )?;
                Ok(Rest {
                    paths: paths.into_iter().collect(),
                    name: input.parse()?,
                })
            }
        }
        announce::helper::callback(input.into(), |input| {
            let rest: Rest = input.rest_as()?;
            let needed = rest.paths.iter().cloned().map(Cont::bang);
            let entries = match input.gather(needed) {
                Gather::Ready(e) => e,
                Gather::Fetch(ts) => return Ok(ts),
            };
            let mut idents = Vec::new();
            for e in entries {
                idents.push(match e.item()? {
                    syn::Item::Struct(s) => s.ident.to_string(),
                    syn::Item::Enum(s) => s.ident.to_string(),
                    syn::Item::Trait(s) => s.ident.to_string(),
                    other => return Err(syn::Error::new_spanned(other, "unexpected item")),
                });
            }
            let name = &rest.name;
            Ok(quote! { pub const #name: &[&str] = &[ #(#idents),* ]; })
        })
        .into()
    }
}

#[subcrate(::announce, callbacks)]
pub mod defs {
    use announce::announce;

    /// Fails to compile here, unless `T` is announced: the `Announce` impl is the one of
    /// `announce` as this crate links it, which is the one that `#[announce]` names.
    pub fn is_announced<T: ?Sized + announce::Announce>() {}

    #[announce]
    #[derive(Clone, Debug)]
    pub struct Named {
        pub a: u32,
        // The definer is not built with `cfg(test)`, the calling crate is.
        #[cfg(test)]
        pub only_in_test: u32,
        #[cfg(not(test))]
        pub only_in_definer: u32,
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
        OnlyInTest,
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
    }

    #[announce]
    pub type Alias = ::std::vec::Vec<u8>;

    #[announce]
    pub mod inline {
        pub fn six() -> i32 {
            1 + 2 + 3
        }
    }

    /// The name `Parse` is taken by the derive, so the trampoline is renamed.
    pub mod conflict {
        use announce::announce;

        #[announce(macro_name = ParseAnnounce)]
        pub trait Parse {
            fn parse(s: &str) -> Self;
        }
        pub use callbacks::{parse_attr, Parse};
    }

    /// `meta = path(args)` is carried to the callbacks as written, not invoked.
    #[announce(meta = my_lib::info(version = 3), meta = my_lib::flag)]
    pub struct WithMeta;
}

macro_rules! capture {
    ($(@announce_format "1")? [$($l:tt)*] $($t:tt)*) => {
        pub const OUT: &str = stringify!([$($l)*] $($t)*);
    };
}

/// The payload as a string without spaces, so assertions do not depend on the spacing
/// of `stringify!`.
fn norm(s: &str) -> String {
    s.split_whitespace().collect()
}

mod bang {
    ::defs::Greet! { [::defs::Greet!, capture!] rest tokens }
}

#[test]
fn a_single_hop_hands_the_definition_and_the_rest_to_the_callback() {
    let out = norm(bang::OUT);
    // `[continuations] head! => [meta] { item } rest`: the head is dropped from the list
    // and named in front of the payload.
    assert!(
        out.starts_with("[capture!]::defs::Greet!=>[]{pubtraitGreet"),
        "{}",
        out
    );
    assert!(out.ends_with("resttokens"), "{}", out);
}

mod kinds {
    use crate::callbacks::summary;
    ::defs::Named! {
        [::defs::Named!, ::defs::Tuple!, ::defs::Shape!, ::defs::Bits!, ::defs::Greet!,
         ::defs::Alias!, ::defs::inline!, summary!] ALL
    }
}

#[test]
fn a_chain_visits_every_kind_of_item_in_order() {
    let items: Vec<&str> = kinds::ALL.iter().map(|r| r.2).collect();
    assert_eq!(
        items,
        [
            "struct Named",
            "struct Tuple",
            "enum Shape",
            "union Bits",
            "trait Greet (2 items)",
            "type Alias",
            "mod inline"
        ]
    );
    let keys: Vec<&str> = kinds::ALL.iter().map(|r| r.0).collect();
    assert_eq!(
        keys,
        ["Named", "Tuple", "Shape", "Bits", "Greet", "Alias", "inline"]
    );
}

mod cfg_eval {
    ::defs::Named! { [::defs::Named!, capture!] }
    pub mod shape {
        ::defs::Shape! { [::defs::Shape!, capture!] }
    }
}

#[test]
fn cfg_is_evaluated_in_the_defining_crate() {
    // This crate is built with `cfg(test)`, the definer is not.
    let out = norm(cfg_eval::OUT);
    assert!(out.contains("only_in_definer"), "{}", out);
    assert!(!out.contains("only_in_test"), "{}", out);
    assert!(!out.contains("#[cfg"), "{}", out);
    // `cfg_attr(.., doc = ..)` is evaluated too, and the doc is not in the payload.
    assert!(!out.contains("cfg_attrevaluatedinthedefiner"), "{}", out);
    assert!(out.contains("pubc:u8"), "{}", out);
    assert!(!norm(cfg_eval::shape::OUT).contains("OnlyInTest"));
}

mod metas {
    use crate::callbacks::summary;
    ::defs::WithMeta! { [::defs::WithMeta!, summary!] METAS }
}

#[test]
fn meta_is_listed_for_the_callback_and_not_invoked() {
    assert_eq!(
        norm(metas::METAS[0].1),
        "my_lib::info(version=3),my_lib::flag"
    );
}

mod derive_route {
    use crate::callbacks::summary;
    // The continuation is the derive: the name `Parse` is the derive, which redirects the
    // payload to `ParseAnnounce!`.
    ::defs::Greet! {
        [::defs::Greet!, #[derive(::defs::conflict::Parse)] Named1, summary!] VIA_DERIVE
    }
    // Two unnamed derive routes in one module: `announce::helper` names the dummies.
    pub mod twice {
        use crate::callbacks::{start, summary};
        start!([#[derive(::defs::conflict::Parse)], summary!] ONE);
        start!([#[derive(::defs::conflict::Parse)], summary!] TWO);
    }
    // The attribute form: the attribute consumes the dummy.
    pub mod attr {
        use crate::callbacks::summary;
        ::defs::Shape! {
            [::defs::Shape!, #[::defs::conflict::parse_attr], summary!] VIA_ATTR
        }
    }
}

#[test]
fn a_derive_or_an_attribute_of_the_same_name_can_take_the_trampoline() {
    let items: Vec<&str> = derive_route::VIA_DERIVE.iter().map(|r| r.2).collect();
    assert_eq!(items, ["trait Greet (2 items)", "trait Parse (1 items)"]);
    assert_eq!(derive_route::twice::ONE.len(), 1);
    assert_eq!(derive_route::twice::TWO.len(), 1);
    let items: Vec<&str> = derive_route::attr::VIA_ATTR.iter().map(|r| r.2).collect();
    assert_eq!(items, ["enum Shape", "trait Parse (1 items)"]);
}

mod gathered {
    use crate::callbacks::gathered;
    gathered! { [gathered!] (::defs::Shape, ::defs::Greet, ::defs::Tuple<u8>) G }
}

#[test]
fn a_callback_that_gathers_fetches_the_definitions_once_in_the_order_asked() {
    // The callback starts the chain itself: it asks the three trampolines and is called
    // again with the entries.
    assert_eq!(gathered::G, ["Shape", "Greet", "Tuple"]);
}

mod reemitted {
    macro_rules! reemit {
        ($(@announce_format "1")? [$($l:tt)*] $($p:ident)? $(:: $s:ident)* ! => [$($m:tt)*] { $($item:tt)* }) => {
            $($item)*
        };
    }
    ::defs::inline! { [::defs::inline!, reemit!] }
}

#[test]
fn the_payload_is_a_definition_the_caller_can_emit_again() {
    assert_eq!(reemitted::inline::six(), 6);
}

#[test]
fn the_announce_impl_is_in_the_defining_crate() {
    defs::is_announced::<defs::Named>();
    defs::is_announced::<defs::Tuple<u8>>();
    defs::is_announced::<defs::Shape>();
    defs::is_announced::<defs::Bits>();
    defs::is_announced::<dyn defs::Greet>();
    // The derive of another trait ran, and the item is a real type here.
    let n = defs::Named {
        a: 1,
        only_in_definer: 2,
        c: 3,
    };
    assert!(format!("{:?}", n.clone()).contains("only_in_definer"));
}
