//! A callback built with announce 0.2.0. It copies each item under the name `<Item>Copy02` and
//! reports what it read: whether `Entry::type_leak()` found a table, and the META elements.

use proc_macro::TokenStream;
use quote::{format_ident, quote};

#[proc_macro]
pub fn copy(input: TokenStream) -> TokenStream {
    let input: announce02::helper::Input = match syn::parse(input) {
        Ok(i) => i,
        Err(e) => return e.to_compile_error().into(),
    };
    let mut out = proc_macro2::TokenStream::new();
    for e in &input.entries {
        let table = match e.type_leak() {
            Ok(t) => t.is_some(),
            Err(err) => return err.to_compile_error().into(),
        };
        let mut item = match e.item() {
            Ok(i) => i,
            Err(err) => return err.to_compile_error().into(),
        };
        if let syn::Item::Struct(s) = &mut item {
            s.ident = format_ident!("{}Copy02", s.ident);
        }
        let metas = e
            .meta
            .iter()
            .map(|m| quote!(#m).to_string())
            .collect::<Vec<_>>()
            .join(" | ");
        out.extend(quote! {
            #item
            pub fn has_type_leak() -> bool {
                #table
            }
            pub fn metas() -> &'static str {
                #metas
            }
        });
    }
    out.into()
}
