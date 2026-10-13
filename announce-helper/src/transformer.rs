//! Transformers: list elements that rewrite the newest entry and forward.
//!
//! The caller places a transformer right after the definition it rewrites:
//!
//! ```text
//! Partly! { [Partly!, my_lib::public_fields!, cb!] REST }
//! ```
//!
//! `public_fields!` rewrites the newest entry (item and/or META) and forwards
//! to `cb!`. The definer writes a plain `#[announce]`.

use crate::{Entry, Input};
use proc_macro2::TokenStream;
use syn::{Error, Item, Result};
use template_quote::quote;

impl Input {
    /// The newest entry, the one a transformer rewrites.
    pub fn newest(&self) -> Option<&Entry> {
        self.entries.first()
    }

    /// For a transformer: forwards to the next element with the newest entry
    /// replaced by `entry`. Errors if there is no entry or no next element.
    pub fn forward_transformed(&self, entry: Entry) -> Result<TokenStream> {
        if self.entries.is_empty() {
            return Err(Error::new(
                proc_macro2::Span::call_site(),
                "announce: a transformer must come after the definition it transforms, \
                 e.g. `[F!, transformer!, callback!]`",
            ));
        }
        let mut next = self.clone();
        next.entries[0] = entry;
        next.forward().ok_or_else(|| {
            Error::new(
                proc_macro2::Span::call_site(),
                "announce: a transformer cannot be the last element of the continuation list",
            )
        })
    }
}

/// Defines a transformer: a `#[proc_macro]` that gets the newest entry as an
/// [`Entry`], rewrites it (the body returns `syn::Result<Entry>`), and forwards it
/// to the next list element.
///
/// ```ignore
/// announce_helper::transformer! {
///     /// Drops non-`pub` fields and lists the kept ones in META.
///     pub fn public_fields(entry) {
///         let mut item: syn::ItemStruct = syn::parse2(entry.item.clone())?;
///         // ..
///         entry.item = quote::quote!(#item);
///         entry.meta.push(syn::parse_quote!(fields(a, c)));
///         Ok(entry)
///     }
/// }
/// ```
#[macro_export]
macro_rules! transformer {
    ($(#[$m:meta])* $vis:vis fn $name:ident ($entry:ident) $body:block) => {
        $(#[$m])*
        #[proc_macro]
        $vis fn $name(__announce_ts: ::proc_macro::TokenStream) -> ::proc_macro::TokenStream {
            let __announce_run = |__announce_ts: ::proc_macro::TokenStream|
             -> $crate::__private::syn::Result<$crate::__private::proc_macro2::TokenStream> {
                let __announce_input: $crate::Input = $crate::__private::syn::parse(__announce_ts)?;
                let __announce_newest = __announce_input.newest().cloned().ok_or_else(|| {
                    $crate::__private::syn::Error::new(
                        $crate::__private::proc_macro2::Span::call_site(),
                        "announce: a transformer must come after the definition it transforms",
                    )
                })?;
                #[allow(unused_mut)]
                let mut $entry: $crate::Entry = __announce_newest;
                let __announce_new = (move || -> $crate::__private::syn::Result<$crate::Entry> { $body })()?;
                __announce_input.forward_transformed(__announce_new)
            };
            match __announce_run(__announce_ts) {
                ::core::result::Result::Ok(ts) => ts.into(),
                ::core::result::Result::Err(e) => e.to_compile_error().into(),
            }
        }
    };
}

/// Strips an item down to its signatures. This is what the
/// [`signature_only!`] transformer does, and custom transformers can use it too.
/// That macro lists the rules: function bodies are replaced with
/// `{ ::core::unimplemented!() }`, except in a `const fn` and in a fn that returns
/// `impl Trait`.
///
/// [`signature_only!`]: https://docs.rs/announce/latest/announce/macro.signature_only.html
pub fn signature_only(item: &mut Item) {
    fn returns_impl(sig: &syn::Signature) -> bool {
        fn has_impl(ts: TokenStream) -> bool {
            ts.into_iter().any(|tt| match tt {
                proc_macro2::TokenTree::Ident(i) => i == "impl",
                proc_macro2::TokenTree::Group(g) => has_impl(g.stream()),
                _ => false,
            })
        }
        match &sig.output {
            syn::ReturnType::Default => false,
            syn::ReturnType::Type(_, ty) => has_impl(quote!(#ty)),
        }
    }
    fn placeholder() -> syn::Block {
        syn::parse_quote!({ ::core::unimplemented!() })
    }
    match item {
        Item::Trait(t) => {
            for ti in &mut t.items {
                if let syn::TraitItem::Fn(f) = ti {
                    if f.default.is_some() && !returns_impl(&f.sig) {
                        f.default = Some(placeholder());
                    }
                }
            }
        }
        Item::Impl(i) => {
            for ii in &mut i.items {
                if let syn::ImplItem::Fn(f) = ii {
                    if !returns_impl(&f.sig) && f.sig.constness.is_none() {
                        f.block = placeholder();
                    }
                }
            }
        }
        Item::Fn(f) => {
            // `const fn` bodies are kept: a re-emitted `const` may call them.
            if !returns_impl(&f.sig) && f.sig.constness.is_none() {
                *f.block = placeholder();
            }
        }
        Item::Mod(m) => {
            if let Some((_, items)) = &mut m.content {
                for i in items {
                    signature_only(i);
                }
            }
        }
        _ => {}
    }
}
