//! UI tests: checks that fail, that do not answer the attribute query, and attributes
//! owned by checks.

use subcrate::subcrate;

#[subcrate(
    ::announce,
    announce_test_derive = derive,
    should_error = "this macro does not support tuple structs",
    should_spanned = "(u8, u8)",
    should_error = "aborting due to 1 previous error\n"
)]
pub mod check_fails {
    #[announce::announce(announce_test_derive::no_tuple(needs named fields))]
    pub struct Pair(u8, u8);
}

// The checks run at the definition when the item has a derive of another trait too: the
// runner makes the trampoline, after the derive.
#[subcrate(
    ::announce,
    announce_test_derive = derive,
    should_error = "this macro does not support tuple structs",
    should_spanned = "(u8, u8)",
    should_error = "aborting due to 1 previous error\n"
)]
pub mod check_fails_with_derive {
    #[announce::announce(announce_test_derive::no_tuple(needs named fields))]
    #[derive(Debug)]
    pub struct Pair(u8, u8);
}

// A proc-macro check written before the attribute query: it parses
// `CheckInput` and does not answer the query, so `#[announce]` cannot go on.
#[subcrate(
    ::announce,
    announce_test_derive = derive,
    should_error = "this check received the attribute query `@__announce_attrs` that `#[announce]` sends first, and does not answer it",
    should_spanned = "#[announce::announce(announce_test_derive::old_style_check)]",
    should_error = "aborting due to 1 previous error\n"
)]
pub mod check_no_answer {
    #[announce::announce(announce_test_derive::old_style_check)]
    pub struct S {
        pub a: u8,
    }
}

// A `macro_rules!` check without the `@__announce_attrs` arm: rustc reports the
// call, and `S` is not emitted until the check answers.
#[subcrate(
    ::announce,
    should_error = "no rules expected `@`",
    should_spanned = "#[announce::announce(check)]",
    should_spanned = "macro_rules! check",
    should_spanned = "(",
    should_error = "aborting due to 1 previous error\n"
)]
pub mod check_rules_no_answer {
    macro_rules! check {
        (@announce_format "1" ($($args:tt)*), $($item:tt)*) => {};
    }

    #[announce::announce(check)]
    pub struct S {
        pub a: u8,
    }
}

#[subcrate(
    ::announce,
    announce_test_derive = derive,
    should_error = "`Plain` is not announced",
    should_spanned = "Plain",
    should_spanned = "pub struct Plain",
    should_spanned = "announce_test_derive::check_marker!(Plain)",
    should_error = "aborting due to 1 previous error\n"
)]
pub mod not_announced {
    pub struct Plain;
    announce_test_derive::check_marker!(Plain);
}

// A check that owns `my_lib` sees the attributes and can reject them where the
// item is defined, also deep inside the item.
#[subcrate(
    ::announce,
    announce_test_derive = derive,
    should_error = "`#[my_lib(bad)]` is not allowed",
    should_error = "`#[other(forbidden)]` is not allowed",
    should_spanned = "#[my_lib(bad)]",
    should_spanned = "#[other(forbidden)]",
    should_error = "aborting due to 2 previous errors\n"
)]
pub mod owned_bad {
    #[announce::announce(announce_test_derive::lib_check)]
    pub struct S {
        #[my_lib(fine)]
        pub a: u8,
        #[my_lib(bad)]
        pub b: u8,
    }

    // A second check, which owns other names; the first one's attributes pass.
    #[announce::announce(announce_test_derive::lib_check, announce_test_derive::other_check)]
    pub enum E {
        A(#[my_lib(fine)] u8),
        B {
            #[other(forbidden)]
            x: u8,
        },
    }
}

#[subcrate(
    ::announce,
    announce_test_derive = derive,
    should_error = "`Local` cannot be named outside its module",
    should_spanned = "Local",
    should_error = "aborting due to 1 previous error\n"
)]
pub mod owned_error_keeps_item {
    mod m {
        pub struct Local;
    }
    use m::Local;

