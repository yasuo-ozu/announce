//! The hand tables of the tool: which traits and items to mock, and the few facts that
//! std's source does not tell. Everything else (signatures, whether an item is
//! provided, stability) is read from std.

/// One trait to mock.
pub struct Trait {
    /// The module of announce-std that holds the mock.
    pub module: &'static str,
    pub name: &'static str,
    /// The public path of the real trait: what the mock announces (`mock = PATH`) and
    /// re-exports.
    pub path: &'static str,
    /// A part of the path of the file (relative to `library/`) that defines the trait in
    /// std. It tells apart traits of one name (`fmt::Write`, `io::Write`) and must match
    /// one file.
    pub src: &'static str,
    /// The items (methods, associated types and consts) that the mock has, in this
    /// order. Each must be stable in std's source.
    pub items: &'static [&'static str],
}

const fn t(
    module: &'static str,
    name: &'static str,
    path: &'static str,
    src: &'static str,
    items: &'static [&'static str],
) -> Trait {
    Trait {
        module,
        name,
        path,
        src,
        items,
    }
}

/// The modules of announce-std are written in the order of their first trait here, and
/// the traits of a module in this order.
pub const TRAITS: &[Trait] = &[
    t(
        "alloc",
        "GlobalAlloc",
        "::core::alloc::GlobalAlloc",
        "core/src/alloc/",
        &["alloc", "dealloc", "alloc_zeroed", "realloc"],
    ),
    t(
        "any",
        "Any",
        "::core::any::Any",
        "core/src/any.rs",
        &["type_id"],
    ),
    t(
        "borrow",
        "Borrow",
        "::core::borrow::Borrow",
        "core/src/borrow.rs",
        &["borrow"],
    ),
    t(
        "borrow",
        "BorrowMut",
        "::core::borrow::BorrowMut",
        "core/src/borrow.rs",
        &["borrow_mut"],
    ),
    t(
        "borrow",
        "ToOwned",
        "::std::borrow::ToOwned",
        "alloc/src/borrow.rs",
        &["Owned", "to_owned", "clone_into"],
    ),
    t(
        "clone",
        "Clone",
        "::core::clone::Clone",
        "core/src/clone.rs",
        &["clone", "clone_from"],
    ),
    t(
        "cmp",
        "PartialEq",
        "::core::cmp::PartialEq",
        "core/src/cmp.rs",
        &["eq", "ne"],
    ),
    t("cmp", "Eq", "::core::cmp::Eq", "core/src/cmp.rs", &[]),
    t(
        "cmp",
        "PartialOrd",
        "::core::cmp::PartialOrd",
        "core/src/cmp.rs",
        &["partial_cmp", "lt", "le", "gt", "ge"],
    ),
    t(
        "cmp",
        "Ord",
        "::core::cmp::Ord",
        "core/src/cmp.rs",
        &["cmp"],
    ),
    t(
        "convert",
        "AsRef",
        "::core::convert::AsRef",
        "core/src/convert/",
        &["as_ref"],
    ),
    t(
        "convert",
        "AsMut",
        "::core::convert::AsMut",
        "core/src/convert/",
        &["as_mut"],
    ),
    t(
        "convert",
        "Into",
        "::core::convert::Into",
        "core/src/convert/",
        &["into"],
    ),
    t(
        "convert",
        "TryInto",
        "::core::convert::TryInto",
        "core/src/convert/",
        &["Error", "try_into"],
    ),
    t(
        "default",
        "Default",
        "::core::default::Default",
        "core/src/default.rs",
        &["default"],
    ),
    t(
        "error",
        "Error",
        "::core::error::Error",
        "core/src/error.rs",
        &["source"],
    ),
    t(
        "fmt",
        "Display",
        "::core::fmt::Display",
        "core/src/fmt/",
        &["fmt"],
    ),
    t(
        "fmt",
        "Debug",
        "::core::fmt::Debug",
        "core/src/fmt/",
        &["fmt"],
    ),
    t(
        "fmt",
        "Binary",
        "::core::fmt::Binary",
        "core/src/fmt/",
        &["fmt"],
    ),
    t(
        "fmt",
        "Octal",
        "::core::fmt::Octal",
        "core/src/fmt/",
        &["fmt"],
    ),
    t(
        "fmt",
        "LowerHex",
        "::core::fmt::LowerHex",
        "core/src/fmt/",
        &["fmt"],
    ),
    t(
        "fmt",
        "UpperHex",
        "::core::fmt::UpperHex",
        "core/src/fmt/",
        &["fmt"],
    ),
    t(
        "fmt",
        "LowerExp",
        "::core::fmt::LowerExp",
        "core/src/fmt/",
        &["fmt"],
    ),
    t(
        "fmt",
        "UpperExp",
        "::core::fmt::UpperExp",
        "core/src/fmt/",
        &["fmt"],
    ),
    t(
        "fmt",
        "Pointer",
        "::core::fmt::Pointer",
        "core/src/fmt/",
        &["fmt"],
    ),
    t(
        "fmt",
        "Write",
        "::core::fmt::Write",
        "core/src/fmt/",
        &["write_str", "write_char", "write_fmt"],
    ),
    t(
        "future",
        "Future",
        "::core::future::Future",
        "core/src/future/",
        &["Output", "poll"],
    ),
    t(
        "future",
        "IntoFuture",
        "::core::future::IntoFuture",
        "core/src/future/",
        &["Output", "IntoFuture", "into_future"],
    ),
    t(
        "hash",
        "Hash",
        "::core::hash::Hash",
        "core/src/hash/",
        &["hash"],
    ),
    t(
        "hash",
        "Hasher",
        "::core::hash::Hasher",
        "core/src/hash/",
        &[
            "finish",
            "write",
            "write_u8",
            "write_u16",
            "write_u32",
            "write_u64",
            "write_u128",
            "write_usize",
            "write_i8",
            "write_i16",
            "write_i32",
            "write_i64",
            "write_i128",
            "write_isize",
        ],
    ),
    t(
        "hash",
        "BuildHasher",
        "::core::hash::BuildHasher",
        "core/src/hash/",
        &["Hasher", "build_hasher", "hash_one"],
    ),
    t(
        "io",
        "Read",
        "::std::io::Read",
        "alloc/src/io/read.rs",
        &["read"],
    ),
    t(
        "io",
        "Write",
        "::std::io::Write",
        "core/src/io/write.rs",
        &["write", "flush", "write_all"],
    ),
    t(
        "io",
        "BufRead",
        "::std::io::BufRead",
        "alloc/src/io/buf_read.rs",
        &["fill_buf", "consume"],
    ),
    t(
        "io",
        "Seek",
        "::std::io::Seek",
        "core/src/io/seek.rs",
        &["seek", "rewind", "stream_position", "seek_relative"],
    ),
    t(
        "iter",
        "IntoIterator",
        "::core::iter::IntoIterator",
        "core/src/iter/",
        &["Item", "IntoIter", "into_iter"],
    ),
    t(
        "iter",
        "Extend",
        "::core::iter::Extend",
        "core/src/iter/",
        &["extend"],
    ),
    t(
        "iter",
        "Iterator",
        "::core::iter::Iterator",
        "core/src/iter/",
        &["Item", "next", "size_hint", "count", "last", "nth"],
    ),
    t(
        "iter",
        "FusedIterator",
        "::core::iter::FusedIterator",
        "core/src/iter/",
        &[],
    ),
    t(
        "iter",
        "ExactSizeIterator",
        "::core::iter::ExactSizeIterator",
        "core/src/iter/",
        &["len"],
    ),
    t(
        "iter",
        "DoubleEndedIterator",
        "::core::iter::DoubleEndedIterator",
        "core/src/iter/",
        &["next_back", "nth_back", "rfold", "rfind"],
    ),
    t(
        "marker",
        "Copy",
        "::core::marker::Copy",
        "core/src/marker.rs",
        &[],
    ),
    t(
        "net",
        "ToSocketAddrs",
        "::std::net::ToSocketAddrs",
        "std/src/net/",
        &["Iter", "to_socket_addrs"],
    ),
    t(
        "ops",
        "Deref",
        "::core::ops::Deref",
        "core/src/ops/",
        &["Target", "deref"],
    ),
    t(
        "ops",
        "DerefMut",
        "::core::ops::DerefMut",
        "core/src/ops/",
        &["deref_mut"],
    ),
    t(
        "ops",
        "Index",
        "::core::ops::Index",
        "core/src/ops/",
        &["Output", "index"],
    ),
    t(
        "ops",
        "IndexMut",
        "::core::ops::IndexMut",
        "core/src/ops/",
        &["index_mut"],
    ),
    t(
        "ops",
        "Not",
        "::core::ops::Not",
        "core/src/ops/",
        &["Output", "not"],
    ),
    t(
        "ops",
        "BitAnd",
        "::core::ops::BitAnd",
        "core/src/ops/",
        &["Output", "bitand"],
    ),
    t(
        "ops",
        "BitOr",
        "::core::ops::BitOr",
        "core/src/ops/",
        &["Output", "bitor"],
    ),
    t(
        "ops",
        "BitXor",
        "::core::ops::BitXor",
        "core/src/ops/",
        &["Output", "bitxor"],
    ),
    t(
        "ops",
        "Shl",
        "::core::ops::Shl",
        "core/src/ops/",
        &["Output", "shl"],
    ),
    t(
        "ops",
        "Shr",
        "::core::ops::Shr",
        "core/src/ops/",
        &["Output", "shr"],
    ),
    t(
        "ops",
        "BitAndAssign",
        "::core::ops::BitAndAssign",
        "core/src/ops/",
        &["bitand_assign"],
    ),
    t(
        "ops",
        "BitOrAssign",
        "::core::ops::BitOrAssign",
        "core/src/ops/",
        &["bitor_assign"],
    ),
    t(
        "ops",
        "BitXorAssign",
        "::core::ops::BitXorAssign",
        "core/src/ops/",
        &["bitxor_assign"],
    ),
    t(
        "ops",
        "ShlAssign",
        "::core::ops::ShlAssign",
        "core/src/ops/",
        &["shl_assign"],
    ),
    t(
        "ops",
        "ShrAssign",
        "::core::ops::ShrAssign",
        "core/src/ops/",
        &["shr_assign"],
    ),
    t(
        "ops",
        "Add",
        "::core::ops::Add",
        "core/src/ops/",
        &["Output", "add"],
    ),
    t(
        "ops",
        "Sub",
        "::core::ops::Sub",
        "core/src/ops/",
        &["Output", "sub"],
    ),
    t(
        "ops",
        "Mul",
        "::core::ops::Mul",
        "core/src/ops/",
        &["Output", "mul"],
    ),
    t(
        "ops",
        "Div",
        "::core::ops::Div",
        "core/src/ops/",
        &["Output", "div"],
    ),
    t(
        "ops",
        "Rem",
        "::core::ops::Rem",
        "core/src/ops/",
        &["Output", "rem"],
    ),
    t(
        "ops",
        "Neg",
        "::core::ops::Neg",
        "core/src/ops/",
        &["Output", "neg"],
    ),
    t(
        "ops",
        "AddAssign",
        "::core::ops::AddAssign",
        "core/src/ops/",
        &["add_assign"],
    ),
    t(
        "ops",
        "SubAssign",
        "::core::ops::SubAssign",
        "core/src/ops/",
        &["sub_assign"],
    ),
    t(
        "ops",
        "MulAssign",
        "::core::ops::MulAssign",
        "core/src/ops/",
        &["mul_assign"],
    ),
    t(
        "ops",
        "DivAssign",
        "::core::ops::DivAssign",
        "core/src/ops/",
        &["div_assign"],
    ),
    t(
        "ops",
        "RemAssign",
        "::core::ops::RemAssign",
        "core/src/ops/",
        &["rem_assign"],
    ),
    t(
        "process",
        "Termination",
        "::std::process::Termination",
        "std/src/process.rs",
        &["report"],
    ),
    t(
        "string",
        "ToString",
        "::std::string::ToString",
        "alloc/src/string.rs",
        &["to_string"],
    ),
    t(
        "task",
        "Wake",
        "::std::task::Wake",
        "alloc/src/task.rs",
        &["wake", "wake_by_ref"],
    ),
];

