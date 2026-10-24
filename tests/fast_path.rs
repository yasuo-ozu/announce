//! The fast path of the type-context analysis (`announce-core`'s `fastpath`) must never
//! accept an item that the full analysis would reject or need an entry for. The tests ask it
//! about the items of the repository's own sources, a list of shapes, and random items;
//! `announce_core::check_fast_path` runs the full analysis on each and says when they disagree.
//!
//! The `announce-core` unit tests have the same check built in (`leak::analyze` calls it on
//! every item in a test build), but only on the items they use.

use announce_core::{check_fast_path, Options};
use std::path::{Path as FsPath, PathBuf};
use syn::Item;

fn item(src: &str) -> Item {
    syn::parse_str(src).unwrap_or_else(|e| panic!("{}: {}", e, src))
}

fn opts(allow_primitive: bool, allow_std_prelude: bool) -> Options {
    // `Options` is `#[non_exhaustive]`: no struct update syntax from another crate.
    let mut o = Options::default();
    o.allow_primitive = allow_primitive;
    o.allow_std_prelude = allow_std_prelude;
    o
}

/// Whether the fast path accepts `item`. `check_fast_path` also runs the full analysis,
/// so a disagreement (an accepted item that needs an entry, or is rejected) panics.
fn accepts(item: &Item, opts: &Options) -> bool {
    check_fast_path(item, opts).unwrap_or_else(|e| panic!("{}", e))
}

/// Whether the fast path accepts `src` with the default options.
fn fast(src: &str) -> bool {
    accepts(&item(src), &Options::default())
}

/// Asks the fast path about every item under the four combinations of options; the full
/// analysis checks each answer (`accepts`), so a disagreement panics. Returns how many
/// (item, options) pairs the fast path accepted, and how many it saw.
fn agree_on(items: impl IntoIterator<Item = Item>) -> (usize, usize) {
    let (mut accepted, mut seen) = (0, 0);
    for it in items {
        for (p, s) in [(true, false), (true, true), (false, false), (false, true)] {
            let o = opts(p, s);
            seen += 1;
            if accepts(&it, &o) {
                accepted += 1;
            }
        }
    }
    (accepted, seen)
}

/// The struct, enum, union, trait and type items of `file`, at any depth of
/// inline modules.
fn items_of(file: &syn::File) -> Vec<Item> {
    fn walk(items: &[Item], out: &mut Vec<Item>) {
        for i in items {
            match i {
                Item::Struct(_)
                | Item::Enum(_)
                | Item::Union(_)
                | Item::Trait(_)
                | Item::Type(_) => out.push(i.clone()),
                Item::Mod(m) => {
                    if let Some((_, inner)) = &m.content {
                        walk(inner, out);
                    }
                }
                Item::Fn(f) => {
                    // Items declared in a function body (the UI tests do that).
                    for s in &f.block.stmts {
                        if let syn::Stmt::Item(it) = s {
                            walk(std::slice::from_ref(it), out);
                        }
                    }
                }
                _ => {}
            }
        }
    }
    let mut out = Vec::new();
    walk(&file.items, &mut out);
    out
}

fn rust_files(dir: &FsPath, out: &mut Vec<PathBuf>) {
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };
    // Symlinks are skipped: `tests/tests` points back to `tests`.
    let mut paths: Vec<(PathBuf, std::fs::FileType)> = entries
        .filter_map(|e| e.ok())
        .filter_map(|e| Some((e.path(), e.file_type().ok()?)))
        .filter(|(_, t)| !t.is_symlink())
        .collect();
    paths.sort_by(|a, b| a.0.cmp(&b.0));
    for (p, t) in paths {
        let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if t.is_dir() {
            if name != "target" {
                rust_files(&p, out);
            }
        } else if p.extension().map_or(false, |e| e == "rs") {
            out.push(p);
        }
    }
}

