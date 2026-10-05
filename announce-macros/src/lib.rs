//! Proc macros behind [`announce`](https://docs.rs/announce). Use them through the
//! `announce` crate; the paths here are not a stable interface.

mod cratepath;
mod generate;
mod leak;
mod options;

use proc_macro::TokenStream;
use proc_macro2::{Ident, Span, TokenStream as TokenStream2};
use std::sync::atomic::{AtomicU64, Ordering};
use template_quote::quote;

fn random_u64() -> u64 {
    use std::hash::{BuildHasher, Hasher};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let mut h = std::collections::hash_map::RandomState::new().build_hasher();
    h.write_u64(COUNTER.fetch_add(1, Ordering::Relaxed));
    h.finish()
}

/// Emits a trampoline macro for the annotated item. See the `announce` crate docs.
#[proc_macro_attribute]
pub fn announce(attr: TokenStream, item: TokenStream) -> TokenStream {
    match announce_impl(attr.into(), item.into()) {
        Ok(ts) => ts.into(),
        Err(e) => e.to_compile_error().into(),
    }
}

fn announce_impl(attr: TokenStream2, item: TokenStream2) -> syn::Result<TokenStream2> {
    let opts: options::Options = syn::parse2(attr.clone())?;
    let parsed: syn::Item = syn::parse2(item.clone())?;
    generate::ident_vis(&parsed)?;
    let krate = opts.krate();
    let random = random_u64();
    let cap = Ident::new(
        &format!("__AnnounceCap_{}", generate::hex(random)),
        Span::call_site(),
    );
    let random_lit = syn::LitInt::new(&format!("{}u64", random), Span::call_site());
    let emit_item = opts.mock.is_none() && opts.emit_item;
    // DESIGN.md section 4.2: the derive sees the copy with every nested cfg
    // evaluated in this crate, then `remove` deletes the dummy.
    Ok(quote! {
        #(if emit_item) { #item }
        #[derive(#krate::__private::Capture)]
        #[__announce(#random_lit; #attr)]
        #[#krate::__private::remove]
        #[allow(dead_code, non_camel_case_types)]
        enum #cap { V = { #item 0 } }
    })
}

#[doc(hidden)]
#[proc_macro_attribute]
pub fn remove(_attr: TokenStream, _item: TokenStream) -> TokenStream {
    TokenStream::new()
}

announce_helper::transformer! {
    /// Transformer that strips the newest entry's item down to signatures:
    /// provided trait methods and fns in modules get a placeholder body
    /// `{ ::core::unimplemented!() }` (bodies of fns returning `impl Trait`
    /// are kept). Use it in the continuation list, right after the
    /// definition: `F! { [F!, ::announce::signature_only!, callback!] .. }`.
    pub fn signature_only(entry) {
        let mut item: syn::Item = syn::parse2(entry.item.clone())?;
        announce_helper::signature_only(&mut item);
        entry.item = template_quote::ToTokens::to_token_stream(&item);
        Ok(entry)
    }
}

#[doc(hidden)]
#[proc_macro_derive(Capture, attributes(__announce))]
pub fn capture(input: TokenStream) -> TokenStream {
    match capture_impl(input.into()) {
        Ok(ts) => ts.into(),
        Err(e) => e.to_compile_error().into(),
    }
}

fn capture_impl(input: TokenStream2) -> syn::Result<TokenStream2> {
    let di: syn::DeriveInput = syn::parse2(input)?;
    let attr = di
        .attrs
        .iter()
        .find(|a| a.path().is_ident("__announce"))
        .ok_or_else(|| {
            syn::Error::new_spanned(
                &di.ident,
                "announce: missing #[__announce] on capture dummy",
            )
        })?;
    let args: options::CaptureArgs = attr.parse_args()?;
    let expr = match &di.data {
        syn::Data::Enum(e) => e
            .variants
            .first()
            .and_then(|v| v.discriminant.as_ref())
            .map(|d| &d.1),
        _ => None,
    };
    let stmts = match expr {
        Some(syn::Expr::Block(b)) => &b.block.stmts,
        _ => {
            return Err(syn::Error::new_spanned(
                &di.ident,
                "announce: malformed capture dummy",
            ))
        }
    };
    // The copy is gone when the item itself was `#[cfg]`'d out.
    let item = stmts.iter().find_map(|s| match s {
        syn::Stmt::Item(i) => Some(i.clone()),
        _ => None,
    });
    match item {
        Some(item) => generate::generate(item, &args.opts, args.random),
        None => Ok(TokenStream2::new()),
    }
}
