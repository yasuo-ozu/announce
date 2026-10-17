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

use crate::helper::{Entry, Input};
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

/// Runs a transformer: the body of a `#[proc_macro]` that gets the newest entry,
/// rewrites it and forwards to the next list element.
///
/// - `input` is the input of the proc macro: the tokens that the trampoline before it
///   sends, parsed as an [`Input`].
/// - `f` is the transformer itself. It gets the newest [`Entry`] by value and returns
///   the rewritten one, or a `syn::Error`.
///
/// The result forwards the rewritten entry to the next element of the list
/// ([`Input::forward_transformed`]). A `syn::Error` (the input does not parse, there is
/// no entry, the transformer is the last element of the list, or `f` fails) becomes a
/// `compile_error!`, so the result can be returned from the proc macro as it is.
///
/// In a proc-macro crate this is a `#[proc_macro]` function. The example is run as a
/// doctest, which cannot define a proc macro: it has no attribute, and `proc_macro`
/// stands for `proc_macro2`.
///
/// ```
/// # use announce_core as announce;
/// # use proc_macro2 as proc_macro;
/// # use template_quote as quote;
/// /// Drops non-`pub` fields and lists the kept ones in META.
/// pub fn public_fields(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
///     announce::helper::transformer(input.into(), |mut entry| {
///         // `entry` is an `announce::helper::Entry`.
///         let mut item: syn::ItemStruct = syn::parse2(entry.item.clone())?;
///         if let syn::Fields::Named(fields) = &mut item.fields {
///             let all = std::mem::take(&mut fields.named);
///             fields.named = all
///                 .into_iter()
///                 .filter(|f| matches!(f.vis, syn::Visibility::Public(_)))
///                 .collect();
///             let kept = fields.named.iter().filter_map(|f| f.ident.as_ref());
///             entry.meta.push(syn::parse_quote!(fields(#(#kept),*)));
///         }
///         entry.item = quote::quote!(#item);
///         Ok(entry)
///     })
///     .into()
/// }
/// #
/// # let out = public_fields(syn::parse_quote! {
/// #     [t!, cb!] a::S! => [] { struct S { pub a: u8, b: u8, pub c: u8 } } REST
/// # });
/// # let out = out.to_string();
/// # assert!(out.starts_with("cb !"), "{}", out);
/// # assert!(out.contains("struct S { pub a : u8 , pub c : u8 }"), "{}", out);
/// # assert!(out.contains("fields (a , c)"), "{}", out);
/// ```
///
/// The function can be tested with `proc_macro2` streams, because it does not use the
/// `proc_macro` crate:
///
/// ```
/// # use announce_core as announce;
/// use announce::helper::transformer;
/// use proc_macro2::TokenStream;
///
/// let run = |input: TokenStream| {
///     transformer(input, |mut entry| {
///         entry.meta.push(syn::parse_quote!(seen));
///         Ok(entry)
///     })
///     .to_string()
/// };
/// // The next element gets the rewritten entry.
/// let out = run(syn::parse_quote! { [t!, cb!] a::S! => [] { struct S; } REST });
/// assert!(out.starts_with("cb !") && out.contains("[seen]"), "{}", out);
/// // Nothing to transform, and nothing to forward to.
/// assert!(run(syn::parse_quote! { [t!, cb!] REST }).contains("compile_error"));
/// assert!(run(syn::parse_quote! { [t!] a::S! => [] { struct S; } REST }).contains("compile_error"));
/// ```
///
/// The macro `transformer!` (`announce::helper::transformer! { pub fn name(entry) { .. } }`)
/// does the same and defines the `#[proc_macro]` function too. It is kept for
/// compatibility; write the function above instead.
pub fn transformer(input: TokenStream, f: impl FnOnce(Entry) -> Result<Entry>) -> TokenStream {
    let run = || -> Result<TokenStream> {
        let input: Input = syn::parse2(input)?;
        let newest = input.newest().cloned().ok_or_else(|| {
            Error::new(
                proc_macro2::Span::call_site(),
                "announce: a transformer must come after the definition it transforms",
            )
        })?;
        input.forward_transformed(f(newest)?)
    };
    run().unwrap_or_else(|e| e.to_compile_error())
}

/// Defines a transformer: the same as the function [`transformer()`], in the form
/// of a macro.
///
/// **Kept for compatibility; prefer the function [`transformer()`]**, called from a
/// `#[proc_macro]` function you write yourself. This macro is a thin wrapper that
/// defines that function: the body gets the newest entry as an [`Entry`] (as `entry`,
/// by value, mutable) and returns `syn::Result<Entry>`.
///
/// ```ignore
/// announce::helper::transformer! {
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
///
/// is the same as
///
/// ```
/// # use announce_core as announce;
/// # use proc_macro2 as proc_macro; // a doctest cannot define a `#[proc_macro]`
/// # use template_quote as quote;
/// /// Drops non-`pub` fields and lists the kept ones in META.
/// pub fn public_fields(ts: proc_macro::TokenStream) -> proc_macro::TokenStream {
///     announce::helper::transformer(ts.into(), |mut entry| {
///         let mut item: syn::ItemStruct = syn::parse2(entry.item.clone())?;
///         // ..
///         entry.item = quote::quote!(#item);
///         entry.meta.push(syn::parse_quote!(fields(a, c)));
///         Ok(entry)
///     })
///     .into()
/// }
/// #
/// # let out = public_fields(syn::parse_quote! { [t!, cb!] a::S! => [] { struct S; } REST });
/// # let out = out.to_string();
/// # assert!(out.starts_with("cb !") && out.contains("fields (a , c)"), "{}", out);
/// ```
#[doc(hidden)]
#[macro_export]
macro_rules! transformer {
    ($(#[$m:meta])* $vis:vis fn $name:ident ($entry:ident) $body:block) => {
        $(#[$m])*
        #[proc_macro]
        $vis fn $name(__announce_ts: ::proc_macro::TokenStream) -> ::proc_macro::TokenStream {
            ::core::convert::Into::into($crate::helper::transformer(
                ::core::convert::Into::into(__announce_ts),
                |__announce_entry: $crate::helper::Entry|
                 -> $crate::helper::__private::syn::Result<$crate::helper::Entry> {
                    #[allow(unused_mut)]
                    let mut $entry: $crate::helper::Entry = __announce_entry;
                    $body
                },
            ))
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