    // The type-context error is the only error: the item (without its owned
    // attributes) is still emitted, so the use of `S` below is fine.
    #[announce::announce(announce_test_derive::lib_check)]
    pub struct S {
        #[my_lib(skip)]
        pub a: Local,
    }

    pub fn main() {
        let _ = S { a: Local };
    }
}

// An attribute that no check owns is rustc's: it is not declared by any macro,
// so it is rejected, with or without checks.
#[subcrate(
    ::announce,
    announce_test_derive = derive,
    should_error = "cannot find attribute `other` in this scope",
    should_error = "cannot find attribute `my_lib` in this scope",
    should_spanned = "other",
    should_spanned = "my_lib",
    should_error = "aborting due to 3 previous errors\n"
)]
pub mod owned_not_owned {
    #[announce::announce]
    pub struct A {
        #[my_lib(x)]
        pub a: u8,
    }

    #[announce::announce(announce_test_derive::no_tuple)]
    pub struct B {
        #[my_lib(x)]
        pub b: u8,
    }

    // The item's own attribute that is not owned either.
    #[announce::announce(announce_test_derive::lib_check)]
    pub struct C {
        #[other(x)]
        pub c: u8,
    }
}

// An owned attribute name must not be an attribute that rustc or a tool uses.
// A proc-macro check (`announce::helper::check`) reports it where the check is used.
#[subcrate(
    ::announce,
    announce_test_derive = derive,
    should_error = "`doc` cannot be an owned attribute name",
    should_spanned = "#[announce::announce(announce_test_derive::doc_owner)]",
    should_error = "`cfg` cannot be an owned attribute name",
    should_spanned = "cfg",
    should_spanned = "#[announce::announce(cfg_owner)]",
    should_error = "`rustc_thing` cannot be an owned attribute name",
    should_spanned = "rustc_thing",
    should_spanned = "#[announce::announce(announce_owner)]",
    should_error = "aborting due to 3 previous errors\n"
)]
pub mod owned_reserved {
    #[announce::announce(announce_test_derive::doc_owner)]
    pub struct A {
        pub a: u8,
    }

    // A `macro_rules!` check reports it at `finish!`, at the name it wrote.
    macro_rules! cfg_owner {
        (@announce_format "1" @__announce_attrs [ $($hp:ident)? $(:: $hs:ident)* ! , $($np:ident)? $(:: $ns:ident)* ! $(, $($rest:tt)*)? ] { $($state:tt)* }) => {
            $($np)? $(:: $ns)* ! { @announce_format "1" @__announce_attrs [ $($np)? $(:: $ns)* ! $(, $($rest)*)? ] { $($state)* attrs(cfg) } }
        };
        (@announce_format "1" ($($args:tt)*), $($item:tt)*) => {};
    }

    #[announce::announce(cfg_owner)]
    pub struct B {
        pub b: u8,
    }

    macro_rules! announce_owner {
        (@announce_format "1" @__announce_attrs [ $($hp:ident)? $(:: $hs:ident)* ! , $($np:ident)? $(:: $ns:ident)* ! $(, $($rest:tt)*)? ] { $($state:tt)* }) => {
            $($np)? $(:: $ns)* ! { @announce_format "1" @__announce_attrs [ $($np)? $(:: $ns)* ! $(, $($rest)*)? ] { $($state)* attrs(rustc_thing) } }
        };
        (@announce_format "1" ($($args:tt)*), $($item:tt)*) => {};
    }

    #[announce::announce(announce_owner)]
    pub struct C {
        pub c: u8,
    }
}