/// Traits whose name is also a derive macro. The mock macro takes the macro namespace
/// of the name, so the type namespace needs the real trait (a glob re-export, from
/// `GLOB_SINCE`) or a blanket subtrait (before it).
pub const DERIVE_NAMES: &[&str] = &[
    "Debug",
    "Hash",
    "PartialEq",
    "Eq",
    "PartialOrd",
    "Ord",
    "Clone",
    "Copy",
    "Default",
];

/// The first rustc minor version (1.N) where the glob re-export of the derive-named
/// real traits works. An announce-std decision, not a fact of std: the glob breaks the
/// trampoline's resolution before it.
pub const GLOB_SINCE: u32 = 94;

/// The path of a trait that stability does not tell: where the real trait is before
/// `from`'s trait became stable. The record of `from` gets a second record that is
/// valid from `since` until the stability version of the first, with the path `from`
/// replaced by `to` in all its paths. It needs `feature = "std"` when `to` is in std.
pub struct Variant {
    pub from: &'static str,
    pub to: &'static str,
    /// The minor version (1.N) where the older path exists. Std's source of today
    /// does not say it.
    pub since: u32,
}

/// `core::error::Error` is stable since 1.81; before that the trait is
/// `std::error::Error`, which needs the std feature.
pub const VARIANTS: &[Variant] = &[Variant {
    from: "::core::error::Error",
    to: "::std::error::Error",
    since: 0,
}];

