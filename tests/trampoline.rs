use announce_tests::derive::{check_marker, summary};
use subcrate::subcrate;

macro_rules! capture {
    (@announce_format "1" [$($l:tt)*] $($t:tt)*) => {
        pub const OUT: &str = stringify!([$($l)*] $($t)*);
    };
}

/// Whitespace-free form, so assertions do not depend on `stringify!` spacing.
fn norm(s: &str) -> String {
    s.split_whitespace().collect()
}

mod bang {
    ::announce_tests::defs::kinds::Greet! { [::announce_tests::defs::kinds::Greet!, capture!] rest tokens }
}

#[test]
fn single_hop_shape() {
    let out = norm(bang::OUT);
    assert!(
        out.starts_with("[capture!]::announce_tests::defs::kinds::Greet!=>[]{pubtraitGreet"),
        "{}",
        out
    );
    assert!(out.ends_with("resttokens"), "{}", out);
}

mod kinds {
    use super::summary;
    use announce_tests::defs::kinds;
    kinds::Named! { [kinds::Named!, kinds::Tuple!, kinds::Shape!, kinds::Bits!, kinds::Greet!, kinds::Alias!, kinds::with_macro!, summary!] ALL }
}

#[test]
fn all_kinds_chain_in_order() {
    let items: Vec<&str> = kinds::ALL.iter().map(|r| r.2).collect();
    assert_eq!(
        items,
        [
            "struct Named",
            "struct Tuple",
            "enum Shape",
            "union Bits",
            "trait Greet (2 items)",
            "type Alias",
            "mod with_macro"
        ]
    );
}

mod cfg_eval {
    ::announce_tests::defs::kinds::Named! { [::announce_tests::defs::kinds::Named!, capture!] }
    pub mod shape {
        ::announce_tests::defs::kinds::Shape! { [::announce_tests::defs::kinds::Shape!, capture!] }
    }
}

#[test]
fn cfg_is_evaluated_in_the_defining_crate() {
    // This crate enables `x`; the definitions crate does not.
    let out = norm(cfg_eval::OUT);
    assert!(out.contains("only_without_x"), "{}", out);
    assert!(!out.contains("only_with_x"), "{}", out);
    assert!(!out.contains("#[cfg"), "{}", out);
    // `cfg_attr(.., doc = ..)` is evaluated too, and the doc it gives is not
    // in the payload (`tests/docs.rs`).
    assert!(!out.contains("cfg_attrevaluatedinthedefiner"), "{}", out);
    assert!(out.contains("pubc:u8"), "{}", out);
    assert!(!norm(cfg_eval::shape::OUT).contains("OnlyWithX"));
}

// The payload is re-emitted as real items, so `$` inside it must survive.
macro_rules! reemit {
    (@announce_format "1" [$($l:tt)*] $($p:ident)? $(:: $s:ident)* ! => [$($m:tt)*] { $($item:tt)* }) => {
        $($item)*
    };
}
mod copied {
    ::announce_tests::defs::kinds::with_macro! { [::announce_tests::defs::kinds::with_macro!, reemit!] }
}

#[test]
fn dollar_and_repetition_in_payload() {
    assert_eq!(copied::with_macro::six(), 6);
}

mod sig {
    ::announce_tests::defs::kinds::Sig! { [::announce_tests::defs::kinds::Sig!, ::announce::signature_only!, capture!] }
    pub mod module {
        ::announce_tests::defs::kinds::sig_mod! { [::announce_tests::defs::kinds::sig_mod!, ::announce::signature_only!, reemit!] }
    }
}

#[test]
fn signature_only_transformer_strips_bodies() {
    let out = norm(sig::OUT);
    assert!(out.contains("fnrequired(&self)->u32;"), "{}", out);
    assert!(
        out.contains("fnbody(&self)->u32{::core::unimplemented!()}"),
        "{}",
        out
    );
    assert!(!out.contains("42"), "{}", out);
    // Re-emitted module: the placeholder panics, the `impl Trait` fn keeps its body.
    assert_eq!(sig::module::sig_mod::opaque().to_string(), "8");
    assert!(std::panic::catch_unwind(sig::module::sig_mod::plain).is_err());
}

