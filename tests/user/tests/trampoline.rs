use announce_test_derive::{check_marker, summary};

macro_rules! capture {
    ([$($l:tt)*] $($t:tt)*) => {
        pub const OUT: &str = stringify!([$($l)*] $($t)*);
    };
}

/// Whitespace-free form, so assertions do not depend on `stringify!` spacing.
fn norm(s: &str) -> String {
    s.split_whitespace().collect()
}

mod bang {
    ::announce_test_defs::kinds::Greet! { [::announce_test_defs::kinds::Greet!, capture!] rest tokens }
}

#[test]
fn single_hop_shape() {
    let out = norm(bang::OUT);
    assert!(
        out.starts_with("[capture!]::announce_test_defs::kinds::Greet!=>[]{pubtraitGreet"),
        "{}",
        out
    );
    assert!(out.ends_with("resttokens"), "{}", out);
}

mod kinds {
    use super::summary;
    use announce_test_defs::kinds;
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
    ::announce_test_defs::kinds::Named! { [::announce_test_defs::kinds::Named!, capture!] }
    pub mod shape {
        ::announce_test_defs::kinds::Shape! { [::announce_test_defs::kinds::Shape!, capture!] }
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
    ([$($l:tt)*] $($p:ident)? $(:: $s:ident)* ! => [$($m:tt)*] { $($item:tt)* }) => {
        $($item)*
    };
}
mod copied {
    ::announce_test_defs::kinds::with_macro! { [::announce_test_defs::kinds::with_macro!, reemit!] }
}

#[test]
fn dollar_and_repetition_in_payload() {
    assert_eq!(copied::with_macro::six(), 6);
}

mod sig {
    ::announce_test_defs::kinds::Sig! { [::announce_test_defs::kinds::Sig!, ::announce::signature_only!, capture!] }
    pub mod module {
        ::announce_test_defs::kinds::sig_mod! { [::announce_test_defs::kinds::sig_mod!, ::announce::signature_only!, reemit!] }
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
    ::announce_test_defs::kinds::Greet! {
        [::announce_test_defs::kinds::Greet!, #[derive(::announce_test_defs::conflict::Parse)] Named1, summary!] VIA_DERIVE
    }
    // Two unnamed derive routes in one module: announce-helper names them.
    pub mod twice {
        use announce_test_derive::{start, summary};
        start!([#[derive(::announce_test_defs::conflict::Parse)], summary!] ONE);
        start!([#[derive(::announce_test_defs::conflict::Parse)], summary!] TWO);
    }
    // Attribute head: the attribute consumes the dummy.
    pub mod attr {
        use announce_test_derive::summary;
        ::announce_test_defs::kinds::Shape! {
            [::announce_test_defs::kinds::Shape!, #[::announce_test_defs::conflict::parse_attr], summary!] VIA_ATTR
        }
    }
    // Anonymous derive head written by hand: the fallback name is used.
    pub mod anon {
        use announce_test_derive::summary;
        ::announce_test_defs::kinds::Bits! {
            [::announce_test_defs::kinds::Bits!, #[derive(::announce_test_defs::conflict::Parse)], summary!] ANON
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
    ::announce_test_defs::checks::Checked! { [::announce_test_defs::checks::Checked!, summary!] CHECKED }
}

#[test]
fn check_is_listed_in_meta() {
    assert_eq!(
        norm(checked::CHECKED[0].1),
        "announce_test_derive::no_tuple(namedonly)"
    );
}

mod dollar_crate {
    announce_test_defs::via_dollar_crate!();
}

#[test]
fn dollar_crate_in_list() {
    assert!(norm(dollar_crate::VIA_DOLLAR_CRATE).contains("pubtraitGreet"));
}

check_marker!(::announce_test_defs::kinds::Named);
check_marker!(::announce_test_defs::kinds::Tuple<u8>);
check_marker!(dyn ::announce_test_defs::markers::Simple);
check_marker!(dyn ::announce_test_defs::kinds::Greet);

#[test]
fn same_crate_calls() {
    assert!(norm(announce_test_defs::vis::a::SAME_CRATE).contains("structCrateOnly"));
    assert!(norm(announce_test_defs::vis::b::SAME_CRATE).contains("structPrivate"));
    assert!(norm(announce_test_defs::vis::c::SAME_CRATE).contains("enumSuperOnly"));
    assert!(norm(announce_test_defs::vis::d::SAME_CRATE).contains("traitGreet"));
}

mod start_bang {
    use announce_test_derive::{start, summary};
    start!([::announce_test_defs::kinds::Alias!, summary!] FROM_PROC);
}

#[test]
fn proc_macro_starter() {
    assert_eq!(start_bang::FROM_PROC[0].2, "type Alias");
}

mod metas {
    use super::summary;
    ::announce_test_defs::metas::WithMeta! { [::announce_test_defs::metas::WithMeta!, summary!] METAS }
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
    ::announce_test_defs::derived::ByDerive! { [::announce_test_defs::derived::ByDerive!, summary!] DERIVED }
}

#[test]
fn derive_announces_its_input() {
    assert_eq!(derived::DERIVED[0].2, "struct ByDerive");
    let _ = announce_test_defs::derived::ByDerive { field: 1 };
}

mod reemitted_crate_paths {
    ::announce_test_defs::uses_crate_paths! { [::announce_test_defs::uses_crate_paths!, reemit!] }
}

#[test]
fn crate_paths_refer_to_the_defining_crate() {
    let v: ::announce_test_defs::kinds::Alias = reemitted_crate_paths::uses_crate_paths::make();
    assert_eq!(v, [1, 2, 3]);
}

mod reemitted_for_in {
    // `crate::ARR` is the defining crate's: this crate has no `ARR`.
    ::announce_test_defs::for_in_crate! { [::announce_test_defs::for_in_crate!, reemit!] }
}

#[test]
fn crate_after_in_of_a_for_loop_refers_to_the_defining_crate() {
    // ALGORITHM_FINDINGS #8.
    assert_eq!(reemitted_for_in::for_in_crate::sum(), 6);
}

mod transformed {
    use super::summary;
    use announce_test_derive::public_fields;
    // The caller chooses the transformer; the definition is a plain #[announce].
    ::announce_test_defs::transformed::Partly! { [::announce_test_defs::transformed::Partly!, public_fields!, summary!] T }
    ::announce_test_defs::transformed::Partly! { [::announce_test_defs::transformed::Partly!, public_fields!, capture!] }
    // Without the transformer the full definition arrives.
    pub mod untouched {
        ::announce_test_defs::transformed::Partly! { [::announce_test_defs::transformed::Partly!, capture!] }
    }
}

#[test]
fn call_side_transformer_rewrites_payload_and_adds_meta() {
    assert!(norm(transformed::untouched::OUT).contains("hidden:u8"));
    assert_eq!(norm(transformed::T[0].1), "orig(1),fields(a,c)");
    let out = norm(transformed::OUT);
    assert!(out.contains("pubstructPartly{puba:u8,pubc:u8}"), "{}", out);
}

mod gathered {
    use announce_test_derive::gathered;
    gathered! { [gathered!] (::announce_test_defs::kinds::Shape, ::announce_test_defs::kinds::Greet, ::announce_test_defs::kinds::Tuple<u8>) G }
}

#[test]
fn gather_fetches_once_and_orders_entries() {
    assert_eq!(gathered::G, ["Shape", "Greet", "Tuple"]);
}

mod fetched_raw {
    use announce_test_derive::fetched_raw;
    fetched_raw! { [fetched_raw!] (::announce_test_defs::kinds::Shape, ::announce_test_defs::kinds::Greet) F }
    pub mod with_options {
        // The options group (a leading `expr`, a namespace) survives the fetch.
        use super::fetched_raw;
        fetched_raw! { (expr, mine(1)) [fetched_raw!] (::announce_test_defs::kinds::Shape) F }
    }
}

#[test]
fn raw_fetch_gets_the_definitions_and_records_what_it_fetched() {
    // ALGORITHM_FINDINGS #13: `Input::fetch` keeps the books `gather` keeps.
    assert_eq!(fetched_raw::F, ["Shape", "Greet"]);
    assert_eq!(
        norm(fetched_raw::F_OPTIONS),
        "__announce_fetched(::announce_test_defs::kinds::Shape,::announce_test_defs::kinds::Greet)"
    );
    assert_eq!(fetched_raw::with_options::F, ["Shape"]);
    assert_eq!(
        norm(fetched_raw::with_options::F_OPTIONS),
        "expr,mine(1),__announce_fetched(::announce_test_defs::kinds::Shape)"
    );
}

mod leaked {
    use announce_test_derive::apply_leak;
    pub mod s {
        use super::apply_leak;
        ::announce_test_defs::leaky::Leaky! { [::announce_test_defs::leaky::Leaky!, apply_leak!] M }
    }
    pub mod t {
        use super::apply_leak;
        ::announce_test_defs::leaky::LeakTrait! { [::announce_test_defs::leaky::LeakTrait!, apply_leak!] M }
    }
    // The payload itself is the original item.
    pub mod raw {
        ::announce_test_defs::leaky::Leaky! { [::announce_test_defs::leaky::Leaky!, capture!] }
    }
}

#[test]
fn type_leak_info_lets_callbacks_resolve_local_types() {
    use announce_test_defs::leaky::inner::Local;
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
    ::announce_test_defs::crate_paths2::Field! { [::announce_test_defs::crate_paths2::Field!, reemit!] }
    ::announce_test_defs::crate_paths2::with_dollar_crate! { [::announce_test_defs::crate_paths2::with_dollar_crate!, capture!] }
}

#[test]
fn crate_after_single_colon_and_dollar_crate() {
    let f = crate_paths2::Field {
        a: ::announce_test_defs::crate_paths2::Foo(1),
    };
    assert_eq!(f.a.0, 1);
    let out = norm(crate_paths2::OUT);
    assert!(out.contains("$crate::crate_paths2::Foo(9)"), "{}", out);
    assert!(!out.contains("$$crate"), "{}", out);
}

mod allow_opts {
    ::announce_test_defs::allow_opts::WithString! { [::announce_test_defs::allow_opts::WithString!, capture!] }
}

#[test]
fn allow_std_prelude_accepts_prelude_names() {
    let out = norm(allow_opts::OUT);
    assert!(out.contains("pubs:String"), "{}", out);
    assert!(!out.contains("type_leak("), "{}", out);
}

// AUDIT #3: two `$`-payloads in one chain, and REST containing `$(..)*`.
macro_rules! reemit_two {
    ([$($l:tt)*]
     $($p1:ident)? $(:: $s1:ident)* ! => [$($m1:tt)*] { $($a:tt)* }
     $($p2:ident)? $(:: $s2:ident)* ! => [$($m2:tt)*] { $($b:tt)* }) => {
        $($a)*
        $($b)*
    };
}
mod two_dollar_payloads {
    ::announce_test_defs::kinds::with_macro! {
        [::announce_test_defs::kinds::with_macro!, ::announce_test_defs::kinds::with_macro2!, reemit_two!]
    }
    pub mod rest {
        ::announce_test_defs::kinds::with_macro! {
            [::announce_test_defs::kinds::with_macro!, capture!]
            macro_rules! in_rest { ($($x:tt)*) => { $($x)* }; }
        }
    }
    pub mod via_summary {
        use announce_test_derive::summary;
        ::announce_test_defs::kinds::with_macro! {
            [::announce_test_defs::kinds::with_macro!, ::announce_test_defs::kinds::with_macro2!, summary!] S
        }
    }
    pub mod via_derive {
        use announce_test_derive::summary;
        ::announce_test_defs::kinds::with_macro2! {
            [::announce_test_defs::kinds::with_macro2!, #[derive(::announce_test_defs::conflict::Parse)] D1, ::announce_test_defs::kinds::with_macro!, summary!] S
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
    ::announce_test_defs::audit_high::r#match! { [::announce_test_defs::audit_high::r#match!, capture!] }
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
    ($( ($($o:tt)*) )? [$($l:tt)*] $($p:ident)? $(:: $s:ident)* ! => [$($m:tt)*] { $($item:tt)* } $($r:tt)*) => {
        stringify!($($item)*)
    };
}
macro_rules! expr_two {
    ($( ($($o:tt)*) )? [$($l:tt)*]
     $($p1:ident)? $(:: $s1:ident)* ! => [$($m1:tt)*] { $($a:tt)* }
     $($p2:ident)? $(:: $s2:ident)* ! => [$($m2:tt)*] { $($b:tt)* }) => {
        [stringify!($($a)*), stringify!($($b)*)]
    };
}
macro_rules! ty_cb {
    ([$($l:tt)*] $($t:tt)*) => {
        u32
    };
}
type FromTypePosition =
    ::announce_test_defs::kinds::Named! { [::announce_test_defs::kinds::Named!, ty_cb!] };

#[test]
fn expression_and_type_positions() {
    // `$`-payload in expression position needs `(expr)`.
    let m: &str = ::announce_test_defs::kinds::with_macro! { (expr) [::announce_test_defs::kinds::with_macro!, expr_cb!] };
    assert!(
        norm(m).contains("macro_rules!sum{($($x:expr),*)=>{0$(+$x)*};}"),
        "{}",
        m
    );
    // Two `$`-payloads in one chain.
    let both = ::announce_test_defs::kinds::with_macro! {
        (expr) [::announce_test_defs::kinds::with_macro!, ::announce_test_defs::kinds::with_macro2!, expr_two!]
    };
    assert!(norm(both[0]).contains("modwith_macro2"));
    assert!(norm(both[1]).contains("modwith_macro{"));
    // A plain payload works in expression position without the option.
    let g: &str =
        ::announce_test_defs::kinds::Greet! { [::announce_test_defs::kinds::Greet!, expr_cb!] };
    assert!(norm(g).starts_with("pubtraitGreet"));
    // With a transformer in the chain.
    let t: &str = ::announce_test_defs::kinds::Sig! { (expr) [::announce_test_defs::kinds::Sig!, ::announce::signature_only!, expr_cb!] };
    assert!(norm(t).contains("{::core::unimplemented!()}"), "{}", t);
    // Type position.
    let x: FromTypePosition = 7;
    assert_eq!(x, 7u32);
}

mod audit3 {
    use announce_test_derive::apply_leak;
    pub mod mg {
        use super::apply_leak;
        ::announce_test_defs::audit3::MethodGenerics! { [::announce_test_defs::audit3::MethodGenerics!, apply_leak!] M }
    }
    pub mod bnd {
        use super::apply_leak;
        ::announce_test_defs::audit3::Bnd! { [::announce_test_defs::audit3::Bnd!, apply_leak!] M }
    }
    pub mod dflt {
        use super::apply_leak;
        ::announce_test_defs::audit3::Dflt! { [::announce_test_defs::audit3::Dflt!, apply_leak!] M }
    }
}

#[test]
fn audit3_repeater_tables() {
    use announce_test_defs::audit3::inner::{Local, LocalG, LocalL};
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
    (($($o:tt)*) [$($l:tt)*] $($r:tt)*) => { stringify!($($o)*) };
}

#[test]
fn options_are_forwarded() {
    let o: &str = ::announce_test_defs::kinds::with_macro! {
        (expr, my_opt(1)) [::announce_test_defs::kinds::with_macro!, ::announce_test_defs::kinds::Greet!, opts_cb!]
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
type DollarInTypePosition = ::announce_test_defs::kinds::with_macro! { [::announce_test_defs::kinds::with_macro!, dollar_ty_cb!] };

#[test]
fn dollar_payloads_in_any_position() {
    let x: DollarInTypePosition = 5;
    assert_eq!(x, 5u64);
    let m: &str = ::announce_test_defs::kinds::with_macro! { [::announce_test_defs::kinds::with_macro!, expr_cb!] };
    assert!(
        norm(m).contains("macro_rules!sum{($($x:expr),*)=>{0$(+$x)*};}"),
        "{}",
        m
    );
    let both = ::announce_test_defs::kinds::with_macro! {
        [::announce_test_defs::kinds::with_macro!, ::announce_test_defs::kinds::with_macro2!, expr_two!]
    };
    assert!(norm(both[1]).contains("modwith_macro{"));
}

mod derive_announce_m {
    ::announce_test_defs::markers::Derived! { [::announce_test_defs::markers::Derived!, capture!] }
    pub mod e {
        ::announce_test_defs::markers::DerivedEnum! { [::announce_test_defs::markers::DerivedEnum!, capture!] }
    }
}

#[test]
fn derive_announce() {
    fn is_announced<T: ?Sized + announce::Announce>() {}
    is_announced::<announce_test_defs::markers::Derived>();
    is_announced::<announce_test_defs::markers::DerivedEnum>();
    let out = norm(derive_announce_m::OUT);
    // The helper attribute, the cfg'd-out field and the docs are gone; META is kept.
    assert!(
        out.contains("=>[$crate::info(1)]{pubstructDerived{puba:u8,}}")
            && out.ends_with("pubstructDerived{puba:u8,}}"),
        "{}",
        out
    );
    assert!(
        !out.contains("Debug") && !out.contains("#[announce(meta"),
        "{}",
        out
    );
    assert!(norm(derive_announce_m::e::OUT).contains("pubenumDerivedEnum"));
}

// `Probe::<T>::IS` is true exactly when `T: announce::Announce`.
mod probe {
    pub trait NotAnnounced {
        const IS: bool = false;
    }
    impl<T: ?Sized> NotAnnounced for T {}
    pub struct Probe<T: ?Sized>(std::marker::PhantomData<T>);
    impl<T: ?Sized + announce::Announce> Probe<T> {
        pub const IS: bool = true;
    }
}
use probe::{NotAnnounced as _, Probe};

/// Hides a constant from clippy (`std::hint::black_box` needs rustc 1.66).
fn opaque(b: bool) -> bool {
    b
}

/// `T: announce::Announce`, as a value clippy does not take for a constant.
macro_rules! announced {
    ($t:ty) => {
        opaque(Probe::<$t>::IS)
    };
}

mod crate_opt {
    use announce_test_defs::crate_opt as d;
    // `crate = crate::crate_opt::spy`: the marker impl names the spy's `Announce`.
    pub fn spy<T: ?Sized + d::spy::Announce>() {}
    pub fn real<T: ?Sized + announce::Announce>() {}
    d::ViaSpy! { [d::ViaSpy!, capture!] }
}

#[test]
fn crate_option_names_the_announce_crate() {
    use announce_test_defs::crate_opt as d;
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
    ::announce_test_defs::sig_full::m! { [::announce_test_defs::sig_full::m!, ::announce::signature_only!, reemit!] }
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
    use announce_test_defs::wrapper as w;
    use announce_test_derive::summary;
    pub mod real {
        use super::*;
        w::Gadget! { [w::Gadget!, summary!] S }
    }
    pub mod wrapped {
        use super::*;
        w::GadgetWrapped! { [w::GadgetWrapped!, summary!] S }
        pub mod raw {
            use announce_test_defs::wrapper as w;
            w::GadgetWrapped! { [w::GadgetWrapped!, capture!] }
        }
    }
    // Everything in the wrapped copy resolves in this crate: unwrap and use it.
    pub mod resolved {
        use announce_test_defs::wrapper as w;
        w::ResolvedWrapped! { [w::ResolvedWrapped!, reemit!] }
    }
}

#[test]
fn module_wrapper_pattern() {
    use announce_test_defs::wrapper as w;
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

// M10: `#[derive(Announce)]` on a union, used across crates.
mod derived_union {
    ::announce_test_defs::derived_union::Raw! { [::announce_test_defs::derived_union::Raw!, reemit!] }
    pub mod meta {
        use announce_test_derive::summary;
        ::announce_test_defs::derived_union::Raw! { [::announce_test_defs::derived_union::Raw!, summary!] S }
    }
}

#[test]
fn derive_announce_on_a_union() {
    assert!(announced!(announce_test_defs::derived_union::Raw));
    assert_eq!(derived_union::meta::S[0].2, "union Raw");
    assert_eq!(norm(derived_union::meta::S[0].1), "$crate::info(union)");
    // The re-emitted union is the one as written.
    let r = derived_union::Raw { f: 1.0 };
    assert_eq!(unsafe { r.i }, 1.0f32.to_bits());
}

// M11: pattern and statement positions.
macro_rules! struct_pat {
    ([$($l:tt)*] $($p:ident)? $(:: $s:ident)* ! => [$($m:tt)*]
     { $(#[$($a:tt)*])* $v:vis struct $name:ident $($body:tt)* } $($r:tt)*) => {
        $name { .. }
    };
}
macro_rules! first_variant_pat {
    ([$($l:tt)*] $($p:ident)? $(:: $s:ident)* ! => [$($m:tt)*]
     { $(#[$($a:tt)*])* $v:vis enum $name:ident { $first:ident $($body:tt)* } } $($r:tt)*) => {
        $name::$first(..)
    };
}
macro_rules! let_cb {
    ([$($l:tt)*] $($p:ident)? $(:: $s:ident)* ! => [$($m:tt)*] { $($item:tt)* } $bound:ident) => {
        let $bound = stringify!($($item)*);
    };
}

#[test]
fn pattern_position() {
    use announce_test_defs::kinds::{Named, Shape};
    let n = Named {
        a: 1,
        only_without_x: 2,
        c: 3,
    };
    assert!(matches!(
        n,
        ::announce_test_defs::kinds::Named! { [::announce_test_defs::kinds::Named!, struct_pat!] }
    ));
    // A `let` is a pattern position too.
    let ::announce_test_defs::kinds::Named! { [::announce_test_defs::kinds::Named!, struct_pat!] } =
        n;
    let classify = |s: &Shape| match s {
        ::announce_test_defs::kinds::Shape! { [::announce_test_defs::kinds::Shape!, first_variant_pat!] } => {
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
    ::announce_test_defs::kinds::Named! { [::announce_test_defs::kinds::Named!, let_cb!] bound }
    assert!(norm(bound).contains("pubstructNamed"), "{}", bound);
    // Items re-emitted into a function body, followed by a use.
    ::announce_test_defs::kinds::Tuple! { [::announce_test_defs::kinds::Tuple!, reemit!] }
    let t = Tuple::<&str>("x", 1);
    assert_eq!((t.0, t.1), ("x", 1));
    // A union through the same derive-free route (`!` only), still in statement position.
    ::announce_test_defs::kinds::Bits! { [::announce_test_defs::kinds::Bits!, reemit!] }
    let b = Bits { i: 0 };
    assert_eq!(unsafe { b.f }, 0.0);
}

// M12: the item-level `#[cfg]` is false in the defining crate: the other half is
// `tests/ui/cfg_gone.rs`. A true cfg leaves the trampoline.
mod cfg_present {
    ::announce_test_defs::gone::Present! { [::announce_test_defs::gone::Present!, capture!] }
}

#[test]
fn item_level_true_cfg_keeps_the_trampoline() {
    let out = norm(cfg_present::OUT);
    assert!(out.contains("pubstructPresent;"), "{}", out);
    assert!(!out.contains("cfg"), "{}", out);
}
