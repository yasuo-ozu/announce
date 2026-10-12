#!/usr/bin/env python3
"""Regenerates announce-std/src/*.rs (except lib.rs and the hand-written modules)
from the trait copies of newer-type-std (only the trait text: no version gate is
taken from newer-type), then formats them with `rustfmt --edition 2021`.

Usage:
  python3 tools/gen_std.py                  write announce-std/src (the default)
  python3 tools/gen_std.py --out DIR        write the generated modules into DIR instead
  python3 tools/gen_std.py --check          generate into a temp dir and compare with
                                            announce-std/src; print a short diff and exit 1
                                            on any difference (exit 2: cannot run)
  python3 tools/gen_std.py --newer-type DIR use DIR/std/src instead of
                                            ~/ghq/github.com/yasuo-ozu/newer-type/std/src

`--check` and `--out DIR` can be combined (the generated files are kept in DIR).

The generator output is meant to reproduce the checked-in sources byte for byte:

* Version gates. No gate is derived from newer-type: its `#[rustversion::..]`
  attributes are stripped when a source file is read, so its version-gated items are
  plain items, and its compiler version never decides what announce-std gates.
  Where newer-type has `since`/`before` alternatives of one trait (the same trait
  name in several `emit_traits!` blocks of a module), the first definition wins
  (newer-type lists the newer, `since` form first; the others are discarded).
  Every `#[cfg(announce_std_rust_1_N)]` in the output comes from announce's own
  tables here: GATES (trait or trait::method -> first rustc) with OLDER_PATH (how a
  gated trait looks before its gate), plus GLOB_SINCE. announce-std/build.rs is the
  one place that lists the compiler versions (`gates![1.61, 1.64, ..]`, setting
  `announce_std_rust_1_N` cfgs). This script reads that list and fails if GATES or
  GLOB_SINCE, or any generated or checked-in source, uses a version outside it.
* Hand edits live in the tables below, not in the sources: PROVIDED (provided
  methods get the placeholder body `{ ::core::unimplemented!() }`), FALLBACK
  (blanket subtraits for rustc < 1.94), EXTRA_CFG, EXTRA_ITEMS (items newer-type
  has no `emit_traits!` for) and HAND_WRITTEN (modules that are not generated).
* Formatting: the output is run through `rustfmt --edition 2021` with an empty
  config (no rustfmt.toml is picked up), so the result depends on the rustfmt
  version only through its default style.

`--check` also verifies announce-std/src: no unexpected `.rs` or dot files (the old
`.mods`/`.versions` helper files are no longer written; build.rs now holds the
version list and lib.rs the module list), `lib.rs` declares every module, and every
`announce_std_rust_1_N` used anywhere in src/ is listed in build.rs.
"""
import argparse
import difflib
import os
import re
import shutil
import subprocess
import sys
import tempfile

ROOT = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..")
CRATE = os.path.normpath(os.path.join(ROOT, "announce-std"))
DST = os.path.join(CRATE, "src")
BUILD_RS = os.path.join(CRATE, "build.rs")
NEWER_TYPE = os.path.expanduser("~/ghq/github.com/yasuo-ozu/newer-type")

