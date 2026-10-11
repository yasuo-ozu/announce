#!/usr/bin/env python3
"""Benchmark for announce: token counts and compile wall time.

Varies the definition size S (fields per struct / methods per trait) and the
number K of definitions referenced at once. See bench/README.md.

Standard library only. Generates one standalone cargo workspace (with its own
`[workspace]`, not part of the repository workspace) under OUT/ws and writes
results to OUT/results.{csv,json}; prints a Markdown table to stdout.
"""

import argparse
import datetime
import json
import os
import platform
import shutil
import statistics
import subprocess
import sys
import time
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent


def sh(cmd, cwd, env=None, capture=False, check=True, stdin=None):
    r = subprocess.run(
        cmd, cwd=cwd, env=env, check=False, text=True, input=stdin,
        stdout=subprocess.PIPE if capture else subprocess.DEVNULL,
        stderr=subprocess.PIPE,
    )
    if check and r.returncode != 0:
        sys.stderr.write(r.stderr[-6000:])
        raise SystemExit(f"command failed ({r.returncode}): {' '.join(cmd)}")
    return r


def write(path: Path, text: str):
    path.parent.mkdir(parents=True, exist_ok=True)
    if path.exists() and path.read_text() == text:
        return
    path.write_text(text)


def toml_pkg(name, deps="", extra=""):
    return (
        f'[package]\nname = "{name}"\nversion = "0.0.0"\nedition = "2021"\npublish = false\n'
        f"{extra}\n[dependencies]\n{deps}"
    )


# --------------------------------------------------------------------------
# generated sources

CB_LIB = r'''//! Benchmark callback: counts what it receives.
use proc_macro::TokenStream;
use proc_macro2::{TokenStream as TS, TokenTree};

/// Token trees, recursively; a group counts as 1 + its contents.
fn tt_count(ts: TS) -> usize {
    ts.into_iter()
        .map(|tt| match tt {
            TokenTree::Group(g) => 1 + tt_count(g.stream()),
            _ => 1,
        })
        .sum()
}

/// `count! { [count!] ENTRY* NAME }` emits
/// `pub mod NAME { ENTRIES, INPUT_TOKENS, ITEM_TOKENS }`.
#[proc_macro]
pub fn count(input: TokenStream) -> TokenStream {
    let input: TS = input.into();
    let input_tokens = tt_count(input.clone());
    let parsed: announce_helper::Input = match syn::parse2(input) {
        Ok(p) => p,
        Err(e) => return e.to_compile_error().into(),
    };
    let entries = parsed.entries.len();
    let item_tokens: usize = parsed.entries.iter().map(|e| tt_count(e.item.clone())).sum();
    let name = parsed.rest.to_string();
    format!(
        "pub mod {name} {{ pub const ENTRIES: usize = {entries}; \
         pub const INPUT_TOKENS: usize = {input_tokens}; \
         pub const ITEM_TOKENS: usize = {item_tokens}; }}"
    )
    .parse()
    .unwrap()
}
'''

TOKCOUNT_MAIN = r'''//! Reads Rust source on stdin, prints its token-tree count
//! (recursive; a group counts as 1 + its contents), via proc_macro2.
use proc_macro2::{TokenStream, TokenTree};
use std::io::Read;
use std::str::FromStr;

fn count(ts: TokenStream) -> usize {
    ts.into_iter()
        .map(|tt| match tt {
            TokenTree::Group(g) => 1 + count(g.stream()),
            _ => 1,
        })
        .sum()
}

fn main() {
    let mut s = String::new();
    std::io::stdin().read_to_string(&mut s).unwrap();
    match TokenStream::from_str(&s) {
        Ok(ts) => println!("{}", count(ts)),
        Err(e) => {
            eprintln!("lex error: {e:?}");
            std::process::exit(1)
        }
    }
}
'''


def definition(kind, i, s, announced):
    attr = "#[::announce::announce]\n" if announced else ""
    if kind == "struct":
        body = "".join(f"    pub f{j}: u32,\n" for j in range(s))
        return f"{attr}pub struct D{i} {{\n{body}}}\n"
    if kind == "trait":
        body = "".join(f"    fn m{j}(&self, x: u32) -> u32;\n" for j in range(s))
        return f"{attr}pub trait D{i} {{\n{body}}}\n"
    raise ValueError(kind)


def gen_definer(kind, k, s, announced):
    head = "#![allow(dead_code)]\n"
    return head + "\n".join(definition(kind, i, s, announced) for i in range(k))


def gen_user(def_crate, k, separate):
    out = [
        "// Trampoline chains hop once (or twice) per definition.",
        '#![recursion_limit = "1024"]',
        "",
    ]
    if separate:
        for i in range(k):
            out.append(f"::{def_crate}::D{i}! {{ [::{def_crate}::D{i}!, ::bench_cb::count!] c{i} }}")
        parts = lambda c: " + ".join(f"c{i}::{c}" for i in range(k))
        out.append(f"pub const ENTRIES: usize = {parts('ENTRIES')};")
        out.append(f"pub const INPUT_TOKENS: usize = {parts('INPUT_TOKENS')};")
        out.append(f"pub const ITEM_TOKENS: usize = {parts('ITEM_TOKENS')};")
    else:
        lst = ", ".join(f"::{def_crate}::D{i}!" for i in range(k))
        out.append(f"::{def_crate}::D0! {{ [{lst}, ::bench_cb::count!] all }}")
        out.append("pub use all::{ENTRIES, INPUT_TOKENS, ITEM_TOKENS};")
    out.append(f"const _: () = assert!(ENTRIES == {k});")
    return "\n".join(out) + "\n"


