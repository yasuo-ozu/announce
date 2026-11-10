//! UI tests: the runner, and the announcements of one item that are merged into it.

use subcrate::subcrate;

// A macro below `#[announce]` that returns the item without its outer attributes drops the
// runner, so nothing announces the item. The sentinel that the announcement emits next to
// the item fails at the definition: `Announce` is not implemented for a struct, enum or
// union without generics.
#[subcrate(
    ::announce,
    announce_test_derive = derive,
    should_error = "`S` is not announced",
    should_spanned = "S",
    should_spanned = "pub struct S",
    should_error = "`E` is not announced",
    should_spanned = "E",
    should_spanned = "pub enum E",
    should_error = "aborting due to 2 previous errors\n"
)]
pub mod dropped_runner {
    #[::announce::announce(meta = my_lib::a)]
    #[announce_test_derive::drop_attrs]
    pub struct S(pub u8);

    #[::announce::announce]
    #[announce_test_derive::drop_attrs]
    pub enum E {
        A,
    }
}

// The same for the items that cannot be named without reading their generics, and for the
// other kinds of item: the sentinel is a call of the trampoline, which is not there.
#[subcrate(
    ::announce,
    announce_test_derive = derive,
    should_error = "cannot find macro `Tr` in this scope",
    should_spanned = "Tr",
    should_error = "cannot find macro `Generic` in this scope",
    should_spanned = "Generic",
    should_error = "cannot find macro `Renamed` in this scope",
    should_spanned = "Renamed",
    should_error = "cannot find macro `NotEmitted` in this scope",
    should_spanned = "NotEmitted",
    should_error = "aborting due to 4 previous errors\n"
)]
pub mod dropped_runner_macro {
    #[::announce::announce]
    #[announce_test_derive::drop_attrs]
    pub trait Tr {}

    #[::announce::announce]
    #[announce_test_derive::drop_attrs]
    pub struct Generic<T>(pub T);

    #[::announce::announce(macro_name = Renamed)]
    #[announce_test_derive::drop_attrs]
    pub mod m {}

    #[::announce::announce(emit_item = false)]
    #[announce_test_derive::drop_attrs]
    pub struct NotEmitted(pub u8);
}

// The announcements of one item are merged into one trampoline, so an option that
// has one value must have one value. The error is one, at the attribute that was
// expanded, and the item is still there.
#[subcrate(
    ::announce,
    should_error = "this item is announced twice with different trampoline names (`B` and `A`)",
    should_spanned = "#[announce::announce(macro_name = B)]",
    should_error = "this item is announced twice with different trampoline names (`ParseAlt` and `Parse`)",
    should_spanned = "#[announce::announce(macro_name = ParseAlt)]",
    should_error = "this announcement sets `layout = direct`, but another announcement of the same item sets `layout = single`",
    should_spanned = "#[announce::announce(layout = direct)]",
    should_error = "this item is announced twice with different trampoline names (`D` and `C`)",
    should_spanned = "#[announce::announce(macro_name = D)]",
    should_error = "aborting due to 4 previous errors\n"
)]
pub mod merge_conflict {
    #[announce::announce(macro_name = A)]
    #[announce::announce(macro_name = B)]
    pub struct Attrs(pub u8);

    // The name of the trampoline is `macro_name`, else the name of the item: a name that
    // is set in one announcement and not in the other is a different name (`Parse` and
    // `ParseAlt` here).
    #[announce::announce]
    #[announce::announce(macro_name = ParseAlt)]
    pub trait Parse {}

    // With a derive of another trait between them.
    #[announce::announce(layout = single)]
    #[derive(Debug)]
    #[announce::announce(layout = direct)]
    pub struct WithDerive(pub u8);

    // Three announcements: the third disagrees with the merge of the first two.
    #[announce::announce(macro_name = C)]
    #[announce::announce(macro_name = C, meta = my_lib::info)]
    #[announce::announce(macro_name = D)]
    pub struct Three(pub u8);

    pub fn main() {
        let _ = (Attrs(0), WithDerive(0), Three(0));
    }
}

// A macro that announces what it produces must put `#[announce]` first among the
// attributes of its output. This one (`late_announce`) puts it below the others, after the
// attribute that makes the trampoline has expanded, so it would make a second one. It is
// one error, at the attribute that was put there, and the item is still there.
#[subcrate(
    ::announce,
    announce_test_derive = derive,
    should_error = "this item has an `#[announce]` that expands after its announcements were run, so it makes a second trampoline",
    should_spanned = "#[announce::announce(meta = my_lib::first)]",
    should_error = "aborting due to 1 previous error\n"
)]
pub mod merge_late {
    #[announce::announce(meta = my_lib::first)]
    #[announce_test_derive::passthrough]
    #[announce_test_derive::late_announce(meta = my_lib::late)]
    pub struct Late(pub u8);

    pub fn main() {
        let _ = Late(0);
    }
}

