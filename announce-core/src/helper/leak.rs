//! Numbering by token text for [`type_leak`].
//!
//! `type_leak::Referrer` numbers its entries by context (in order of first
//! appearance) and then by token text. [`StableReferrer`] numbers the entries by token text
//! alone. A definer and a consumer then get the same indices
//! even when their analyses give the contexts other ids.
//!
//! Both sides must see the same tokens. A definer sees the item before `#[cfg]`
//! evaluation and before `crate::` becomes `$crate::`. So run both sides on the
//! entry payload, and call [`normalize_crate`] on it first.
//!
//! With `#[announce(repeater = ..)]` none of this is needed. The definer builds the
//! table, the `type_leak(..)` META carries it, and the callback applies it with
//! [`TypeLeak::try_apply`](crate::helper::TypeLeak::try_apply).

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

/// A `type_leak::Referrer` whose entries are numbered by token text.
///
/// Every live entry has a number, the position of its type among the distinct types of the
/// entries, sorted by token text. An absolute path of another crate is an entry only if the
/// analysis was not told that the crate is the same on both sides
/// (`type_leak::Rules::external_crates()`, which announce uses).
#[derive(Clone, Debug)]
pub struct StableReferrer {
    inner: type_leak::Referrer,
    /// The distinct types of the entries, sorted by token text.
    order: Vec<Type>,
    /// The stable index of each entry of `inner`, by its `EntryId`; `None` for an index that
    /// is no live entry (one that `LeakerResult::ignore` removed).
    stable: Vec<Option<usize>>,
}

/// The repeater that [`StableReferrer`] has the visitor write, to read the entry back from its
/// `KEY`.
const MARK: &str = "__AnnounceStable";

