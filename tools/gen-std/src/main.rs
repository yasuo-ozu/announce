//! Writes `announce-std/std-traits.txt`: the signatures of the std traits that
//! announce-std mocks, read from the rust-src of a toolchain.
//!
//! ```text
//! # write the file
//! cargo run --manifest-path tools/gen-std/Cargo.toml --target-dir target/gen-std
//! # compare, exit 1 on a difference
//! cargo run --manifest-path tools/gen-std/Cargo.toml --target-dir target/gen-std -- --check
//! ```
//!
//! Options: `--sysroot DIR` (default: `rustc +stable --print sysroot`), `--out FILE`
//! (default: `announce-std/std-traits.txt` of this repository).
//!
//! `--target-dir target/gen-std` keeps the build under the repository's ignored `target/`.
//!
//! `announce-std/build.rs` makes the mocks from the file, for the compiling rustc.
//! The lists of what to mock are in `config.rs`; everything else about an item (its
//! signature, whether it is provided, since which version it is stable) is read from
//! std. An item that is unstable or missing in std is an error.
//!
//! Where the source needs a rule: a trait is as new as its module (`core::error` has
//! `#![stable(since = "1.81.0")]`, so `core::error::Error` is 1.81); an item that
//! `#[rustc_must_implement_one_of(..)]` names (`io::Read::read`) is required in the mock
//! though std gives it a default; `PointeeSized` stands for `?Sized`, and `Destruct`
//! is dropped, in bounds. Paths come out absolute (`::core::option::Option`); a name
//! that cannot be resolved is an error, and `config::PUBLIC_PATHS` maps private paths
//! to public ones.

mod config;
mod data;
mod render;
mod resolve;
mod source;

use config::{Trait, DERIVE_NAMES, DOC_NOTES, GLOB_SINCE, TRAITS, VARIANTS};
use data::{Item, Record};
use source::{FileInfo, Library, Res, TraitSrc};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::{self, Command};
use syn::visit_mut::VisitMut;

fn main() {
    match run() {
        Ok(code) => process::exit(code),
        Err(e) => {
            eprintln!("gen-std: {}", e);
            process::exit(2);
        }
    }
}

struct Args {
    sysroot: Option<PathBuf>,
    out: PathBuf,
    check: bool,
}

/// The repository: `tools/gen-std/../..`.
fn repo_root() -> PathBuf {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    manifest
        .parent()
        .and_then(Path::parent)
        .unwrap_or(manifest)
        .to_path_buf()
}

fn parse_args() -> Res<Args> {
    let mut args = Args {
        sysroot: None,
        out: repo_root().join("announce-std").join("std-traits.txt"),
        check: false,
    };
    let mut it = std::env::args().skip(1);
    while let Some(a) = it.next() {
        match a.as_str() {
            "--check" => args.check = true,
            "--sysroot" => {
                args.sysroot = Some(it.next().ok_or("--sysroot needs a directory")?.into())
            }
            "--out" => args.out = it.next().ok_or("--out needs a file")?.into(),
            "-h" | "--help" => {
                println!(
                    "usage: gen-std [--sysroot DIR] [--out FILE] [--check]\n\
                     Writes announce-std/std-traits.txt from the rust-src of a toolchain\n\
                     (default: the stable one). --check compares instead and exits 1 on a difference."
                );
                process::exit(0);
            }
            other => return Err(format!("unknown argument `{}` (try --help)", other)),
        }
    }
    Ok(args)
}

fn sysroot_of(cmd: &[&str]) -> Option<PathBuf> {
    let out = Command::new("rustc").args(cmd).output().ok()?;
    if !out.status.success() {
        return None;
    }
    let s = String::from_utf8(out.stdout).ok()?;
    let s = s.trim();
    if s.is_empty() {
        None
    } else {
        Some(PathBuf::from(s))
    }
}