/// Every definition in the source of the test suites of the repository
/// (`tests/`, the std mocks of `announce-std`, and the sources of `announce` and
/// `announce-core`'s `helper`, whose tests are full of items in strings that the tests
/// below cover separately). The items are those a user could pass to `#[announce]`, and more.
#[test]
fn fast_path_never_accepts_more_than_the_analysis_on_the_test_suites() {
    // The repository: this package is in `tests/` of it.
    let root = FsPath::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let mut files = Vec::new();
    for dir in [
        "tests",
        "announce-std/src",
        "announce-core/src/helper",
        "src",
    ] {
        rust_files(&root.join(dir), &mut files);
    }
    let (mut accepted, mut seen, mut parsed) = (0, 0, 0);
    for f in &files {
        let text = std::fs::read_to_string(f).unwrap();
        let file = match syn::parse_file(&text) {
            Ok(file) => file,
            Err(_) => continue,
        };
        parsed += 1;
        let (a, s) = agree_on(items_of(&file));
        accepted += a;
        seen += s;
    }
    assert!(parsed > 20, "only {} files parsed", parsed);
    // The check is not empty: many items are accepted, many are not.
    assert!(seen > 200, "only {} items seen", seen);
    assert!(accepted * 10 > seen, "{} of {} accepted", accepted, seen);
    assert!(accepted < seen, "{} of {} accepted", accepted, seen);
}

/// The items that the unit tests of `announce-core` write: those of `leak`, `generate`
/// and `lib`, as the strings and `quote!` blocks they are, are in the files above; these are
/// the shapes the fast path is designed for.
const ACCEPTED: &[&str] = &[
    // The benchmark's items.
    "pub trait D { fn m0(&self, x: u32) -> u32; fn m1(&self, x: u32) -> u32; }",
    "pub struct D { pub f0: u32, pub f1: u32 }",
    "struct S;",
    "struct S(u8, bool, str);",
    "pub enum E { A, B(u8), C { x: u8, y: (u8, bool) }, D = 4 }",
    "pub union U { a: u8, b: [u8; 4] }",
    "type A = (u8, [u8; 3], &'static str, fn(u8) -> bool);",
    "type B<T> = ::std::vec::Vec<T>;",
    // Paths that resolve everywhere.
    "struct S { a: ::std::vec::Vec<u8>, b: ::core::option::Option<crate::Local>, c: crate::m::T<u8> }",
    "struct S<T> { a: T, b: ::std::boxed::Box<T>, c: T::Output }",
    "struct S { a: *const u8, b: *mut u8, c: &'static u8, d: &'static mut u8, e: [u8] }",
    "trait Tr { type Out; fn f(&self) -> Self::Out; fn g(&self, o: &Self) -> bool; fn h(self) -> Self; }",
    "trait Tr { type Out; fn f(&self) -> (Self::Out, Self); fn g(&mut self) -> &Self::Out; }",
    "trait Tr { type Out; fn f(&self) -> ::std::boxed::Box<Self::Out>; fn g(&self) -> [Self::Out; 2]; }",
    "trait Tr<T> { fn f(&self, t: T) -> T; fn g<U>(&self, u: U) -> (T, U); }",
    "trait Tr<'a> { fn f(&'a self) -> &'a u8; fn g<'b>(&self, x: &'b u8) -> &'b u8; }",
    "trait Tr: ::core::fmt::Debug + Sized + 'static { fn f(&self) where Self: Sized; }",
    "trait Tr { type G<'a>: ::core::fmt::Debug where Self: 'a; const C: usize; const D: u8 = 3; }",
    "trait Tr { fn f(&self) -> u8 { helper() } }",
    "struct S<const N: usize> { a: [u8; N], b: [u8; 2 * N + 1] }",
    "struct S<T: ?Sized + ::core::fmt::Debug, const N: usize = 3> where T: ::core::clone::Clone { a: [u8; N], b: &'static T }",
    "struct S<'a, T: 'a> { a: &'a T, b: &'a [T] }",
    "trait Tr { fn f<const N: usize>(&self) -> [u8; N]; }",
    "struct S { a: [u8; crate::N], b: [u8; ::core::primitive::usize::MAX] }",
    "enum E { A = crate::N, B = 3, C = 1 << 3 }",
    "trait Tr { fn f(&self, g: fn(u8, bool) -> ::std::vec::Vec<u8>); }",
    "trait Tr { fn f<F: ::core::ops::Fn(u8) -> u8>(&self, f: F) -> u8; }",
    "trait Tr { fn f<T>(&self, t: T) where T: ::core::iter::Iterator<Item = u8>; }",
    "struct S { a: (), b: ! }",
    "trait Tr { fn f(&self, _: u8, mut x: u16) -> u32; }",
    "mod m { pub struct S(Local); }",
];

