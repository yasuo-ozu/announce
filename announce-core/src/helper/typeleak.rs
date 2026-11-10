//! The table of an entry announced with `#[announce(repeater = ..)]`: the
//! `type_leak(..)` META.
//!
//! The definer analyses the item with [`type_leak`] and sends the resulting
//! [`LeakerResult`] to the consumer inside META. Its [`Referrer`] knows the context
//! (the generics and bounds) of every leaked type. The consumer parses it back and
//! rewrites the payload with [`Referrer::visitor`]. So a type is replaced by the
//! entry of the scope where it is written, never by the entry of a same-named type
//! of another scope.
//!
//! [`TypeLeak`] is only the parsed META. It has no methods for the errors or for the
//! rewriting: a callback uses type-leak's API on its fields, or
//! [`Entry::item_resolved`](crate::helper::Entry::item_resolved), which does it in one call.
//!
//! The errors of type-leak (the parts that no table can carry) are in the result too.
//! The definer does not report them: the consumer does. `Entry::item_resolved` reports them
//! all at once. A callback that allows some first uses `allow`, `ignore` and `ignore_if` of
//! [`TypeLeak::result`], then `finish`.
//!
//! The use-site arguments of a replacement come from [`Referrer::complete_args`]. The
//! definer's impls come from [`LeakerResult::repeater_impls`], which encodes the same
//! generics, so the two sides cannot disagree.

use crate::helper::keys::{need, read_element};
use proc_macro2::{Literal, Span, TokenStream};
use syn::parse::{Parse, ParseStream};
use syn::visit_mut::VisitMut;
use syn::{Error, Item, Path, Result, Type};
use template_quote::quote;
use type_leak::{ContextId, ImplSelf, LeakerResult, Projection, Referrer};

/// Type table of an entry announced with `#[announce(repeater = ..)]`, from
/// [`Entry::type_leak`](crate::helper::Entry::type_leak).
///
/// The payload is the original item. The definer numbered its relative types: the
/// *entries* `0, 1, ..` of the [`result`](Self::result). An entry is a type in the
/// *context* where it is written (the item, a method, a generic associated type),
/// with that context's generics. So the same text in two methods with different
/// generics is two entries, and a lifetime that a `for<'b>` of the item binds
/// belongs to the context of its type. `&Local`, also with an elided lifetime, is
/// the entry `Local`.
///
/// The entry `ix` resolves from any crate as `<() as REPEATER<ARGS, KEY>>::Type`, where
/// `KEY` is `projection.key(ix)`: the `id` of the table in the high 64 bits and the entry in
/// the low ones. `ARGS` encodes the generics that the type uses: a type parameter as itself,
/// a lifetime as `&'a ()`, a `usize` const as `[(); N]`, and a const of another primitive type
/// (`bool`, `char`, `u8` .. `u128`, `i8` .. `i128`, `isize`) through the const
/// parameter that the repeater has for that type. A lifetime bound (`T: 'a`) is
/// encoded too (`&'a T`), and the impl does not declare it.
/// [`Entry::item_resolved`](crate::helper::Entry::item_resolved) rewrites an item that way.
///
/// The META element is
///
/// ```text
/// type_leak(repeater = $crate::Repeater, id = 42u64, context = 0, result = [ .. ])
/// ```
///
/// `result` is the `type_leak::LeakerResult` of the item in type-leak's own token format:
/// the `Referrer`, then the errors that remain, if any. It is opaque to announce: do not
/// parse it yourself. Its first token is a format tag, and a version of type-leak that does
/// not match is an error. `context` is the top-level `ContextId` of the item in it, where a
/// callback rewrites the item. `repeater` and `id` are there when the definition has
/// `repeater = ..`, both or neither: without them the META has no impl behind it, and it is
/// there for the errors of type-leak that the consumer reports ([`result`](Self::result)).
/// Other keys are ignored when reading with `syn::parse2::<TypeLeak>(meta)`.
///
/// A callback calls `Entry::item_resolved`. To decide on the errors first, it uses type-leak's
/// API on the fields: `allow`, `ignore` or `ignore_if` of `result`, then `finish` for the
/// `Referrer`, then its `visitor` (or `complete_args` for a use site):
///
/// ```
/// # use announce_core as announce;
/// use announce::helper::TypeLeak;
/// use syn::visit_mut::VisitMut;
///
/// let meta: proc_macro2::TokenStream = {
///     let mut leaker = announce::helper::type_leak::Leaker::new();
///     let item: syn::ItemStruct = syn::parse_quote! { struct S { a: Local } };
///     let context = leaker.intern_struct(&item);
///     let projection = TypeLeak::projection_of(syn::parse_quote!(crate::Repeater), 7);
///     TypeLeak::new(Some(projection), context, leaker.result()).meta()
/// };
/// let tl: TypeLeak = syn::parse2(meta)?;
/// // Errors of type-leak would be accepted here: `tl.result.ignore_if(..)`.
/// let referrer = tl.result.clone().finish().unwrap();
/// let projection = tl.projection.as_ref().expect("a repeater");
/// let mut item: syn::Item = syn::parse_quote! { struct S { a: Local } };
/// referrer.visitor(tl.context, projection).visit_item_mut(&mut item);
/// let written = template_quote::ToTokens::to_token_stream(&item).to_string();
/// assert!(written.contains("crate :: Repeater"));
/// # Ok::<(), syn::Error>(())
/// ```
#[derive(Clone, Debug)]
pub struct TypeLeak {
    /// How the definer wrote its impls, and so how the entries are written: the repeater trait
    /// (`$crate::Repeater`) and the `id` of the table, which is the `KEY`'s high half, for the
    /// marker `()`. `None` for a definition without `repeater`, which has no impl: its table has
    /// no entries (the definer refuses entries without a `repeater`) and is there for the errors.
    ///
    /// `Referrer::complete_args` takes a `&Projection` even then. Pass `Projection::unused()`:
    /// nothing is written through it.
    pub projection: Option<Projection>,
    /// The top-level context of the item in the [`result`](Self::result). Reading the META checks
    /// that the result has it.
    pub context: ContextId,
    /// The analysis of the item at the definition: the `Referrer` and the errors of type-leak
    /// that remain. Entry `ix` is the entry number of the repeater's `KEY`.
    ///
    /// Use type-leak's API on it: `errors()`, `allow(..)`, `ignore(..)` and `ignore_if(..)`, then
    /// `finish()`, which is the `Referrer`, or the errors that remain: `?` turns them into one
    /// `syn::Error`, each over its whole cause, as `Entry::item_resolved` reports them.
    pub result: LeakerResult,
}

