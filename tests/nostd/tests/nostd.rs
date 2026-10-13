//! The trampolines of the `#![no_std]` definer crate, called from a crate with std.
//! `announce_test_nostd` is built here for the host and without the feature
//! `proc-macro-api` of `announce`; the callbacks are plain `macro_rules!`.

macro_rules! capture {
    ([$($l:tt)*] $($t:tt)*) => {
        pub const OUT: &str = stringify!([$($l)*] $($t)*);
    };
}

fn norm(s: &str) -> String {
    s.split_whitespace().collect()
}

mod named {
    announce_test_nostd::Named! { [announce_test_nostd::Named!, capture!] rest }
}

#[test]
fn a_struct_is_passed_to_the_callback() {
    let out = norm(named::OUT);
    assert!(
        out.starts_with("[capture!]announce_test_nostd::Named!=>[]{pubstructNamed{"),
        "{}",
        out
    );
    assert!(out.ends_with("rest"), "{}", out);
}

macro_rules! kind {
    ([$($l:tt)*] $($p:ident)::+ ! => [$($m:tt)*] { $($item:tt)* } $($rest:tt)*) => {
        pub const KIND: &str = stringify!($($item)*);
    };
}

macro_rules! kinds {
    ($($name:ident => $needle:expr),* $(,)?) => {
        $(
            #[allow(non_snake_case)]
            mod $name {
                announce_test_nostd::$name! { [announce_test_nostd::$name!, kind!] }
                pub fn check() {
                    let item: String = super::norm(KIND);
                    assert!(item.contains($needle), "{}: {}", stringify!($name), item);
                }
            }
        )*
        #[test]
        fn every_kind_is_passed() {
            $($name::check();)*
        }
    };
}

kinds! {
    Tuple => "structTuple<T>(pubT,pubu8)",
    Shape => "enumShape",
    Bits => "unionBits",
    Greet => "traitGreet",
    Alias => "typeAlias=[u8;4]",
    inner => "modinner",
    Derived => "structDerived",
    WithMeta => "structWithMeta",
}

mod renamed {
    announce_test_nostd::RenamedTrampoline! { [announce_test_nostd::RenamedTrampoline!, capture!] }
}

#[test]
fn macro_name_renames_the_trampoline() {
    assert!(
        norm(renamed::OUT).contains("structRenamed"),
        "{}",
        renamed::OUT
    );
}

mod meta {
    announce_test_nostd::WithMeta! { [announce_test_nostd::WithMeta!, capture!] }
}

#[test]
fn meta_is_carried() {
    assert!(
        norm(meta::OUT).contains("[nostd_info(version=1)]"),
        "{}",
        meta::OUT
    );
}

mod leaky {
    announce_test_nostd::leaky::Leaky! { [announce_test_nostd::leaky::Leaky!, capture!] }
}

#[test]
fn the_repeater_option_works() {
    let out = norm(leaky::OUT);
    assert!(
        out.contains("type_leak(repeater=$crate::leaky::Repeater"),
        "{}",
        out
    );
}

#[test]
fn announce_impls_are_there() {
    announce_test_nostd::is_announced::<announce_test_nostd::Named>();
    announce_test_nostd::is_announced::<announce_test_nostd::Shape>();
    announce_test_nostd::is_announced::<announce_test_nostd::Derived>();
    announce_test_nostd::is_announced::<dyn announce_test_nostd::Greet>();
    assert_eq!(announce_test_nostd::inner::answer(), 42);
}
