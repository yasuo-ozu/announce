//! Items with attributes of their own (`#[my_lib(..)]`, `#[other(..)]`, ..),
//! which the checks of `#[announce]` own: they are removed from the emitted item
//! (by the runner, or by the announcement when nothing can expand after it) and kept in
//! the payload.

use announce::announce;

/// A check written with `macro_rules!`: it forwards the attribute query and adds
/// `rules` (the one arm every check needs), and does nothing else.
#[macro_export]
macro_rules! rules_check {
    (@__announce_attrs [ $($hp:ident)? $(:: $hs:ident)* ! , $($np:ident)? $(:: $ns:ident)* ! $(, $($rest:tt)*)? ] { $($state:tt)* }) => {
        $($np)? $(:: $ns)* ! { @__announce_attrs [ $($np)? $(:: $ns)* ! $(, $($rest)*)? ] { $($state)* attrs(rules) } }
    };
    (($($args:tt)*), $($item:tt)*) => {};
}

/// A `macro_rules!` check that owns nothing: it forwards the state unchanged.
#[macro_export]
macro_rules! plain_rules_check {
    (@__announce_attrs [ $($hp:ident)? $(:: $hs:ident)* ! , $($np:ident)? $(:: $ns:ident)* ! $(, $($rest:tt)*)? ] { $($state:tt)* }) => {
        $($np)? $(:: $ns)* ! { @__announce_attrs [ $($np)? $(:: $ns)* ! $(, $($rest)*)? ] { $($state)* } }
    };
    (($($args:tt)*), $($item:tt)*) => {};
}

/// A `macro_rules!` check that asserts, with its pattern, that the list of the
/// attribute query is headed by the check itself: `crate::head_check!`, as written
/// in `#[announce(..)]`. A list that starts with anything else matches no arm.
#[macro_export]
// `crate::head_check` in the pattern is the path the item below writes, on purpose.
#[allow(clippy::crate_in_macro_def)]
macro_rules! head_check {
    (@__announce_attrs [ crate::head_check ! , $($np:ident)? $(:: $ns:ident)* ! $(, $($rest:tt)*)? ] { $($state:tt)* }) => {
        $($np)? $(:: $ns)* ! { @__announce_attrs [ $($np)? $(:: $ns)* ! $(, $($rest)*)? ] { $($state)* attrs(head_seen) } }
    };
    (($($args:tt)*), $($item:tt)*) => {};
}

/// Attributes on every kind of position of a struct. The feature `x` is off here.
#[announce(announce_test_derive::record_attrs)]
#[derive(Clone, Debug, PartialEq)]
pub struct Fields {
    #[my_lib(skip)]
    pub a: u8,
    pub b: u8,
    #[my_lib::tag = "x"]
    #[doc = "a documented field"]
    pub c: u8,
    // `cfg_attr` is evaluated first: only the one that holds is seen.
    #[cfg_attr(not(feature = "x"), my_lib(from_cfg_attr))]
    pub d: u8,
    #[cfg_attr(feature = "x", my_lib(from_cfg_attr_x))]
    pub e: u8,
    #[cfg(feature = "x")]
    #[my_lib(only_with_x)]
    pub f: u8,
}

/// On the item, its variants and their fields.
#[announce(announce_test_derive::record_attrs)]
#[my_lib(on_enum)]
#[derive(Debug, PartialEq)]
pub enum Choice {
    #[my_lib(first)]
    A,
    #[my_lib(skip)]
    B(#[my_lib(inner)] u8),
    C {
        #[my_lib(named)]
        f: u8,
    },
}

/// On a trait, its generics, its items, a parameter and a statement.
#[announce(announce_test_derive::record_attrs)]
#[my_lib(on_trait)]
pub trait Tr<#[my_lib(generic)] T> {
    #[my_lib(assoc_type)]
    type A;
    #[my_lib(assoc_const)]
    const C: u8;
    #[my_lib(method)]
    #[allow(clippy::let_and_return)]
    fn f(&self, #[my_lib(param)] _x: T) -> u8 {
        #[my_lib(stmt)]
        let y = 1;
        y
    }
}

/// On the inside of an inline `mod`, also as an inner attribute.
#[announce(announce_test_derive::record_attrs)]
pub mod inline {
    #![my_lib(inner_attr)]
    #[my_lib(in_mod)]
    pub struct Z {
        #[my_lib(field)]
        pub z: u8,
    }
    pub fn three() -> u8 {
        3
    }
}

/// Three checks that own different attributes: a proc-macro check, a second one
/// and a `macro_rules!` check.
#[announce(
    announce_test_derive::lib_check,
    announce_test_derive::other_check,
    crate::rules_check
)]
pub struct Two {
    #[my_lib(a)]
    pub a: u8,
    #[other(b)]
    pub b: u8,
    #[tag]
    pub t: u8,
    #[rules(c)]
    pub c: u8,
}

