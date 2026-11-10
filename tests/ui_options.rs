//! UI tests: the arguments of `#[announce]` and `define_repeater!`, the items it takes, and
//! errors of `announce::machinery`.

use subcrate::subcrate;

#[subcrate(
    ::announce,
    should_error = "`u8` cannot be named outside its module",
    should_spanned = "u8",
    should_error = "expected boolean literal",
    should_spanned = "yes",
    should_error = "aborting due to 2 previous errors\n"
)]
pub mod allow_options {
    // `allow_primitive = false`: primitives need a repeater (or cannot be used).
    #[announce::announce(allow_primitive = false)]
    pub struct NoPrim {
        pub a: u8,
    }

    // `allow_std_prelude = true` accepts prelude names, also with generic
    // arguments (`Vec<u8>`): no error here.
    #[announce::announce(allow_std_prelude = true)]
    pub struct PreludeGeneric {
        pub v: Vec<u8>,
    }

    // The values are booleans.
    #[announce::announce(allow_std_prelude = yes)]
    pub struct BadValue;
}

#[subcrate(
    ::announce,
    should_error = "#[announce] supports struct, enum, union, trait, type alias and inline mod items",
    should_spanned = "fn f() {}",
    should_error = "unknown announce option",
    should_spanned = "macro_nam",
    should_error = "#[announce] supports only inline modules",
    should_spanned = "mod m;",
    should_error = "aborting due to 3 previous errors\n"
)]
pub mod bad_items {
    #[announce::announce]
    fn f() {}

    #[announce::announce(macro_nam = X)]
    struct S;

    // Through a macro, so that rustfmt does not look for the file of `m`.
    macro_rules! items {
        ($($t:tt)*) => { $($t)* };
    }

    items! {
        #[announce::announce]
        mod m;
    }
}

// `#[announce]` reads only the kind and the name of the item;
// its syntax is checked once, by the hidden derive. The errors are the same as when
// the attribute parsed the item itself: rustc's own parse error of the source, and
// `syn`'s at the same token, one for each item.
#[subcrate(
    ::announce,
    should_error = "expected type, found `,`",
    should_spanned = ",",
    should_error = "expected type, found `;`",
    should_spanned = ";",
    should_error = "expected one of: `for`, parentheses",
    should_error = "aborting due to 4 previous errors\n"
)]
pub mod item_syntax {
    // The test file must parse: the items reach rustc's parser through a macro.
    macro_rules! items {
        ($($t:tt)*) => { $($t)* };
    }

    items! {
        #[announce::announce]
        pub struct S {
            pub a: ,
        }
    }

    items! {
        #[announce::announce(crate = ::announce)]
        pub trait T {
            fn f(&self) -> ;
        }
    }
}

#[subcrate(
    ::announce,
    should_error = "`export_name` was removed",
    should_spanned = "export_name",
    should_error = "`marker` was removed",
    should_spanned = "marker",
    should_error = "aborting due to 2 previous errors\n"
)]
pub mod removed_options {
    #[announce::announce(export_name = Hidden)]
    pub struct A;

    #[announce::announce(marker = false)]
    pub struct B;
}

// Errors of `announce::machinery` come back as `compile_error!` tokens.
#[subcrate(
    announce_test_machinery_lib = machinery_lib,
    should_error = "unknown announce option",
    should_spanned = "macro_nam",
    should_error = "expected `struct`",
    should_spanned = "fn",
    should_error = "aborting due to 2 previous errors\n"
)]
pub mod machinery_errors {
    use announce_test_machinery_lib::tracked;

    #[tracked(macro_nam = X)]
    pub struct A {
        pub a: u8,
    }

    #[tracked]
    fn not_an_item() {}
}

// `define_repeater!` takes a visibility and a name.
#[subcrate(
    ::announce,
    should_error = "expected `define_repeater!(<visibility> <Name>)`",
    should_spanned = "::announce::define_repeater!()",
    should_spanned = "::announce::define_repeater!(pub)",
    should_spanned = "extra",
    should_spanned = "1",
    should_error = "aborting due to 4 previous errors\n"
)]
pub mod define_repeater_misuse {
    ::announce::define_repeater!();
    ::announce::define_repeater!(pub);
    ::announce::define_repeater!(pub Repeater extra);
    ::announce::define_repeater!(pub 1);
}