/// With the options: `Vec` and the primitives need them.
const ACCEPTED_WITH_PRELUDE: &[&str] = &[
    "struct S { a: Vec<u8>, b: Option<Box<Self>>, c: String }",
    "trait Tr: Clone + Send + Sync { fn f(&self) -> Option<u8>; }",
    "struct S<T: Clone + Default> where T: PartialEq { a: Vec<T>, b: Result<T, String> }",
    "trait Tr { fn f<I: IntoIterator<Item = u8>>(&self, i: I) -> Box<Self>; }",
];

/// Not accepted: every one of them takes the full analysis (the first ones are
/// errors or entries there, the last ones are only unknown to the scan).
const NOT_ACCEPTED: &[&str] = &[
    // Relative paths.
    "struct S { a: Local }",
    "struct S { a: inner::Item }",
    "struct S { a: super::X }",
    "struct S { a: std::vec::Vec<u8> }",
    "struct S { a: Vec<u8> }",
    "struct S { a: Option<u8> }",
    "struct S { a: ::std::vec::Vec<Local> }",
    "struct S<T> { a: Vec<T> }",
    "struct S<T> { a: Local<T> }",
    "struct S { a: [u8; N] }",
    "struct S { a: [u8; LOCAL] }",
    "struct S { a: [u8; ::core::mem::size_of::<u8>()] }",
    "enum E { A = LOCAL }",
    "enum E { A = f() }",
    "trait Tr: Base { }",
    "trait Tr { fn f<T: Display>(&self, t: T); }",
    "struct S<T: Clone>(T);",
    "struct S<T> where T: Debug { a: T }",
    "trait Tr { fn f(&self) where Self: Clone; }",
    "trait Tr { fn f(&self, x: Local); }",
    // `Self` where it is not accepted.
    "struct S { a: Self::A }",
    "struct S<T> { a: T::A::B }",
    "type A = Self;",
    "type A<T> = T::Out::X;",
    "trait Tr { type Out; fn f(&self) -> Self::Out::X; }",
    "trait Tr { fn f(&self) -> Local<Self>; }",
    "trait Tr { type Out; fn f(&self) -> Local<Self::Out>; }",
    "trait Tr { type G<'a>; fn f<'a>(&'a self) -> Self::G<'a>; }",
    "trait Tr { fn f(&self) where Self::Out: Clone; }",
    // Forms the scan does not know.
    "struct S { a: Box<dyn ::core::fmt::Debug> }",
    "struct S { a: &'static dyn ::core::fmt::Debug }",
    "trait Tr { fn f(&self) -> impl ::core::fmt::Debug; }",
    "trait Tr { fn f(&self, x: impl ::core::fmt::Debug); }",
    "struct S { a: <u8 as ::core::ops::Add>::Output }",
    "trait Tr { fn f<T: ::core::ops::Add>(&self) -> <T as ::core::ops::Add>::Output; }",
    "struct S { a: ::other::Wrap<u8> }",
    "struct S { a: _ }",
    "struct S { a: m!() }",
    "struct S<'a> { a: &'_ u8, b: &'a u8 }",
    "struct S { a: for<'a> fn(&'a u8) -> &'a u8 }",
    "struct S<F: for<'a> ::core::ops::Fn(&'a u8)>(F);",
    "struct S<T> where for<'a> &'a T: ::core::marker::Copy { a: T }",
    "struct S<T> where Vec<T>: ::core::clone::Clone { a: T }",
    "struct S<const N: Idx>;",
    "struct S<T, const N: usize> { a: Foo<N> }",
    "trait Tr { fn f(&self, (a, b): (u8, u8)); }",
    "trait Tr { async fn f(&self) -> u8; }",
    "trait Tr { unsafe extern \"C\" fn f(&self, ...); }",
    "trait Tr { m!(); }",
    "trait Tr { const N: usize = size_of::<u8>(); }",
    "trait Tr { type Out: Local; }",
    "pub(in crate::m) struct S;",
    "fn f() {}",
];

