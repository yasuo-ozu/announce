//! Every public item of the former `announce-helper` crate is reachable as
//! `announce::helper::*`, from a crate that depends on `announce` alone (this one:
//! `announce-core`, which holds the code, is not a dependency here).
//!
//! The macros `callback!` and `transformer!` define `#[proc_macro]` functions, so
//! they can only be expanded in a proc-macro crate. `tests/derive` does that, also
//! with `announce` as its only link to the code. Here their paths are resolved.

use announce::helper::{
    // Functions.
    check,
    check_trait,
    check_type,
    gensym,
    is_expression_position,
    payload,
    redirect,
    remove_attributes,
    signature_only,
    // Types.
    CheckCall,
    CheckInput,
    Cont,
    ContList,
    Entry,
    Gather,
    Input,
    KvArgs,
    Meta,
    OwnedAttrs,
    Query,
    TypeLeak,
};
// Modules.
use announce::helper::{attrs, leak, type_leak};
// Macros (defined with `#[macro_export]` in `announce-core`, re-exported by `helper`).
// They are not expanded here (they emit `#[proc_macro]`); the import fails to compile
// if either path does not resolve.
#[allow(unused_imports)]
use announce::helper::{callback, transformer};

use proc_macro2::TokenStream;
use template_quote::ToTokens;

fn squash(ts: impl ToTokens) -> String {
    ts.to_token_stream()
        .to_string()
        .split_whitespace()
        .collect()
}

#[test]
fn input_entries_and_conts() {
    let input: Input = syn::parse_quote! {
        (expr, other)
        [cb!, tail!]
        a::F! => [my_lib::check, mock(::core::iter::Iterator), attrs(my_lib), info(k = v)] {
            pub trait F {}
        }
        REST tokens
    };
    assert!(is_expression_position(input.options.as_ref().unwrap()));
    assert!(input.has_option("other"));
    assert_eq!(input.option_items().len(), 2);
    assert_eq!(squash(&input.rest), "RESTtokens");
    assert_eq!(
        input.rest_as::<TokenStream>().unwrap().to_string(),
        "REST tokens"
    );

    let entry: &Entry = &input.entries[0];
    assert!(entry.has_check("check"));
    assert!(entry.mock().is_some());
    assert!(entry.owned_attrs().contains("my_lib"));
    assert!(entry.type_leak().unwrap().is_none());
    let meta: &Meta = entry.find_meta("info").unwrap();
    assert_eq!(meta.name_values().unwrap().len(), 1);
    let mut kv: KvArgs = meta.kv().unwrap();
    assert_eq!(squash(kv.tokens("k").unwrap()), "v");
    kv.finish().unwrap();
    assert!(matches!(entry.item().unwrap(), syn::Item::Trait(_)));
    assert!(entry.item_without_owned_attrs().is_ok());
    assert_eq!(input.entries_in_order().count(), 1);

    // Continuations.
    let a = Cont::bang(syn::parse_quote!(a::F<u8>));
    assert_eq!(squash(a.to_token_stream()), "a::F!");
    let d = Cont::derive(syn::parse_quote!(a::D));
    assert_eq!(squash(d.path()), "a::D");
    let mut list: ContList = [a.clone(), d].into_iter().collect();
    list.name_dummies();
    assert_eq!(list.0.len(), 2);
    assert!(input.entry_for(&a).is_some());
    let out = ContList(vec![a.clone()]).invoke(TokenStream::new());
    assert!(squash(out).starts_with("a::F!{[a::F!]"));
    let out = ContList(vec![a.clone()])
        .invoke_with_options(Some(&syn::parse_quote!(expr)), TokenStream::new());
    assert!(squash(out).contains("(expr)"));

    // gather / fetch / forward.
    match input.gather(vec![a.clone()]) {
        Gather::Fetch(ts) => assert!(!ts.is_empty()),
        Gather::Ready(entries) => assert_eq!(entries.len(), 1),
    }
    let missing = Cont::bang(syn::parse_quote!(g::G));
    match input.gather(vec![missing.clone()]) {
        Gather::Fetch(ts) => assert!(squash(ts).contains("g::G!")),
        Gather::Ready(_) => panic!("g::G was never fetched"),
    }
    assert!(!input.fetch(vec![missing]).is_empty());
    assert!(input.forward().is_some());
    assert!(input.newest().is_some());
    assert!(input.forward_transformed(input.entries[0].clone()).is_ok());

    let id = gensym("__x_");
    assert!(id.to_string().starts_with("__x_"));
}

#[test]
fn derive_route() {
    let list = ContList(vec![Cont::derive(syn::parse_quote!(p::D))]);
    let invoked = list.invoke(TokenStream::new());
    assert!(payload(invoked.clone()).is_some());
    assert!(redirect(invoked, &syn::parse_quote!(::x::Y)).is_some());
    assert!(redirect(TokenStream::new(), &syn::parse_quote!(::x::Y)).is_none());
}