/// `rustc 1.99.1 (58cf8f9c3 2026-10-13)` of the toolchain, and its minor version (99).
fn rustc_version(sysroot: &Path) -> Res<(String, u32)> {
    let rustc = sysroot.join("bin").join("rustc");
    let out = Command::new(&rustc)
        .arg("--version")
        .output()
        .map_err(|e| format!("cannot run {} --version: {}", rustc.display(), e))?;
    let line = String::from_utf8_lossy(&out.stdout).trim().to_string();
    let minor = line
        .strip_prefix("rustc 1.")
        .and_then(|r| r.split(|c: char| !c.is_ascii_digit()).next())
        .and_then(|n| n.parse::<u32>().ok())
        .ok_or_else(|| format!("cannot read the rustc version from `{}`", line))?;
    Ok((line, minor))
}

fn make_record(
    t: &Trait,
    info: &FileInfo,
    src: &TraitSrc,
    variant: Option<&config::Variant>,
) -> Res<Record> {
    let mut res = resolve::Resolver::new(info, variant.map(|v| (v.from, v.to)));
    let mut head = src.head.clone();
    res.with_generics(&head.generics.clone(), |r| {
        r.visit_item_trait_mut(&mut head)
    });
    resolve::fix_supertraits(&mut head);
    resolve::BoundFix.visit_item_trait_mut(&mut head);

    let mut items = Vec::new();
    for it in &src.items {
        let mut item = it.item.clone();
        res.with_generics(&src.head.generics, |r| match &mut item {
            syn::TraitItem::Fn(f) => {
                let g = f.sig.generics.clone();
                r.with_generics(&g, |r| r.visit_trait_item_fn_mut(f))
            }
            syn::TraitItem::Type(ty) => {
                let g = ty.generics.clone();
                r.with_generics(&g, |r| r.visit_trait_item_type_mut(ty))
            }
            _ => {}
        });
        resolve::BoundFix.visit_trait_item_mut(&mut item);
        clean_item(&mut item);
        items.push(Item {
            since: it.since,
            provided: it.provided,
            text: render::render_item(&item)?,
        });
    }
    if !res.errors.is_empty() {
        res.errors.sort();
        res.errors.dedup();
        return Err(format!("{}: {}", t.name, res.errors.join("; ")));
    }

    let path = match variant {
        Some(v) => v.to.to_string(),
        None => t.path.to_string(),
    };
    let derive = DERIVE_NAMES.contains(&t.name);
    let head_text = render::render_head(&head)?;
    let fallback = if derive {
        Some(render::render_fallback(&head, &path)?)
    } else {
        None
    };
    let std = path.starts_with("::std::")
        || head_text.contains("::std::")
        || items.iter().any(|i| i.text.contains("::std::"));
    let notes = DOC_NOTES
        .iter()
        .filter(|(n, _)| *n == t.name && !derive)
        .flat_map(|(_, text)| text.split('\n').map(str::to_string))
        .collect();
    Ok(Record {
        module: t.module.to_string(),
        name: t.name.to_string(),
        path,
        since: variant.map_or(src.since, |v| v.since),
        until: variant.map(|_| src.since),
        std,
        derive,
        unsafe_: src.head.unsafety.is_some(),
        head: head_text,
        notes,
        fallback,
        items,
    })
}

/// Drops what the mock does not keep: attributes, patterns that are `mut x`.
fn clean_item(item: &mut syn::TraitItem) {
    match item {
        syn::TraitItem::Fn(f) => {
            f.attrs.clear();
            f.default = None;
            f.semi_token = Some(Default::default());
            for arg in f.sig.inputs.iter_mut() {
                match arg {
                    syn::FnArg::Receiver(r) => {
                        r.attrs.clear();
                        if r.reference.is_none() {
                            r.mutability = None;
                        }
                    }
                    syn::FnArg::Typed(pt) => {
                        pt.attrs.clear();
                        if let syn::Pat::Ident(pi) = &mut *pt.pat {
                            pi.attrs.clear();
                            pi.by_ref = None;
                            pi.mutability = None;
                        }
                    }
                }
            }
        }
        syn::TraitItem::Type(t) => t.attrs.clear(),
        _ => {}
    }
}

