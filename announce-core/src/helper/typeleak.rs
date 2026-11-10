//! The table of an entry announced with `#[announce(repeater = ..)]`: the
//! `type_leak(..)` META and the rewriting of a payload with it.
//!
//! The definer analyses the item with [`type_leak`] and sends the resulting
//! [`LeakerResult`] to the consumer inside META. Its [`Referrer`] knows the context
//! (the generics and bounds) of every leaked type. The consumer parses it back and
//! rewrites the payload with [`Referrer::visitor`]. So a type is replaced by the
//! entry of the scope where it is written, never by the entry of a same-named type
//! of another scope.
//!
//! The errors of type-leak (the parts that no table can carry) are in the result too.
//! The definer does not report them: the consumer does, with [`TypeLeak::try_apply`], or
//! allows them ([`TypeLeak::allow`], [`TypeLeak::ignore`]) before it applies the table.
//!
//! The use-site arguments of a replacement come from [`Referrer::complete_args`]. The
//! definer's impls come from [`LeakerResult::repeater_impls`], which encodes the same
//! generics, so the two sides cannot disagree.

use crate::helper::keys::{need, read_element};
use proc_macro2::{Literal, Span, TokenStream};
use syn::parse::{Parse, ParseStream};
use syn::visit_mut::VisitMut;
use syn::{Error, GenericArgument, Item, Path, Result, Type};
use template_quote::{quote, ToTokens};
use type_leak::{
    Allow, ContextId, EntryId, ErrorItem, ImplSelf, LeakerResult, Projection, Referrer,
};

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
/// encoded too (`&'a T`), and the impl does not declare it. [`try_apply`](Self::try_apply)
/// rewrites an item that way.
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
/// there for the errors of type-leak that the consumer reports ([`errors`](Self::errors)).
/// Other keys are ignored when reading with `syn::parse2::<TypeLeak>(meta)`.
///
/// ```
/// # use announce_core as announce;
/// use announce::helper::TypeLeak;
///
/// let meta: proc_macro2::TokenStream = {
///     let mut leaker = announce::helper::type_leak::Leaker::new();
///     let item: syn::ItemStruct = syn::parse_quote! { struct S { a: Local } };
///     let context = leaker.intern_struct(&item);
///     let projection = TypeLeak::projection_of(syn::parse_quote!(crate::Repeater), 7);
///     TypeLeak::new(Some(projection), context, leaker.result()).meta()
/// };
/// let table: TypeLeak = syn::parse2(meta)?;
/// let mut item: syn::Item = syn::parse_quote! { struct S { a: Local } };
/// table.try_apply(&mut item)?;
/// assert!(template_quote::ToTokens::to_token_stream(&item).to_string().contains("crate :: Repeater"));
/// # Ok::<(), syn::Error>(())
/// ```
#[derive(Clone, Debug)]
pub struct TypeLeak {
    /// How the definer wrote its impls, and so how the entries are written: the repeater trait
    /// (`$crate::Repeater`) and the `id` of the table, which is the `KEY`'s high half, for the
    /// marker `()`. `None` for a definition without `repeater`, which has no impl: its entries
    /// cannot be written (an error), and its table is there for the errors.
    pub projection: Option<Projection>,
    /// The top-level context of the item in the [`result`](Self::result). Reading the META does
    /// not check that the result has it (type-leak cannot tell without panicking): a context it
    /// does not have panics where the table is applied, in type-leak.
    pub context: ContextId,
    /// The analysis of the item at the definition: the `Referrer` and the errors of type-leak
    /// that remain. Entry `ix` is the entry number of the repeater's `KEY`.
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

    /// The errors of type-leak that remain, in the order found. Report them with
    /// [`try_apply`](Self::try_apply), or drop them with [`allow`](Self::allow) and
    /// [`ignore`](Self::ignore).
    pub fn errors(&self) -> &[ErrorItem] {
        self.result.errors()
    }

    /// The table without the errors that `rule` accepts: the copy writes that part as it is
    /// (`LeakerResult::allow`). You promise that it compiles as written where the copy is.
    #[must_use]
    pub fn allow(mut self, rule: impl Allow) -> Self {
        self.result = self.result.allow(rule);
        self
    }

