//! Numbering by token text for [`type_leak`].
//!
//! `type_leak::Referrer` numbers its entries by context (in order of first
//! appearance) and then by token text. [`StableReferrer`] numbers the required
//! entries by token text alone. A definer and a consumer then get the same indices
//! even when their analyses give the contexts other ids.
//!
//! Both sides must see the same tokens. A definer sees the item before `#[cfg]`
//! evaluation and before `crate::` becomes `$crate::`. So run both sides on the
//! entry payload, and call [`normalize_crate`] on it first.
//!
//! With `#[announce(repeater = ..)]` none of this is needed. The definer builds the
//! table, the `type_leak(..)` META carries it, and the callback applies it with
//! [`TypeLeak::apply`](crate::helper::TypeLeak::apply).

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
            .map(|(ix, t)| {
                referrer
                    .must(ix)
                    .then(|| order.iter().position(|o| o == t))
                    .flatten()
            })
            .collect();
        StableReferrer {
            inner: referrer,
            order,
            stable,
        }
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
    pub fn expand(
        &self,
        context: type_leak::ContextId,
        ty: Type,
        f: impl FnMut(Type, usize) -> Type,
    ) -> Type {
        self.inner
            .expand(context, ty, Self::stable_fn(&self.stable, f))
    }

    /// `Referrer::visitor` with stable indices: rewrites every leaked type in an
    /// item. `context` is what the `intern_*` method of the `Leaker` returned.
    pub fn visit_item_mut(
        &self,
        context: type_leak::ContextId,
        item: &mut syn::Item,
        f: impl FnMut(Type, usize) -> Type,
    ) {
        let mut v = self
            .inner
            .clone()
            .visitor(context, Self::stable_fn(&self.stable, f));
        syn::visit_mut::VisitMut::visit_item_mut(&mut v, item);
    }

    /// The types as a tuple type `(T0, T1, ..)` (handy for META).
    pub fn to_tuple(&self) -> TokenStream {
        let t = &self.order;
        template_quote::quote! { ( #(#t,)* ) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn squash(ts: impl ToTokens) -> String {
        ts.to_token_stream()
            .to_string()
            .split_whitespace()
            .collect()
    }

    fn analyse(item: &syn::ItemStruct) -> (StableReferrer, type_leak::ContextId) {
        let mut l = type_leak::Leaker::new();
        let context = l.intern_struct(item, true).unwrap();
        (StableReferrer::new(l.finish()), context)
    }

    fn t(i: usize) -> Type {
        syn::parse_str(&format!("T{}", i)).unwrap()
    }

    #[test]
    fn normalize_crate_leaves_ordinary_paths_alone() {
        // A real `$crate` ident cannot be built outside a proc macro
        // (`Ident::new("$crate")` panics), so only the no-op side is testable here.
        let src = "struct S { a: crate::A, b: self::B, c: ::other::C, d: other::D<crate::E> }\n\
                   fn f() -> crate::R { crate::g::<crate::T>(self::x, ::std::mem::drop) }";
        let file: syn::File = syn::parse_str(src).unwrap();
        for item in file.items {
            let before = squash(&item);
            let mut it = item;
            normalize_crate(&mut it);
            assert_eq!(squash(&it), before);
        }
    }

    #[test]
    fn normalize_crate_is_idempotent_and_walks_nested_items() {
        let mut it: syn::Item = syn::parse_quote! {
            mod m { pub trait Tr: crate::Base { fn f(x: crate::X) -> Vec<crate::Y>; } }
        };
        let before = squash(&it);
        normalize_crate(&mut it);
        normalize_crate(&mut it);
        assert_eq!(squash(&it), before);
    }

    #[test]
    fn empty_referrer() {
        let item: syn::ItemStruct =
            syn::parse_quote! { pub struct S { a: u8, b: ::std::string::String, c: crate::X } };
        // Primitives and `crate::` paths need allowing, as announce does.
        let mut config = type_leak::LeakerConfig::new();
        config.allow_primitive();
        config.allow_crate();
        let mut l = type_leak::Leaker::from_config(config);
        let context = l.intern_struct(&item, true).unwrap();
        let r = StableReferrer::new(l.finish());
        assert!(r.is_empty());
        assert!(r.types().is_empty());
        assert_eq!(squash(r.to_tuple()), "()");
        let ty: Type = syn::parse_quote!(Foo);
        assert_eq!(r.index_of(&ty), None);
        // `expand` and `visit_item_mut` never call the closure.
        let out = r.expand(context, ty.clone(), |_, _| panic!("no entries"));
        assert_eq!(squash(out), "Foo");
        let mut it = syn::Item::Struct(item.clone());
        r.visit_item_mut(context, &mut it, |_, _| panic!("no entries"));
        assert_eq!(squash(&it), squash(&item));
    }

    #[test]
    fn to_tuple_lists_the_types_in_stable_order() {
        let item: syn::ItemStruct = syn::parse_quote! { pub struct S { a: Foo, b: Bar } };
        let (r, _) = analyse(&item);
        assert!(!r.is_empty());
        assert_eq!(squash(r.to_tuple()), "(Bar,Foo,)");
        let one: syn::ItemStruct = syn::parse_quote! { pub struct S { a: Foo } };
        assert_eq!(squash(analyse(&one).0.to_tuple()), "(Foo,)");
    }

    #[test]
    fn index_of_is_none_for_unknown_and_optional_types() {
        let item: syn::ItemStruct =
            syn::parse_quote! { pub struct S { a: Foo, b: ::other::X, c: Bar } };
        let (r, _) = analyse(&item);
        assert_eq!(r.index_of(&syn::parse_quote!(Bar)), Some(0));
        assert_eq!(r.index_of(&syn::parse_quote!(Foo)), Some(1));
        assert_eq!(r.index_of(&syn::parse_quote!(::other::X)), None);
        assert_eq!(r.index_of(&syn::parse_quote!(Baz)), None);
    }

    #[test]
    fn expand_leaves_optional_entries_as_written() {
        let item: syn::ItemStruct = syn::parse_quote! { pub struct S { a: Foo, b: ::other::X } };
        let (r, context) = analyse(&item);
        let x: Type = syn::parse_quote!(::other::X);
        // The closure is not called for an optional entry.
        assert_eq!(
            squash(r.expand(context, x.clone(), |_, _| panic!("optional"))),
            "::other::X"
        );
        let mut seen = Vec::new();
        let foo = r.expand(context, syn::parse_quote!(Foo), |ty, i| {
            seen.push((squash(&ty), i));
            t(i)
        });
        assert_eq!(squash(foo), "T0");
        assert_eq!(seen, [("Foo".to_string(), 0)]);
    }

    #[test]
    fn visit_item_mut_passes_the_original_type_and_the_stable_index() {
        let item: syn::ItemStruct = syn::parse_quote! { pub struct S { a: Foo, b: Bar, c: Foo } };
        let (r, context) = analyse(&item);
        assert_eq!(r.types().len(), 2);
        let mut seen: Vec<(String, usize)> = Vec::new();
        let mut it = syn::Item::Struct(item);
        r.visit_item_mut(context, &mut it, |ty, i| {
            seen.push((squash(&ty), i));
            t(i)
        });
        let out = squash(&it);
        assert!(
            out.contains("a:T1") && out.contains("b:T0") && out.contains("c:T1"),
            "{}",
            out
        );
        assert!(
            seen.iter()
                .all(|(ty, i)| (ty == "Foo" && *i == 1) || (ty == "Bar" && *i == 0)),
            "{:?}",
            seen
        );
        assert!(seen.len() >= 3, "{:?}", seen);
    }

    #[test]
    fn numbering_does_not_depend_on_declaration_order() {
        let a: syn::ItemStruct = syn::parse_quote! { pub struct S { a: Foo, b: Bar, c: Baz } };
        let b: syn::ItemStruct = syn::parse_quote! { pub struct S { c: Baz, b: Bar, a: Foo } };
        let (ra, _) = analyse(&a);
        let (rb, _) = analyse(&b);
        assert_eq!(squash(ra.to_tuple()), squash(rb.to_tuple()));
        for ty in ["Foo", "Bar", "Baz"] {
            let ty: Type = syn::parse_str(ty).unwrap();
            assert_eq!(ra.index_of(&ty), rb.index_of(&ty));
        }
    }

    #[test]
    fn the_same_type_is_one_entry() {
        let item: syn::ItemStruct =
            syn::parse_quote! { pub struct S { a: Foo, b: Foo, c: Vec<Foo> } };
        let (r, _) = analyse(&item);
        let names: Vec<String> = r.types().iter().map(squash).collect();
        assert!(names.contains(&"Foo".to_string()), "{:?}", names);
        let mut dedup = names.clone();
        dedup.dedup();
        assert_eq!(dedup, names);
    }
}
