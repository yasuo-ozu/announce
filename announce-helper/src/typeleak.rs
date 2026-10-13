//! The table of an entry announced with `#[announce(repeater = ..)]`: the
//! `type_leak(..)` META and the rewriting of a payload with it.
//!
//! The definer analyses the item with [`type_leak`] and sends the resulting
//! [`Referrer`] to the consumer inside META. The `Referrer` knows the context (the
//! generics and bounds) of every leaked type. The consumer parses it back and
//! rewrites the payload with [`Referrer::visitor`]. So a type is replaced by the
//! entry of the scope where it is written, never by the entry of a same-named type
//! of another scope.
//!
//! The arguments of a replacement are the ones that [`Referrer::entry_args_with`]
//! gives for a [`UseSite`]. The definer's impls come from
//! [`Referrer::repeater_impl_with`], which encodes the same generics, so the two
//! sides cannot disagree.

use crate::KvArgs;
use proc_macro2::TokenStream;
use syn::{Item, LitInt, Path, Result, Type};
use template_quote::quote;
use type_leak::{ContextId, Referrer, RepeaterSlots, UseSite};

/// Type table of an entry announced with `#[announce(repeater = ..)]`, from
/// [`Entry::type_leak`](crate::Entry::type_leak).
///
/// The payload is the original item. The definer numbered its relative types: the
/// *entries* `0, 1, ..` of [`referrer`](Self::referrer). An entry is a type in the
/// *context* where it is written (the item, a method, a generic associated type),
/// with that context's generics. So the same text in two methods with different
/// generics is two entries, and a lifetime that a `for<'b>` of the item binds
/// belongs to the context of its type. `&Local`, also with an elided lifetime, is
/// the entry `Local`.
///
/// The `ix`-th entry resolves from any crate as `<() as REPEATER<ARGS, ID, ix>>::Type`.
/// `ARGS` encodes the generics that the type uses (see
/// [`entry_args`](Self::entry_args)): a type parameter as itself, a lifetime as
/// `&'a ()`, a `usize` const as `[(); N]`, and a const of another primitive type
/// (`bool`, `char`, `u8` .. `u128`, `i8` .. `i128`, `isize`) through the const
/// parameter that the repeater has for that type. A lifetime bound (`T: 'a`) is
/// encoded too (`&'a T`), and the impl does not declare it. [`apply`](Self::apply)
/// rewrites an item that way.
///
/// The META element is
///
/// ```text
/// type_leak(repeater = $crate::Repeater, id = 42u64, context = 0, referrer = [ .. ])
/// ```
///
/// `referrer` is the `type_leak::Referrer` of the item in type-leak's own token
/// format. It is opaque to announce: do not parse it yourself. Its first token is a
/// format tag, and a version of type-leak that does not match is an error.
/// `context` is the top-level `ContextId` of the item in it, where a callback
/// rewrites the item. Other keys are ignored when reading.
#[derive(Clone, Debug)]
pub struct TypeLeak {
    /// The repeater trait, e.g. `$crate::Repeater`.
    pub repeater: Path,
    /// The definition's table id: the `ID` argument of the repeater.
    pub id: LitInt,
    /// The top-level context of the item in [`referrer`](Self::referrer).
    pub context: ContextId,
    /// The analysis of the item at the definition. Entry `ix` is the `NTH`
    /// argument of the repeater. Only the required entries have an impl. This
    /// `Referrer` is not strict, so the optional ones (absolute paths of other
    /// crates) are left as written.
    pub referrer: Referrer,
}

impl TypeLeak {
    /// The table of `referrer`, for the item whose top-level context is `context`.
    pub fn new(repeater: Path, id: LitInt, referrer: Referrer, context: ContextId) -> Self {
        TypeLeak {
            repeater,
            id,
            context,
            referrer,
        }
    }

    /// The encoder of const generics for this repeater, the trait that
    /// `announce::define_repeater!` (`type_leak::repeater_definition`) defines.
    pub fn slots(&self) -> RepeaterSlots {
        RepeaterSlots::new(self.repeater.clone())
    }