    /// The table without the error `error`, one of [`errors`](Self::errors): the copy writes the
    /// whole part that it is about as it is, and an entry that it is about is no entry any more
    /// (`LeakerResult::ignore`).
    #[must_use]
    pub fn ignore(mut self, error: &ErrorItem) -> Self {
        self.result = self.result.ignore(error);
        self
    }

    /// The `Referrer` of the result, for what [`entries`](Self::entries) does not tell: the
    /// context of each entry (`Referrer::iter`). It is a copy: `LeakerResult` has no accessor
    /// for it.
    pub fn referrer(&self) -> Referrer {
        self.result.clone().into_referrer()
    }

    /// The types that are replaced, as the definer wrote them: the entries of the
    /// [`result`](Self::result), each with its [`EntryId`], in order.
    pub fn entries(&self) -> Vec<(EntryId, Type)> {
        self.referrer()
            .iter()
            .map(|(entry, ty, _)| (entry, ty.clone()))
            .collect()
    }

    /// The errors that remain as one error, or `None` without any: one message for each, as
    /// type-leak words it, at all of its cause (`Error::from(ErrorItem)` is at the first token of
    /// the cause on stable).
    fn remaining(&self) -> Option<Error> {
        self.errors()
            .iter()
            .map(|e| {
                let message = Error::from(e.clone()).to_string();
                Error::new_spanned(e.cause.to_token_stream(), message)
            })
            .reduce(|mut all, e| {
                all.combine(e);
                all
            })
    }

    /// The `Referrer`, and the projection to write its entries by, once nothing is in the way: no
    /// error remains, and the entries can be written (there is a projection, and each entry has an
    /// impl for the marker, as the definer's impls are).
    fn ready(&self) -> Result<(Referrer, Option<&Projection>)> {
        if let Some(error) = self.remaining() {
            return Err(error);
        }
        let referrer = self.referrer();
        if referrer.is_empty() {
            return Ok((referrer, self.projection.as_ref()));
        }
        let projection = self.projection.as_ref().ok_or_else(|| {
            Error::new(
                Span::call_site(),
                "announce: the `type_leak(..)` META has types to replace but no `repeater` and \
                 `id`: the definition has no impl to write them by",
            )
        })?;
        let unit: Type = syn::parse_quote!(());
        for (entry, _, context) in referrer.iter() {
            projection.of(&unit, context, entry)?;
        }
        Ok((referrer, Some(projection)))
    }

    /// Rewrites every leaked type in `item` (usually the entry's payload) to its
    /// projection, so the item resolves from any crate. The copy is in the scope
    /// of the item itself: every generic by its own name.
    ///
    /// # Panics
    ///
    /// If an error of type-leak remains, or the table is inconsistent. Use
    /// [`try_apply`](Self::try_apply) to get the error.
    pub fn apply(&self, item: &mut Item) {
        if let Err(e) = self.try_apply(item) {
            panic!("announce: cannot apply the type_leak table: {}", e);
        }
    }

    /// Like [`apply`](Self::apply), returning the error.
    ///
    /// It fails if an error of type-leak remains ([`errors`](Self::errors): all of them, as one
    /// error, so that one run lists everything the item cannot carry), if there are types to
    /// replace but no [`projection`](Self::projection), or if the table is inconsistent.
    pub fn try_apply(&self, item: &mut Item) -> Result<()> {
        let (referrer, projection) = self.ready()?;
        if let Some(projection) = projection {
            referrer
                .visitor(self.context, projection)
                .visit_item_mut(item);
        }
        Ok(())
    }

