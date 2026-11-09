// Makes the mocks of announce-std for the compiling rustc.
//
// `std-traits.txt` has the signatures of the std traits, read from std's own source by
// `tools/gen-std`. To refresh it, run
// `cargo run --manifest-path tools/gen-std/Cargo.toml --target-dir target/gen-std`.
// Each record and each item in it says from which rustc version it exists. This
// script keeps what the compiling rustc has and writes `OUT_DIR/mocks.rs`, which
// `src/lib.rs` includes. So the mocks need no `cfg` for a compiler version: a trait or a
// method that the compiler does not have is not in the mock.
//
// The script reads the file with std only, and must build on Rust 1.57 (the MSRV): no
// `syn`, no inline format arguments (`{x}`), no let-else.
//
// The data format, one record per mock:
//
//   trait cmp PartialEq              module and name; starts a record
//   path ::core::cmp::PartialEq      the mocked path
//   since 1.0                        the record exists from this rustc minor version
//   until 1.81                       (optional) and before this one
//   std no                           yes: needs feature = "std"
//   derive yes                       the trait name is also a derive macro
//   unsafe no
//   head <Rhs = Self>                generics, supertraits and where clause
//   note ...                         (optional, many) a line of a doc paragraph
//   fallback-trait ...               (derive-named) the blanket subtrait that stands in
//   fallback-impl ...                  for the real trait before the glob re-export
//   item 1.0 required fn eq(&self, other: &Rhs) -> ::core::primitive::bool
//   item 1.0 provided fn ne(&self, other: &Rhs) -> ::core::primitive::bool
//
// A line that starts with `#` is a comment. `glob-since 1.94` (once) is the first rustc
// where the real derive-named traits can be glob re-exported.

use std::env;
use std::fmt::Write as _;
use std::fs;
use std::path::PathBuf;
use std::process::Command;

struct Item {
    since: u32,
    provided: bool,
    text: String,
}

#[derive(Default)]
struct Record {
    module: String,
    name: String,
    path: String,
    since: u32,
    until: Option<u32>,
    std: bool,
    derive: bool,
    unsafe_: bool,
    head: String,
    notes: Vec<String>,
    fallback_trait: Option<String>,
    fallback_impl: Option<String>,
    items: Vec<Item>,
}

fn bad(line_no: usize, line: &str, why: &str) -> ! {
    panic!(
        "announce-std: std-traits.txt:{}: {}: `{}`",
        line_no + 1,
        why,
        line
    )
}

/// `1.61` -> 61
fn minor(value: &str, line_no: usize, line: &str) -> u32 {
    match value.strip_prefix("1.").map(str::parse::<u32>) {
        Some(Ok(n)) => n,
        _ => bad(line_no, line, "expected a version like 1.61"),
    }
}

fn yes_no(value: &str, line_no: usize, line: &str) -> bool {
    match value {
        "yes" => true,
        "no" => false,
        _ => bad(line_no, line, "expected yes or no"),
    }
}

