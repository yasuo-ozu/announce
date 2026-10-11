//! Internal: generic-parameter bookkeeping shared by `#[announce(repeater = ..)]`
//! and [`TypeLeak::apply`](crate::TypeLeak::apply).
//!
//! The `G` of a type is the tuple of the generic parameters it mentions, in
//! order of first appearance. Type parameters are themselves, lifetimes are
//! `&'a ()`, consts are `[(); N]`. `()` if there are none.

use proc_macro2::{TokenStream, TokenTree};
use syn::{GenericParam, Item, Type};
use template_quote::{quote, ToTokens};

/// Every generic parameter declared anywhere in `item` (first declaration of
/// each name wins): item generics, fn signatures (in traits, impls, modules),
/// and generic associated types.
pub fn collect_params(item: &Item) -> Vec<GenericParam> {
    struct V(Vec<GenericParam>);
    impl<'ast> syn::visit::Visit<'ast> for V {
        fn visit_generics(&mut self, g: &'ast syn::Generics) {
            for p in &g.params {
                let name = param_name(p);
                if !self.0.iter().any(|q| param_name(q) == name) {
                    self.0.push(p.clone());
                }
            }
            syn::visit::visit_generics(self, g);
        }
    }
    let mut v = V(Vec::new());
    syn::visit::Visit::visit_item(&mut v, item);
    v.0
}

/// The name of a parameter (`T`, `'a` or `N`).
pub fn param_name(p: &GenericParam) -> String {
    match p {
        GenericParam::Type(t) => t.ident.to_string(),
        GenericParam::Lifetime(l) => l.lifetime.to_string(),
        GenericParam::Const(c) => c.ident.to_string(),
    }
}

/// The parameters of `params` that `ty` mentions, in order of first
/// appearance in `ty`.
pub fn mentioned_params(ty: &Type, params: &[GenericParam]) -> Vec<GenericParam> {
    fn walk(ts: TokenStream, params: &[GenericParam], out: &mut Vec<GenericParam>, prev_quote: &mut bool) {
        for tt in ts {
            match tt {
                TokenTree::Group(g) => {
                    *prev_quote = false;
                    walk(g.stream(), params, out, prev_quote)
                }
                TokenTree::Punct(p) => *prev_quote = p.as_char() == '\'',
                TokenTree::Ident(i) => {
                    let name = if *prev_quote { format!("'{}", i) } else { i.to_string() };
                    *prev_quote = false;
                    if let Some(p) = params.iter().find(|p| param_name(p) == name) {
                        if !out.iter().any(|q| param_name(q) == name) {
                            out.push(p.clone());
                        }
                    }
                }
                TokenTree::Literal(_) => *prev_quote = false,
            }
        }
    }
    let mut out = Vec::new();
    walk(ty.to_token_stream(), params, &mut out, &mut false);
    out
}

/// `(T, &'a (), [(); N],)` for the given parameters.
pub fn encode(params: &[GenericParam]) -> TokenStream {
    let elems = params.iter().map(|p| match p {
        GenericParam::Type(t) => {
            let i = &t.ident;
            quote! { #i }
        }
        GenericParam::Lifetime(l) => {
            let l = &l.lifetime;
            quote! { & #l () }
        }
        GenericParam::Const(c) => {
            let i = &c.ident;
            quote! { [(); #i] }
        }
    });
    quote! { ( #(#elems,)* ) }
}