#[subcrate(
    ::announce,
    should_error = "`repeater` is not supported on type aliases",
    should_error = "`repeater` is supported on struct, enum, union and trait items",
    should_spanned = "crate::Repeater",
    should_error = "aborting due to 2 previous errors\n"
)]
pub mod repeater_unsupported {
    ::announce::define_repeater!(pub Repeater);

    // `repeater` works on a struct, enum, union and trait only.
    #[announce::announce(repeater = crate::Repeater)]
    pub type Alias = ::core::option::Option<u8>;

    #[announce::announce(repeater = crate::Repeater)]
    pub mod m {
        pub struct S;
    }
}

#[subcrate(
    ::announce,
    should_error = "`repeater` must be a trait of this crate",
    should_spanned = "Repeater",
    should_error = "aborting due to 1 previous error\n"
)]
pub mod type_leak_misuse {
    ::announce::define_repeater!(pub Repeater);

    // Relative repeater paths would resolve at the caller.
    #[announce::announce(repeater = Repeater)]
    pub struct S;
}

// The sentinel of an announced struct, enum or union (`#[derive]` makes the runner, so the
// announcement emits one) is not a use of the type: a private item that nothing uses is
// reported as unused, as it would be without `#[announce]`. A sentinel that names the type in
// the body of a function would make the lint blame the field (`field `0` is never read`)
// instead of the struct.
//
// A UI subcrate is compiled with `--cap-lints warn`, so the denied lint is a warning here.
#[subcrate(
    ::announce,
    should_warn = "struct `S` is never constructed",
    should_spanned = "S",
    should_warn = "enum `E` is never used",
    should_spanned = "E",
    should_warn = "the lint level is defined here",
    should_spanned = "dead_code",
    should_warn = "2 warnings emitted\n"
)]
pub mod unused_announced_struct {
    #![deny(dead_code)]

    #[::announce::announce]
    #[derive(Clone)]
    struct S(u8);

    #[::announce::announce]
    #[derive(Clone)]
    enum E {
        A,
    }
}

// An item-level `#[cfg]` that is false in the defining crate removes the item, and
// with it the trampoline: there is no `Gone!` to call (`gone` in `defs`).
// The names come in through globs, so that the error is the plain "cannot find
// macro": a path call would add rustc's notes about configured-out items, which word
// differently from one rustc to the next.
#[subcrate(
    ::announce,
    announce_test_defs = defs,
    should_error = "cannot find macro `AnnounceFirst` in this scope",
    should_spanned = "AnnounceFirst",
    should_error = "cannot find macro `CfgFirst` in this scope",
    should_spanned = "CfgFirst",
    should_error = "cannot find macro `GoneWithoutX` in this scope",
    should_spanned = "GoneWithoutX",
    should_error = "aborting due to 3 previous errors\n"
)]
pub mod cfg_gone {
    #[allow(unused_imports)]
    use announce_test_defs::gone::*;
    #[allow(unused_imports)]
    use announce_test_defs::kinds::*;

    macro_rules! cb {
        ($($t:tt)*) => {};
    }

    // `#[announce]` first, then a false `#[cfg]`.
    AnnounceFirst! { [announce_test_defs::gone::AnnounceFirst!, cb!] }

    // A false `#[cfg]` before `#[announce]`: the attribute macro never runs.
    CfgFirst! { [announce_test_defs::gone::CfgFirst!, cb!] }

    // The `cfg(test)` of `defs` is off there (a subcrate is not built with `cfg(test)`),
    // whatever this crate has.
    GoneWithoutX! { [announce_test_defs::kinds::GoneWithoutX!, cb!] }

    // A true `#[cfg]` leaves the trampoline: the call below resolves (and is the only
    // one of the four that does; trampoline.rs expands it).
    Present! { [announce_test_defs::gone::Present!, cb!] }
}