# announce's own version gates (the only source of `announce_std_rust_1_N` cfgs besides
# GLOB_SINCE; every N must be listed in announce-std/build.rs). Key "Trait": the mock
# exists only from rustc 1.N, so it is emitted under `#[cfg(announce_std_rust_1_N)]`
# and omitted before that (unless OLDER_PATH gives an older form). Key
# "Trait::method": the method exists only from rustc 1.N, so the trait is emitted
# twice, the full copy under `cfg(announce_std_rust_1_N)` and one without that method
# under `cfg(not(..))`. None of this follows newer-type's own `rustversion` attributes.
GATES = {
    "Termination": 61,
    "IntoFuture": 64,
    "Seek::seek_relative": 80,
    "Error": 81,
}
# Older form of a trait gated by GATES["Trait"]: a second copy under `cfg(not(gate))`
# whose mocked path and body have the first prefix replaced by the second.
# `core::error` is missing before rustc 1.81, so `Error` is `::std::error::Error` there.
OLDER_PATH = {"Error": ("::core::error::", "::std::error::")}
# Extra `#[cfg]` line, after the version gates, by the mocked path. The
# `::std::error::Error` copy is the rustc < 1.81 variant: `core::error` is missing
# there, so it needs the std feature.
EXTRA_CFG = {"::std::error::Error": '#[cfg(feature = "std")]'}
# rustc >= this: the real derive-named traits are glob re-exported (see `__real`).
# The other announce gate besides GATES; also listed in announce-std/build.rs.
GLOB_SINCE = 94
# Modules that are written by hand and never touched here.
HAND_WRITTEN = ["any", "clone", "default", "marker"]
DERIVE_NAMES = {"Debug", "Clone", "Copy", "Default", "Hash", "PartialEq", "Eq", "PartialOrd", "Ord"}
# Provided (default) methods of each trait. In the mocks they get a
# placeholder body, so consumers can tell them from required methods
# (`TraitItemFn::default.is_some()`); the body itself is never meaningful.
PROVIDED = {
    "PartialEq": {"ne"}, "PartialOrd": {"lt", "le", "gt", "ge"},
    "Write": {"write_char", "write_fmt", "write_all"},
    "Hasher": {"write_u8", "write_u16", "write_u32", "write_u64", "write_u128", "write_usize",
               "write_i8", "write_i16", "write_i32", "write_i64", "write_i128", "write_isize"},
    "BuildHasher": {"hash_one"}, "Seek": {"rewind", "stream_position", "seek_relative"},
    "Iterator": {"size_hint", "count", "last", "nth"}, "ExactSizeIterator": {"len"},
    "DoubleEndedIterator": {"nth_back", "rfold", "rfind"}, "GlobalAlloc": {"alloc_zeroed", "realloc"},
    "ToOwned": {"clone_into"}, "Error": {"source"}, "Wake": {"wake_by_ref"},
}

# rustc < GLOB_SINCE: the glob re-export cannot be used, so the type namespace gets a
# blanket subtrait with the same name. It works in bounds (`T: fmt::Debug`),
# but it is not the real trait (it cannot be implemented directly).
FALLBACK = {
    "Debug": ("", "::core::fmt::Debug", ""),
    "Hash": ("", "::core::hash::Hash", ""),
    "PartialEq": ("<Rhs: ?::core::marker::Sized = Self>", "::core::cmp::PartialEq<Rhs>", "<Rhs: ?::core::marker::Sized>"),
    "Eq": ("", "::core::cmp::Eq", ""),
    "PartialOrd": ("<Rhs: ?::core::marker::Sized = Self>", "::core::cmp::PartialOrd<Rhs>", "<Rhs: ?::core::marker::Sized>"),
    "Ord": ("", "::core::cmp::Ord", ""),
}

# Items with no `emit_traits!` in newer-type-std, appended after the generated
# items of a module (before the `__real` glob block). `GlobalAlloc` is an
# `unsafe trait`; `ToOwned` comes from a `macro_rules!` in newer-type.
EXTRA_ITEMS = {
    "alloc": ['/// announce mock of [`::core::alloc::GlobalAlloc`].\n///\n/// The real trait is re-exported under the same path.\n#[announce(mock = ::core::alloc::GlobalAlloc)]\npub unsafe trait GlobalAlloc {\n    unsafe fn alloc(&self, layout: ::core::alloc::Layout) -> *mut ::core::primitive::u8;\n    unsafe fn dealloc(&self, ptr: *mut ::core::primitive::u8, layout: ::core::alloc::Layout);\n    unsafe fn alloc_zeroed(&self, layout: ::core::alloc::Layout) -> *mut ::core::primitive::u8 { ::core::unimplemented!() }\n    unsafe fn realloc(&self, ptr: *mut ::core::primitive::u8, layout: ::core::alloc::Layout, new_size: ::core::primitive::usize) -> *mut ::core::primitive::u8 { ::core::unimplemented!() }\n}\n#[doc(no_inline)]\npub use ::core::alloc::GlobalAlloc;\n'],
    "borrow": ['#[cfg(feature = "std")]\n/// announce mock of [`::std::borrow::ToOwned`].\n#[announce(mock = ::std::borrow::ToOwned)]\npub trait ToOwned {\n    type Owned: ::core::borrow::Borrow<Self>;\n    fn to_owned(&self) -> Self::Owned;\n    fn clone_into(&self, target: &mut Self::Owned) { ::core::unimplemented!() }\n}\n#[cfg(feature = "std")]\n#[doc(no_inline)]\npub use ::std::borrow::ToOwned;\n'],
}