impl TypeLeak {
    /// The table of `result`, for the item whose top-level context is `context`.
    pub fn new(projection: Option<Projection>, context: ContextId, result: LeakerResult) -> Self {
        TypeLeak {
            projection,
            context,
            result,
        }
    }

    /// The projection of a table: the `repeater` trait, the `id` of the table, and the marker `()`
    /// that its impls are for. It is the one that [`LeakerResult::repeater_impls`] writes the impls
    /// by, and the one that [`TypeLeak`] reads from the META.
    pub fn projection_of(repeater: Path, id: u64) -> Projection {
        Projection::new(repeater, id, ImplSelf::Marker(syn::parse_quote!(())))
    }

    /// Rewrites every leaked type in `item` (the entry's payload) to its projection, for
    /// [`Entry::item_resolved`](crate::helper::Entry::item_resolved): the item then resolves from
    /// any crate. The copy is in the scope of the item itself: every generic by its own name.
    ///
    /// It fails if an error of type-leak remains (all of them, as one error, so that one run lists
    /// everything the item cannot carry), if there are types to replace but no
    /// [`projection`](Self::projection), or if the table is inconsistent (an entry has no impl
    /// for the marker, as the definer's impls are). An empty `Referrer` leaves the item as it is.
    pub(crate) fn resolve_into(&self, item: &mut Item) -> Result<()> {
        let referrer = self.result.clone().finish()?;
        if !referrer.is_empty() {
            let projection = self.projection.as_ref().ok_or_else(|| {
                Error::new(
                    Span::call_site(),
                    "announce: the `type_leak(..)` META has types to replace but no `repeater` \
                     and `id`: the definition has no impl to write them by",
                )
            })?;
            let unit: Type = syn::parse_quote!(());
            for (entry, _, context) in referrer.iter() {
                projection.of(&unit, context, entry)?;
            }
        }
        if let Some(projection) = self.projection.as_ref() {
            referrer
                .visitor(self.context, projection)
                .visit_item_mut(item);
        }
        Ok(())
    }