mod derive_route {
    use super::summary;
    // Derive head: the name `Parse` is the derive, which redirects to ParseAnnounce!.
    ::announce_tests::defs::kinds::Greet! {
        [::announce_tests::defs::kinds::Greet!, #[derive(::announce_tests::defs::conflict::Parse)] Named1, summary!] VIA_DERIVE
    }
    // Two unnamed derive routes in one module: `announce::helper` names them.
    pub mod twice {
        use announce_tests::derive::{start, summary};
        start!([#[derive(::announce_tests::defs::conflict::Parse)], summary!] ONE);
        start!([#[derive(::announce_tests::defs::conflict::Parse)], summary!] TWO);
    }
    // Attribute head: the attribute consumes the dummy.
    pub mod attr {
        use announce_tests::derive::summary;
        ::announce_tests::defs::kinds::Shape! {
            [::announce_tests::defs::kinds::Shape!, #[::announce_tests::defs::conflict::parse_attr], summary!] VIA_ATTR
        }
    }
    // Anonymous derive head written by hand: the fallback name is used.
    pub mod anon {
        use announce_tests::derive::summary;
        ::announce_tests::defs::kinds::Bits! {
            [::announce_tests::defs::kinds::Bits!, #[derive(::announce_tests::defs::conflict::Parse)], summary!] ANON
        }
    }
}

#[test]
fn derive_and_attribute_routes() {
    let keys: Vec<(&str, &str)> = derive_route::VIA_DERIVE
        .iter()
        .map(|r| (r.0, r.2))
        .collect();
    assert_eq!(
        keys,
        [
            ("Greet", "trait Greet (2 items)"),
            ("Parse", "trait Parse (1 items)")
        ]
    );
    assert_eq!(derive_route::twice::ONE.len(), 1);
    assert_eq!(derive_route::twice::TWO.len(), 1);
    assert_eq!(derive_route::attr::VIA_ATTR[1].0, "parse_attr");
    assert_eq!(derive_route::anon::ANON.len(), 2);
}

mod checked {
    use super::summary;
    ::announce_tests::defs::checks::Checked! { [::announce_tests::defs::checks::Checked!, summary!] CHECKED }
}

#[test]
fn check_is_listed_in_meta() {
    assert_eq!(
        norm(checked::CHECKED[0].1),
        "announce_test_derive::no_tuple(namedonly)"
    );
}

mod dollar_crate {
    announce_tests::defs::via_dollar_crate!();
}

#[test]
fn dollar_crate_in_list() {
    assert!(norm(dollar_crate::VIA_DOLLAR_CRATE).contains("pubtraitGreet"));
}

// Through the `announce` that `defs` links (see `Probe` below).
check_marker!(
    crate = ::announce_tests::defs::announce,
    ::announce_tests::defs::kinds::Named
);
check_marker!(
    crate = ::announce_tests::defs::announce,
    ::announce_tests::defs::kinds::Tuple<u8>
);
check_marker!(
    crate = ::announce_tests::defs::announce,
    dyn ::announce_tests::defs::markers::Simple
);
check_marker!(
    crate = ::announce_tests::defs::announce,
    dyn ::announce_tests::defs::kinds::Greet
);

#[test]
fn same_crate_calls() {
    assert!(norm(announce_tests::defs::vis::a::SAME_CRATE).contains("structCrateOnly"));
    assert!(norm(announce_tests::defs::vis::b::SAME_CRATE).contains("structPrivate"));
    assert!(norm(announce_tests::defs::vis::c::SAME_CRATE).contains("enumSuperOnly"));
    assert!(norm(announce_tests::defs::vis::d::SAME_CRATE).contains("traitGreet"));
}

mod start_bang {
    use announce_tests::derive::{start, summary};
    start!([::announce_tests::defs::kinds::Alias!, summary!] FROM_PROC);
}

#[test]
fn proc_macro_starter() {
    assert_eq!(start_bang::FROM_PROC[0].2, "type Alias");
}

mod metas {
    use super::summary;
    ::announce_tests::defs::metas::WithMeta! { [::announce_tests::defs::metas::WithMeta!, summary!] METAS }
}

#[test]
fn meta_option_is_listed_not_invoked() {
    assert_eq!(
        norm(metas::METAS[0].1),
        "my_lib::info(version=3),my_lib::flag"
    );
}

mod derived {
    use super::summary;
    ::announce_tests::defs::derived::ByDerive! { [::announce_tests::defs::derived::ByDerive!, summary!] DERIVED }
}

#[test]
fn a_derive_of_a_library_can_emit_a_trampoline() {
    assert_eq!(derived::DERIVED[0].2, "struct ByDerive");
    let _ = announce_tests::defs::derived::ByDerive { field: 1 };
}

mod reemitted_crate_paths {
    ::announce_tests::defs::uses_crate_paths! { [::announce_tests::defs::uses_crate_paths!, reemit!] }
}

#[test]
fn crate_paths_refer_to_the_defining_crate() {
    let v: ::announce_tests::defs::kinds::Alias = reemitted_crate_paths::uses_crate_paths::make();
    assert_eq!(v, [1, 2, 3]);
}

mod reemitted_for_in {
    // `crate::ARR` is the defining crate's: this crate has no `ARR`.
    ::announce_tests::defs::for_in_crate! { [::announce_tests::defs::for_in_crate!, reemit!] }
}

#[test]
fn crate_after_in_of_a_for_loop_refers_to_the_defining_crate() {
    // The `crate` after the `in` of a `for` loop is the defining crate's.
    assert_eq!(reemitted_for_in::for_in_crate::sum(), 6);
}

mod transformed {
    use super::summary;
    use announce_tests::derive::public_fields;
    // The caller chooses the transformer; the definition is a plain #[announce].
    ::announce_tests::defs::transformed::Partly! { [::announce_tests::defs::transformed::Partly!, public_fields!, summary!] T }
    ::announce_tests::defs::transformed::Partly! { [::announce_tests::defs::transformed::Partly!, public_fields!, capture!] }
    // Without the transformer the full definition arrives.
    pub mod untouched {
        ::announce_tests::defs::transformed::Partly! { [::announce_tests::defs::transformed::Partly!, capture!] }
    }
}

#[test]
fn call_side_transformer_rewrites_payload_and_adds_meta() {
    assert!(norm(transformed::untouched::OUT).contains("hidden:u8"));
    assert_eq!(norm(transformed::T[0].1), "orig(1),fields(a,c)");
    let out = norm(transformed::OUT);
    assert!(out.contains("pubstructPartly{puba:u8,pubc:u8}"), "{}", out);
}

mod compat_macros {
    // `callback!` and `transformer!` still define working proc macros.
    use super::summary;
    use announce_tests::derive::{compat_idents, compat_noop, compat_tag};
    ::announce_tests::defs::transformed::Partly! { [::announce_tests::defs::transformed::Partly!, compat_noop!, compat_tag!, summary!] T }
    ::announce_tests::defs::transformed::Partly! { [::announce_tests::defs::transformed::Partly!, compat_idents!] NAMES }
}

#[test]
fn compat_macros_callback_and_transformer_still_work() {
    assert_eq!(norm(compat_macros::T[0].1), "orig(1),tagged");
    assert_eq!(compat_macros::NAMES, ["Partly"]);
}

mod gathered {
    use announce_tests::derive::gathered;
    gathered! { [gathered!] (::announce_tests::defs::kinds::Shape, ::announce_tests::defs::kinds::Greet, ::announce_tests::defs::kinds::Tuple<u8>) G }
}

#[test]
fn gather_fetches_once_and_orders_entries() {
    assert_eq!(gathered::G, ["Shape", "Greet", "Tuple"]);
}

mod fetched_raw {
    use announce_tests::derive::fetched_raw;
    fetched_raw! { [fetched_raw!] (::announce_tests::defs::kinds::Shape, ::announce_tests::defs::kinds::Greet) F }
    pub mod with_options {
        // The options group (a leading `expr`, a namespace) survives the fetch.
        use super::fetched_raw;
        fetched_raw! { (expr, mine(1)) [fetched_raw!] (::announce_tests::defs::kinds::Shape) F }
    }
}

#[test]
fn raw_fetch_gets_the_definitions_and_records_what_it_fetched() {
    // `Input::fetch` keeps the books `gather` keeps.
    assert_eq!(fetched_raw::F, ["Shape", "Greet"]);
    assert_eq!(
        norm(fetched_raw::F_OPTIONS),
        "__announce_fetched(::announce_tests::defs::kinds::Shape,::announce_tests::defs::kinds::Greet)"
    );
    assert_eq!(fetched_raw::with_options::F, ["Shape"]);
    assert_eq!(
        norm(fetched_raw::with_options::F_OPTIONS),
        "expr,mine(1),__announce_fetched(::announce_tests::defs::kinds::Shape)"
    );
}

mod leaked {
    use announce_tests::derive::apply_leak;
    pub mod s {
        use super::apply_leak;
        ::announce_tests::defs::leaky::Leaky! { [::announce_tests::defs::leaky::Leaky!, apply_leak!] M }
    }
    pub mod t {
        use super::apply_leak;
        ::announce_tests::defs::leaky::LeakTrait! { [::announce_tests::defs::leaky::LeakTrait!, apply_leak!] M }
    }
    // The payload itself is the original item.
    pub mod raw {
        ::announce_tests::defs::leaky::Leaky! { [::announce_tests::defs::leaky::Leaky!, capture!] }
    }
}

#[test]
fn type_leak_info_lets_callbacks_resolve_local_types() {
    use announce_tests::defs::leaky::inner::Local;
    // Original payload plus type_leak(..) META.
    let raw = norm(leaked::raw::OUT);
    assert!(
        raw.contains("{pubstructLeaky<T>{puba:Local,pubb:Vec<T>,pubc:u8"),
        "{}",
        raw
    );
    assert!(
        raw.contains("type_leak(repeater=$crate::leaky::Repeater,id="),
        "{}",
        raw
    );
    // `Local` is not in scope in `leaked::s`; `TypeLeak::apply` resolves it.
    let v = leaked::s::Leaky::<u32> {
        a: Local(1),
        b: vec![2],
        c: 3,
    };
    assert_eq!(v.a, Local(1));
    struct X;
    impl leaked::t::LeakTrait for X {
        fn make(&self) -> Local {
            Local(5)
        }
    }
    assert_eq!(leaked::t::LeakTrait::make(&X), Local(5));
    // The META carries type-leak's `Referrer` (the types with the generics of
    // their contexts) and the context of the item.
    let m = norm(leaked::s::M[0]);
    assert!(m.contains("referrer=[\"type-leak-format-"), "{}", m);
    assert!(m.contains("(Local)") && m.contains("<T>;(Vec<T>)"), "{}", m);
    let t = norm(leaked::t::M[0]);
    assert!(
        t.contains("referrer=[\"type-leak-format-") && t.contains("(Local)"),
        "{}",
        t
    );
    assert!(!t.contains("Vec"), "{}", t);
}

mod crate_paths2 {
    ::announce_tests::defs::crate_paths2::Field! { [::announce_tests::defs::crate_paths2::Field!, reemit!] }
    ::announce_tests::defs::crate_paths2::with_dollar_crate! { [::announce_tests::defs::crate_paths2::with_dollar_crate!, capture!] }
}

#[test]
fn crate_after_single_colon_and_dollar_crate() {
    let f = crate_paths2::Field {
        a: ::announce_tests::defs::crate_paths2::Foo(1),
    };
    assert_eq!(f.a.0, 1);
    let out = norm(crate_paths2::OUT);
    assert!(out.contains("$crate::crate_paths2::Foo(9)"), "{}", out);
    assert!(!out.contains("$$crate"), "{}", out);
}

mod allow_opts {
    ::announce_tests::defs::allow_opts::WithString! { [::announce_tests::defs::allow_opts::WithString!, capture!] }
}

#[test]
fn allow_std_prelude_accepts_prelude_names() {
    let out = norm(allow_opts::OUT);
    assert!(out.contains("pubs:String"), "{}", out);
    assert!(!out.contains("type_leak("), "{}", out);
}

// Two `$`-payloads in one chain, and REST containing `$(..)*`.
macro_rules! reemit_two {
    (@announce_format "1" [$($l:tt)*]
     $($p1:ident)? $(:: $s1:ident)* ! => [$($m1:tt)*] { $($a:tt)* }
     $($p2:ident)? $(:: $s2:ident)* ! => [$($m2:tt)*] { $($b:tt)* }) => {
        $($a)*
        $($b)*
    };
}
mod two_dollar_payloads {
    ::announce_tests::defs::kinds::with_macro! {
        [::announce_tests::defs::kinds::with_macro!, ::announce_tests::defs::kinds::with_macro2!, reemit_two!]
    }
    pub mod rest {
        ::announce_tests::defs::kinds::with_macro! {
            [::announce_tests::defs::kinds::with_macro!, capture!]
            macro_rules! in_rest { ($($x:tt)*) => { $($x)* }; }
        }
    }
    pub mod via_summary {
        use announce_tests::derive::summary;
        ::announce_tests::defs::kinds::with_macro! {
            [::announce_tests::defs::kinds::with_macro!, ::announce_tests::defs::kinds::with_macro2!, summary!] S
        }
    }
    pub mod via_derive {
        use announce_tests::derive::summary;
        ::announce_tests::defs::kinds::with_macro2! {
            [::announce_tests::defs::kinds::with_macro2!, #[derive(::announce_tests::defs::conflict::Parse)] D1, ::announce_tests::defs::kinds::with_macro!, summary!] S
        }
    }
}

#[test]
fn chains_of_dollar_payloads_and_dollar_rest() {
    assert_eq!(two_dollar_payloads::with_macro::six(), 6);
    assert_eq!(two_dollar_payloads::with_macro2::twenty_four(), 24);
    let rest = norm(two_dollar_payloads::rest::OUT);
    assert!(
        rest.ends_with("macro_rules!in_rest{($($x:tt)*)=>{$($x)*};}"),
        "{}",
        rest
    );
    let kinds: Vec<&str> = two_dollar_payloads::via_summary::S
        .iter()
        .map(|r| r.2)
        .collect();
    assert_eq!(kinds, ["mod with_macro", "mod with_macro2"]);
    let kinds: Vec<&str> = two_dollar_payloads::via_derive::S
        .iter()
        .map(|r| r.2)
        .collect();
    assert_eq!(
        kinds,
        ["mod with_macro2", "trait Parse (1 items)", "mod with_macro"]
    );
}

mod raw_ident {
    ::announce_tests::defs::audit_high::r#match! { [::announce_tests::defs::audit_high::r#match!, capture!] }
}

#[test]
fn raw_identifier_items_work() {
    assert!(
        norm(raw_ident::OUT).contains("pubstructr#match;"),
        "{}",
        raw_ident::OUT
    );
}

// Positions: expression (with `(expr)` for `$`-payloads) and type.
macro_rules! expr_cb {
    (@announce_format "1" $( ($($o:tt)*) )? [$($l:tt)*] $($p:ident)? $(:: $s:ident)* ! => [$($m:tt)*] { $($item:tt)* } $($r:tt)*) => {
        stringify!($($item)*)
    };
}
macro_rules! expr_two {
    (@announce_format "1" $( ($($o:tt)*) )? [$($l:tt)*]
     $($p1:ident)? $(:: $s1:ident)* ! => [$($m1:tt)*] { $($a:tt)* }
     $($p2:ident)? $(:: $s2:ident)* ! => [$($m2:tt)*] { $($b:tt)* }) => {
        [stringify!($($a)*), stringify!($($b)*)]
    };
}
macro_rules! ty_cb {
    (@announce_format "1" [$($l:tt)*] $($t:tt)*) => {
        u32
    };
}
type FromTypePosition =
    ::announce_tests::defs::kinds::Named! { [::announce_tests::defs::kinds::Named!, ty_cb!] };

#[test]
fn expression_and_type_positions() {
    // `$`-payload in expression position needs `(expr)`.
    let m: &str = ::announce_tests::defs::kinds::with_macro! { (expr) [::announce_tests::defs::kinds::with_macro!, expr_cb!] };
    assert!(
        norm(m).contains("macro_rules!sum{($($x:expr),*)=>{0$(+$x)*};}"),
        "{}",
        m
    );
    // Two `$`-payloads in one chain.
    let both = ::announce_tests::defs::kinds::with_macro! {
        (expr) [::announce_tests::defs::kinds::with_macro!, ::announce_tests::defs::kinds::with_macro2!, expr_two!]
    };
    assert!(norm(both[0]).contains("modwith_macro2"));
    assert!(norm(both[1]).contains("modwith_macro{"));
    // A plain payload works in expression position without the option.
    let g: &str =
        ::announce_tests::defs::kinds::Greet! { [::announce_tests::defs::kinds::Greet!, expr_cb!] };
    assert!(norm(g).starts_with("pubtraitGreet"));
    // With a transformer in the chain.
    let t: &str = ::announce_tests::defs::kinds::Sig! { (expr) [::announce_tests::defs::kinds::Sig!, ::announce::signature_only!, expr_cb!] };
    assert!(norm(t).contains("{::core::unimplemented!()}"), "{}", t);
    // Type position.
    let x: FromTypePosition = 7;
    assert_eq!(x, 7u32);
}

mod audit3 {
    use announce_tests::derive::apply_leak;
    pub mod mg {
        use super::apply_leak;
        ::announce_tests::defs::audit3::MethodGenerics! { [::announce_tests::defs::audit3::MethodGenerics!, apply_leak!] M }
    }
    pub mod bnd {
        use super::apply_leak;
        ::announce_tests::defs::audit3::Bnd! { [::announce_tests::defs::audit3::Bnd!, apply_leak!] M }
    }
    pub mod dflt {
        use super::apply_leak;
        ::announce_tests::defs::audit3::Dflt! { [::announce_tests::defs::audit3::Dflt!, apply_leak!] M }
    }
}

#[test]
fn audit3_repeater_tables() {
    use announce_tests::defs::audit3::inner::{Local, LocalG, LocalL};
    struct X;
    impl audit3::mg::MethodGenerics for X {
        fn f<U>(&self, x: LocalG<U>) -> U {
            x.0
        }
        fn g<'a>(&'a self, x: &'a u8) -> LocalL<'a> {
            LocalL(x)
        }
    }
    let x = X;
    assert_eq!(audit3::mg::MethodGenerics::f(&x, LocalG(5u32)), 5);
    assert_eq!(*audit3::mg::MethodGenerics::g(&x, &7).0, 7);
    let b = audit3::bnd::Bnd::<u8>(1, Local(2));
    assert_eq!(b.1, Local(2));
    let d: audit3::dflt::Dflt = audit3::dflt::Dflt(Local(3));
    assert_eq!(d.0, Local(3));
    // Two contexts, each with the generics it uses: the type parameter `U` of
    // `f` and the lifetime `'a` of `g`.
    let m = norm(audit3::mg::M[0]);
    assert!(m.contains("<U>;(LocalG<U>)"), "{}", m);
    assert!(m.contains("<'a>;(LocalL<'a>)"), "{}", m);
}

macro_rules! opts_cb {
    (@announce_format "1" ($($o:tt)*) [$($l:tt)*] $($r:tt)*) => { stringify!($($o)*) };
}

#[test]
fn options_are_forwarded() {
    let o: &str = ::announce_tests::defs::kinds::with_macro! {
        (expr, my_opt(1)) [::announce_tests::defs::kinds::with_macro!, ::announce_tests::defs::kinds::Greet!, opts_cb!]
    };
    assert_eq!(norm(o), "expr,my_opt(1)");
}

// `$`-payloads no longer emit helper items: they work in type position and in
// expression position without `(expr)`.
macro_rules! dollar_ty_cb {
    ($($t:tt)*) => {
        u64
    };
}
type DollarInTypePosition = ::announce_tests::defs::kinds::with_macro! { [::announce_tests::defs::kinds::with_macro!, dollar_ty_cb!] };

#[test]
fn dollar_payloads_in_any_position() {
    let x: DollarInTypePosition = 5;
    assert_eq!(x, 5u64);
    let m: &str = ::announce_tests::defs::kinds::with_macro! { [::announce_tests::defs::kinds::with_macro!, expr_cb!] };
    assert!(
        norm(m).contains("macro_rules!sum{($($x:expr),*)=>{0$(+$x)*};}"),
        "{}",
        m
    );
    let both = ::announce_tests::defs::kinds::with_macro! {
        [::announce_tests::defs::kinds::with_macro!, ::announce_tests::defs::kinds::with_macro2!, expr_two!]
    };
    assert!(norm(both[1]).contains("modwith_macro{"));
}

mod with_derive_m {
    ::announce_tests::defs::markers::WithDerive! { [::announce_tests::defs::markers::WithDerive!, capture!] }
    pub mod e {
        ::announce_tests::defs::markers::WithDeriveEnum! { [::announce_tests::defs::markers::WithDeriveEnum!, capture!] }
    }
}

#[test]
fn announce_with_a_derive_of_another_trait() {
    fn is_announced<T: ?Sized + announce_tests::defs::announce::Announce>() {}
    is_announced::<announce_tests::defs::markers::WithDerive>();
    is_announced::<announce_tests::defs::markers::WithDeriveEnum>();
    let out = norm(with_derive_m::OUT);
    // The derive, the cfg'd-out field and the docs are gone; META is kept.
    assert!(
        out.contains("=>[$crate::info(1)]{pubstructWithDerive{puba:u8,}}")
            && out.ends_with("pubstructWithDerive{puba:u8,}}"),
        "{}",
        out
    );
    assert!(
        !out.contains("Debug") && !out.contains("#[announce(meta"),
        "{}",
        out
    );
    assert!(norm(with_derive_m::e::OUT).contains("pubenumWithDeriveEnum"));
}

// `Probe::<T>::IS` is true exactly when `T: Announce`, for the `announce` that `defs` links
// (under `cargo check` and clippy, not the build of `announce` that this file links).
mod probe {
    pub trait NotAnnounced {
        const IS: bool = false;
    }
    impl<T: ?Sized> NotAnnounced for T {}
    pub struct Probe<T: ?Sized>(std::marker::PhantomData<T>);
    impl<T: ?Sized + announce_tests::defs::announce::Announce> Probe<T> {
        pub const IS: bool = true;
    }
}
use probe::{NotAnnounced as _, Probe};

/// Hides a constant from clippy (`std::hint::black_box` needs rustc 1.66).
fn opaque(b: bool) -> bool {
    b
}

/// `T: Announce`, as a value clippy does not take for a constant.
macro_rules! announced {
    ($t:ty) => {
        opaque(Probe::<$t>::IS)
    };
}

mod crate_opt {
    use announce_tests::defs::crate_opt as d;
    // `crate = crate::crate_opt::spy`: the marker impl names the spy's `Announce`.
    pub fn spy<T: ?Sized + d::spy::Announce>() {}
    pub fn real<T: ?Sized + announce_tests::defs::announce::Announce>() {}
    d::ViaSpy! { [d::ViaSpy!, capture!] }
}

#[test]
fn crate_option_names_the_announce_crate() {
    use announce_tests::defs::crate_opt as d;
    crate_opt::spy::<d::ViaSpy>();
    crate_opt::spy::<dyn d::SpyTrait>();
    // The default path (`::announce`) was not used for these items.
    assert!(!announced!(d::ViaSpy));
    assert!(!announced!(dyn d::SpyTrait));
    // `crate = ::announce` is the same as the default.
    crate_opt::real::<d::Explicit>();
    assert!(announced!(d::Explicit));
    // The trampoline does not depend on the option.
    assert!(
        norm(crate_opt::OUT).contains("pubstructViaSpy"),
        "{}",
        crate_opt::OUT
    );
}

// M8: `signature_only!` on a module with const fn, `impl Trait`, plain fns, nested
// modules and an impl block. The module is re-emitted and used.
mod sig_full {
    ::announce_tests::defs::sig_full::m! { [::announce_tests::defs::sig_full::m!, ::announce::signature_only!, reemit!] }
}

#[test]
fn signature_only_on_a_module_keeps_const_and_opaque_fns() {
    use sig_full::m;
    // `const fn` keeps its body: it is usable in a `const`, also through a `const` item.
    const Z: u32 = m::zero();
    const C: u32 = m::S::cmethod();
    assert_eq!((Z, m::ZERO, C), (0, 0, 11));
    // `impl Trait` fns keep their bodies, also in impl blocks.
    assert_eq!(m::opaque().to_string(), "8");
    assert_eq!(format!("{:?}", m::S.opaque_method()), "12");
    // The other fns, in impl blocks and nested modules too, now panic.
    let panics =
        |f: &dyn Fn() -> u32| std::panic::catch_unwind(std::panic::AssertUnwindSafe(f)).is_err();
    assert!(panics(&|| m::plain()));
    assert!(panics(&|| m::S.method()));
    assert!(panics(&|| m::nested::deep()));
}

// M9: the module-wrapper pattern.
mod wrapper {
    use announce_tests::defs::wrapper as w;
    use announce_tests::derive::summary;
    pub mod real {
        use super::*;
        w::Gadget! { [w::Gadget!, summary!] S }
    }
    pub mod wrapped {
        use super::*;
        w::GadgetWrapped! { [w::GadgetWrapped!, summary!] S }
        pub mod raw {
            use announce_tests::defs::wrapper as w;
            w::GadgetWrapped! { [w::GadgetWrapped!, capture!] }
        }
    }
    // Everything in the wrapped copy resolves in this crate: unwrap and use it.
    pub mod resolved {
        use announce_tests::defs::wrapper as w;
        w::ResolvedWrapped! { [w::ResolvedWrapped!, reemit!] }
    }
}

#[test]
fn module_wrapper_pattern() {
    use announce_tests::defs::wrapper as w;
    // The real trait is announced as usual.
    assert_eq!(wrapper::real::S[0].2, "trait Gadget (1 items)");
    // The wrapper's trampoline hands out the module, with its META.
    assert_eq!(wrapper::wrapped::S[0].2, "mod Gadget");
    assert_eq!(norm(wrapper::wrapped::S[0].1), "$crate::info(wrapped)");
    let raw = norm(wrapper::wrapped::raw::OUT);
    assert!(
        raw.ends_with("pubmodGadget{pubtraitGadget:Base{fnf(&self)->Local;}}}"),
        "{}",
        raw
    );
    // The trait `Gadget` is not shadowed or duplicated: it is the real one, with
    // exactly one `Announce` impl for `dyn Gadget` (a second would not compile).
    struct X;
    impl w::Gadget for X {
        fn f(&self) -> u8 {
            4
        }
    }
    assert_eq!(w::Gadget::f(&X), 4);
    assert!(announced!(dyn w::Gadget));
    // The unwrapped, resolved copy.
    impl w::Base for X {}
    impl wrapper::resolved::Resolved::Resolved for X {
        fn f(&self) -> w::Local {
            w::Local(6)
        }
    }
    assert_eq!(wrapper::resolved::Resolved::Resolved::f(&X).0, 6);
}

// M10: `#[announce]` on a union with a derive, used across crates.
mod union_with_meta {
    ::announce_tests::defs::union_with_meta::Raw! { [::announce_tests::defs::union_with_meta::Raw!, reemit!] }
    pub mod meta {
        use announce_tests::derive::summary;
        ::announce_tests::defs::union_with_meta::Raw! { [::announce_tests::defs::union_with_meta::Raw!, summary!] S }
    }
}

#[test]
fn announce_on_a_union_with_a_derive() {
    assert!(announced!(announce_tests::defs::union_with_meta::Raw));
    assert_eq!(union_with_meta::meta::S[0].2, "union Raw");
    assert_eq!(norm(union_with_meta::meta::S[0].1), "$crate::info(union)");
    // The re-emitted union is the one as written.
    let r = union_with_meta::Raw { f: 1.0 };
    assert_eq!(unsafe { r.i }, 1.0f32.to_bits());
}

// M11: pattern and statement positions.
macro_rules! struct_pat {
    (@announce_format "1" [$($l:tt)*] $($p:ident)? $(:: $s:ident)* ! => [$($m:tt)*]
     { $(#[$($a:tt)*])* $v:vis struct $name:ident $($body:tt)* } $($r:tt)*) => {
        $name { .. }
    };
}
macro_rules! first_variant_pat {
    (@announce_format "1" [$($l:tt)*] $($p:ident)? $(:: $s:ident)* ! => [$($m:tt)*]
     { $(#[$($a:tt)*])* $v:vis enum $name:ident { $first:ident $($body:tt)* } } $($r:tt)*) => {
        $name::$first(..)
    };
}
macro_rules! let_cb {
    (@announce_format "1" [$($l:tt)*] $($p:ident)? $(:: $s:ident)* ! => [$($m:tt)*] { $($item:tt)* } $bound:ident) => {
        let $bound = stringify!($($item)*);
    };
}

#[test]
fn pattern_position() {
    use announce_tests::defs::kinds::{Named, Shape};
    let n = Named {
        a: 1,
        only_without_x: 2,
        c: 3,
    };
    assert!(matches!(
        n,
        ::announce_tests::defs::kinds::Named! { [::announce_tests::defs::kinds::Named!, struct_pat!] }
    ));
    // A `let` is a pattern position too.
    let ::announce_tests::defs::kinds::Named! { [::announce_tests::defs::kinds::Named!, struct_pat!] } =
        n;
    let classify = |s: &Shape| match s {
        ::announce_tests::defs::kinds::Shape! { [::announce_tests::defs::kinds::Shape!, first_variant_pat!] } => {
            "circle"
        }
        _ => "other",
    };
    assert_eq!(classify(&Shape::Circle(1.0)), "circle");
    assert_eq!(classify(&Shape::Rect { w: 1.0, h: 2.0 }), "other");
}

#[test]
fn statement_position() {
    // A `let` produced by the callback; `bound` comes from REST, so it is visible here.
    ::announce_tests::defs::kinds::Named! { [::announce_tests::defs::kinds::Named!, let_cb!] bound }
    assert!(norm(bound).contains("pubstructNamed"), "{}", bound);
    // Items re-emitted into a function body, followed by a use.
    ::announce_tests::defs::kinds::Tuple! { [::announce_tests::defs::kinds::Tuple!, reemit!] }
    let t = Tuple::<&str>("x", 1);
    assert_eq!((t.0, t.1), ("x", 1));
    // A union through the same derive-free route (`!` only), still in statement position.
    ::announce_tests::defs::kinds::Bits! { [::announce_tests::defs::kinds::Bits!, reemit!] }
    let b = Bits { i: 0 };
    assert_eq!(unsafe { b.f }, 0.0);
}

// M12: the item-level `#[cfg]` is false in the defining crate: the other half is
// `cfg_gone` in `tests/ui_options.rs`. A true cfg leaves the trampoline.
mod cfg_present {
    ::announce_tests::defs::gone::Present! { [::announce_tests::defs::gone::Present!, capture!] }
}

#[test]
fn item_level_true_cfg_keeps_the_trampoline() {
    let out = norm(cfg_present::OUT);
    assert!(out.contains("pubstructPresent;"), "{}", out);
    assert!(!out.contains("cfg"), "{}", out);
}

// ---- A callback crate and a definer of this file ----

// A trampoline called across real crate boundaries, and a chain of trampolines
// delivered to a callback written with `announce::helper::callback`.
//
// - `callbacks`: a proc-macro crate with the callbacks (`#[subcrate(proc_macro, ..)]`);
// - `definer`: the defining crate, with the `#[announce]`d items;
// - this file is the calling crate.
//
// The test macros print what they receive as strings, so the assertions read the
// payloads and the META exactly as the calling crate gets them.

#[subcrate(proc_macro, ::announce, ::syn, ::proc_macro2, ::template_quote)]
pub mod callbacks {
    use announce::helper::{Cont, Gather};
    use proc_macro::TokenStream;
    use template_quote::{quote, ToTokens};

    /// `summary! { [summary!] ENTRY* NAME }` emits
    /// `pub const NAME: &[(&str, &str, &str)] = &[(key, meta, item kind and ident), ..]`
    /// in visiting order. It forwards to the next continuation if there is one.
    #[proc_macro]
    pub fn summary(input: TokenStream) -> TokenStream {
        announce::helper::callback(input.into(), |input| {
            if let Some(fwd) = input.forward() {
                return Ok(fwd);
            }
            let name: syn::Ident = input.rest_as()?;
            let rows = input.entries_in_order().map(|e| {
                let key = e.key.path().segments.last().unwrap().ident.to_string();
                let meta = e
                    .meta
                    .iter()
                    .map(|m| m.to_token_stream().to_string())
                    .collect::<Vec<_>>()
                    .join(", ");
                let item = match e.item() {
                    Ok(syn::Item::Struct(s)) => format!("struct {}", s.ident),
                    Ok(syn::Item::Enum(s)) => format!("enum {}", s.ident),
                    Ok(syn::Item::Union(s)) => format!("union {}", s.ident),
                    Ok(syn::Item::Trait(s)) => {
                        format!("trait {} ({} items)", s.ident, s.items.len())
                    }
                    Ok(syn::Item::Type(s)) => format!("type {}", s.ident),
                    Ok(syn::Item::Mod(s)) => format!("mod {}", s.ident),
                    Ok(_) => "other".to_string(),
                    Err(e) => format!("error: {}", e),
                };
                quote! { (#key, #meta, #item) }
            });
            Ok(quote! { pub const #name: &[(&str, &str, &str)] = &[ #(#rows),* ]; })
        })
        .into()
    }

    /// An announce-aware derive: when a trampoline is called with `#[derive(path::Parse)]` as
    /// its continuation, the derive gets the payload in a dummy item and redirects it to the
    /// trampoline of the trait, which is renamed because this derive has the name `Parse`.
    #[proc_macro_derive(Parse)]
    pub fn derive_parse(input: TokenStream) -> TokenStream {
        let input: proc_macro2::TokenStream = input.into();
        match announce::helper::redirect(
            input,
            &syn::parse_quote!(::definer::conflict::ParseAnnounce),
        ) {
            Some(out) => out.into(),
            None => TokenStream::new(),
        }
    }

    /// The attribute form of [`derive_parse`], which consumes the dummy.
    #[proc_macro_attribute]
    pub fn parse_attr(_: TokenStream, input: TokenStream) -> TokenStream {
        let input: proc_macro2::TokenStream = input.into();
        match announce::helper::redirect(
            input.clone(),
            &syn::parse_quote!(::definer::conflict::ParseAnnounce),
        ) {
            Some(out) => out.into(),
            None => input.into(),
        }
    }

    /// Starts a trampoline from a proc macro: `start!([a!, #[derive(B)], ..] REST)`.
    /// Continuations that are attributes get random dummy names.
    #[proc_macro]
    pub fn start(input: TokenStream) -> TokenStream {
        struct Start(announce::helper::ContList, proc_macro2::TokenStream);
        impl syn::parse::Parse for Start {
            fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
                let content;
                syn::bracketed!(content in input);
                Ok(Start(content.parse()?, input.parse()?))
            }
        }
        match syn::parse::<Start>(input) {
            Ok(Start(list, rest)) => list.invoke(rest).into(),
            Err(e) => e.to_compile_error().into(),
        }
    }

    /// `gathered! { [gathered!] (a::A, b::B) NAME }`: asks for the listed definitions, in
    /// one go, and emits `NAME: &[&str]` with their item idents in the requested order.
    #[proc_macro]
    pub fn gathered(input: TokenStream) -> TokenStream {
        struct Rest {
            paths: Vec<syn::Path>,
            name: syn::Ident,
        }
        impl syn::parse::Parse for Rest {
            fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
                let content;
                syn::parenthesized!(content in input);
                let paths =
                    syn::punctuated::Punctuated::<syn::Path, syn::Token![,]>::parse_terminated(
                        &content,
                    )?;
                Ok(Rest {
                    paths: paths.into_iter().collect(),
                    name: input.parse()?,
                })
            }
        }
        announce::helper::callback(input.into(), |input| {
            let rest: Rest = input.rest_as()?;
            let needed = rest.paths.iter().cloned().map(Cont::bang);
            let entries = match input.gather(needed) {
                Gather::Ready(e) => e,
                Gather::Fetch(ts) => return Ok(ts),
            };
            let mut idents = Vec::new();
            for e in entries {
                idents.push(match e.item()? {
                    syn::Item::Struct(s) => s.ident.to_string(),
                    syn::Item::Enum(s) => s.ident.to_string(),
                    syn::Item::Trait(s) => s.ident.to_string(),
                    other => return Err(syn::Error::new_spanned(other, "unexpected item")),
                });
            }
            let name = &rest.name;
            Ok(quote! { pub const #name: &[&str] = &[ #(#idents),* ]; })
        })
        .into()
    }
}

#[subcrate(::announce, callbacks)]
pub mod definer {
    use announce::announce;

    #[announce]
    #[derive(Clone, Debug)]
    pub struct Named {
        pub a: u32,
        // The definer is not built with `cfg(test)`, the calling crate is.
        #[cfg(test)]
        pub only_in_test: u32,
        #[cfg(not(test))]
        pub only_in_definer: u32,
        #[cfg_attr(not(test), doc = "cfg_attr evaluated in the definer")]
        pub c: u8,
    }

    #[announce]
    pub struct Tuple<T>(pub T, pub u8);

    #[announce]
    pub enum Shape {
        Circle(f64),
        Rect {
            w: f64,
            h: f64,
        },
        #[cfg(test)]
        OnlyInTest,
    }

    #[announce]
    #[derive(Clone, Copy)]
    pub union Bits {
        pub i: u32,
        pub f: f32,
    }

    #[announce]
    pub trait Greet {
        fn hi(&self) -> &'static str;
        fn twice(&self) -> [&'static str; 2] {
            [self.hi(), self.hi()]
        }
    }

    #[announce]
    pub type Alias = ::std::vec::Vec<u8>;

    #[announce]
    pub mod inline {
        pub fn six() -> i32 {
            1 + 2 + 3
        }
    }

    /// The name `Parse` is taken by the derive, so the trampoline is renamed.
    pub mod conflict {
        use announce::announce;

        #[announce(macro_name = ParseAnnounce)]
        pub trait Parse {
            fn parse(s: &str) -> Self;
        }
        pub use callbacks::{parse_attr, Parse};
    }

    /// `meta = path(args)` is carried to the callbacks as written, not invoked.
    #[announce(meta = my_lib::info(version = 3), meta = my_lib::flag)]
    pub struct WithMeta;
}

mod crates {
    use super::*;

    /// Fails to compile unless `T` is announced. The trait is that of the `announce` that
    /// `definer` links: under `cargo check` and clippy, another build than this file links.
    fn is_announced<T: ?Sized + definer::announce::Announce>() {}

    macro_rules! capture {
        ($(@announce_format "1")? [$($l:tt)*] $($t:tt)*) => {
            pub const OUT: &str = stringify!([$($l)*] $($t)*);
        };
    }

    /// The payload as a string without spaces, so assertions do not depend on the spacing
    /// of `stringify!`.
    fn norm(s: &str) -> String {
        s.split_whitespace().collect()
    }

    mod bang {
        ::definer::Greet! { [::definer::Greet!, capture!] rest tokens }
    }

    #[test]
    fn a_single_hop_hands_the_definition_and_the_rest_to_the_callback() {
        let out = norm(bang::OUT);
        // `[continuations] head! => [meta] { item } rest`: the head is dropped from the list
        // and named in front of the payload.
        assert!(
            out.starts_with("[capture!]::definer::Greet!=>[]{pubtraitGreet"),
            "{}",
            out
        );
        assert!(out.ends_with("resttokens"), "{}", out);
    }

    mod kinds {
        use crate::callbacks::summary;
        ::definer::Named! {
            [::definer::Named!, ::definer::Tuple!, ::definer::Shape!, ::definer::Bits!, ::definer::Greet!,
             ::definer::Alias!, ::definer::inline!, summary!] ALL
        }
    }

    #[test]
    fn a_chain_visits_every_kind_of_item_in_order() {
        let items: Vec<&str> = kinds::ALL.iter().map(|r| r.2).collect();
        assert_eq!(
            items,
            [
                "struct Named",
                "struct Tuple",
                "enum Shape",
                "union Bits",
                "trait Greet (2 items)",
                "type Alias",
                "mod inline"
            ]
        );
        let keys: Vec<&str> = kinds::ALL.iter().map(|r| r.0).collect();
        assert_eq!(
            keys,
            ["Named", "Tuple", "Shape", "Bits", "Greet", "Alias", "inline"]
        );
    }

    mod cfg_eval {
        ::definer::Named! { [::definer::Named!, capture!] }
        pub mod shape {
            ::definer::Shape! { [::definer::Shape!, capture!] }
        }
    }

    #[test]
    fn cfg_is_evaluated_in_the_defining_crate() {
        // This crate is built with `cfg(test)`, the definer is not.
        let out = norm(cfg_eval::OUT);
        assert!(out.contains("only_in_definer"), "{}", out);
        assert!(!out.contains("only_in_test"), "{}", out);
        assert!(!out.contains("#[cfg"), "{}", out);
        // `cfg_attr(.., doc = ..)` is evaluated too, and the doc is not in the payload.
        assert!(!out.contains("cfg_attrevaluatedinthedefiner"), "{}", out);
        assert!(out.contains("pubc:u8"), "{}", out);
        assert!(!norm(cfg_eval::shape::OUT).contains("OnlyInTest"));
    }

    mod metas {
        use crate::callbacks::summary;
        ::definer::WithMeta! { [::definer::WithMeta!, summary!] METAS }
    }

    #[test]
    fn meta_is_listed_for_the_callback_and_not_invoked() {
        assert_eq!(
            norm(metas::METAS[0].1),
            "my_lib::info(version=3),my_lib::flag"
        );
    }

    mod derive_route {
        use crate::callbacks::summary;
        // The continuation is the derive: the name `Parse` is the derive, which redirects the
        // payload to `ParseAnnounce!`.
        ::definer::Greet! {
            [::definer::Greet!, #[derive(::definer::conflict::Parse)] Named1, summary!] VIA_DERIVE
        }
        // Two unnamed derive routes in one module: `announce::helper` names the dummies.
        pub mod twice {
            use crate::callbacks::{start, summary};
            start!([#[derive(::definer::conflict::Parse)], summary!] ONE);
            start!([#[derive(::definer::conflict::Parse)], summary!] TWO);
        }
        // The attribute form: the attribute consumes the dummy.
        pub mod attr {
            use crate::callbacks::summary;
            ::definer::Shape! {
                [::definer::Shape!, #[::definer::conflict::parse_attr], summary!] VIA_ATTR
            }
        }
    }

    #[test]
    fn a_derive_or_an_attribute_of_the_same_name_can_take_the_trampoline() {
        let items: Vec<&str> = derive_route::VIA_DERIVE.iter().map(|r| r.2).collect();
        assert_eq!(items, ["trait Greet (2 items)", "trait Parse (1 items)"]);
        assert_eq!(derive_route::twice::ONE.len(), 1);
        assert_eq!(derive_route::twice::TWO.len(), 1);
        let items: Vec<&str> = derive_route::attr::VIA_ATTR.iter().map(|r| r.2).collect();
        assert_eq!(items, ["enum Shape", "trait Parse (1 items)"]);
    }

    mod gathered {
        use crate::callbacks::gathered;
        gathered! { [gathered!] (::definer::Shape, ::definer::Greet, ::definer::Tuple<u8>) G }
    }

    #[test]
    fn a_callback_that_gathers_fetches_the_definitions_once_in_the_order_asked() {
        // The callback starts the chain itself: it asks the three trampolines and is called
        // again with the entries.
        assert_eq!(gathered::G, ["Shape", "Greet", "Tuple"]);
    }

    mod reemitted {
        macro_rules! reemit {
            ($(@announce_format "1")? [$($l:tt)*] $($p:ident)? $(:: $s:ident)* ! => [$($m:tt)*] { $($item:tt)* }) => {
                $($item)*
            };
        }
        ::definer::inline! { [::definer::inline!, reemit!] }
    }

    #[test]
    fn the_payload_is_a_definition_the_caller_can_emit_again() {
        assert_eq!(reemitted::inline::six(), 6);
    }

    #[test]
    fn the_announce_impl_is_in_the_defining_crate() {
        is_announced::<definer::Named>();
        is_announced::<definer::Tuple<u8>>();
        is_announced::<definer::Shape>();
        is_announced::<definer::Bits>();
        is_announced::<dyn definer::Greet>();
        // The derive of another trait ran, and the item is a real type here.
        let n = definer::Named {
            a: 1,
            only_in_definer: 2,
            c: 3,
        };
        assert!(format!("{:?}", n.clone()).contains("only_in_definer"));
    }
}