def die(msg, code=2):
    print("gen_std.py: " + msg, file=sys.stderr)
    sys.exit(code)


def cfg_since(v):
    return f"#[cfg(announce_std_rust_1_{v})]"


def cfg_before(v):
    return f"#[cfg(not(announce_std_rust_1_{v}))]"


def read_build_gates():
    """The versions (N of `1.N`) in announce-std/build.rs's `gates![..]` list."""
    try:
        s = open(BUILD_RS).read()
    except OSError as e:
        die(f"cannot read {BUILD_RS}: {e}")
    m = re.search(r'^gates!\s*\[([^\]]*)\]\s*;', s, re.M)
    if not m:
        die(f"no `gates![..];` invocation found in {BUILD_RS}")
    vs = [int(v) for v in re.findall(r'\b1\.(\d+)\b', m.group(1))]
    if not vs:
        die(f"empty `gates![..]` list in {BUILD_RS}")
    return set(vs)


def check_versions(text, where, allowed):
    used = {int(v) for v in re.findall(r'announce_std_rust_1_(\d+)', text)}
    bad = sorted(used - allowed)
    if bad:
        die(f"{where} uses cfg version(s) {', '.join('1.%d' % v for v in bad)} that "
            f"announce-std/build.rs does not list in gates![..] ({', '.join('1.%d' % v for v in sorted(allowed))}); "
            f"add them there, or change GATES/GLOB_SINCE in tools/gen_std.py")
    return used


def check_gate_tables(allowed):
    """GATES/GLOB_SINCE may only name versions that announce-std/build.rs sets a cfg for."""
    for what, v in list(GATES.items()) + [("GLOB_SINCE", GLOB_SINCE)]:
        if v not in allowed:
            die(f"{what} uses 1.{v}, which announce-std/build.rs does not list in gates![..] "
                f"({', '.join('1.%d' % x for x in sorted(allowed))}); add it there or change GATES/GLOB_SINCE")


def fallback(name):
    params, bound, extra = FALLBACK[name]
    args = "<Rhs>" if params else ""
    extra_params = (", " + extra[1:-1]) if extra else ""
    return (f"{cfg_before(GLOB_SINCE)}\n"
            f"/// Fallback for rustc < 1.{GLOB_SINCE}: a blanket subtrait of [`{bound.split('<')[0]}`], usable in bounds.\n"
            f"pub trait {name}{params}: {bound} {{}}\n"
            f"{cfg_before(GLOB_SINCE)}\n"
            f"impl<__T: ?::core::marker::Sized + {bound}{extra_params}> {name}{args} for __T {{}}\n")


def fn_semi(content, m):
    """Index of the `;` ending the signature of the `fn` found by regex match m (None: no such `;`)."""
    depth = 0
    for k in range(m.end(), len(content)):
        ch = content[k]
        if ch in "([{<": depth += 1
        elif ch in ")]}>" and not (ch == ">" and content[k-1] == "-"): depth -= 1
        elif ch == ";" and depth == 0:
            return k
    return None


def add_default_bodies(name, content):
    for f in PROVIDED.get(name, ()):
        m = re.search(r'\bfn ' + f + r'\b', content)
        if not m:
            continue
        k = fn_semi(content, m)
        if k is not None:
            content = content[:k] + " { ::core::unimplemented!() }" + content[k+1:]
    return content