    /// The types that are replaced, as the definer wrote them: the required
    /// entries of the [`referrer`](Self::referrer), each with its index.
    pub fn types(&self) -> impl Iterator<Item = (usize, &Type)> {
        self.referrer.required()
    }

    /// The index of the first entry that is `ty`, if it was interned.
    ///
    /// The same type can be an entry in several contexts (a method generic named
    /// like a generic of the trait). Then use [`apply`](Self::apply), which tells
    /// them apart by the scope.
    pub fn lookup(&self, ty: &Type) -> Option<usize> {
        self.types().find(|(_, t)| *t == ty).map(|(ix, _)| ix)
    }

    /// The first argument of the repeater for entry `ix`, as written at `site`.
    ///
    /// It is `Referrer::entry_args_with` with the const slots of the repeater.
    /// [`UseSite::identity()`] is the scope of the item itself (every generic by its
    /// own name): the arguments of the impl the definer wrote.
    /// [`UseSite::substituting`] gives the generics arguments, as in `impl Tr<u8>
    /// for ..`.
    pub fn entry_args(&self, ix: usize, site: &UseSite) -> Result<Type> {
        let slots = self.slots();
        self.referrer
            .entry_args_with(ix, site, |value, param| {
                slots.encode_const(value, &param.ty)
            })
            .map_err(syn::Error::from)
    }