// A check that answers wrongly: it adds something that is not `attrs(..)` to
// the state.
#[subcrate(
    ::announce,
    announce_test_derive = derive,
    should_error = "the state of the attribute query was changed by a check",
    should_spanned = "oops",
    should_spanned = "#[announce::announce(bad_state)]",
    should_error = "the list of the attribute query is empty or does not start with a macro",
    should_spanned = "$np)? $(:: $ns)* ! { @announce_format \"1\" @__announce_attrs [] { $($state)* } }",
    should_spanned = "#[announce::announce(drops_list, announce_test_derive::no_tuple)]",
    should_error = "the attribute query reached `finish!` with an empty list",
    should_spanned = ":: $ns)* ! { @announce_format \"1\" @__announce_attrs [] { $($state)* } }",
    should_spanned = "#[announce::announce(drops_list)]",
    should_error = "the attribute query reached `finish!` with elements left in its list",
    should_spanned = ":: $ns)* ! { @announce_format \"1\" @__announce_attrs [ $($hp)? $(:: $hs)* ! , $($np)? $(:: $ns)* ! $(, $($rest)*)? ] { $($state)* } }",
    should_spanned = "#[announce::announce(keeps_head)]",
    should_error = "head_probe: the list does not start with `head_probe!`",
    should_spanned = "$np)? $(:: $ns)* ! { @announce_format \"1\" @__announce_attrs [ $($hp)? $(:: $hs)* ! , $($np)? $(:: $ns)* ! $(, $($rest)*)? ] { $($state)* } }",
    should_spanned = "#[announce::announce(keeps_head, announce_test_derive::head_probe)]",
    should_error = "aborting due to 5 previous errors\n"
)]
pub mod owned_wrong_state {
    macro_rules! bad_state {
        (@announce_format "1" @__announce_attrs [ $($hp:ident)? $(:: $hs:ident)* ! , $($np:ident)? $(:: $ns:ident)* ! $(, $($rest:tt)*)? ] { $($state:tt)* }) => {
            $($np)? $(:: $ns)* ! { @announce_format "1" @__announce_attrs [ $($np)? $(:: $ns)* ! $(, $($rest)*)? ] { $($state)* oops(my_lib) } }
        };
        (@announce_format "1" ($($args:tt)*), $($item:tt)*) => {};
    }

    #[announce::announce(bad_state)]
    pub struct S {
        pub a: u8,
    }

    // A check that drops the rest of the list: `finish!` is never reached, and the
    // next check says so.
    macro_rules! drops_list {
        (@announce_format "1" @__announce_attrs [ $($hp:ident)? $(:: $hs:ident)* ! , $($np:ident)? $(:: $ns:ident)* ! $(, $($rest:tt)*)? ] { $($state:tt)* }) => {
            $($np)? $(:: $ns)* ! { @announce_format "1" @__announce_attrs [] { $($state)* } }
        };
        (@announce_format "1" ($($args:tt)*), $($item:tt)*) => {};
    }

    #[announce::announce(drops_list, announce_test_derive::no_tuple)]
    pub struct T {
        pub a: u8,
    }

    // The same, as the last check: `finish!` receives an empty list.
    #[announce::announce(drops_list)]
    pub struct U {
        pub a: u8,
    }

    // A check that forwards its list without dropping its own element (the head): the
    // list that the next macro receives is not headed by that macro. `finish!` wants
    // exactly `[ finish! ]` ...
    macro_rules! keeps_head {
        (@announce_format "1" @__announce_attrs [ $($hp:ident)? $(:: $hs:ident)* ! , $($np:ident)? $(:: $ns:ident)* ! $(, $($rest:tt)*)? ] { $($state:tt)* }) => {
            $($np)? $(:: $ns)* ! { @announce_format "1" @__announce_attrs [ $($hp)? $(:: $hs)* ! , $($np)? $(:: $ns)* ! $(, $($rest)*)? ] { $($state)* } }
        };
        (@announce_format "1" ($($args:tt)*), $($item:tt)*) => {};
    }

    #[announce::announce(keeps_head)]
    pub struct V {
        pub a: u8,
    }

    // ... and a check that looks at its head finds the wrong macro there.
    #[announce::announce(keeps_head, announce_test_derive::head_probe)]
    pub struct W {
        pub a: u8,
    }
}