    /// Like [`try_apply`](Self::try_apply), for a copy at another use site, such as the `impl
    /// Tr<u8> for X` of a trait: `args` are the arguments of the use site (`[u8]`). It completes
    /// them with the defaults of the item's generics (`Referrer::complete_args`) and writes them
    /// into the types and lifetimes of `item`, in the entries too, so the entries are written
    /// as `<() as REPEATER<(u8,), KEY>>::Type`. The header, which declares the generics, is left.
    ///
    /// The scope of the copy must be the item's: pass the whole item (not a type cut out of it),
    /// with the generics it declares as the definer wrote them (`Referrer::visitor` documents what
    /// it matches). It fails like [`try_apply`](Self::try_apply), and where an argument is
    /// missing, is too many, or cannot be encoded.
    pub fn try_apply_at(&self, item: &mut Item, args: &[GenericArgument]) -> Result<()> {
        let (referrer, projection) = self.ready()?;
        // Without a projection there is no entry, and no path is written through it.
        let none;
        let projection = match projection {
            Some(projection) => projection,
            None => {
                none = Self::projection_of(syn::parse_quote!(Repeater), 0);
                &none
            }
        };
        referrer
            .visitor(self.context, projection)
            .visit_item_mut(item);
        let mut substitute = referrer.complete_args(self.context, args, projection)?;
        substitute.visit_item_mut(item);
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
    use type_leak::{Cause, Leaker, Place, Rules};

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

    /// The entry of `tl` whose type is `ty`.
    fn lookup(tl: &TypeLeak, ty: Type) -> Option<usize> {
        tl.entries()
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
        assert_eq!(
            back.result.clone().into_parts(),
            tl.result.clone().into_parts()
        );
        assert_eq!(back.context, tl.context);
        let projection = back.projection.as_ref().unwrap();
        assert_eq!(s(&projection.path), "::my::R");
        assert_eq!(projection.id, 7);
        assert!(back.entries().len() >= 3);
        assert_eq!(back.entries(), tl.entries());
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
        assert_eq!(back.errors(), tl.errors());
        assert_eq!(back.errors().len(), 1);
    }

    #[test]
    fn the_errors_travel_in_the_result() {
        let item: Item = syn::parse_quote! {
            struct S<T: Base> { a: Local, b: Wrap<'_>, t: T }
        };
        let tl = table_of(&item);
        assert_eq!(tl.errors().len(), 2);
        let back = read(&tl);
        assert_eq!(back.errors(), tl.errors());
        assert_eq!(back.entries().len(), 1);
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
        let entry = tl.entries()[wrap].0;
        assert_eq!(projection.key(entry) >> 64, 7);
        assert_eq!(projection.key(entry) as u64 as usize, wrap);
        assert!(matches!(&projection.impl_self, ImplSelf::Marker(t) if s(t) == "()"));
    }

    #[test]
    fn apply_rewrites_leaked_types_only() {
        let item: Item = syn::parse_quote! {
            struct S<T> { a: Local, b: Wrap<T>, c: ::core::option::Option<Local>, d: u8, e: T }
        };
        let tl = table_of(&item);
        let mut it = item;
        tl.apply(&mut it);
        let out = s(&it);
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
                || tl.entries().iter().any(|(ix, _)| any_entry(ix.index())),
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
    fn apply_tells_scopes_apart() {
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
        let gc: Vec<usize> = tl
            .entries()
            .into_iter()
            .filter(|(_, t)| *t == syn::parse_quote!(Gc<X>))
            .map(|(ix, _)| ix.index())
            .collect();
        assert_eq!(gc.len(), 2, "one entry per context");
        let mut it = item;
        tl.apply(&mut it);
        let out = s(&it);
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
    fn apply_at_a_use_site_writes_the_arguments() {
        let item: Item = syn::parse_quote! {
            trait Tr<K, const N: usize> { fn get(&self, key: K, n: [u8; N]) -> Wrap<K>; }
        };
        let tl = table_of(&item);
        // At the item's own scope: the parameters by name.
        let mut here = item.clone();
        tl.try_apply(&mut here).unwrap();
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
        tl.try_apply_at(&mut at, &given).unwrap();
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
        let err = tl
            .try_apply_at(&mut item.clone(), &given[..1])
            .unwrap_err()
            .to_string();
        assert!(err.contains("missing generic argument"), "{}", err);
    }

    #[test]
    fn apply_encodes_const_generics_of_every_primitive_type() {
        let item: Item = syn::parse_quote! {
            struct C<const A: u8, const B: bool, const D: usize, const E: u64, const K: u128> {
                x: inner::W<A, B, D, E, K>,
            }
        };
        let tl = table_of(&item);
        // At the item's own scope: the parameters by name.
        let mut here = item.clone();
        tl.try_apply(&mut here).unwrap();
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
        tl.try_apply_at(&mut at, &given).unwrap();
        let args = "(*constdyn::my::R<(),0,false,'\\0',1,Type=()>,\
             *constdyn::my::R<(),0,true,Type=()>,[();{3}],\
             *constdyn::my::R<(),0,false,'\\0',0,0,0,9,Type=()>,*constdyn::my::R<(),10,Type=()>)";
        assert!(s(&at).contains(args), "{}", s(&at));
    }

    #[test]
    fn a_remaining_error_is_an_error_of_apply_with_all_of_them() {
        let item: Item = syn::parse_quote! {
            struct S<T: Base> { a: Local, b: Wrap<'_>, c: [u8; two!()], t: T }
        };
        let tl = table_of(&item);
        assert_eq!(tl.errors().len(), 3);
        let mut it = item.clone();
        let err = tl.try_apply(&mut it).unwrap_err();
        // The item is as it was.
        assert_eq!(s(&it), s(&item));
        let all: Vec<String> = err.into_iter().map(|e| e.to_string()).collect();
        assert_eq!(all.len(), 3, "{:?}", all);
        assert!(all[0].contains("use absolute path: 'Base'"), "{:?}", all);
        assert!(all.iter().any(|m| m.contains("'Wrap<'_>'")), "{:?}", all);
        assert!(all.iter().any(|m| m.contains("'two'")), "{:?}", all);
        // `apply` panics with them, and `try_apply_at` fails the same way.
        assert!(tl.try_apply_at(&mut item.clone(), &[]).is_err());
        let panicked = std::panic::catch_unwind(|| tl.apply(&mut item.clone()));
        assert!(panicked.is_err());
    }

    #[test]
    fn allow_and_ignore_decide_what_remains() {
        let item: Item = syn::parse_quote! {
            struct S<T: Base> { a: Local, b: Wrap<'_>, t: T }
        };
        let tl = table_of(&item);
        assert_eq!(tl.errors().len(), 2);
        // Allow by rule: the part is copied as written.
        let rule = |cause: &Cause, _: Place| cause.starts_with(&syn::parse_quote!(Base));
        let one = tl.clone().allow(rule);
        assert_eq!(one.errors().len(), 1);
        assert_eq!(s(&one.errors()[0].cause), "Wrap<'_>");
        // Ignore one error: the whole part is copied as written.
        let error = one.errors()[0].clone();
        let none = one.ignore(&error);
        assert!(none.errors().is_empty());
        let mut it = item;
        none.try_apply(&mut it).unwrap();
        let local = lookup(&none, syn::parse_quote!(Local)).unwrap();
        let out = s(&it);
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
    fn types_to_replace_need_a_projection() {
        let item: Item = syn::parse_quote! { struct S { a: Local } };
        let mut tl = table_of(&item);
        tl.projection = None;
        let err = tl.try_apply(&mut item.clone()).unwrap_err().to_string();
        assert!(err.contains("no `repeater` and `id`"), "{}", err);
        // Nothing to replace: nothing to write.
        let item: Item = syn::parse_quote! { struct S { a: u8 } };
        let mut tl = table_of(&item);
        tl.projection = None;
        let mut it = item.clone();
        tl.try_apply(&mut it).unwrap();
        tl.try_apply_at(&mut it, &[]).unwrap();
        assert_eq!(s(&it), s(&item));
    }

    #[test]
    fn a_table_that_cannot_write_its_entries_is_an_error() {
        // An entry that names `Self` has no impl for the marker: a META of another definer.
        let item: Item = syn::parse_quote! { struct S { a: inner::G<Self> } };
        let tl = table_of(&item);
        assert_eq!(tl.entries().len(), 1);
        let err = tl.try_apply(&mut item.clone()).unwrap_err().to_string();
        assert!(err.contains("needs `Self`"), "{}", err);
    }
}
