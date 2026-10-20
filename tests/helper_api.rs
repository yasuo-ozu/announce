//! Every public item of the former `announce-helper` crate is reachable as
//! `announce::helper::*`, from a crate that depends on `announce` alone (this one:
//! `announce-core`, which holds the code, is not a dependency here).
//!
//! The functions `callback` and `transformer` take and return `proc_macro2` streams, so
//! they are called here directly. The macros `callback!` and `transformer!` of the same
//! names (kept for compatibility) define `#[proc_macro]` functions, so they can only be
//! expanded in a proc-macro crate. `tests/derive` does that, also with `announce` as its
//! only link to the code. Here their paths are resolved.
#![cfg(feature = "proc-macro-api")]

use announce::helper::{
    // Functions.
    callback,
    check,
    check_trait,
    check_type,
    format_prefix,
    gensym,
    is_expression_position,
    payload,
    redirect,
    remove_attributes,
    signature_only,
    transformer,
    // Types.
    CheckCall,
    CheckInput,
    Cont,
    ContList,
    Entry,
    Gather,
    Input,
    KvArgs,
    LeakError,
    LeakFailure,
    Meta,
    OwnedAttrs,
    Query,
    TypeLeak,
    // Constants.
    ANNOUNCE_FORMAT,
};
// Modules.
use announce::helper::{attrs, leak, type_leak};
// The names `callback` and `transformer` above are also the macros (defined with
// `#[macro_export]` in `announce-core`, re-exported by `helper`): a function and a macro
// of one name are in different namespaces, so one `use` imports both. The macros are not
// expanded here (they emit `#[proc_macro]`); `callback!` and `transformer!` are tested in
// `tests/user` through `tests/derive`.

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
    assert!(squash(out).starts_with("a::F!{@announce_format\"1\"[a::F!]"));
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
fn callback_and_transformer_run_closures() {
    // `callback` parses an `Input`, runs the closure and turns errors into `compile_error!`.
    let run = |input: TokenStream| {
        callback(input, |input: Input| {
            let name: syn::Ident = input.rest_as()?;
            let n = input.entries.len();
            Ok(template_quote::quote!(const #name: usize = #n;))
        })
    };
    let out = run(syn::parse_quote! { [cb!] a::S! => [] { struct S; } N });
    assert_eq!(squash(out), "constN:usize=1usize;");
    assert!(squash(run(syn::parse_quote! { [cb!] 1 })).starts_with("::core::compile_error!"));
    assert!(squash(run(syn::parse_quote! { not a callback })).contains("compile_error"));

    // `transformer` hands over the newest entry and forwards the result.
    let run = |input: TokenStream| {
        transformer(input, |mut entry: Entry| {
            entry.meta.push(syn::parse_quote!(seen));
            Ok(entry)
        })
    };
    let out = squash(run(
        syn::parse_quote! { [t!, cb!] b::T! => [] { struct T; } a::S! => [] { struct S; } REST },
    ));
    assert!(out.starts_with("cb!"), "{}", out);
    assert!(out.contains("[seen]{structT;}"), "{}", out);
    assert!(out.contains("structS;"), "{}", out);
    assert!(squash(run(syn::parse_quote! { [t!, cb!] REST })).contains("compile_error"));
    assert!(squash(run(
        syn::parse_quote! { [t!] a::S! => [] { struct S; } REST }
    ))
    .contains("compile_error"));
    // An error of the closure is reported, not forwarded.
    let err = transformer(
        syn::parse_quote! { [t!, cb!] a::S! => [] { struct S; } REST },
        |_| Err(syn::Error::new(proc_macro2::Span::call_site(), "refused")),
    );
    assert!(squash(err).contains("refused"));
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
    assert!(
        squash(query.answer(&owned)).starts_with("next!{@announce_format\"1\"@__announce_attrs")
    );
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

#[test]
fn recorded_leak_failure() {
    // An entry whose definition recorded that its types do not resolve outside its module.
    let input: Input = syn::parse_quote! {
        [cb!]
        deflib::lm::Leaky! => [
            type_leak_error(id = 42u64,
                            errors = [(Local) "without `repeater`" {
                                message = "use absolute path: 'Local'",
                                notes = ["a note"],
                                help = "write `Local` with an absolute path"
                            }])
        ] { pub struct Leaky { pub a: Local } }
        deflib::Plain! => [] { pub struct Plain; }
    };
    let (leaky, plain) = (&input.entries[0], &input.entries[1]);
    assert!(plain.leak_failure().unwrap().is_none());
    let failure: LeakFailure = leaky.leak_failure().unwrap().unwrap();
    assert_eq!(failure.id.base10_digits(), "42");
    assert_eq!(failure.ident, "Leaky");
    assert_eq!(squash(&failure.path), "deflib::lm::Leaky");
    let errors: &[LeakError] = &failure.errors;
    assert_eq!(errors.len(), 1);
    assert_eq!(squash(&errors[0].tokens), "Local");
    assert_eq!(errors[0].reason, "without `repeater`");
    // What the record carries for the cause, and nothing where it has none.
    assert_eq!(
        errors[0].message.as_deref(),
        Some("use absolute path: 'Local'")
    );
    assert_eq!(errors[0].notes, ["a note"]);
    assert_eq!(
        errors[0].help.as_deref(),
        Some("write `Local` with an absolute path")
    );

    // `type_leak()` fails, with a plain line for each cause that is true wherever the callback
    // is called, and ends with what the definer can do.
    let e = leaky.type_leak().err().unwrap();
    assert_eq!(
        e.to_string(),
        "this macro cannot use `deflib::lm::Leaky`: `Local` cannot be named outside its module; \
         add `repeater = ..` to its `#[announce]`, or write the path absolutely"
    );
    assert_eq!(failure.to_error().to_string(), e.to_string());

    // `item_resolved`: the item as written, and the check.
    let krate: syn::Path = syn::parse_quote!(::announce);
    let (item, check) = leaky.item_resolved(&krate).unwrap();
    assert_eq!(squash(&item), "pubstructLeaky{puba:Local}");
    let check = squash(&check);
    // A block of its own: the trait of the item, its impl and the call of the probe. The
    // texts are those of the trait where the compiler knows `on_unimplemented`: the message and
    // the help are what the record carries, and the notes are the way in, why and what to do.
    assert!(
        check.starts_with("const_:()={") && check.contains("traitInModuleOf_Leaky{}"),
        "{}",
        check
    );
    assert!(
        check.ends_with(
            "fn__check(){Check::used_outside_its_module(__AnnounceProbe::<42u64>.__announce_ctx());}};"
        ),
        "{}",
        check
    );
    if check.contains("on_unimplemented") {
        assert!(
            check.contains("message=\"useabsolutepath:'Local'\","),
            "{}",
            check
        );
        assert!(
            check.contains("label=\"relativetype,notinscopewhere`deflib::lm::Leaky!`iscalled\","),
            "{}",
            check
        );
        assert!(
            check.contains("note=\"`deflib::lm::Leaky`isusedoutsideitsmodule`deflib::lm`\","),
            "{}",
            check
        );
        assert!(
            check.contains("note=\"anote\",note=\"write`Local`withanabsolutepath\""),
            "{}",
            check
        );
    }
    assert_eq!(squash(failure.assume_success(&krate)), check);
    // Nothing for an item without a failure or a table.
    let (item, check) = plain.item_resolved(&krate).unwrap();
    assert_eq!(squash(&item), "pubstructPlain;");
    assert!(check.is_empty());

    // The META that the definer writes is the one that is read.
    let written = LeakFailure::new(
        syn::parse_quote!(S),
        syn::parse_quote!(7u64),
        vec![LeakError::new(
            "super::Other<u8>".parse::<TokenStream>().unwrap(),
            "with a relative trait path in a bound",
        )],
    );
    let meta: Meta = syn::parse2(written.meta()).unwrap();
    let read = LeakFailure::from_kv(
        &mut meta.kv().unwrap(),
        syn::parse_quote!(a::S),
        syn::parse_quote!(S),
    )
    .unwrap();
    assert_eq!(squash(&read.errors[0].tokens), "super::Other<u8>");
    assert_eq!(read.krate, None);

    // With the crate of the definer, which is read back and written again.
    let written = written.with_krate("deflib");
    let meta: Meta = syn::parse2(written.meta()).unwrap();
    let read = LeakFailure::from_kv(
        &mut meta.kv().unwrap(),
        syn::parse_quote!(a::S),
        syn::parse_quote!(S),
    )
    .unwrap();
    assert_eq!(read.krate.as_deref(), Some("deflib"));
    assert!(squash(read.meta()).ends_with(",krate=\"deflib\")"));
}

#[test]
fn recorded_leak_failure_is_read_with_what_a_later_version_adds() {
    // A later version may add keys to the record, to the texts of an error, and tokens after
    // them, with values of any shape. This one is read as if it had none of them.
    let input: Input = syn::parse_quote! {
        [cb!]
        deflib::lm::Leaky! => [
            type_leak_error(id = 42u64,
                            future = Vec<u8, A>,
                            errors = [(Local) "without `repeater`" {
                                message = "m",
                                span = a::b,
                                level = -1,
                                ty = Vec<u8, A>,
                                internal,
                                help = "h"
                            } later = [1, 2] { x = 1 },
                            (Other) "without `repeater`" flag],
                            krate = "deflib")
        ] { pub struct Leaky { pub a: Local, pub b: Other } }
    };
    let failure = input.entries[0].leak_failure().unwrap().unwrap();
    assert_eq!(failure.krate.as_deref(), Some("deflib"));
    assert_eq!(failure.errors.len(), 2);
    assert_eq!(failure.errors[0].message.as_deref(), Some("m"));
    assert_eq!(failure.errors[0].help.as_deref(), Some("h"));
    assert_eq!(squash(&failure.errors[1].tokens), "Other");
    // The error that a callback gets is for each cause, as before.
    let err = input.entries[0].type_leak().unwrap_err().to_string();
    assert!(
        err.contains("`Local` cannot be named outside its module"),
        "{}",
        err
    );
}

#[test]
fn announce_format_prefix() {
    // The version, the prefix, and `Input::format`.
    assert_eq!(ANNOUNCE_FORMAT, "1");
    assert_eq!(squash(format_prefix()), "@announce_format\"1\"");
    let by_hand: Input = syn::parse_quote! { [cb!] a::F! => [] { struct F; } };
    assert_eq!(by_hand.format, None);
    let prefixed: Input =
        syn::parse_quote! { @announce_format "1" [cb!] a::F! => [] { struct F; } };
    assert_eq!(prefixed.format.as_deref(), Some(ANNOUNCE_FORMAT));
    // Everything that the helpers emit starts with the prefix.
    let out = prefixed.fetch(vec![Cont::bang(syn::parse_quote!(g::G))]);
    assert!(squash(out).starts_with("g::G!{@announce_format\"1\""));
    // Another version is an error that names both.
    let e =
        syn::parse2::<Input>(template_quote::quote! { @announce_format "2" [cb!] }).unwrap_err();
    assert!(
        e.to_string().contains("found `@announce_format \"2\"`")
            && e.to_string().contains("expected `@announce_format \"1\"`"),
        "{}",
        e
    );
}
