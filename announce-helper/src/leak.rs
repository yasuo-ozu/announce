//! Deterministic numbering for [`type_leak`] (feature `type-leak`).
//!
//! `type_leak::Referrer` numbers the leaked types in `HashSet` order, which
//! differs between macro invocations. A definer and a consumer that both run
//! the leaker on the same item therefore disagree on the indices, and the
//! indices have to be shipped in META. [`StableReferrer`] renumbers the types
//! by their token text, so both sides compute the same indices independently.
//!
//! **Only when both sides see the same tokens.** An attribute on the
//! definition sees the item *before* cfg evaluation and before announce
//! rewrites `crate::` to `$crate::`. A consumer sees the evaluated,
//! rewritten payload. type-leak interns `crate::X` but not `$crate::X`, so the
//! two lists (and the indices) can differ silently. Run both sides on the
//! same token stream (e.g. both on the entry payload), or ship the indices in
//! `meta =`. [`normalize_crate`] turns `$crate::` back into `crate::` in an
//! item received through a trampoline, so the leaker treats both sides alike.
//!
//! (With `#[announce(repeater = ..)]`, the definer builds the table and the
//! callback only applies it (`TypeLeak::apply`), so no recomputation is
//! needed at all.)

use proc_macro2::TokenStream;
use syn::Type;
use template_quote::ToTokens;

/// Rewrites a path-leading `$crate` to `crate` everywhere in `item`, undoing
/// announce's `crate::` → `$crate::` rewrite before running a
/// `type_leak::Leaker` on a received payload (AUDIT #23).
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

    /// The leaked types, in stable order: `types()[i]` has index `i`.
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

    /// `Referrer::into_visitor` with stable indices: rewrites every leaked
    /// type in an item (respecting generic shadowing, as type-leak does).
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
