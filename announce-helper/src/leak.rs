//! Deterministic numbering for [`type_leak`] (feature `type-leak`).
//!
//! `type_leak::Referrer` numbers types in an order that changes between macro
//! invocations. [`StableReferrer`] numbers them by token text instead, so a
//! definer and a consumer get the same indices.
//!
//! Both sides must see the same tokens. A definer sees the item before `#[cfg]`
//! evaluation and before `crate::` becomes `$crate::`. Run both sides on the
//! entry payload, and call [`normalize_crate`] on it first.
//!
//! With `#[announce(repeater = ..)]` none of this is needed: the definer builds
//! the table and the callback applies it with `TypeLeak::apply`.

use proc_macro2::TokenStream;
use syn::Type;
use template_quote::ToTokens;

/// Rewrites a path-leading `$crate` to `crate` everywhere in `item`. This undoes
/// announce's `crate::` to `$crate::` rewrite, before you run a
/// `type_leak::Leaker` on a received payload.
pub fn normalize_crate(item: &mut syn::Item) {
    struct V;
    impl syn::visit_mut::VisitMut for V {
        fn visit_path_mut(&mut self, p: &mut syn::Path) {
            if let Some(first) = p.segments.first_mut() {
                if first.ident == "$crate" {
                    first.ident = proc_macro2::Ident::new("crate", first.ident.span());
                }
            }
            syn::visit_mut::visit_path_mut(self, p);
        }
    }
    syn::visit_mut::VisitMut::visit_item_mut(&mut V, item);
}

/// A `type_leak::Referrer` with a deterministic order.
#[derive(Clone, Debug)]
pub struct StableReferrer {
    inner: type_leak::Referrer,
    order: Vec<Type>,
}

impl StableReferrer {
    /// Renumbers `referrer`'s types by their token text.
    pub fn new(referrer: type_leak::Referrer) -> Self {
        let mut order: Vec<Type> = referrer.iter().cloned().collect();
        order.sort_by_cached_key(|t| t.to_token_stream().to_string());
        order.dedup();
        StableReferrer { inner: referrer, order }
    }

    /// The leaked types in stable order: `types()[i]` has index `i`.
    pub fn types(&self) -> &[Type] {
        &self.order
    }

    /// Whether no type was leaked.
    pub fn is_empty(&self) -> bool {
        self.order.is_empty()
    }

    /// The stable index of `ty`.
    pub fn index_of(&self, ty: &Type) -> Option<usize> {
        self.order.iter().position(|t| t == ty)
    }

    fn stable_fn<'a>(
        order: &'a [Type],
        mut f: impl FnMut(Type, usize) -> Type + 'a,
    ) -> impl FnMut(Type, usize) -> Type + 'a {
        move |ty, _unstable| {
            let i = order
                .iter()
                .position(|t| *t == ty)
                .expect("type-leak returned a type outside its own list");
            f(ty, i)
        }
    }

    /// `Referrer::expand` with stable indices.
    pub fn expand(&self, ty: Type, f: impl FnMut(Type, usize) -> Type) -> Type {
        self.inner.expand(ty, Self::stable_fn(&self.order, f))
    }

    /// `Referrer::into_visitor` with stable indices: rewrites every leaked type
    /// in an item.
    pub fn visit_item_mut(&self, item: &mut syn::Item, f: impl FnMut(Type, usize) -> Type) {
        let mut v = self.inner.clone().into_visitor(Self::stable_fn(&self.order, f));
        syn::visit_mut::VisitMut::visit_item_mut(&mut v, item);
    }

    /// The types as a tuple type `(T0, T1, ..)` (handy for META).
    pub fn to_tuple(&self) -> TokenStream {
        let t = &self.order;
        template_quote::quote! { ( #(#t,)* ) }
    }
}