/// A check that owns nothing, among checks that do: the answers are merged.
#[announce(
    crate::plain_rules_check,
    announce_test_derive::no_tuple,
    announce_test_derive::lib_check
)]
pub struct Mixed {
    #[my_lib(m)]
    pub m: u8,
}

/// The same check twice: the names are listed once.
#[announce(announce_test_derive::lib_check, announce_test_derive::lib_check)]
pub struct Twice {
    #[my_lib(t)]
    pub t: u8,
}

/// Every check finds itself at the head of the list: a proc-macro check that reads
/// the query by hand (twice, once with an absolute path), a `macro_rules!` check
/// that matches its own path in the pattern, and one that skips its head.
#[announce(
    announce_test_derive::head_probe,
    crate::head_check,
    ::announce_test_derive::head_probe,
    crate::rules_check
)]
pub struct Heads {
    #[probed(a)]
    pub a: u8,
    #[head_seen(b)]
    pub b: u8,
    #[rules(c)]
    pub c: u8,
}

/// Checks that own nothing at all.
#[announce(announce_test_derive::no_tuple, crate::plain_rules_check)]
pub struct Nothing {
    pub n: u8,
}

/// Both layouts, with the item held once or copied.
#[announce(layout = single, announce_test_derive::record_attrs)]
pub struct SingleOwned {
    #[my_lib(s)]
    pub s: u8,
}

#[announce(layout = direct, announce_test_derive::record_attrs)]
pub struct DirectOwned {
    #[my_lib(d)]
    pub d: u8,
}

macro_rules! big_owned {
    ($($f:ident),*) => {
        /// More than 300 tokens, so `layout = auto` holds it once.
        #[announce(announce_test_derive::record_attrs)]
        pub struct BigOwned {
            $(
                #[my_lib(big)]
                pub $f: u8,
            )*
        }
    };
}
big_owned!(
    f00, f01, f02, f03, f04, f05, f06, f07, f08, f09, f10, f11, f12, f13, f14, f15, f16, f17, f18,
    f19, f20, f21, f22, f23, f24, f25, f26, f27, f28, f29, f30, f31, f32, f33, f34, f35, f36, f37,
    f38, f39
);

/// `$` in the item, with attributes on and around the macro.
#[announce(announce_test_derive::record_attrs)]
pub mod with_dollar {
    #[my_lib(on_macro)]
    macro_rules! k {
        ($($x:expr),*) => { 0 $(+ $x)* };
    }
    #[my_lib(on_fn)]
    pub fn six() -> i32 {
        k!(1, 2, 3)
    }
}

/// A mock: its hidden copy is compiled too, and must not have the attributes.
pub mod mocks {
    use super::announce;

    pub trait Real {
        fn f(&self) -> u8;
    }

    #[announce(mock = crate::owned::mocks::Real, announce_test_derive::record_attrs)]
    pub trait Mocked {
        #[my_lib(on_method)]
        fn f(&self) -> u8;
    }
}

/// The item is not emitted (`emit_item = false`): the attributes are only in the payload.
#[announce(emit_item = false, announce_test_derive::record_attrs)]
pub mod NotEmitted {
    #[my_lib(never_compiled)]
    pub struct NotEmitted {
        #[my_lib(field)]
        pub a: u8,
    }
}

/// An item that is `cfg`'d out is gone with its attributes.
#[cfg(feature = "x")]
#[announce(announce_test_derive::record_attrs)]
pub struct OnlyWithX {
    #[my_lib(x)]
    pub a: u8,
}

/// A name that announce renames, with an owned attribute.
#[announce(macro_name = RenamedOwned, announce_test_derive::record_attrs)]
pub struct Renamed {
    #[my_lib(r)]
    pub r: u8,
}

/// `$` in the item, through a `macro_rules!` check.
#[announce(crate::rules_check)]
pub mod with_dollar_rules {
    #[rules(on_macro)]
    macro_rules! k {
        ($($x:expr),*) => { 1 $(* $x)* };
    }
    pub fn twenty_four() -> i32 {
        k!(2, 3, 4)
    }
}

/// The derive form with a check that owns `rules`: a derive removes nothing, so
/// `Rules` declares the attribute, and the callbacks see `attrs(rules)`.
#[derive(::announce::Announce, announce_test_derive::Rules, Debug)]
#[announce(crate::rules_check)]
pub struct DerivedOwned {
    #[rules(derived)]
    pub a: u8,
}

#[doc(hidden)]
pub use announce as __announce;

/// A macro that defines announced items in the caller's crate, as a library would:
/// `crate = $crate::__announce` and a check path with `$crate`.
#[macro_export]
macro_rules! make_owned {
    ($name:ident) => {
        #[$crate::owned::__announce::announce(crate = $crate::owned::__announce, $crate::plain_rules_check, $crate::rules_check, announce_test_derive::no_tuple)]
        pub struct $name {
            #[rules(made)]
            pub x: u8,
        }
    };
}