#[test]
fn the_shapes_it_is_designed_for_are_accepted() {
    for src in ACCEPTED {
        assert!(fast(src), "not accepted: {}", src);
    }
    for src in ACCEPTED_WITH_PRELUDE {
        assert!(!fast(src), "accepted without the prelude: {}", src);
        assert!(
            accepts(&item(src), &opts(true, true)),
            "not accepted with the prelude: {}",
            src
        );
    }
    // Primitives need `allow_primitive`.
    let u8_only = item("struct S { a: u8, b: ::core::primitive::u8 }");
    assert!(accepts(&u8_only, &opts(true, false)));
    assert!(!accepts(&u8_only, &opts(false, false)));
}

#[test]
fn other_shapes_are_left_to_the_analysis() {
    for src in NOT_ACCEPTED {
        let it = item(src);
        assert!(!accepts(&it, &Options::default()), "accepted: {}", src);
    }
}

#[test]
fn the_analysis_agrees_on_the_listed_items() {
    let all: Vec<Item> = ACCEPTED
        .iter()
        .chain(ACCEPTED_WITH_PRELUDE)
        .chain(NOT_ACCEPTED)
        .map(|s| item(s))
        .collect();
    let (accepted, seen) = agree_on(all);
    assert!(accepted > 60 && accepted < seen, "{} of {}", accepted, seen);
    // The listed accepted items really pass the full analysis, with no entry and no error.
    for src in ACCEPTED {
        let it = item(src);
        assert_eq!(
            check_fast_path(&it, &Options::default()),
            Ok(true),
            "{}",
            src
        );
    }
}

#[test]
fn a_module_is_not_analysed() {
    // Accepted, and the analysis agrees: it has no entry and no error for a module.
    assert!(fast("mod m { struct S { a: Local } }"));
    assert!(!fast("fn f() {}"));
}

// ---- random items ------------------------------------------------------

/// xorshift64*: the same items on every run.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
    fn pick<'a>(&mut self, xs: &[&'a str]) -> &'a str {
        xs[self.below(xs.len())]
    }
    fn chance(&mut self, percent: usize) -> bool {
        self.below(100) < percent
    }
}

/// What a type may name where it is written.
#[derive(Clone)]
struct Ctx {
    /// Type parameters in scope.
    types: Vec<&'static str>,
    /// Const parameters in scope.
    consts: Vec<&'static str>,
    /// A trait item (`Self::Out` and the like make sense).
    in_trait: bool,
    /// `'a` is declared.
    lifetime: bool,
    /// Only forms that resolve everywhere, mostly: the share of "good" leaves.
    good: usize,
}

const PRIMS: &[&str] = &["u8", "u32", "bool", "char", "str", "f64", "usize", "i128"];
const GOOD_PATHS: &[&str] = &[
    "::std::string::String",
    "::core::marker::PhantomData<u8>",
    "crate::Local",
    "crate::m::Item",
    "::core::option::Option<u8>",
];
const BAD_LEAVES: &[&str] = &[
    "Local",
    "inner::Item",
    "super::X",
    "std::vec::Vec<u8>",
    "Self::A::B",
    "::other::Wrap<u8>",
    "_",
    "m!()",
    "dyn ::core::fmt::Debug",
    "dyn Local",
    "impl ::core::fmt::Debug",
    "impl Local",
    "<u8 as ::core::ops::Add>::Output",
    "for<'a> fn(&'a u8) -> &'a u8",
    "Local<'_>",
    "&'_ u8",
    "[u8; LOCAL_N]",
    "[u8; ::core::mem::size_of::<u8>()]",
    "(Local, u8)",
    "fn(Local) -> u8",
    "[Local; 2]",
    "$crate::Local",
];
const PRELUDE_LEAVES: &[&str] = &[
    "Vec<u8>",
    "Option<u8>",
    "String",
    "Box<u8>",
    "Result<u8, u8>",
];