def drop_fn(name, f, content):
    """content without the required method `f` (its signature up to the `;`)."""
    m = re.search(r'\bfn ' + f + r'\b', content)
    k = fn_semi(content, m) if m else None
    if k is None:
        die(f"GATES has {name}::{f}, but the {name} copy has no `fn {f}(..);` to drop")
    return content[:m.start()].rstrip(" \t") + content[k+1:]


def match(s, i, o, c):
    assert s[i] == o, (s[i:i+40])
    d = 0
    for j in range(i, len(s)):
        if s[j] == o: d += 1
        elif s[j] == c:
            d -= 1
            if d == 0: return j
    raise ValueError


USED_GATES = set()


def variants(name, alt, content):
    """The copies of one trait that announce-std emits: [(version cfg line, mocked path, body)].

    Which gates apply comes from GATES/OLDER_PATH only (see the script header)."""
    tg = GATES.get(name)
    mg = {k.split("::", 1)[1]: v for k, v in GATES.items() if k.startswith(name + "::")}
    if tg is not None:
        USED_GATES.add(name)
    if len(mg) + (tg is not None) > 1:
        die(f"GATES: at most one gate per trait is supported ({name})")
    if mg:
        (f, v), = mg.items()
        USED_GATES.add(f"{name}::{f}")
        return [(cfg_since(v), alt, content), (cfg_before(v), alt, drop_fn(name, f, content))]
    if tg is not None:
        out = [(cfg_since(tg), alt, content)]
        if name in OLDER_PATH:
            a, b = OLDER_PATH[name]
            out.append((cfg_before(tg), alt.replace(a, b), content.replace(a, b)))
        return out
    if name in OLDER_PATH:
        die(f"OLDER_PATH has {name}, but GATES has no gate for it")
    return [("", alt, content)]


def strip_version_attrs(s):
    """newer-type's `#[rustversion::..]` attributes dropped: its gated items become plain items."""
    return re.sub(r'#\[rustversion::[^\]]*\]\s*', '', s)


def is_version_attr(a):
    return a.startswith("rustversion") or (a.startswith("cfg") and re.search(r'rustversion|version\(|rustc', a))


def skipws(s, i):
    while i < len(s) and s[i].isspace(): i += 1
    return i


def conv_block(body, seen, real):
    out = []
    i = 0
    while True:
        i = skipws(body, i)
        if i >= len(body): break
        attrs = []
        while body[i] == '#':
            j = match(body, i + 1, '[', ']')
            attrs.append(body[i + 2:j]); i = skipws(body, j + 1)
        assert body.startswith("pub trait", i), body[i:i+50]
        i += len("pub trait")
        m = re.compile(r'\s*(\w+)').match(body, i); name = m.group(1); i = m.end()
        i = skipws(body, i); params = supers = where = ""
        if body[i] == '[':
            j = match(body, i, '[', ']'); params = "<" + body[i+1:j].strip() + ">"; i = skipws(body, j + 1)
        if body[i] == ':':
            i = skipws(body, i + 1); j = match(body, i, '[', ']'); supers = ": " + body[i+1:j].strip(); i = skipws(body, j + 1)
        if body.startswith("where", i):
            i = skipws(body, i + 5); j = match(body, i, '[', ']'); where = " where " + body[i+1:j].strip(); i = skipws(body, j + 1)
        j = match(body, i, '{', '}'); content = body[i+1:j]; i = j + 1
        if name in seen:
            # An alternative of a trait already taken (newer-type's since/before pair):
            # the first definition wins, the rest is discarded.
            continue
        seen.add(name)
        content = re.sub(r",\s*mut (\w+):", r", \1:", content)
        attrs = [a for a in attrs if not is_version_attr(a)]
        alt0 = next(re.search(r'alternative\s*=\s*([\w:]+)', a).group(1) for a in attrs if a.startswith("target"))
        for g, alt, content in variants(name, alt0, content):
            content = add_default_bodies(name, content)
            cfgs = [f"#[{a}]" for a in attrs if a.startswith("cfg")] + ([g] if g else [])
            if alt in EXTRA_CFG:
                cfgs.append(EXTRA_CFG[alt])
            cfg = "\n".join(cfgs)
            doc = f"/// announce mock of [`{alt}`]: invoke `{name}!` to receive its definition.\n"
            if name not in DERIVE_NAMES:
                doc += "///\n/// The real trait is re-exported under the same path.\n"
            else:
                doc += "///\n/// The real trait is available under the same path in the type namespace\n/// (from rustc 1.94, through a glob re-export, since the macro namespace holds this\n/// trampoline).\n"
            out.append(f"{cfg}\n{doc}#[announce(mock = {alt})]\npub trait {name}{params}{supers}{where} {{{content}}}\n")
            if name not in DERIVE_NAMES:
                out.append(f"{cfg}\n#[doc(no_inline)]\npub use {alt};\n")
            else:
                if name in FALLBACK:
                    out.append(fallback(name))
                real.append(f"    {cfg}\n    pub use {alt};\n".replace("\n#", "\n    #"))
    return out


