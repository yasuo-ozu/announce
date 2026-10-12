//! Sources of the generated crates (see bench/README.md).

use std::fmt::Write;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Kind {
    Struct,
    Trait,
}

impl Kind {
    pub fn name(self) -> &'static str {
        match self {
            Kind::Struct => "struct",
            Kind::Trait => "trait",
        }
    }
}

/// One grid point: K definitions of size S.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Point {
    pub kind: Kind,
    pub s: usize,
    pub k: usize,
}

impl Point {
    /// `s10_k4`: the benchmark parameter and the suffix of the crate names.
    pub fn param(&self) -> String {
        format!("s{}_k{}", self.s, self.k)
    }
    fn tag(&self) -> String {
        format!("{}_{}", self.kind.name(), self.param())
    }
    /// The definer without `#[announce]`.
    pub fn plain(&self) -> String {
        format!("plain_{}", self.tag())
    }
    /// The definer with `#[announce]`.
    pub fn definer(&self) -> String {
        format!("def_{}", self.tag())
    }
    /// The user crate with one chain over all K definitions.
    pub fn chain(&self) -> String {
        format!("use_chain_{}", self.tag())
    }
    /// The user crate with K separate calls.
    pub fn calls(&self) -> String {
        format!("use_calls_{}", self.tag())
    }
}

pub fn bin_name(krate: &str) -> String {
    format!("p-{}", krate.replace('_', "-"))
}

pub fn cargo_toml(name: &str, deps: &str, extra: &str) -> String {
    format!(
        "[package]\nname = \"{name}\"\nversion = \"0.0.0\"\nedition = \"2021\"\npublish = false\n\
         {extra}\n[dependencies]\n{deps}"
    )
}

/// The attribute of an announced definition. `ANNOUNCE_BENCH_LAYOUT=single|direct|auto` adds
/// `layout = ..` (DESIGN.md section 4.1); by default the attribute has no options.
fn announce_attr() -> String {
    match std::env::var("ANNOUNCE_BENCH_LAYOUT") {
        Ok(l) if !l.is_empty() => format!("#[::announce::announce(layout = {l})]\n"),
        _ => "#[::announce::announce]\n".to_string(),
    }
}

fn definition(out: &mut String, kind: Kind, i: usize, s: usize, announced: bool) {
    let attr = if announced {
        announce_attr()
    } else {
        String::new()
    };
    match kind {
        Kind::Struct => {
            write!(out, "{attr}pub struct D{i} {{\n").unwrap();
            for j in 0..s {
                write!(out, "    pub f{j}: u32,\n").unwrap();
            }
        }
        Kind::Trait => {
            write!(out, "{attr}pub trait D{i} {{\n").unwrap();
            for j in 0..s {
                write!(out, "    fn m{j}(&self, x: u32) -> u32;\n").unwrap();
            }
        }
    }
    out.push_str("}\n");
}

/// K definitions of size S, with or without `#[announce]`.
pub fn definer(p: Point, announced: bool) -> String {
    let mut out = String::from("#![allow(dead_code)]\n");
    for i in 0..p.k {
        if i > 0 {
            out.push('\n');
        }
        definition(&mut out, p.kind, i, p.s, announced);
    }
    out
}

/// The callback: consumes the whole payload (parsed as `announce_helper::Input`)
/// and emits a trivial item, so the user crate's size is the protocol's cost
/// plus a constant.
pub const CALLBACK_LIB: &str = r#"//! Benchmark callback: consumes what it receives, emits a trivial item.
use proc_macro::TokenStream;

/// `count! { [count!] ENTRY* NAME }` parses its input (the whole chain
/// payload) and emits `pub mod NAME { pub const ENTRIES: usize = N; }`.
#[proc_macro]
pub fn count(input: TokenStream) -> TokenStream {
    let input: proc_macro2::TokenStream = input.into();
    let parsed: announce_helper::Input = match syn::parse2(input) {
        Ok(p) => p,
        Err(e) => return e.to_compile_error().into(),
    };
    let entries = parsed.entries.len();
    let name = parsed.rest.to_string();
    format!("pub mod {name} {{ pub const ENTRIES: usize = {entries}; }}")
        .parse()
        .unwrap()
}
"#;

/// The user crate: one chain over all K definitions, or K separate calls.
pub fn user(def_crate: &str, k: usize, separate: bool) -> String {
    let mut out = String::new();
    if k > 16 {
        // A chain over K definitions nests about K macro expansions with the direct layout of a
        // small item, and 2 to 3 per definition with the single-copy layout of a large one
        // (DESIGN.md section 4.1); rustc's default recursion limit is 128.
        out.push_str("#![recursion_limit = \"1024\"]\n\n");
    }
    if separate {
        for i in 0..k {
            writeln!(
                out,
                "::{def_crate}::D{i}! {{ [::{def_crate}::D{i}!, ::bench_cb::count!] c{i} }}"
            )
            .unwrap();
        }
        let sum: Vec<String> = (0..k).map(|i| format!("c{i}::ENTRIES")).collect();
        writeln!(out, "pub const ENTRIES: usize = {};", sum.join(" + ")).unwrap();
    } else {
        let list: Vec<String> = (0..k).map(|i| format!("::{def_crate}::D{i}!")).collect();
        writeln!(
            out,
            "::{def_crate}::D0! {{ [{}, ::bench_cb::count!] all }}",
            list.join(", ")
        )
        .unwrap();
        out.push_str("pub use all::ENTRIES;\n");
    }
    writeln!(out, "const _: () = assert!(ENTRIES == {k});").unwrap();
    out
}

/// A tiny executable using the user crate and the definer, so both are linked.
/// Prints `ENTRIES` and the size of the first definition (a struct or a `dyn` reference).
pub fn user_bin(kind: Kind, krate: &str, def_crate: &str) -> String {
    let ty = match kind {
        Kind::Struct => format!("{def_crate}::D0"),
        Kind::Trait => format!("&dyn {def_crate}::D0"),
    };
    format!(
        "fn main() {{ println!(\"{{}} {{}}\", {krate}::ENTRIES, std::mem::size_of::<{ty}>()); }}\n"
    )
}