fn gen_type(r: &mut Rng, c: &Ctx, depth: usize) -> String {
    // Leaves.
    if depth == 0 || r.chance(35) {
        if r.chance(c.good) {
            let mut leaves: Vec<String> = PRIMS.iter().map(|s| s.to_string()).collect();
            leaves.extend(GOOD_PATHS.iter().map(|s| s.to_string()));
            leaves.push("Self".into());
            for t in &c.types {
                leaves.push(t.to_string());
                leaves.push(format!("{}::Output", t));
            }
            if c.in_trait {
                leaves.push("Self::Out".into());
            }
            return leaves[r.below(leaves.len())].clone();
        }
        return match r.below(10) {
            0..=3 => r.pick(BAD_LEAVES).to_string(),
            4..=6 => r.pick(PRELUDE_LEAVES).to_string(),
            7 => "Self::Out".into(),
            8 => "Self".into(),
            _ => format!("{}::Out", c.types.first().copied().unwrap_or("Local")),
        };
    }
    let inner = |r: &mut Rng| gen_type(r, c, depth - 1);
    match r.below(16) {
        0 => format!("&{}", inner(r)),
        1 if c.lifetime => format!("&'a {}", inner(r)),
        1 => format!("&'b {}", inner(r)),
        2 => format!("&'static {}", inner(r)),
        3 => format!("&mut {}", inner(r)),
        4 => format!("*const {}", inner(r)),
        5 => format!("[{}]", inner(r)),
        6 => {
            let len = if c.consts.is_empty() || r.chance(40) {
                r.pick(&[
                    "3",
                    "crate::N",
                    "Self::N",
                    "LOCAL_N",
                    "2 * 3",
                    "::core::mem::size_of::<u8>()",
                ])
                .to_string()
            } else {
                c.consts[r.below(c.consts.len())].to_string()
            };
            format!("[{}; {}]", inner(r), len)
        }
        7 => format!("({}, {})", inner(r), inner(r)),
        8 => format!("({},)", inner(r)),
        9 => format!("fn({}) -> {}", inner(r), inner(r)),
        10 => format!("::std::vec::Vec<{}>", inner(r)),
        11 => format!("::core::option::Option<{}>", inner(r)),
        12 => format!("::std::boxed::Box<{}>", inner(r)),
        13 => format!("crate::Wrap<{}, {}>", inner(r), inner(r)),
        14 => format!("Local<{}>", inner(r)),
        _ => format!("Vec<{}>", inner(r)),
    }
}

const BOUNDS: &[&str] = &[
    "::core::convert::From<Self>",
    "::core::convert::Into<Self::Out>",
    "::core::iter::Iterator<Item = Self::Out>",
    "::core::ops::Fn(&Self) -> u8",
    "::core::ops::Fn(&u8) -> &u8",
    "::core::ops::FnMut(&mut u8, &u8) -> u8",
    "::core::convert::AsRef<[u8]>",
    "::core::ops::Add<u8, Output = u8>",
    "::core::borrow::Borrow<T>",
    "::core::borrow::Borrow<T::Output>",
    "::core::convert::From<[u8; N]>",
    "::core::convert::From<[u8; Self::N]>",
    "::core::fmt::Debug",
    "::core::clone::Clone + ::core::marker::Copy",
    "crate::Tr",
    "::core::ops::Add<Output = u8>",
    "::core::ops::Fn(u8) -> u8",
    "::core::iter::Iterator<Item = u8>",
    "::core::convert::From<u8>",
    "Sized",
    "?Sized",
    "'a",
];
const BAD_BOUNDS: &[&str] = &[
    "Clone",
    "Local",
    "std::fmt::Debug",
    "Iterator<Item = Local>",
    "Fn(Local)",
    "for<'b> ::core::ops::Fn(&'b u8)",
    "::other::Tr",
];

fn gen_bound(r: &mut Rng, good: usize) -> String {
    if r.chance(good) {
        r.pick(BOUNDS).to_string()
    } else {
        r.pick(BAD_BOUNDS).to_string()
    }
}

/// `<..>` and its names: type parameters and const parameters.
fn gen_generics(r: &mut Rng, c: &mut Ctx, allow_lifetime: bool) -> String {
    let mut params = Vec::new();
    if allow_lifetime && r.chance(30) {
        params.push("'a".to_string());
        c.lifetime = true;
    }
    if r.chance(45) {
        let name = r.pick(&["T", "U"]);
        let g = c.good;
        let p = match r.below(5) {
            0 => name.to_string(),
            1 => format!("{}: {}", name, gen_bound(r, g)),
            2 => format!("{}: {} + {}", name, gen_bound(r, g), gen_bound(r, g)),
            3 => format!(
                "{} = {}",
                name,
                r.pick(&[
                    "u8",
                    "Local",
                    "crate::Local",
                    "Self",
                    "Self::Out",
                    "&'a u8",
                    "&'static u8",
                    "::std::vec::Vec<Self>",
                    "T",
                    "[u8; 2]"
                ])
            ),
            _ => format!("{}: ?Sized", name),
        };
        c.types.push(if name == "T" { "T" } else { "U" });
        params.push(p);
    }
    if r.chance(25) {
        let ty = r.pick(&["usize", "u8", "bool", "Idx", "crate::Idx"]);
        let default = if r.chance(30) { " = 3" } else { "" };
        params.push(format!("const N: {}{}", ty, default));
        c.consts.push("N");
    }
    if params.is_empty() {
        String::new()
    } else {
        format!("<{}>", params.join(", "))
    }
}