    /// The `type_leak(..)` META element, as the definer writes it.
    pub fn meta(&self) -> TokenStream {
        let (context, result) = (&self.context, &self.result);
        let projection = self.projection.as_ref().map(|p| {
            let (repeater, id) = (&p.path, Literal::u64_suffixed(p.id));
            quote! { repeater = #repeater, id = #id, }
        });
        quote! {
            type_leak(#projection context = #context, result = [#result])
        }
    }
}

/// The `Referrer` of `result` whatever errors remain: they are ignored in a copy
/// (`LeakerResult::ignore_if`), so that the part of each is whole and the entries are those that
/// the impls of `repeater_impls` are written for. Ignoring removes only an entry that has an error
/// of its own (`Reason::NestedSelfInBounds`), which needs an item nested in the analyzed syntax,
/// and announce analyzes none. This is the view of the definer and of the check that the META reads
/// back; a consumer that rewrites an item needs no errors left (`Entry::item_resolved` reports
/// them).
pub(crate) fn ignored_view(result: &LeakerResult) -> Referrer {
    result
        .clone()
        .ignore_if(|_| true)
        .finish()
        .expect("no error remains once every one is ignored")
}

impl Parse for TypeLeak {
    /// Reads the `type_leak(..)` META element ([`meta`](Self::meta)).
    fn parse(input: ParseStream) -> Result<Self> {
        let (mut repeater, mut id, mut context, mut result) = (None, None, None, None);
        let name = read_element(input, "type_leak", |key, input| {
            match key {
                "repeater" => repeater = Some(input.parse::<Path>()?),
                "id" => id = Some(input.parse::<syn::LitInt>()?.base10_parse::<u64>()?),
                "context" => context = Some(input.parse::<ContextId>()?),
                "result" => {
                    let content;
                    syn::bracketed!(content in input);
                    result = Some(content.parse::<LeakerResult>()?);
                    if !content.is_empty() {
                        return Err(content.error("unexpected tokens after the result"));
                    }
                }
                _ => return Ok(false),
            }
            Ok(true)
        })?;
        let old = || {
            Error::new(
                name.span(),
                "announce: the `type_leak(..)` META has no `context` and `result`: the \
                 definition was announced by an older `announce`, whose output this version \
                 cannot read",
            )
        };
        let result = result.ok_or_else(old)?;
        let context = context.ok_or_else(old)?;
        // `Referrer::visitor` and `complete_args` panic for a context that is not one of its own.
        if !ignored_view(&result).has_context(context) {
            return Err(Error::new(
                name.span(),
                format!(
                    "announce: the `type_leak(..)` META names the context {}, which its \
                     `result` does not have",
                    context
                ),
            ));
        }
        // The path names the trait without arguments (`Projection::new` panics otherwise).
        if let Some(repeater) = &repeater {
            let last = repeater.segments.last().map(|segment| &segment.arguments);
            if !matches!(last, Some(syn::PathArguments::None)) {
                return Err(Error::new_spanned(
                    repeater,
                    "announce: the `repeater` of the `type_leak(..)` META names the trait \
                     without arguments",
                ));
            }
        }
        // Both or neither: without them there is no impl.
        let projection = match (repeater, id) {
            (None, None) => None,
            (repeater, id) => Some(Self::projection_of(
                need(repeater, &name, "repeater")?,
                need(id, &name, "id")?,
            )),
        };
        Ok(TypeLeak::new(projection, context, result))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use syn::GenericArgument;
    use type_leak::{Cause, EntryId, Leaker, Place, Rules};

    fn s(ts: impl template_quote::ToTokens) -> String {
        ts.to_token_stream()
            .to_string()
            .split_whitespace()
            .collect()
    }

    fn rules() -> Rules {
        Rules::new().crate_paths().primitive().external_crates()
    }

    /// The table of `item` as announce makes it, with the repeater `::my::R` and the id 7.
    fn table_of(item: &Item) -> TypeLeak {
        let mut leaker = Leaker::with_rule(rules());
        let context = match item {
            Item::Struct(i) => leaker.intern_struct(i),
            Item::Enum(i) => leaker.intern_enum(i),
            Item::Trait(i) => leaker.intern_trait(i),
            _ => panic!("struct, enum or trait"),
        };
        TypeLeak::new(
            Some(TypeLeak::projection_of(syn::parse_quote!(::my::R), 7)),
            context,
            leaker.result(),
        )
    }

    /// The `KEY` argument of the repeater for the entry `ix` of the table of `table_of` (the id 7).
    fn key(ix: usize) -> String {
        let entry: EntryId = syn::parse_str(&ix.to_string()).unwrap();
        let projection = TypeLeak::projection_of(syn::parse_quote!(::my::R), 7);
        format!("{}u128", projection.key(entry))
    }

    fn read(tl: &TypeLeak) -> TypeLeak {
        syn::parse2(tl.meta()).unwrap()
    }

    /// `item` resolved by `tl` (`Entry::item_resolved` is this, for an entry).
    fn resolved(tl: &TypeLeak, item: &Item) -> Item {
        let mut item = item.clone();
        tl.resolve_into(&mut item).unwrap();
        item
    }

    /// `item` written for a use site with `args`, with type-leak's API on the fields of `tl`:
    /// the visitor, then `Referrer::complete_args` (`Projection::unused()` when the table has
    /// none, as the docs of `TypeLeak::projection` say).
    fn resolved_at(tl: &TypeLeak, item: &mut Item, args: &[GenericArgument]) -> Result<()> {
        let referrer = tl.result.clone().finish()?;
        let placeholder = Projection::unused();
        let projection = tl.projection.as_ref().unwrap_or(&placeholder);
        referrer
            .visitor(tl.context, projection)
            .visit_item_mut(item);
        let mut substitute = referrer.complete_args(tl.context, args, projection)?;
        substitute.visit_item_mut(item);
        Ok(())
    }

    /// The entries of `tl`, with its errors ignored in a copy (`ignored_view`).
    fn entries(tl: &TypeLeak) -> Vec<(EntryId, Type)> {
        ignored_view(&tl.result)
            .iter()
            .map(|(entry, ty, _)| (entry, ty.clone()))
            .collect()
    }

    /// The entry of `tl` whose type is `ty`.
    fn lookup(tl: &TypeLeak, ty: Type) -> Option<usize> {
        entries(tl)
            .into_iter()
            .find(|(_, t)| *t == ty)
            .map(|(ix, _)| ix.index())
    }

    #[test]
    fn meta_round_trips() {
        let item: Item = syn::parse_quote! {
            pub trait Tr<'a, T: 'a + ::core::clone::Clone, const N: usize> {
                fn f<U>(&self, a: &'a Local<T>, b: Wrap<U>) -> Other<N>;
                fn g(&self) -> Local<u8>;
            }
        };
        let tl = table_of(&item);
        let back = read(&tl);
        // The indices are the definer's: an impl and a projection agree.
        assert_eq!(back.result.errors(), tl.result.errors());
        assert_eq!(ignored_view(&back.result), ignored_view(&tl.result));
        assert_eq!(back.context, tl.context);
        let projection = back.projection.as_ref().unwrap();
        assert_eq!(s(&projection.path), "::my::R");
        assert_eq!(projection.id, 7);
        assert!(entries(&back).len() >= 3);
        assert_eq!(entries(&back), entries(&tl));
        // The META has the repeater and the id, then the context and the result.
        let meta = s(tl.meta());
        assert!(
            meta.starts_with(&format!(
                "type_leak(repeater=::my::R,id=7u64,context={},result=[\"type-leak-format-",
                tl.context
            )),
            "{}",
            meta
        );
    }

    #[test]
    fn a_table_without_a_projection_has_neither_repeater_nor_id() {
        let item: Item = syn::parse_quote! { struct S<T: Base>(T); };
        let mut tl = table_of(&item);
        tl.projection = None;
        let meta = s(tl.meta());
        assert!(
            meta.starts_with(&format!(
                "type_leak(context={},result=[\"type-leak-format-",
                tl.context
            )),
            "{}",
            meta
        );
        let back = read(&tl);
        assert!(back.projection.is_none());
        assert_eq!(back.result.errors(), tl.result.errors());
        assert_eq!(back.result.errors().len(), 1);
    }

    #[test]
    fn the_errors_travel_in_the_result() {
        let item: Item = syn::parse_quote! {
            struct S<T: Base> { a: Local, b: Wrap<'_>, t: T }
        };
        let tl = table_of(&item);
        assert_eq!(tl.result.errors().len(), 2);
        let back = read(&tl);
        assert_eq!(back.result.errors(), tl.result.errors());
        assert_eq!(entries(&back).len(), 1);
        // The result is `LeakerResult`'s own syntax: the Referrer, then `@ [ {error} .. ]`.
        let meta = s(tl.meta());
        assert!(meta.contains("@["), "{}", meta);
    }

    #[test]
    fn meta_ignores_unknown_keys_and_reports_an_old_format() {
        let tl = table_of(&syn::parse_quote! { struct S { a: Local } });
        let meta = tl.meta().to_string();
        let with_extra = meta.replace("id = 7u64", "id = 7u64, future = [1, 2], other");
        assert!(syn::parse_str::<TypeLeak>(&with_extra).is_ok());
        // An unknown value that is not a type ends at the `,` before the next key.
        let unclosed = meta.replace("id = 7u64", "cmp = a < b, id = 7u64");
        assert!(syn::parse_str::<TypeLeak>(&unclosed).is_ok());

        // The META of an announce before `generics` and `types` went, and the one that carried
        // the `Referrer` alone: the errors of type-leak were reported at the definition then.
        let old = "type_leak(repeater = ::my::R, id = 7u64, generics = [()], types = [Local])";
        let err = syn::parse_str::<TypeLeak>(old).unwrap_err();
        assert!(err.to_string().contains("older `announce`"), "{}", err);
        let old = meta.replace("result = [", "referrer = [");
        let err = syn::parse_str::<TypeLeak>(&old).unwrap_err();
        assert!(err.to_string().contains("older `announce`"), "{}", err);
    }

    #[test]
    fn the_keys_and_the_name_of_the_element_are_checked() {
        let tl = table_of(&syn::parse_quote! { struct S { a: Local } });
        let (result, context) = (s(&tl.result), tl.context);
        let meta = |repeater: &str, id: &str, extra: &str| {
            format!(
                "type_leak({} {} context = {}, result = [{} {}])",
                repeater, id, context, result, extra
            )
        };
        let (r, i) = ("repeater = ::my::R,", "id = 7u64,");
        assert!(syn::parse_str::<TypeLeak>(&meta(r, i, "")).is_ok());
        let err = |src: String| syn::parse_str::<TypeLeak>(&src).unwrap_err().to_string();
        // Both or neither.
        assert!(err(meta("", i, "")).contains("missing `repeater`"));
        assert!(err(meta(r, "", "")).contains("missing `id`"));
        let none = syn::parse_str::<TypeLeak>(&meta("", "", "")).unwrap();
        assert!(none.projection.is_none());
        assert!(err(meta(r, i, "extra")).contains("unexpected tokens after the result"));
        // The id is the high half of the `KEY`, a `u64`.
        assert!(err(meta(r, "id = 18446744073709551616u64,", "")).contains("too large"));
        let msg = err(meta(r, i, "").replace("type_leak(", "type_leak_error("));
        assert!(msg.contains("expected `type_leak(..)`, found `type_leak_error`"));
        // Without a context.
        let src = meta(r, i, "").replace(&format!("context = {},", context), "");
        assert!(err(src).contains("older `announce`"));
        // The repeater is a trait without arguments.
        let src = meta("repeater = ::my::R<u8>,", i, "");
        assert!(err(src).contains("without arguments"));
        // The context is one of the result's: another is an error at reading, not a panic where
        // the table is applied.
        let src = meta(r, i, "").replace(&format!("context = {},", context), "context = 9,");
        let message = err(src);
        assert!(message.contains("names the context 9"), "{}", message);
        assert!(message.contains("`result` does not have"), "{}", message);
    }

    #[test]
    fn entries_and_the_projection() {
        let tl = table_of(&syn::parse_quote! {
            struct S<T> { a: Local, b: Wrap<T>, c: u8 }
        });
        let local = lookup(&tl, syn::parse_quote!(Local)).unwrap();
        let wrap = lookup(&tl, syn::parse_quote!(Wrap<T>)).unwrap();
        assert_ne!(local, wrap);
        assert_eq!(lookup(&tl, syn::parse_quote!(Wrap<U>)), None);
        assert_eq!(lookup(&tl, syn::parse_quote!(T)), None);
        assert_eq!(lookup(&tl, syn::parse_quote!(u8)), None);
        // The marker is `()`, the KEY has the id in its high half.
        let projection = tl.projection.as_ref().unwrap();
        let entry = entries(&tl)[wrap].0;
        assert_eq!(projection.key(entry) >> 64, 7);
        assert_eq!(projection.key(entry) as u64 as usize, wrap);
        assert!(matches!(&projection.impl_self, ImplSelf::Marker(t) if s(t) == "()"));
    }

    #[test]
    fn resolving_rewrites_leaked_types_only() {
        let item: Item = syn::parse_quote! {
            struct S<T> { a: Local, b: Wrap<T>, c: ::core::option::Option<Local>, d: u8, e: T }
        };
        let tl = table_of(&item);
        let out = s(resolved(&tl, &item));
        let (local, wrap) = (
            lookup(&tl, syn::parse_quote!(Local)),
            lookup(&tl, syn::parse_quote!(Wrap<T>)).unwrap(),
        );
        let wrap = format!("<()as::my::R<(T,),{}>>::Type", key(wrap));
        assert!(out.contains(&format!("b:{}", wrap)), "{}", out);
        // `Local` is an entry unless a larger absolute type that contains it is
        // (`Option<Local>`, the whole type of a field).
        let local = format!("<()as::my::R<(),{}>>::Type", key(local.unwrap_or(99)));
        let any_entry = |ix: usize| out.contains(&format!("a:<()as::my::R<(),{}>>::Type", key(ix)));
        assert!(
            out.contains(&format!("a:{}", local))
                || entries(&tl).iter().any(|(ix, _)| any_entry(ix.index())),
            "{}",
            out
        );
        assert!(
            !out.contains("a:Local") && !out.contains("c:::core"),
            "{}",
            out
        );
        assert!(out.contains("d:u8") && out.contains("e:T}"), "{}", out);
    }

    #[test]
    fn resolving_tells_scopes_apart() {
        // The same text in two scopes with different
        // generics is two entries; a method generic named like a leaked type
        // does not make `Local` a generic elsewhere.
        let item: Item = syn::parse_quote! {
            trait Tr {
                fn f<Local>(&self, x: Local);
                fn g(&self) -> Local;
                fn h<X: ::core::clone::Clone>(&self) -> Gc<X>;
                fn i<X>(&self) -> Gc<X>;
            }
        };
        let tl = table_of(&item);
        let gc: Vec<usize> = entries(&tl)
            .into_iter()
            .filter(|(_, t)| *t == syn::parse_quote!(Gc<X>))
            .map(|(ix, _)| ix.index())
            .collect();
        assert_eq!(gc.len(), 2, "one entry per context");
        let out = s(resolved(&tl, &item));
        assert!(out.contains("fnf<Local>(&self,x:Local);"), "{}", out);
        let local = lookup(&tl, syn::parse_quote!(Local)).unwrap();
        let g = format!("fng(&self)-><()as::my::R<(),{}>>", key(local));
        assert!(out.contains(&g), "{}", out);
        for ix in gc {
            assert!(
                out.contains(&format!("::my::R<(X,),{}>>::Type", key(ix))),
                "{}",
                out
            );
        }
    }

    #[test]
    fn a_use_site_writes_the_arguments() {
        let item: Item = syn::parse_quote! {
            trait Tr<K, const N: usize> { fn get(&self, key: K, n: [u8; N]) -> Wrap<K>; }
        };
        let tl = table_of(&item);
        // At the item's own scope: the parameters by name.
        let here = resolved(&tl, &item);
        let wrap = lookup(&tl, syn::parse_quote!(Wrap<K>)).unwrap();
        assert!(
            s(&here)
                .contains(&format!("->()as::my::R<(K,),{}>", key(wrap)).replace("->()", "-><()")),
            "{}",
            s(&here)
        );
        // At `impl Tr<u8, 3> for ..`: the arguments are written into the item, the entries too (a
        // const as a block, which an array length takes as well).
        let given: Vec<GenericArgument> = vec![syn::parse_quote!(u8), syn::parse_quote!(3)];
        let mut at = item.clone();
        resolved_at(&tl, &mut at, &given).unwrap();
        let out = s(&at);
        assert!(
            out.contains(&format!(
                "fnget(&self,key:u8,n:[u8;{{3}}])-><()as::my::R<(u8,),{}>>::Type",
                key(wrap)
            )),
            "{}",
            out
        );
        // A missing argument is an error.
        let err = resolved_at(&tl, &mut item.clone(), &given[..1]).unwrap_err();
        assert!(
            err.to_string().contains("missing generic argument"),
            "{}",
            err
        );
    }

    #[test]
    fn const_generics_of_every_primitive_type_are_encoded() {
        let item: Item = syn::parse_quote! {
            struct C<const A: u8, const B: bool, const D: usize, const E: u64, const K: u128> {
                x: inner::W<A, B, D, E, K>,
            }
        };
        let tl = table_of(&item);
        // At the item's own scope: the parameters by name.
        let here = resolved(&tl, &item);
        let args = "(*constdyn::my::R<(),0,false,'\\0',A,Type=()>,\
             *constdyn::my::R<(),0,B,Type=()>,[();D],\
             *constdyn::my::R<(),0,false,'\\0',0,0,0,E,Type=()>,*constdyn::my::R<(),K,Type=()>)";
        assert!(s(&here).contains(args), "{}", s(&here));
        // A site that gives the generics values, as `impl Tr<..> for ..` does.
        let given: Vec<GenericArgument> = vec![
            syn::parse_quote!(1),
            syn::parse_quote!(true),
            syn::parse_quote!(3),
            syn::parse_quote!(9),
            syn::parse_quote!(10),
        ];
        let mut at = item;
        resolved_at(&tl, &mut at, &given).unwrap();
        let args = "(*constdyn::my::R<(),0,false,'\\0',1,Type=()>,\
             *constdyn::my::R<(),0,true,Type=()>,[();{3}],\
             *constdyn::my::R<(),0,false,'\\0',0,0,0,9,Type=()>,*constdyn::my::R<(),10,Type=()>)";
        assert!(s(&at).contains(args), "{}", s(&at));
    }

    #[test]
    fn every_remaining_error_is_reported_at_once() {
        let item: Item = syn::parse_quote! {
            struct S<T: Base> { a: Local, b: Wrap<'_>, c: [u8; two!()], t: T }
        };
        let tl = table_of(&item);
        assert_eq!(tl.result.errors().len(), 3);
        let mut it = item.clone();
        let err = tl.resolve_into(&mut it).unwrap_err();
        // The item is as it was.
        assert_eq!(s(&it), s(&item));
        let all: Vec<String> = err.into_iter().map(|e| e.to_string()).collect();
        assert_eq!(all.len(), 3, "{:?}", all);
        assert!(all[0].contains("use absolute path: 'Base'"), "{:?}", all);
        assert!(all.iter().any(|m| m.contains("'Wrap<'_>'")), "{:?}", all);
        assert!(all.iter().any(|m| m.contains("'two'")), "{:?}", all);
        // The messages are type-leak's own.
        let own: Vec<String> = tl
            .result
            .errors()
            .iter()
            .map(|e| Error::from(e.clone()).to_string())
            .collect();
        assert_eq!(all, own);
        // The direct path, with `finish()` on the result, fails with the same errors.
        assert_eq!(tl.result.clone().finish().unwrap_err().len(), 3);
        assert!(resolved_at(&tl, &mut item.clone(), &[]).is_err());
    }

    #[test]
    fn allow_and_ignore_decide_what_remains() {
        let item: Item = syn::parse_quote! {
            struct S<T: Base> { a: Local, b: Wrap<'_>, t: T }
        };
        let tl = table_of(&item);
        assert_eq!(tl.result.errors().len(), 2);
        // Allow by rule: the part is copied as written.
        let rule = |cause: &Cause, _: Place| cause.starts_with(&syn::parse_quote!(Base));
        let mut one = tl.clone();
        one.result = one.result.allow(rule);
        assert_eq!(one.result.errors().len(), 1);
        assert_eq!(s(&one.result.errors()[0].cause), "Wrap<'_>");
        assert!(one.resolve_into(&mut item.clone()).is_err());
        // Ignore one error: the whole part is copied as written.
        let error = one.result.errors()[0].clone();
        let mut none = one;
        none.result = none.result.ignore(&error);
        assert!(none.result.errors().is_empty());
        let local = lookup(&none, syn::parse_quote!(Local)).unwrap();
        let out = s(resolved(&none, &item));
        assert!(
            out.contains(&format!("a:<()as::my::R<(),{}>>::Type", key(local))),
            "{}",
            out
        );
        assert!(
            out.contains("b:Wrap<'_>") && out.contains("T:Base"),
            "{}",
            out
        );
    }

    #[test]
    fn ignore_if_picks_the_errors_that_a_closure_says() {
        let item: Item = syn::parse_quote! {
            struct S<T: Base> { a: Local, b: Wrap<'_>, t: T }
        };
        let tl = table_of(&item);
        assert_eq!(tl.result.errors().len(), 2);
        // The closure is asked about each error; the ones it picks are ignored.
        let mut picked = tl.clone();
        picked.result = picked.result.ignore_if(|e| s(&e.cause).starts_with("Wrap"));
        assert_eq!(picked.result.errors().len(), 1);
        assert_eq!(s(&picked.result.errors()[0].cause), "Base");
        assert!(picked.resolve_into(&mut item.clone()).is_err());
        // Once none remains, the table resolves, and the parts are written as they were.
        let mut all = tl;
        all.result = all.result.ignore_if(|_| true);
        let out = s(resolved(&all, &item));
        assert!(
            out.contains("b:Wrap<'_>") && out.contains("T:Base"),
            "{}",
            out
        );
    }

    #[test]
    fn types_to_replace_need_a_projection() {
        let item: Item = syn::parse_quote! { struct S { a: Local } };
        let mut tl = table_of(&item);
        tl.projection = None;
        let err = tl.resolve_into(&mut item.clone()).unwrap_err().to_string();
        assert!(err.contains("no `repeater` and `id`"), "{}", err);
        // Nothing to replace: nothing to write, and `Projection::unused()` is enough for a use
        // site.
        let item: Item = syn::parse_quote! { struct S { a: u8 } };
        let mut tl = table_of(&item);
        tl.projection = None;
        assert_eq!(s(resolved(&tl, &item)), s(&item));
        let mut it = item.clone();
        resolved_at(&tl, &mut it, &[]).unwrap();
        assert_eq!(s(&it), s(&item));
    }

    #[test]
    fn a_table_that_cannot_write_its_entries_is_an_error() {
        // An entry that names `Self` has no impl for the marker: a META of another definer.
        let item: Item = syn::parse_quote! { struct S { a: inner::G<Self> } };
        let tl = table_of(&item);
        assert_eq!(entries(&tl).len(), 1);
        let err = tl.resolve_into(&mut item.clone()).unwrap_err().to_string();
        assert!(err.contains("needs `Self`"), "{}", err);
    }

    #[test]
    fn item_resolved_is_the_table_applied_to_the_payload() {
        use crate::helper::{Entry, Input};
        let krate: Path = syn::parse_quote!(::announce);
        let entry = |tl: &TypeLeak, item: &Item| -> Entry {
            let meta = tl.meta();
            let input: Input = syn::parse_quote! { [cb!] a::S! => [#meta] { #item } };
            input.entries[0].clone()
        };
        let item: Item = syn::parse_quote! { struct S<T> { a: Local, b: Wrap<T>, c: u8 } };
        let tl = table_of(&item);
        let (out, check) = entry(&tl, &item).item_resolved(&krate).unwrap();
        assert_eq!(s(&out), s(resolved(&tl, &item)));
        assert!(!s(&out).contains("a:Local"), "{}", s(&out));
        assert!(check.is_empty());
        // The errors that remain are all reported at once, as one error each.
        let item: Item = syn::parse_quote! { struct S<T: Base> { a: Local, b: Wrap<'_>, t: T } };
        let tl = table_of(&item);
        let err = entry(&tl, &item).item_resolved(&krate).unwrap_err();
        assert_eq!(err.into_iter().count(), 2);
        // An empty table, with no projection or with one, leaves the item as it is.
        let item: Item = syn::parse_quote! { struct S { a: u8 } };
        let mut tl = table_of(&item);
        assert_eq!(
            s(&entry(&tl, &item).item_resolved(&krate).unwrap().0),
            s(&item)
        );
        tl.projection = None;
        assert_eq!(
            s(&entry(&tl, &item).item_resolved(&krate).unwrap().0),
            s(&item)
        );
    }
}