#[test]
fn checks_and_attributes() {
    // `check` answers the query and runs the closure for its own input.
    let q = check(
        syn::parse_quote! { @__announce_attrs [ me!, next! ] { st } },
        &["my_lib"],
        |_| unreachable!("a query is answered without running the check"),
    );
    assert!(squash(q).contains("attrs(my_lib)"));
    let run = check(
        syn::parse_quote! { (a), struct S; },
        &[],
        |c: &mut CheckInput| {
            assert_eq!(squash(&c.args), "a");
            assert!(c.owned_attributes().is_empty());
            Ok(TokenStream::new())
        },
    );
    assert!(run.is_empty());

    // By hand: `CheckCall` and `Query`.
    let call: CheckCall = syn::parse_quote! { @__announce_attrs [ me!, next! ] { st } };
    let query: &Query = match &call {
        CheckCall::Query(q) => q,
        CheckCall::Run(_) => panic!("a query"),
    };
    assert_eq!(query.list.0.len(), 2);
    let owned = OwnedAttrs::from_strs(&["my_lib"]);
    assert!(squash(query.answer(&owned)).starts_with("next!{@__announce_attrs"));
    let call: CheckCall = syn::parse_quote! { (a), struct S; };
    let input: CheckInput = match call {
        CheckCall::Run(i) => i,
        CheckCall::Query(_) => panic!("the check's own input"),
    };
    let input = input.with_owned(OwnedAttrs::from_strs(&["x"]));
    assert!(input.owned.contains("x"));

    // The names, found and stripped, through both paths of the module.
    let mut owned = OwnedAttrs::new();
    assert!(owned.is_empty());
    owned.extend(vec![syn::parse_quote!(my_lib)]);
    assert_eq!(owned.names().len(), 1);
    owned.check_names().unwrap();
    assert!(attrs::OwnedAttrs::parse_names(syn::parse_quote!(a, b)).is_ok());
    let mut item: syn::Item = syn::parse_quote! { struct S { #[my_lib(skip)] a: u8 } };
    assert_eq!(owned.find(&item).len(), 1);
    assert!(owned.owns(&owned.find(&item)[0]));
    assert!(owned.strip(&mut item).unwrap());
    let mut item: syn::Item = syn::parse_quote! { #[a] struct S { #[b] x: u8 } };
    assert!(attrs::remove_attributes(&mut item, |a| a.path().is_ident("b")).unwrap());
    assert!(remove_attributes(&mut item, |a| a.path().is_ident("a")).unwrap());

    // The marker checks.
    let krate: syn::Path = syn::parse_quote!(::announce);
    assert!(squash(check_type(&krate, &syn::parse_quote!(S))).contains("::announce::Announce"));
    assert!(squash(check_trait(&krate, &syn::parse_quote!(Tr))).contains("dynTr"));

    // `Query`, `CheckCall` and `OwnedAttrs` are the same types in `attrs` and `helper`.
    let _: Option<attrs::Query> = None::<Query>;
    let _: Option<attrs::CheckCall> = None::<CheckCall>;
}

#[test]
fn type_leak_and_transformers() {
    // `helper::type_leak` is the crate that `announce::type_leak` is.
    let _: announce::type_leak::Leaker = type_leak::Leaker::new();

    let item: syn::ItemStruct = syn::parse_quote! { pub struct S { a: Local, b: u8 } };
    let mut config = type_leak::LeakerConfig::new();
    config.allow_primitive();
    let mut leaker = type_leak::Leaker::from_config(config);
    let context = leaker.intern_struct(&item, true).unwrap();
    let referrer = leaker.finish();

    // `StableReferrer`.
    let stable = leak::StableReferrer::new(referrer.clone());
    assert!(!stable.is_empty());
    assert_eq!(stable.types().len(), 1);
    assert_eq!(stable.index_of(&syn::parse_quote!(Local)), Some(0));
    assert_eq!(squash(stable.to_tuple()), "(Local,)");
    let mut as_item = syn::Item::Struct(item.clone());
    stable.visit_item_mut(context, &mut as_item, |_, i| {
        syn::parse_str(&format!("T{}", i)).unwrap()
    });
    assert!(squash(&as_item).contains("a:T0"));
    let ty = stable.expand(context, syn::parse_quote!(Local), |_, _| {
        syn::parse_quote!(X)
    });
    assert_eq!(squash(ty), "X");
    let mut crate_item: syn::Item = syn::parse_quote! { struct S(crate::A); };
    leak::normalize_crate(&mut crate_item);

    // `TypeLeak`: build, write as META, read back, apply.
    let tl = TypeLeak::new(
        syn::parse_quote!(::my::R),
        syn::parse_quote!(7u64),
        referrer,
        context,
    );
    assert_eq!(tl.types().count(), 1);
    assert_eq!(tl.lookup(&syn::parse_quote!(Local)), Some(0));
    let _: type_leak::RepeaterSlots = tl.slots();
    let site = type_leak::UseSite::identity();
    let args = tl.entry_args(0, &site).unwrap();
    assert!(squash(tl.projection(0, &args)).contains("::my::R"));
    let meta: Meta = syn::parse2(tl.meta()).unwrap();
    let back = TypeLeak::from_kv(&mut meta.kv().unwrap()).unwrap();
    let mut applied = syn::Item::Struct(item.clone());
    back.apply(&mut applied);
    assert!(squash(&applied).contains("::my::R"), "{}", squash(&applied));
    let mut applied = syn::Item::Struct(item);
    tl.try_apply(&mut applied).unwrap();
    tl.try_apply_at(&mut applied, &site).unwrap();

    // `signature_only`, the function behind `announce::signature_only!`.
    let mut f: syn::Item = syn::parse_quote! { fn f() -> u8 { 1 } };
    signature_only(&mut f);
    assert!(!squash(&f).contains('1'));
}