impl StableReferrer {
    /// Renumbers `referrer`'s entries by their token text.
    pub fn new(referrer: type_leak::Referrer) -> Self {
        let mut order: Vec<Type> = referrer.iter().map(|(_, t, _)| t.clone()).collect();
        order.sort_by_cached_key(|t| t.to_token_stream().to_string());
        order.dedup();
        let mut stable = Vec::new();
        for (entry, ty, _) in referrer.iter() {
            stable.resize(stable.len().max(entry.index() + 1), None);
            stable[entry.index()] = order.iter().position(|o| o == ty);
        }
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

    /// The stable index of the entry `entry` of the `Referrer`, which [`Referrer::iter`] gives.
    ///
    /// [`Referrer::iter`]: type_leak::Referrer::iter
    pub fn stable_index(&self, entry: type_leak::EntryId) -> Option<usize> {
        self.stable.get(entry.index()).copied().flatten()
    }

    /// The marker projection: the visitor writes `<Self as MARK<ARGS, ix>>::Type` for the entry
    /// `ix` (the `id` is 0, so the `KEY` is the index), which [`StableRewrite`] finds again.
    fn projection() -> type_leak::Projection {
        let path: syn::Path = syn::Path::from(proc_macro2::Ident::new(
            MARK,
            proc_macro2::Span::call_site(),
        ));
        let param = proc_macro2::Ident::new("__S", proc_macro2::Span::call_site());
        type_leak::Projection::new(path, 0, type_leak::ImplSelf::Param(param))
    }

    /// `Referrer::visitor` with stable indices: rewrites every leaked type of an item (or of a
    /// type, or of anything a `VisitMut` method takes) by `f`, which gets the type of the entry
    /// as the definer wrote it and its stable index. `context` is what the `intern_*` method of
    /// the `Leaker` returned. `visit` is the `VisitMut` call of the visitor it gets
    /// (`|v| v.visit_item_mut(&mut item)`).
    pub fn rewrite<T>(
        &self,
        context: type_leak::ContextId,
        value: &mut T,
        visit: impl Fn(&mut dyn syn::visit_mut::VisitMut, &mut T),
        f: impl FnMut(Type, usize) -> Type,
    ) {
        let projection = Self::projection();
        {
            let mut v = self.inner.visitor(context, &projection);
            visit(&mut v, value);
        }
        // The entries are written through the marker: read them back, and call `f`.
        let types: Vec<Option<&Type>> = {
            let mut types = vec![None; self.stable.len()];
            for (entry, ty, _) in self.inner.iter() {
                types[entry.index()] = Some(ty);
            }
            types
        };
        let mut rewrite = StableRewrite {
            stable: self,
            types,
            f,
        };
        visit(&mut rewrite, value);
    }

    /// `Referrer::expand` with stable indices. `context` is what the `intern_*` method of the
    /// `Leaker` returned.
    pub fn expand(
        &self,
        context: type_leak::ContextId,
        mut ty: Type,
        f: impl FnMut(Type, usize) -> Type,
    ) -> Type {
        self.rewrite(context, &mut ty, |v, ty| v.visit_type_mut(ty), f);
        ty
    }

    /// `Referrer::visitor` with stable indices: rewrites every leaked type in an
    /// item. `context` is what the `intern_*` method of the `Leaker` returned.
    pub fn visit_item_mut(
        &self,
        context: type_leak::ContextId,
        item: &mut syn::Item,
        f: impl FnMut(Type, usize) -> Type,
    ) {
        self.rewrite(context, item, |v, item| v.visit_item_mut(item), f);
    }

    /// The types as a tuple type `(T0, T1, ..)` (handy for META).
    pub fn to_tuple(&self) -> TokenStream {
        let t = &self.order;
        template_quote::quote! { ( #(#t,)* ) }
    }
}

/// Finds the paths through [`MARK`] that the visitor of type-leak wrote, and replaces each by the
/// closure's type for its entry.
struct StableRewrite<'a, F> {
    stable: &'a StableReferrer,
    types: Vec<Option<&'a Type>>,
    f: F,
}

impl<F: FnMut(Type, usize) -> Type> StableRewrite<'_, F> {
    /// The index of the entry that `ty`, a path through [`MARK`], is for.
    fn entry_of(ty: &Type) -> Option<usize> {
        let path = match ty {
            Type::Path(syn::TypePath {
                qself: Some(qself),
                path,
            }) => {
                // `<impl_for as MARK<ARGS, KEY>>::Type`: the trait is the first segments of `path`.
                let at = qself.position;
                let mut trait_path = path.clone();
                trait_path.segments = path.segments.iter().take(at).cloned().collect();
                trait_path
            }
            _ => return None,
        };
        let last = path.segments.last()?;
        if last.ident != MARK {
            return None;
        }
        match &last.arguments {
            syn::PathArguments::AngleBracketed(args) => match args.args.last()? {
                syn::GenericArgument::Const(syn::Expr::Lit(syn::ExprLit {
                    lit: syn::Lit::Int(key),
                    ..
                })) => key.base10_parse().ok(),
                _ => None,
            },
            _ => None,
        }
    }
}

impl<F: FnMut(Type, usize) -> Type> syn::visit_mut::VisitMut for StableRewrite<'_, F> {
    fn visit_type_mut(&mut self, ty: &mut Type) {
        let entry = Self::entry_of(ty).and_then(|ix| {
            let original = self.types.get(ix).copied().flatten()?;
            Some((
                original.clone(),
                self.stable.stable.get(ix).copied().flatten()?,
            ))
        });
        match entry {
            Some((original, stable)) => *ty = (self.f)(original, stable),
            None => syn::visit_mut::visit_type_mut(self, ty),
        }
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

    /// The analysis as announce makes it: the absolute paths of other crates are no entries.
    fn analyse(item: &syn::ItemStruct) -> (StableReferrer, type_leak::ContextId) {
        let mut l = type_leak::Leaker::with_rule(type_leak::Rules::new().external_crates());
        let context = l.intern_struct(item);
        (StableReferrer::new(l.result().finish().unwrap()), context)
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
        let rules = type_leak::Rules::new()
            .primitive()
            .crate_paths()
            .external_crates();
        let mut l = type_leak::Leaker::with_rule(rules);
        let context = l.intern_struct(&item);
        let r = StableReferrer::new(l.result().finish().unwrap());
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
    fn index_of_is_none_for_unknown_and_absolute_types() {
        let item: syn::ItemStruct =
            syn::parse_quote! { pub struct S { a: Foo, b: ::other::X, c: Bar } };
        let (r, _) = analyse(&item);
        assert_eq!(r.index_of(&syn::parse_quote!(Bar)), Some(0));
        assert_eq!(r.index_of(&syn::parse_quote!(Foo)), Some(1));
        assert_eq!(r.index_of(&syn::parse_quote!(::other::X)), None);
        assert_eq!(r.index_of(&syn::parse_quote!(Baz)), None);
    }

    #[test]
    fn expand_leaves_absolute_types_as_written() {
        let item: syn::ItemStruct = syn::parse_quote! { pub struct S { a: Foo, b: ::other::X } };
        let (r, context) = analyse(&item);
        let x: Type = syn::parse_quote!(::other::X);
        // The closure is not called for an absolute type: it is no entry.
        assert_eq!(
            squash(r.expand(context, x.clone(), |_, _| panic!("no entry"))),
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