fn gen_where(r: &mut Rng, c: &Ctx) -> String {
    if !r.chance(30) {
        return String::new();
    }
    let mut preds = Vec::new();
    for _ in 0..1 + r.below(2) {
        let bounded = match r.below(9) {
            0 | 1 if !c.types.is_empty() => c.types[r.below(c.types.len())].to_string(),
            2 => "Self".to_string(),
            3 => "Self::Out".to_string(),
            4 if !c.types.is_empty() => format!("{}::Output", c.types[0]),
            5 => "&'a T".to_string(),
            6 => "(T, u8)".to_string(),
            _ => gen_type(r, c, 1),
        };
        preds.push(format!("{}: {}", bounded, gen_bound(r, c.good)));
    }
    format!(" where {}", preds.join(", "))
}

fn gen_fields(r: &mut Rng, c: &Ctx, named: bool) -> String {
    let n = r.below(4);
    let fields: Vec<String> = (0..n)
        .map(|i| {
            let ty = gen_type(r, c, 2);
            if named {
                format!("pub f{}: {}", i, ty)
            } else {
                ty
            }
        })
        .collect();
    fields.join(", ")
}

fn gen_method(r: &mut Rng, c: &Ctx, i: usize) -> String {
    let mut c = c.clone();
    let g = gen_generics(r, &mut c, true);
    let g = g.replace("const N", "const M").replace("= 3", "");
    if g.contains("const M") {
        c.consts.push("M");
    }
    let recv = if c.lifetime {
        r.pick(&[
            "&self",
            "&mut self",
            "self",
            "&'a self",
            "&'a mut self",
            "self: ::std::boxed::Box<Self>",
            "self: Box<Self>",
            "self: &Self",
            "self: &'a Self",
            "self: ::std::rc::Rc<Self>",
            "self: ::core::pin::Pin<&mut Self>",
            "",
        ])
    } else {
        r.pick(&[
            "&self",
            "&mut self",
            "self",
            "&'b self",
            "self: ::std::boxed::Box<Self>",
            "self: &mut Self",
            "",
        ])
    };
    let mut args = vec![recv.to_string()];
    for k in 0..r.below(3) {
        args.push(format!("a{}: {}", k, gen_type(r, &c, 2)));
    }
    let args: Vec<String> = args.into_iter().filter(|a| !a.is_empty()).collect();
    let ret = if r.chance(70) {
        format!(" -> {}", gen_type(r, &c, 2))
    } else {
        String::new()
    };
    let wh = gen_where(r, &c);
    let body = if r.chance(15) { " { helper() }" } else { ";" };
    let wh_body = if body == ";" {
        format!("{};", wh)
    } else {
        format!("{}{}", wh, body)
    };
    let _ = wh_body;
    format!("fn m{}{}({}){}{}{}", i, g, args.join(", "), ret, wh, body)
}