// `emit_item` and `mock` are merged like the other options: the value that is set wins,
// and two values that differ are one error, at the announcement that was expanded.
#[subcrate(
    ::announce,
    should_error = "this announcement sets `emit_item = true`, but another announcement of the same item sets `emit_item = false`",
    should_spanned = "#[announce::announce(emit_item = true)]",
    should_error = "this announcement sets `mock = Other`, but another announcement of the same item sets `mock = Real`",
    should_spanned = "#[announce::announce(mock = Other)]",
    should_error = "aborting due to 2 previous errors\n"
)]
pub mod merge_not_emitted {
    #[announce::announce(emit_item = false)]
    #[announce::announce(emit_item = true)]
    pub struct EmitItem(pub u8);

    pub trait Real {}

    #[announce::announce(mock = Real)]
    #[announce::announce(mock = Other)]
    pub trait Mocked {}
}

// `mock` and `emit_item = false` remove the `Announce` impl, which the first announcement of
// a struct, enum or union without generics checks. They belong to the first announcement,
// not to a later one.
#[subcrate(
    ::announce,
    should_error = "`emit_item = false` must be on the first announcement of an item",
    should_spanned = "#[announce::announce(emit_item = false)]",
    should_error = "`mock` must be on the first announcement of an item",
    should_spanned = "#[announce::announce(mock = my_lib::Real)]",
    should_error = "aborting due to 2 previous errors\n"
)]
pub mod merge_not_first {
    #[announce::announce]
    #[announce::announce(emit_item = false)]
    pub struct A(pub u8);

    #[announce::announce]
    #[announce::announce(mock = my_lib::Real)]
    pub enum B {
        X,
    }

    // The first announcement has it: fine, and so is a trait or a generic struct, which have no
    // impl to check.
    #[announce::announce(emit_item = false)]
    #[announce::announce]
    pub struct C(pub u8);

    #[announce::announce]
    #[announce::announce(emit_item = false)]
    pub trait D {}

    #[announce::announce]
    #[announce::announce(emit_item = false)]
    pub struct E<T>(pub T);

    pub fn main() {
        let _ = (A(0), B::X);
    }
}

// The announcement with `emit_item = false` drops the item when the runner expands, after
// the macros below it have expanded on it. A derive that implements a trait for the item
// fails then: the item is gone.
#[subcrate(
    ::announce,
    should_error = "cannot find type `Dropped` in this scope",
    should_spanned = "Dropped",
    should_error = "aborting due to 1 previous error\n"
)]
pub mod not_emitted_derive {
    #[announce::announce(emit_item = false)]
    #[derive(Debug)]
    pub struct Dropped(pub u8);
}

// The arguments of the runner (the hidden attribute that `#[announce]` puts last) have the
// version of their encoding. An item that was announced by another version of `announce`
// is one error, which lists the versions, and the item is still there.
#[subcrate(
    ::announce,
    should_error = "this item is announced by several versions of the `announce` crate",
    should_error = "attributes are `0`",
    should_spanned = "#[announce::__private::__announce_runner(__announce_format_version = \"0\")]",
    should_error = "attributes are `unknown`",
    should_spanned = "#[announce::__private::__announce_runner(meta = my_lib::info)]",
    should_error = "attributes are `1`, `2`",
    should_spanned = "#[announce::__private::__announce_runner(__announce_format_version = \"1\", ?? !!, __announce_format_version = \"2\")]",
    should_error = "attributes are `7` (in `announce`), `1` (in `::announce`)",
    should_spanned = "#[announce::announce(meta = my_lib::info)]",
    should_error = "aborting due to 4 previous errors\n"
)]
pub mod runner_format {
    // The runner itself: another version, no version, two versions.
    #[announce::__private::__announce_runner(__announce_format_version = "0")]
    pub struct Other(pub u8);

    #[announce::__private::__announce_runner(meta = my_lib::info)]
    pub struct Unknown(pub u8);

    #[announce::__private::__announce_runner(__announce_format_version = "1", ?? !!, __announce_format_version = "2")]
    pub struct Two(pub u8);

    // An announcement that finds runners of two versions, at two crate paths.
    #[announce::announce(meta = my_lib::info)]
    #[announce::__private::__announce_runner(__announce_format_version = "7", ?? !!)]
    #[::announce::__private::__announce_runner(__announce_format_version = "1")]
    pub struct Merged(pub u8);

    pub fn main() {
        let _ = (Other(0), Unknown(0), Two(0), Merged(0));
    }
}

// An item announced by two versions of the `announce` crate is one error. The sentinel that
// the announcement emitted next to the item is satisfied, so that no second error follows.
// (`other_version` puts the runner of a version `7` below the attributes, as another
// `announce` in the build would.)
#[subcrate(
    ::announce,
    announce_test_derive = derive,
    should_error = "this item is announced by several versions of the `announce` crate",
    should_error = "attributes are `1`, `7`",
    should_spanned = "#[announce::announce]",
    should_error = "aborting due to 2 previous errors\n"
)]
pub mod runner_other_version {
    #[announce::announce]
    #[announce_test_derive::other_version]
    pub struct Plain(pub u8);

    #[announce::announce]
    #[announce_test_derive::other_version]
    pub trait Tr {}
}