    /// `<() as REPEATER<args, ID, ix>>::Type`, as tokens to emit.
    ///
    /// `args` comes from [`entry_args`](Self::entry_args). The tokens are not a
    /// `syn::Type` because the repeater is `$crate::..` in a callback.
    pub fn projection(&self, ix: usize, args: &Type) -> TokenStream {
        let (repeater, id) = (&self.repeater, &self.id);
        quote! { <() as #repeater<#args, #id, #ix>>::Type }
    }

    /// Rewrites every leaked type in `item` (usually the entry's payload) to its
    /// projection, so the item resolves from any crate. The copy is in the scope
    /// of the item itself ([`UseSite::identity()`]).
    ///
    /// # Panics
    ///
    /// If the table is inconsistent (it cannot be, when it comes from
    /// [`Entry::type_leak`](crate::Entry::type_leak)). Use
    /// [`try_apply`](Self::try_apply) to get the error.
    pub fn apply(&self, item: &mut Item) {
        if let Err(e) = self.try_apply(item) {
            panic!("announce: cannot apply the type_leak table: {}", e);
        }
    }

    /// Like [`apply`](Self::apply), returning the error of a bad table.
    pub fn try_apply(&self, item: &mut Item) -> Result<()> {
        self.try_apply_at(item, &UseSite::identity())
    }

    /// Like [`try_apply`](Self::try_apply), for a copy at another use site, such as
    /// the `impl Tr<u8> for X` of a trait. The arguments in the replacements are
    /// `self.entry_args(ix, site)`.
    ///
    /// The scope of the copy must be the item's: pass the whole item (not a type
    /// cut out of it), with the generics it declares as the definer wrote them
    /// (`Referrer::visitor` documents what it matches).
    pub fn try_apply_at(&self, item: &mut Item, site: &UseSite) -> Result<()> {
        use syn::visit_mut::VisitMut;
        let mut error: Option<syn::Error> = None;
        {
            let mut visitor = self.referrer.clone().visitor(self.context, |ty, ix| {
                match self.entry_args(ix, site) {
                    Ok(args) => Type::Verbatim(self.projection(ix, &args)),
                    Err(e) => {
                        error.get_or_insert(e);
                        ty
                    }
                }
            });
            visitor.visit_item_mut(item);
        }
        error.map_or(Ok(()), Err)
    }

    /// The `type_leak(..)` META element, as the definer writes it.
    pub fn meta(&self) -> TokenStream {
        let (repeater, id, context, referrer) =
            (&self.repeater, &self.id, &self.context, &self.referrer);
        quote! {
            type_leak(repeater = #repeater, id = #id, context = #context, referrer = [#referrer])
        }
    }

    /// Reads the arguments of the `type_leak(..)` META element
    /// ([`Meta::kv`](crate::Meta::kv)). Unknown keys are ignored.
    pub fn from_kv(kv: &mut KvArgs) -> Result<Self> {
        let call_site = proc_macro2::Span::call_site();
        let repeater = kv
            .tokens("repeater")
            .ok_or_else(|| syn::Error::new(call_site, "missing `repeater`"))?;
        let repeater: Path = syn::parse2(repeater)?;
        let id: LitInt = kv.require("id")?;
        let missing = || {
            syn::Error::new(
                call_site,
                "announce: the `type_leak(..)` META has no `context` and `referrer`: the \
                 definition was announced by an older `announce`, which this `announce-helper` \
                 cannot read",
            )
        };
        let context: ContextId = kv.get("context")?.ok_or_else(missing)?;
        let referrer = kv.tokens("referrer").ok_or_else(missing)?;
        let referrer = syn::parse::Parser::parse2(
            |input: syn::parse::ParseStream| {
                let content;
                syn::bracketed!(content in input);
                let referrer: Referrer = content.parse()?;
                if !content.is_empty() {
                    return Err(content.error("unexpected tokens after the Referrer"));
                }
                Ok(referrer)
            },
            referrer,
        )?;
        if !referrer.contexts().any(|c| c == context) {
            return Err(syn::Error::new(
                call_site,
                format!(
                    "announce: the `type_leak(..)` META names the context {}, which its \
                     `referrer` does not have",
                    context
                ),
            ));
        }
        Ok(TypeLeak::new(repeater, id, referrer, context))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use type_leak::Leaker;

    fn s(ts: impl template_quote::ToTokens) -> String {
        ts.to_token_stream()
            .to_string()
            .split_whitespace()
            .collect()
    }

    fn table_of(item: &Item) -> TypeLeak {
        let mut config = type_leak::LeakerConfig::new();
        config.allow_crate();
        config.allow_primitive();
        let mut leaker = Leaker::from_config(config);
        let context = match item {
            Item::Struct(i) => leaker.intern_struct(i, false),
            Item::Enum(i) => leaker.intern_enum(i, false),
            Item::Trait(i) => leaker.intern_trait(i, false),
            _ => panic!("struct, enum or trait"),
        }
        .unwrap();
        TypeLeak::new(
            syn::parse_quote!(::my::R),
            syn::parse_quote!(7u64),
            leaker.finish(),
            context,
        )
    }

    fn read(tl: &TypeLeak) -> TypeLeak {
        let meta: crate::Meta = syn::parse2(tl.meta()).unwrap();
        let mut kv = meta.kv().unwrap();
        TypeLeak::from_kv(&mut kv).unwrap()
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
        assert_eq!(back.referrer, tl.referrer);
        assert_eq!(back.context, tl.context);
        assert_eq!(s(&back.repeater), "::my::R");
        assert_eq!(back.id.base10_digits(), "7");
        assert!(back.types().count() >= 3);
        for (ix, ty) in tl.types() {
            assert_eq!(back.referrer.iter().nth(ix), Some(ty));
        }
    }

    #[test]
    fn meta_ignores_unknown_keys_and_reports_an_old_format() {
        let tl = table_of(&syn::parse_quote! { struct S { a: Local } });
        let meta = tl.meta().to_string();
        let with_extra: crate::Meta =
            syn::parse_str(&meta.replace("id = 7u64", "id = 7u64, future = [1, 2], other"))
                .unwrap();
        let mut kv = with_extra.kv().unwrap();
        assert!(TypeLeak::from_kv(&mut kv).is_ok());

        let old: crate::Meta = syn::parse_str(
            "type_leak(repeater = ::my::R, id = 7u64, generics = [()], types = [Local])",
        )
        .unwrap();
        let err = TypeLeak::from_kv(&mut old.kv().unwrap()).unwrap_err();
        assert!(err.to_string().contains("older `announce`"), "{}", err);

        let bad: crate::Meta = syn::parse_str(&format!(
            "type_leak(repeater = ::my::R, id = 7u64, context = 9, referrer = [\"type-leak-format-{}\" (Local)])",
            type_leak::Referrer::FORMAT_VERSION
        ))
        .unwrap();
        let err = TypeLeak::from_kv(&mut bad.kv().unwrap()).unwrap_err();
        assert!(err.to_string().contains("context 9"), "{}", err);
    }

    #[test]
    fn lookup_and_projection() {
        let tl = table_of(&syn::parse_quote! {
            struct S<T> { a: Local, b: Wrap<T>, c: u8 }
        });
        let local = tl.lookup(&syn::parse_quote!(Local)).unwrap();
        let wrap = tl.lookup(&syn::parse_quote!(Wrap<T>)).unwrap();
        assert_ne!(local, wrap);
        assert_eq!(tl.lookup(&syn::parse_quote!(Wrap<U>)), None);
        assert_eq!(tl.lookup(&syn::parse_quote!(T)), None);
        assert_eq!(tl.lookup(&syn::parse_quote!(u8)), None);
        assert_eq!(
            s(tl.projection(wrap, &syn::parse_quote!((u8,)))),
            format!("<()as::my::R<(u8,),7u64,{}usize>>::Type", wrap)
        );
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
            tl.lookup(&syn::parse_quote!(Local)),
            tl.lookup(&syn::parse_quote!(Wrap<T>)).unwrap(),
        );
        let wrap = format!("<()as::my::R<(T,),7u64,{}usize>>::Type", wrap);
        assert!(out.contains(&format!("b:{}", wrap)), "{}", out);
        // `Local` is an entry unless a larger absolute type that contains it is
        // (`Option<Local>`, the whole type of a field).
        let local = format!("<()as::my::R<(),7u64,{}usize>>::Type", local.unwrap_or(99));
        assert!(
            out.contains(&format!("a:{}", local)) || out.contains("a:<()as::my::R<(),7u64,"),
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
        // ALGORITHM_FINDINGS A: the same text in two scopes with different
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
            .types()
            .filter(|(_, t)| **t == syn::parse_quote!(Gc<X>))
            .map(|(ix, _)| ix)
            .collect();
        assert_eq!(gc.len(), 2, "one entry per context");
        let mut it = item;
        tl.apply(&mut it);
        let out = s(&it);
        assert!(out.contains("fnf<Local>(&self,x:Local);"), "{}", out);
        assert!(out.contains("fng(&self)-><()as::my::R<(),7u64,"), "{}", out);
        for ix in gc {
            assert!(
                out.contains(&format!("::my::R<(X,),7u64,{}usize>>::Type", ix)),
                "{}",
                out
            );
        }
    }

    #[test]
    fn apply_encodes_const_generics_of_every_primitive_type() {
        let item: Item = syn::parse_quote! {
            struct C<const A: u8, const B: bool, const D: usize, const E: u64> {
                x: inner::W<A, B, D, E>,
            }
        };
        let tl = table_of(&item);
        // At the item's own scope: the parameters by name.
        let args = s(tl.entry_args(0, &UseSite::identity()).unwrap());
        assert_eq!(
            args,
            "(*constdyn::my::R<(),0,0,false,'\\0',A,Type=()>,\
             *constdyn::my::R<(),0,0,B,Type=()>,[();D],*constdyn::my::R<(),E,0,Type=()>)"
        );
        // A site that gives the generics values, as `impl Tr<..> for ..` does.
        let params = match &item {
            Item::Struct(st) => st.generics.params.clone(),
            _ => unreachable!(),
        };
        let given: Vec<syn::GenericArgument> = vec![
            syn::parse_quote!(1),
            syn::parse_quote!(true),
            syn::parse_quote!(3),
            syn::parse_quote!(9),
        ];
        let args = s(tl
            .entry_args(0, &UseSite::substituting(&params, &given))
            .unwrap());
        assert_eq!(
            args,
            "(*constdyn::my::R<(),0,0,false,'\\0',1,Type=()>,\
             *constdyn::my::R<(),0,0,true,Type=()>,[();3],*constdyn::my::R<(),9,0,Type=()>)"
        );
        // The same arguments are what `apply_at` writes into the item.
        let mut it = item;
        tl.try_apply_at(&mut it, &UseSite::substituting(&params, &given))
            .unwrap();
        assert!(s(&it).contains(&args), "{}", s(&it));
    }
}