/// The `glob-since` minor version and the records of the data file.
fn parse(data: &str) -> (u32, Vec<Record>) {
    let mut glob_since = None;
    let mut records: Vec<Record> = Vec::new();
    for (n, line) in data.lines().enumerate() {
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let (key, value) = match line.find(' ') {
            Some(i) => (&line[..i], &line[i + 1..]),
            None => (line, ""),
        };
        if key == "glob-since" {
            glob_since = Some(minor(value, n, line));
            continue;
        }
        if key == "trait" {
            let (module, name) = match value.split_once(' ') {
                Some(p) => p,
                None => bad(n, line, "expected `trait MODULE NAME`"),
            };
            records.push(Record {
                module: module.to_string(),
                name: name.to_string(),
                ..Record::default()
            });
            continue;
        }
        let r = match records.last_mut() {
            Some(r) => r,
            None => bad(n, line, "a line before the first `trait`"),
        };
        match key {
            "path" => r.path = value.to_string(),
            "since" => r.since = minor(value, n, line),
            "until" => r.until = Some(minor(value, n, line)),
            "std" => r.std = yes_no(value, n, line),
            "derive" => r.derive = yes_no(value, n, line),
            "unsafe" => r.unsafe_ = yes_no(value, n, line),
            "head" => r.head = value.to_string(),
            "note" => r.notes.push(value.to_string()),
            "fallback-trait" => r.fallback_trait = Some(value.to_string()),
            "fallback-impl" => r.fallback_impl = Some(value.to_string()),
            "item" => {
                let mut parts = value.splitn(3, ' ');
                let since = minor(parts.next().unwrap_or(""), n, line);
                let provided = match parts.next() {
                    Some("provided") => true,
                    Some("required") => false,
                    _ => bad(n, line, "expected `required` or `provided`"),
                };
                let text = match parts.next() {
                    Some(t) if !t.is_empty() => t.to_string(),
                    _ => bad(n, line, "an item without text"),
                };
                if provided && !(text.starts_with("fn ") || text.starts_with("unsafe fn ")) {
                    bad(n, line, "only a method can be provided");
                }
                r.items.push(Item {
                    since,
                    provided,
                    text,
                });
            }
            _ => bad(n, line, "unknown key"),
        }
    }
    for r in &records {
        if r.path.is_empty() {
            panic!("announce-std: std-traits.txt: {} has no path", r.name);
        }
    }
    match glob_since {
        Some(g) => (g, records),
        None => panic!("announce-std: std-traits.txt has no `glob-since`"),
    }
}

/// The records that rustc 1.`minor` has.
fn live(records: &[Record], minor: u32) -> impl Iterator<Item = &Record> {
    records
        .iter()
        .filter(move |r| r.since <= minor && r.until.map_or(true, |u| minor < u))
}

/// The items of a record that rustc 1.`minor` has.
fn live_items(r: &Record, minor: u32) -> impl Iterator<Item = &Item> {
    r.items.iter().filter(move |i| i.since <= minor)
}