def conv_file(path, real):
    s = strip_version_attrs(open(path).read())
    out = []
    seen = set()
    for m in re.finditer(r'emit_traits!\s*\{', s):
        i = m.end() - 1; j = match(s, i, '{', '}')
        out += conv_block(s[i+1:j], seen, real)
    return out


def generate(src, out_dir):
    """Write the unformatted generated modules into out_dir; returns {module: n_mocks}."""
    mods = [f[:-3] for f in sorted(os.listdir(src)) if f.endswith(".rs") and f != "lib.rs"]
    counts = {}
    for mod in mods:
        real = []
        items = conv_file(os.path.join(src, mod + ".rs"), real)
        items += EXTRA_ITEMS.get(mod, [])
        if real:
            g = GLOB_SINCE
            items.append("// Real traits whose name is also a derive macro: explicit names shadow glob\n// imports per namespace, so this glob supplies the *trait* while the announce\n// trampoline above keeps the *macro* namespace. Before rustc 1.%d the glob\n// breaks the trampoline's resolution, so it is only emitted from 1.%d on.\n%s\n#[doc(hidden)]\npub mod __real {\n" % (g, g, cfg_since(g)) + "".join(real) + "}\n%s\n#[doc(no_inline)]\npub use self::__real::*;\n" % cfg_since(g))
        if not items:
            continue
        counts[mod] = sum(1 for x in items if "#[announce(" in x)
        head = f"//! announce mocks of `{mod}` traits. Generated from newer-type-std's copies.\n\n#[allow(unused_imports)]\nuse announce::announce;\n\n"
        with open(os.path.join(out_dir, mod + ".rs"), "w") as f:
            f.write(head + "\n".join(items))
    return counts


def run_rustfmt(files, workdir):
    # An empty rustfmt.toml next to the files, so no outer config changes the style.
    cfgdir = os.path.join(workdir, ".rustfmt-config")
    os.makedirs(cfgdir)
    open(os.path.join(cfgdir, "rustfmt.toml"), "w").close()
    try:
        r = subprocess.run(["rustfmt", "--edition", "2021", "--config-path", cfgdir] + files,
                           stdout=subprocess.PIPE, stderr=subprocess.STDOUT, universal_newlines=True)
    except OSError as e:
        die(f"cannot run rustfmt ({e}); the output cannot be formatted")
    if r.returncode != 0:
        die("rustfmt failed:\n" + r.stdout)


def rustfmt_version():
    try:
        return subprocess.run(["rustfmt", "--version"], stdout=subprocess.PIPE, universal_newlines=True).stdout.strip()
    except OSError:
        return "rustfmt (not found)"


def show_diff(name, old, new, limit=24):
    lines = list(difflib.unified_diff(old.splitlines(), new.splitlines(),
                                      f"announce-std/src/{name} (checked in)", f"{name} (generated)", lineterm="", n=1))
    for l in lines[:limit]:
        print(l)
    if len(lines) > limit:
        print(f"... {len(lines) - limit} more diff lines")