/// Extra paragraph of a mock's doc comment, by trait name (rustdoc markdown, no `///`).
/// Not for derive-named traits.
pub const DOC_NOTES: &[(&str, &str)] = &[(
    "BuildHasher",
    "`hash_one` is part of the mock from Rust 1.71, where it became stable. On older\n\
     compilers the mock has `build_hasher` only: hash a value with `x.hash(&mut b.build_hasher())`.",
)];

/// Private paths in std's source and their public path, for names that std imports from a
/// private module (`crate::iter::traits::collect::IntoIterator`) or that are public
/// under another name (`core::net::SocketAddr` is `std::net::SocketAddr` before Rust
/// 1.77, and the traits of `std::io` live in `core::io` and `alloc::io` of today's
/// source). The longest matching prefix (at a `::` boundary) is replaced, until none
/// matches; the paths are `::crate::a::b` after the crate root mapping (`alloc` is
/// `::std`). That a path is public is not checked here: announce-std's tests and
/// `tools/gen-std`'s users find the ones that are missing.
pub const PUBLIC_PATHS: &[(&str, &str)] = &[
    // `Extend::extend<T: IntoIterator<..>>`
    ("::core::iter::traits::collect", "::core::iter"),
    // `Seek::seek(pos: SeekFrom)`, `BufRead: Read`, `io::Result`
    ("::core::io", "::std::io"),
    ("::std::io::seek", "::std::io"),
    // `ToSocketAddrs::Iter: Iterator<Item = SocketAddr>`
    ("::core::net", "::std::net"),
    // `DerefMut: Deref`, `IndexMut: Index`
    ("::core::ops::deref", "::core::ops"),
    ("::core::ops::index", "::core::ops"),
];

