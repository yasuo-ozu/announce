//! Numbering by token text for [`type_leak`] (feature `type-leak`).
//!
//! `type_leak::Referrer` numbers its entries by context (in order of first
//! appearance) and then by token text. [`StableReferrer`] numbers the required
//! entries by token text alone, so a definer and a consumer get the same
//! indices even when their analyses give the contexts other ids.
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

/// A `type_leak::Referrer` whose required entries are numbered by token text.
///
/// Optional entries (absolute paths of other crates, which a `Referrer`
/// replaces only when it is made strict) have no number: they are left as
/// written.
#[derive(Clone, Debug)]
pub struct StableReferrer {
    inner: type_leak::Referrer,
    /// The distinct types of the required entries, sorted by token text.
    order: Vec<Type>,
    /// The stable index of each entry of `inner`, `None` for an optional one.
    stable: Vec<Option<usize>>,
}

impl StableReferrer {
    /// Renumbers `referrer`'s required entries by their token text.
    pub fn new(referrer: type_leak::Referrer) -> Self {
        let mut order: Vec<Type> = referrer.required().map(|(_, t)| t.clone()).collect();
        order.sort_by_cached_key(|t| t.to_token_stream().to_string());
        order.dedup();
        let stable = referrer
            .iter()
            .enumerate()
            .map(|(ix, t)| referrer.must(ix).then(|| order.iter().position(|o| o == t)).flatten())
            .collect();
        StableReferrer { inner: referrer, order, stable }
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
        stable: &'a [Option<usize>],
        mut f: impl FnMut(Type, usize) -> Type + 'a,
    ) -> impl FnMut(Type, usize) -> Type + 'a {
        // An optional entry is replaced only by a strict `Referrer`, which
        // this is not: leave it as written if it ever shows up.
        move |ty, unstable| match stable[unstable] {
            Some(i) => f(ty, i),
            None => ty,
        }
    }

    /// `Referrer::expand` with stable indices. `context` is what the `intern_*`
    /// method of the `Leaker` returned.
    pub fn expand(&self, context: type_leak::ContextId, ty: Type, f: impl FnMut(Type, usize) -> Type) -> Type {
        self.inner.expand(context, ty, Self::stable_fn(&self.stable, f))
    }

    /// `Referrer::visitor` with stable indices: rewrites every leaked type in an
    /// item. `context` is what the `intern_*` method of the `Leaker` returned.
    pub fn visit_item_mut(
        &self,
        context: type_leak::ContextId,
        item: &mut syn::Item,
        f: impl FnMut(Type, usize) -> Type,
    ) {
        let mut v = self.inner.clone().visitor(context, Self::stable_fn(&self.stable, f));
        syn::visit_mut::VisitMut::visit_item_mut(&mut v, item);
    }

    /// The types as a tuple type `(T0, T1, ..)` (handy for META).
    pub fn to_tuple(&self) -> TokenStream {
        let t = &self.order;
        template_quote::quote! { ( #(#t,)* ) }
    }
}
