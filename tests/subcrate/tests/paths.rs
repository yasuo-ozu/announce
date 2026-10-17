//! `crate::` and `$crate` paths in a payload resolve from another crate.
//!
//! The payload of a trampoline is the definition as it was written in `defs`, and a
//! callback may emit it again where `crate::` would mean the calling crate. `#[announce]`
//! rewrites the paths that name the defining crate (`crate::..` becomes `$crate::..` in the
//! trampoline), so the emitted items refer to `defs`. This crate has none of the names
//! (`Alias`, `ARR`, `shapes`), so a path that was not rewritten would not resolve.

use subcrate::subcrate;

#[subcrate(::announce)]
pub mod defs {
    use announce::announce;

    pub type Alias = ::std::vec::Vec<u8>;

    /// A constant of the defining crate, named by `crate::ARR` inside a payload.
    pub const ARR: [u8; 3] = [1, 2, 3];

    pub mod shapes {
        pub struct Foo(pub u8);
    }

    #[announce]
    pub mod uses_crate_paths {
        pub fn make() -> crate::Alias {
            ::std::vec![1, 2, 3]
        }
        pub(crate) fn _vis_untouched() {}
    }

    /// The `crate` after the `in` of a `for` loop is an ordinary path; only the `in` of
    /// `pub(in ..)` is part of a visibility.
    #[announce]
    pub mod for_in_crate {
        pub fn sum() -> u32 {
            let mut s = 0;
            for x in crate::ARR {
                s += x as u32;
            }
            s
        }
        pub(crate) fn _vis() {}
    }

    /// `crate::` in a field type, after a single `:`.
    #[announce]
    pub struct Field {
        pub a: crate::shapes::Foo,
    }

    /// `crate::` in the signature of a trait.
    #[announce]
    pub trait Makes {
        fn make(&self) -> crate::shapes::Foo;
    }

    /// A macro in the payload that uses `$crate`.
    #[announce]
    pub mod with_dollar_crate {
        #[macro_export]
        macro_rules! mk_foo {
            () => {
                $crate::shapes::Foo(9)
            };
        }
        pub fn make() -> crate::shapes::Foo {
            crate::shapes::Foo(9)
        }
    }

    #[announce]
    pub trait Greet {
        fn hi(&self) -> &'static str;
    }

    #[macro_export]
    #[doc(hidden)]
    macro_rules! greet_cb {
        ($(@announce_format "1")? [$($l:tt)*] $($t:tt)*) => {
            pub const VIA_DOLLAR_CRATE: &str = stringify!($($t)*);
        };
    }

    /// A `$crate` path in a continuation list, as a library's `macro_rules!` writes it: it
    /// starts the chain, and it is used in another crate.
    #[macro_export]
    macro_rules! greet_chain {
        () => {
            $crate::Greet! { [$crate::Greet!, $crate::greet_cb!] }
        };
    }
}

macro_rules! reemit {
    ($(@announce_format "1")? [$($l:tt)*] $($p:ident)? $(:: $s:ident)* ! => [$($m:tt)*] { $($item:tt)* }) => {
        $($item)*
    };
}

macro_rules! capture {
    ($(@announce_format "1")? [$($l:tt)*] $($t:tt)*) => {
        pub const OUT: &str = stringify!($($t)*);
    };
}

fn norm(s: &str) -> String {
    s.split_whitespace().collect()
}

// This crate has none of the names the payloads use.
#[allow(dead_code)]
type Alias = bool;

mod reemitted_alias {
    ::defs::uses_crate_paths! { [::defs::uses_crate_paths!, reemit!] }
}

#[test]
fn crate_paths_refer_to_the_defining_crate() {
    // `crate::Alias` is the defining crate's `Vec<u8>`, not this crate's `Alias`.
    let v: defs::Alias = reemitted_alias::uses_crate_paths::make();
    assert_eq!(v, [1, 2, 3]);
}

mod reemitted_for_in {
    // `crate::ARR` is the defining crate's: this crate has no `ARR`.
    ::defs::for_in_crate! { [::defs::for_in_crate!, reemit!] }
}

#[test]
fn crate_after_in_of_a_for_loop_refers_to_the_defining_crate() {
    assert_eq!(reemitted_for_in::for_in_crate::sum(), 6);
}

mod reemitted_types {
    // `crate::shapes::Foo` is the defining crate's: this crate has no `shapes`.
    ::defs::Field! { [::defs::Field!, reemit!] }
    ::defs::Makes! { [::defs::Makes!, reemit!] }
}

#[test]
fn crate_in_a_field_and_in_a_signature_refers_to_the_defining_crate() {
    let f = reemitted_types::Field {
        a: defs::shapes::Foo(1),
    };
    assert_eq!(f.a.0, 1);
    struct M;
    impl reemitted_types::Makes for M {
        fn make(&self) -> defs::shapes::Foo {
            defs::shapes::Foo(2)
        }
    }
    use reemitted_types::Makes;
    assert_eq!(M.make().0, 2);
}

mod dollar_crate_in_payload {
    ::defs::with_dollar_crate! { [::defs::with_dollar_crate!, capture!] }
}

#[test]
fn dollar_crate_in_a_payload_macro_is_kept() {
    let out = norm(dollar_crate_in_payload::OUT);
    assert!(out.contains("$crate::shapes::Foo(9)"), "{}", out);
    assert!(!out.contains("$$crate"), "{}", out);
    // The macro is the one of `defs`, and so is the `$crate` in it.
    assert_eq!(defs::mk_foo!().0, 9);
}

mod reemitted_dollar_crate {
    ::defs::with_dollar_crate! { [::defs::with_dollar_crate!, reemit!] }
}

#[test]
fn a_payload_macro_with_dollar_crate_can_be_emitted_again() {
    assert_eq!(reemitted_dollar_crate::with_dollar_crate::make().0, 9);
}

mod dollar_crate_in_list {
    defs::greet_chain!();
}

#[test]
fn dollar_crate_in_the_list_of_a_macro_of_the_defining_crate() {
    assert!(norm(dollar_crate_in_list::VIA_DOLLAR_CRATE).contains("pubtraitGreet"));
}