/// The names of the std prelude (and the extern prelude): a name that is not found in the
/// file resolves here. The public path of the item follows.
pub const PRELUDE: &[(&str, &str)] = &[
    ("Option", "::core::option::Option"),
    ("Result", "::core::result::Result"),
    ("Sized", "::core::marker::Sized"),
    ("Copy", "::core::marker::Copy"),
    ("Send", "::core::marker::Send"),
    ("Sync", "::core::marker::Sync"),
    ("Unpin", "::core::marker::Unpin"),
    ("Drop", "::core::ops::Drop"),
    ("Fn", "::core::ops::Fn"),
    ("FnMut", "::core::ops::FnMut"),
    ("FnOnce", "::core::ops::FnOnce"),
    ("AsRef", "::core::convert::AsRef"),
    ("AsMut", "::core::convert::AsMut"),
    ("Into", "::core::convert::Into"),
    ("From", "::core::convert::From"),
    ("TryFrom", "::core::convert::TryFrom"),
    ("TryInto", "::core::convert::TryInto"),
    ("Clone", "::core::clone::Clone"),
    ("Default", "::core::default::Default"),
    ("Eq", "::core::cmp::Eq"),
    ("Ord", "::core::cmp::Ord"),
    ("PartialEq", "::core::cmp::PartialEq"),
    ("PartialOrd", "::core::cmp::PartialOrd"),
    ("Iterator", "::core::iter::Iterator"),
    ("Extend", "::core::iter::Extend"),
    ("IntoIterator", "::core::iter::IntoIterator"),
    ("DoubleEndedIterator", "::core::iter::DoubleEndedIterator"),
    ("ExactSizeIterator", "::core::iter::ExactSizeIterator"),
    ("FromIterator", "::core::iter::FromIterator"),
    ("ToOwned", "::std::borrow::ToOwned"),
    ("ToString", "::std::string::ToString"),
    ("String", "::std::string::String"),
    ("Vec", "::std::vec::Vec"),
    ("Box", "::std::boxed::Box"),
];

/// The primitive types: a single-segment type path of one of these names is
/// `::core::primitive::NAME`.
pub const PRIMITIVES: &[&str] = &[
    "bool", "char", "str", "u8", "u16", "u32", "u64", "u128", "usize", "i8", "i16", "i32", "i64",
    "i128", "isize", "f32", "f64",
];

/// Marker traits of the unstable sized hierarchy. They stand for `?Sized` in a bound,
/// and are dropped from the supertraits.
pub const SIZED_HIERARCHY: &[&str] = &["::core::marker::PointeeSized", "::core::marker::MetaSized"];

/// Unstable marker traits that std's source has in the bounds of its const traits
/// (`Self: [const] Destruct`). They are dropped.
pub const DROPPED_BOUNDS: &[&str] = &["::core::marker::Destruct"];