fn generate(lib: &Library, current: u32) -> Res<Vec<Record>> {
    let mut files: BTreeMap<String, FileInfo> = BTreeMap::new();
    let mut records = Vec::new();
    for t in TRAITS {
        let (rel, text) = lib
            .find_trait(t.name, t.src)
            .map_err(|e| format!("{}: {}", t.name, e))?;
        if !files.contains_key(rel) {
            files.insert(rel.clone(), source::read_file(rel, text, current)?);
        }
        let info = &files[rel];
        let src = source::cut_trait(info, t.name, t.items, current)
            .map_err(|e| format!("{} ({}): {}", t.name, rel, e))?;
        records.push(
            make_record(t, info, &src, None).map_err(|e| format!("{} ({}): {}", t.name, rel, e))?,
        );
        for v in VARIANTS.iter().filter(|v| v.from == t.path) {
            records.push(
                make_record(t, info, &src, Some(v))
                    .map_err(|e| format!("{} ({}): {}", t.name, rel, e))?,
            );
        }
    }
    Ok(records)
}

/// A short line diff (the data is a few hundred lines).
fn diff(old: &str, new: &str, limit: usize) -> Vec<String> {
    let a: Vec<&str> = old.lines().collect();
    let b: Vec<&str> = new.lines().collect();
    let mut lcs = vec![vec![0usize; b.len() + 1]; a.len() + 1];
    for i in (0..a.len()).rev() {
        for j in (0..b.len()).rev() {
            lcs[i][j] = if a[i] == b[j] {
                lcs[i + 1][j + 1] + 1
            } else {
                lcs[i + 1][j].max(lcs[i][j + 1])
            };
        }
    }
    let (mut i, mut j) = (0, 0);
    let mut out = Vec::new();
    while i < a.len() || j < b.len() {
        if i < a.len() && j < b.len() && a[i] == b[j] {
            i += 1;
            j += 1;
        } else if i < a.len() && (j == b.len() || lcs[i + 1][j] >= lcs[i][j + 1]) {
            out.push(format!("- {}", a[i]));
            i += 1;
        } else {
            out.push(format!("+ {}", b[j]));
            j += 1;
        }
    }
    if out.len() > limit {
        let more = out.len() - limit;
        out.truncate(limit);
        out.push(format!("... {} more lines", more));
    }
    out
}

/// The lines that are not comments: what the data is. The header comment names the
/// rustc version, which may differ without the data differing.
fn body(s: &str) -> String {
    s.lines()
        .filter(|l| !l.starts_with('#'))
        .collect::<Vec<_>>()
        .join("\n")
}

fn run() -> Res<i32> {
    let args = parse_args()?;
    let sysroot = match &args.sysroot {
        Some(s) => s.clone(),
        None => sysroot_of(&["+stable", "--print", "sysroot"])
            .or_else(|| sysroot_of(&["--print", "sysroot"]))
            .ok_or("cannot find a sysroot: `rustc +stable --print sysroot` and `rustc --print sysroot` failed (use --sysroot DIR)")?,
    };
    let lib = Library::open(&sysroot)?;
    let (version, current) = rustc_version(&sysroot)?;
    let records = generate(&lib, current)?;
    let text = data::write(&records, &version, GLOB_SINCE);
    let out = &args.out;
    if args.check {
        let old = std::fs::read_to_string(out)
            .map_err(|e| format!("cannot read {}: {}", out.display(), e))?;
        if body(&old) == body(&text) {
            let header_same = old == text;
            println!(
                "gen-std --check: {} matches ({} records from {}{})",
                out.display(),
                records.len(),
                version,
                if header_same {
                    ""
                } else {
                    "; only the header comment differs"
                }
            );
            return Ok(0);
        }
        println!(
            "gen-std --check: {} differs from the data read from {}:",
            out.display(),
            version
        );
        for l in diff(&body(&old), &body(&text), 40) {
            println!("{}", l);
        }
        return Ok(1);
    }
    std::fs::write(out, &text).map_err(|e| format!("cannot write {}: {}", out.display(), e))?;
    println!(
        "gen-std: wrote {} records ({} items) to {} from {}",
        records.len(),
        records.iter().map(|r| r.items.len()).sum::<usize>(),
        out.display(),
        version
    );
    Ok(0)
}