def check_tree(gen_dir, counts, allowed):
    """Compare the generated modules with announce-std/src; returns the number of problems."""
    bad = 0
    gen = sorted(counts)
    for mod in gen:
        name = mod + ".rs"
        new = open(os.path.join(gen_dir, name)).read()
        try:
            old = open(os.path.join(DST, name)).read()
        except OSError:
            old = ""
            print(f"{name}: missing from announce-std/src")
        if old != new:
            bad += 1
            print(f"{name}: differs from the generator output")
            show_diff(name, old, new)
    known = set(gen) | set(HAND_WRITTEN) | {"lib"}
    for f in sorted(os.listdir(DST)):
        if f.startswith("."):
            bad += 1
            print(f"{f}: stray dot file in announce-std/src (the generator no longer writes .mods/.versions)")
        elif f.endswith(".rs") and f[:-3] not in known:
            bad += 1
            print(f"{f}: neither generated nor listed in HAND_WRITTEN")
        elif not f.endswith(".rs") and not f.startswith("."):
            bad += 1
            print(f"{f}: unexpected file in announce-std/src")
    for mod in HAND_WRITTEN:
        if not os.path.exists(os.path.join(DST, mod + ".rs")):
            bad += 1
            print(f"{mod}.rs: listed in HAND_WRITTEN but missing")
    lib = open(os.path.join(DST, "lib.rs")).read()
    for mod in sorted(set(gen) | set(HAND_WRITTEN)):
        if not re.search(r'^pub mod %s;' % mod, lib, re.M):
            bad += 1
            print(f"lib.rs: no `pub mod {mod};`")
    # Every gate used by any source (also the hand-written ones) must be in build.rs.
    for f in sorted(os.listdir(DST)):
        if f.endswith(".rs"):
            used = {int(v) for v in re.findall(r'announce_std_rust_1_(\d+)', open(os.path.join(DST, f)).read())}
            for v in sorted(used - allowed):
                bad += 1
                print(f"{f}: cfg announce_std_rust_1_{v} is not in announce-std/build.rs gates![..]")
    return bad


def main():
    ap = argparse.ArgumentParser(description="Regenerate announce-std/src from newer-type-std (see the script header).")
    ap.add_argument("--check", action="store_true",
                    help="generate into a temp dir and compare with announce-std/src; exit 1 on any difference")
    ap.add_argument("--out", metavar="DIR", help="write the generated modules into DIR (default: announce-std/src)")
    ap.add_argument("--newer-type", metavar="DIR", default=NEWER_TYPE,
                    help="newer-type checkout (default: %(default)s)")
    args = ap.parse_args()

    src = os.path.join(os.path.expanduser(args.newer_type), "std", "src")
    if not os.path.isdir(src):
        die(f"newer-type-std sources not found at {src} (use --newer-type DIR)")
    allowed = read_build_gates()
    check_gate_tables(allowed)

    with tempfile.TemporaryDirectory() as tmp:
        gen_dir = os.path.join(tmp, "src")
        os.makedirs(gen_dir)
        counts = generate(src, gen_dir)
        stale = sorted(set(GATES) - USED_GATES)
        if stale:
            die(f"GATES has no matching trait/method in newer-type-std: {', '.join(stale)}")
        files = [os.path.join(gen_dir, m + ".rs") for m in sorted(counts)]
        run_rustfmt(files, tmp)
        used = set()
        for f in files:
            used |= check_versions(open(f).read(), os.path.basename(f), allowed)

        if args.out:
            os.makedirs(args.out, exist_ok=True)
        if args.check:
            bad = check_tree(gen_dir, counts, allowed)
            if args.out:
                for f in files:
                    shutil.copy(f, args.out)
            if bad:
                print(f"gen_std.py --check: {bad} problem(s) ({rustfmt_version()})")
                sys.exit(1)
            print(f"gen_std.py --check: announce-std/src matches the generator "
                  f"({sum(counts.values())} mocks in {len(counts)} modules, cfg versions "
                  f"{', '.join('1.%d' % v for v in sorted(used))})")
            return
        out = os.path.abspath(args.out) if args.out else DST
        os.makedirs(out, exist_ok=True)
        for f in files:
            shutil.copy(f, out)
        for mod in sorted(counts):
            print(mod, counts[mod])
        print("total", sum(counts.values()), "cfg versions", sorted(used))


if __name__ == "__main__":
    main()