/// The mocks (Rust source) for rustc 1.`minor`.
pub fn generate(data: &str, minor: u32) -> String {
    let (glob_since, records) = parse(data);
    let mut modules: Vec<&str> = Vec::new();
    for r in &records {
        if !modules.contains(&r.module.as_str()) {
            modules.push(&r.module);
        }
    }
    let mut out = String::new();
    let _ = writeln!(
        out,
        "// Made by announce-std/build.rs from std-traits.txt for rustc 1.{}.\n",
        minor
    );
    for module in modules {
        // A module whose records all need std is cfg'd as a whole.
        let std_only = records.iter().filter(|r| r.module == module).all(|r| r.std);
        let _ = writeln!(
            out,
            "#[doc = {:?}]",
            format!(
                "announce mocks of `{}` traits, made at build time from std-traits.txt.",
                module
            )
        );
        if std_only {
            out.push_str("#[cfg(feature = \"std\")]\n");
        }
        let _ = writeln!(out, "pub mod {} {{", module);
        out.push_str("    #[allow(unused_imports)]\n    use announce::announce;\n");
        let mut derived: Vec<&Record> = Vec::new();
        for r in live(&records, minor).filter(|r| r.module == module) {
            let cfg = if r.std && !std_only {
                "#[cfg(feature = \"std\")]\n"
            } else {
                ""
            };
            out.push('\n');
            out.push_str(cfg);
            let _ = writeln!(
                out,
                "/// announce mock of [`{}`]: invoke `{}!` to receive its definition.",
                r.path, r.name
            );
            out.push_str("///\n");
            if r.derive {
                let _ = writeln!(
                    out,
                    "/// The real trait is available under the same path in the type namespace\n\
                     /// (from rustc 1.{}, through a glob re-export, since the macro namespace holds this\n\
                     /// trampoline).",
                    glob_since
                );
            } else {
                out.push_str("/// The real trait is re-exported under the same path.\n");
                if !r.notes.is_empty() {
                    out.push_str("///\n");
                    for note in &r.notes {
                        let _ = writeln!(out, "/// {}", note);
                    }
                }
            }
            let _ = writeln!(out, "#[announce(mock = {})]", r.path);
            let _ = writeln!(
                out,
                "pub {}trait {}{}{} {{",
                if r.unsafe_ { "unsafe " } else { "" },
                r.name,
                if r.head.starts_with("where") { " " } else { "" },
                r.head
            );
            for item in live_items(r, minor) {
                if item.provided {
                    let _ = writeln!(out, "    {} {{ ::core::unimplemented!() }}", item.text);
                } else {
                    let _ = writeln!(out, "    {};", item.text);
                }
            }
            out.push_str("}\n");
            if !r.derive {
                out.push_str(cfg);
                let _ = writeln!(out, "#[doc(no_inline)]\npub use {};", r.path);
            } else if minor < glob_since {
                // The glob re-export of the real trait does not work yet: a blanket subtrait
                // with the same name stands in for it in bounds.
                let (t, i) = match (&r.fallback_trait, &r.fallback_impl) {
                    (Some(t), Some(i)) => (t, i),
                    _ => panic!("announce-std: std-traits.txt: {} has no fallback", r.name),
                };
                out.push('\n');
                out.push_str(cfg);
                let _ = writeln!(
                    out,
                    "/// Fallback for rustc < 1.{}: a blanket subtrait of [`{}`], usable in bounds.\n\
                     pub trait {}{} {{}}",
                    glob_since, r.path, r.name, t
                );
                out.push_str(cfg);
                let _ = writeln!(out, "impl{} {{}}", i);
            } else {
                derived.push(r);
            }
        }
        if !derived.is_empty() {
            out.push_str(
                "\n    // Real traits whose name is also a derive macro: explicit names shadow glob\n",
            );
            out.push_str("    // imports per namespace, so this glob supplies the *trait* while the announce\n");
            out.push_str("    // trampoline above keeps the *macro* namespace. Before rustc 1.");
            let _ = writeln!(out, "{} the glob", glob_since);
            let _ = writeln!(
                out,
                "    // breaks the trampoline's resolution, so it is only emitted from 1.{} on.",
                glob_since
            );
            out.push_str("    #[doc(hidden)]\n    pub mod __real {\n");
            for r in derived {
                if r.std && !std_only {
                    out.push_str("        #[cfg(feature = \"std\")]\n");
                }
                let _ = writeln!(out, "        pub use {};", r.path);
            }
            out.push_str("    }\n    #[doc(no_inline)]\n    pub use self::__real::*;\n");
        }
        out.push_str("}\n\n");
    }
    out
}

/// `rustc 1.99.1 (58cf8f9c3 2026-10-13)`, `rustc 1.101.0-nightly (..)` -> 99, 101
fn rustc_minor(version: &str) -> Option<u32> {
    let rest = version.trim().strip_prefix("rustc 1.")?;
    let digits: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
    digits.parse().ok()
}

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=std-traits.txt");
    let manifest = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
    let data = fs::read_to_string(manifest.join("std-traits.txt"))
        .expect("announce-std: cannot read std-traits.txt");
    let rustc = env::var("RUSTC").unwrap_or_else(|_| "rustc".to_string());
    let output = Command::new(&rustc)
        .arg("--version")
        .output()
        .unwrap_or_else(|e| panic!("announce-std: cannot run `{} --version`: {}", rustc, e));
    let version = String::from_utf8_lossy(&output.stdout).into_owned();
    let minor = rustc_minor(&version).unwrap_or_else(|| {
        panic!(
            "announce-std: cannot read the rustc version from `{}` (expected `rustc 1.N.P ..`)",
            version.trim()
        )
    });
    let out = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR")).join("mocks.rs");
    fs::write(&out, generate(&data, minor))
        .unwrap_or_else(|e| panic!("announce-std: cannot write {}: {}", out.display(), e));
}