def bin_name(crate):
    return "p-" + crate.replace("_", "-")


def gen_print(crate):
    return (
        f"fn main() {{ println!(\"{{}} {{}} {{}}\", {crate}::ENTRIES, "
        f"{crate}::INPUT_TOKENS, {crate}::ITEM_TOKENS); }}\n"
    )


# --------------------------------------------------------------------------


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--sizes", default="1,10,100,1000", help="definition sizes S (fields/methods)")
    ap.add_argument("--counts", default="1,4,16,64", help="numbers K of definitions per chain")
    ap.add_argument("--kinds", default="struct,trait", help="comma list of: struct, trait")
    ap.add_argument("--runs", type=int, default=3, help="timed runs per measurement (median)")
    ap.add_argument("--quick", action="store_true", help="tiny grid: S in {1,10}, K in {1,4}, 1 run")
    ap.add_argument("--no-separate", action="store_true", help="skip the K-separate-calls variant")
    ap.add_argument("--out", default=str(REPO / "target" / "bench"), help="output directory")
    ap.add_argument("--keep", action="store_true", help="keep build output (default: delete it at the end)")
    a = ap.parse_args()
    if a.quick:
        a.sizes, a.counts, a.runs = "1,10", "1,4", 1
    sizes = [int(x) for x in a.sizes.split(",")]
    counts = [int(x) for x in a.counts.split(",")]
    kinds = a.kinds.split(",")
    separate = not a.no_separate

    out = Path(a.out).resolve()
    ws = out / "ws"
    tdir = out / "target"
    env = dict(os.environ, CARGO_TARGET_DIR=str(tdir), RUSTC_BOOTSTRAP="1", CARGO_INCREMENTAL="0")
    env.pop("RUSTFLAGS", None)
    cargo = ["cargo", "--offline"]
    shutil.rmtree(ws, ignore_errors=True)  # regenerate from scratch (keeps target/ for deps)

    # ---- generate the workspace
    points = [(kind, s, k) for kind in kinds for s in sizes for k in counts]
    members = ["bench_cb", "tokcount", "baseline"]
    crate_srcs = {}  # name -> lib.rs path, for touching

    def add(name, cargo_toml, files):
        members.append(name) if name not in members else None
        write(ws / name / "Cargo.toml", cargo_toml)
        for rel, text in files.items():
            write(ws / name / rel, text)
        crate_srcs[name] = ws / name / "src" / "lib.rs"

    announce = REPO / "announce"
    helper = REPO / "announce-helper"
    add("bench_cb", toml_pkg("bench_cb",
        f'announce-helper = {{ path = "{helper}" }}\nsyn = "2"\nproc-macro2 = "1"\n',
        "\n[lib]\nproc-macro = true\n"), {"src/lib.rs": CB_LIB})
    write(ws / "tokcount" / "Cargo.toml", toml_pkg("tokcount", 'proc-macro2 = "1"\n'))
    write(ws / "tokcount" / "src" / "main.rs", TOKCOUNT_MAIN)
    add("baseline", toml_pkg("baseline"), {"src/lib.rs": "pub const X: u32 = 1;\n"})

    for kind, s, k in points:
        tag = f"{kind}_s{s}_k{k}"
        add(f"def_{tag}", toml_pkg(f"def_{tag}", f'announce = {{ path = "{announce}" }}\n'),
            {"src/lib.rs": gen_definer(kind, k, s, True)})
        add(f"plain_{tag}", toml_pkg(f"plain_{tag}"), {"src/lib.rs": gen_definer(kind, k, s, False)})
        variants = [("chain", False)] + ([("sep", True)] if separate else [])
        for vname, sep in variants:
            name = f"use_{vname}_{tag}"
            add(name, toml_pkg(name, f'def_{tag} = {{ path = "../def_{tag}" }}\n'
                                     f'bench_cb = {{ path = "../bench_cb" }}\n'),
                {"src/lib.rs": gen_user(f"def_{tag}", k, sep), f"src/bin/{bin_name(name)}.rs": gen_print(name)})

    write(ws / "Cargo.toml",
          "[workspace]\nresolver = \"2\"\nmembers = [\n" + "".join(f'  "{m}",\n' for m in members) + "]\n")
    if not (ws / "Cargo.lock").exists() and (REPO / "Cargo.lock").exists():
        shutil.copy(REPO / "Cargo.lock", ws / "Cargo.lock")  # pin to versions known to be cached

    ver = sh(["rustc", "-vV"], ws, env, capture=True).stdout
    rustc = next(l for l in ver.splitlines() if l.startswith("rustc "))
    print(f"# announce bench  ({datetime.datetime.now():%Y-%m-%d %H:%M}, {rustc}, "
          f"{platform.machine()}, nproc={os.cpu_count()}, runs={a.runs})", file=sys.stderr)

    # ---- shared deps and tools
    t0 = time.time()
    sh(cargo + ["build", "-p", "tokcount", "-p", "bench_cb", "-p", "baseline"], ws, env)
    tokcount_bin = tdir / "debug" / "tokcount"

    def tokcount(text):
        r = sh([str(tokcount_bin)], ws, env, capture=True, check=False, stdin=text)
        return int(r.stdout) if r.returncode == 0 else None

    def timed_rebuild(pkg, extra=()):
        """Touch pkg's lib.rs and time `cargo build -p pkg --lib`; median of runs."""
        sh(cargo + ["build", "-p", pkg, *extra], ws, env)  # warm: deps and pkg built
        ts = []
        for _ in range(a.runs):
            now = time.time()
            os.utime(crate_srcs[pkg], (now, now))
            st = time.perf_counter()
            sh(cargo + ["build", "-p", pkg, "--lib"], ws, env)
            ts.append(time.perf_counter() - st)
        return statistics.median(ts) * 1000

    def expanded_tokens(pkg):
        r = sh(cargo + ["rustc", "-p", pkg, "--lib", "--profile=check", "--", "-Zunpretty=expanded"],
               ws, env, capture=True)
        return tokcount(r.stdout)

    base_ms = timed_rebuild("baseline")
    print(f"baseline (empty lib) rebuild: {base_ms:.0f} ms", file=sys.stderr)

    rows = []
    for kind, s, k in points:
        tag = f"{kind}_s{s}_k{k}"
        d, p = f"def_{tag}", f"plain_{tag}"
        row = dict(kind=kind, S=s, K=k)
        row["src_tokens"] = tokcount(crate_srcs[d].read_text())
        row["def_expanded_tokens"] = expanded_tokens(d)
        row["plain_expanded_tokens"] = expanded_tokens(p)
        row["plain_ms"] = timed_rebuild(p)
        row["def_ms"] = timed_rebuild(d)
        for vname in ["chain"] + (["sep"] if separate else []):
            u = f"use_{vname}_{tag}"
            row[f"{vname}_ms"] = timed_rebuild(u)
            sh(cargo + ["build", "-p", u, "--bin", bin_name(u)], ws, env)
            e, inp, item = map(int, sh([str(tdir / "debug" / bin_name(u))], ws, env,
                                       capture=True).stdout.split())
            assert e == k, (u, e)
            row[f"{vname}_input_tokens"] = inp
            row[f"{vname}_item_tokens"] = item
        rows.append(row)
        print(f"  {tag}: {row}", file=sys.stderr)

    # ---- report
    meta = dict(date=f"{datetime.date.today()}", rustc=rustc, machine=platform.machine(),
                nproc=os.cpu_count(), runs=a.runs, baseline_ms=round(base_ms, 1),
                elapsed_s=round(time.time() - t0, 1))
    out.mkdir(parents=True, exist_ok=True)
    (out / "results.json").write_text(json.dumps(dict(meta=meta, rows=rows), indent=1))
    keys = list(rows[0].keys()) if rows else []
    with open(out / "results.csv", "w") as f:
        f.write(",".join(keys) + "\n")
        for r in rows:
            f.write(",".join(str(round(r[x], 1) if isinstance(r[x], float) else r[x]) for x in keys) + "\n")

    cols = [("kind", "kind"), ("S", "S"), ("K", "K"), ("src_tokens", "src tok"),
            ("plain_expanded_tokens", "plain exp tok"), ("def_expanded_tokens", "announce exp tok"),
            ("chain_item_tokens", "cb item tok"), ("chain_input_tokens", "cb input tok (chain)")]
    if separate:
        cols.append(("sep_input_tokens", "cb input tok (K calls)"))
    cols += [("plain_ms", "plain def ms"), ("def_ms", "announce def ms"), ("chain_ms", "user chain ms")]
    if separate:
        cols.append(("sep_ms", "user K calls ms"))
    fmt = lambda v: f"{v:.0f}" if isinstance(v, float) else ("n/a" if v is None else str(v))
    print(f"Date {meta['date']}; {rustc}; {meta['machine']}, nproc={meta['nproc']}; "
          f"median of {a.runs} run(s); empty-crate rebuild baseline {base_ms:.0f} ms; "
          f"total {meta['elapsed_s']:.0f} s.\n")
    print("| " + " | ".join(c[1] for c in cols) + " |")
    print("|" + "|".join("---" if c[0] == "kind" else "---:" for c in cols) + "|")
    for r in rows:
        print("| " + " | ".join(fmt(r[c[0]]) for c in cols) + " |")
    print(f"\nresults: {out / 'results.csv'}, {out / 'results.json'}", file=sys.stderr)

    if not a.keep:
        shutil.rmtree(tdir, ignore_errors=True)


if __name__ == "__main__":
    main()