fn gen_item(r: &mut Rng) -> String {
    let good = if r.chance(50) { 95 } else { 70 };
    let mut c = Ctx {
        types: Vec::new(),
        consts: Vec::new(),
        in_trait: false,
        lifetime: false,
        good,
    };
    let kind = r.below(10);
    let attrs = r.pick(&[
        "",
        "",
        "#[derive(Clone)] ",
        "#[doc = \"x\"] ",
        "#[cfg(a)] #[repr(C)] ",
    ]);
    let generics = gen_generics(r, &mut c, true);
    match kind {
        0..=2 => {
            let named = r.chance(70);
            if r.chance(10) {
                format!("{}struct S{}{};", attrs, generics, gen_where(r, &c))
            } else if named {
                format!(
                    "{}struct S{}{} {{ {} }}",
                    attrs,
                    generics,
                    gen_where(r, &c),
                    gen_fields(r, &c, true)
                )
            } else {
                format!(
                    "{}struct S{}({}){};",
                    attrs,
                    generics,
                    gen_fields(r, &c, false),
                    gen_where(r, &c)
                )
            }
        }
        3..=4 => {
            let n = 1 + r.below(3);
            let variants: Vec<String> = (0..n)
                .map(|i| match r.below(4) {
                    0 => format!("V{}", i),
                    1 => format!("V{}({})", i, gen_fields(r, &c, false)),
                    2 => format!("V{} {{ {} }}", i, gen_fields(r, &c, true)),
                    _ => format!(
                        "V{} = {}",
                        i,
                        r.pick(&[
                            "1",
                            "crate::N",
                            "LOCAL",
                            "-2",
                            "1 << 2",
                            "Self::N",
                            "N",
                            "::core::primitive::u8::MAX"
                        ])
                    ),
                })
                .collect();
            format!(
                "{}enum E{}{} {{ {} }}",
                attrs,
                generics,
                gen_where(r, &c),
                variants.join(", ")
            )
        }
        5 => format!(
            "{}union U{} {{ {} }}",
            attrs,
            generics,
            gen_fields(r, &c, true)
        ),
        6 => {
            let ctype = gen_type(r, &c, 3);
            format!("{}type A{} = {};", attrs, generics, ctype)
        }
        _ => {
            c.in_trait = true;
            let supers = if r.chance(30) {
                let mut b = gen_bound(r, c.good);
                if r.chance(30) {
                    b = format!("{} + {}", b, gen_bound(r, c.good));
                }
                format!(": {}", b)
            } else {
                String::new()
            };
            let mut items = Vec::new();
            for i in 0..r.below(5) {
                match r.below(8) {
                    0 => items.push(format!(
                        "type Out{};",
                        if r.chance(50) {
                            format!(": {}", gen_bound(r, c.good))
                        } else {
                            String::new()
                        }
                    )),
                    1 => items.push("type Out = u8;".to_string()),
                    2 => items.push(format!(
                        "type G<'a>: {} where Self: 'a;",
                        gen_bound(r, c.good)
                    )),
                    3 => items.push(format!("const C{}: {};", i, gen_type(r, &c, 1))),
                    4 => items.push(format!(
                        "const D{}: usize = {};",
                        i,
                        r.pick(&["3", "crate::N", "LOCAL", "Self::N", "2 + 1"])
                    )),
                    _ => items.push(gen_method(r, &c, i)),
                }
            }
            // `Out` is declared once.
            let mut seen_out = false;
            items.retain(|i| {
                if i.starts_with("type Out") {
                    let keep = !seen_out;
                    seen_out = true;
                    keep
                } else {
                    true
                }
            });
            format!(
                "{}trait Tr{}{}{} {{ {} }}",
                attrs,
                generics,
                supers,
                gen_where(r, &c),
                items.join(" ")
            )
        }
    }
}

/// `ANNOUNCE_FASTPATH_ITEMS=n` and `ANNOUNCE_FASTPATH_SEED=s` run more items,
/// other ones (a hunt for disagreements; 1200 items take a few seconds).
fn env_number(name: &str, default: u64) -> u64 {
    std::env::var(name)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

#[test]
fn fast_path_never_accepts_more_than_the_analysis_on_random_items() {
    let count = env_number("ANNOUNCE_FASTPATH_ITEMS", 1200);
    let mut r = Rng(0x9E37_79B9_7F4A_7C15 ^ env_number("ANNOUNCE_FASTPATH_SEED", 0));
    let (mut accepted, mut seen, mut parsed) = (0, 0, 0);
    for _ in 0..count {
        let src = gen_item(&mut r);
        let it = match syn::parse_str::<Item>(&src) {
            Ok(it) => it,
            Err(_) => continue,
        };
        parsed += 1;
        let (a, s) = agree_on([it]);
        accepted += a;
        seen += s;
    }
    assert!(
        parsed * 10 > count * 8,
        "only {} of {} items parsed",
        parsed,
        count
    );
    // The test has something to say: a fair share of the items is accepted, and
    // a fair share is not.
    assert!(accepted * 8 > seen, "{} of {} accepted", accepted, seen);
    assert!(accepted * 2 < seen, "{} of {} accepted", accepted, seen);
}
