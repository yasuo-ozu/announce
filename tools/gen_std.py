# Regenerates announce-std/src/*.rs (except lib.rs) from newer-type-std's trait copies.
# Usage: python3 tools/gen_std.py   (expects newer-type at ~/ghq/github.com/yasuo-ozu/newer-type)
import re, os, sys
SRC = os.path.expanduser("~/ghq/github.com/yasuo-ozu/newer-type/std/src")
DST = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "announce-std", "src")
MSRV = 71
DERIVE_NAMES = {"Debug", "Clone", "Copy", "Default", "Hash", "PartialEq", "Eq", "PartialOrd", "Ord"}
versions = set()
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

# rustc < 1.94: the glob re-export cannot be used, so the type namespace gets a
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

def fallback(name):
    params, bound, extra = FALLBACK[name]
    args = "<Rhs>" if params else ""
    extra_params = (", " + extra[1:-1]) if extra else ""
    return (f"#[cfg(not(announce_std_rust_1_94))]\n"
            f"/// Fallback for rustc < 1.94: a blanket subtrait of [`{bound.split('<')[0]}`], usable in bounds.\n"
            f"pub trait {name}{params}: {bound} {{}}\n"
            f"#[cfg(not(announce_std_rust_1_94))]\n"
            f"impl<__T: ?::core::marker::Sized + {bound}{extra_params}> {name}{args} for __T {{}}\n")

def add_default_bodies(name, content):
    for f in PROVIDED.get(name, ()):
        m = re.search(r'\bfn ' + f + r'\b', content)
        if not m:
            continue
        depth = 0
        for k in range(m.end(), len(content)):
            ch = content[k]
            if ch in "([{<": depth += 1
            elif ch in ")]}>" and not (ch == ">" and content[k-1] == "-"): depth -= 1
            elif ch == ";" and depth == 0:
                content = content[:k] + " { ::core::unimplemented!() }" + content[k+1:]
                break
    return content

def match(s, i, o, c):
    assert s[i] == o, (s[i:i+40])
    d = 0
    for j in range(i, len(s)):
        if s[j] == o: d += 1
        elif s[j] == c:
            d -= 1
            if d == 0: return j
    raise ValueError

def gate(attr):
    m = re.match(r'rustversion::(since|before)\(1\.(\d+)\)', attr)
    kind, v = m.group(1), int(m.group(2))
    if v <= MSRV:
        return "" if kind == "since" else None  # None: never compiled
    versions.add(v)
    return f"#[cfg(announce_std_rust_1_{v})]" if kind == "since" else f"#[cfg(not(announce_std_rust_1_{v}))]"

def skipws(s, i):
    while i < len(s) and s[i].isspace(): i += 1
    return i

def conv_block(body, g, extra_cfg):
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
        content = re.sub(r",\s*mut (\w+):", r", \1:", content)
        content = add_default_bodies(name, content)
        alt = next(re.search(r'alternative\s*=\s*([\w:]+)', a).group(1) for a in attrs if a.startswith("target"))
        cfgs = [f"#[{a}]" for a in attrs if a.startswith("cfg")] + ([extra_cfg] if extra_cfg else []) + ([g] if g else [])
        cfg = "\n".join(cfgs)
        doc = f"/// announce mock of [`{alt}`]: invoke `{name}!` to receive its definition.\n"
        if name not in DERIVE_NAMES:
            doc += f"///\n/// The real trait is re-exported under the same path.\n"
        else:
            doc += f"///\n/// The real trait is available under the same path in the type namespace\n/// (from rustc 1.94, through a glob re-export, since the macro namespace holds this\n/// trampoline).\n"
        out.append(f"{cfg}\n{doc}#[announce(mock = {alt})]\npub trait {name}{params}{supers}{where} {{{content}}}\n")
        if name not in DERIVE_NAMES:
            out.append(f"{cfg}\n#[doc(no_inline)]\npub use {alt};\n")
        else:
            if name in FALLBACK:
                out.append(fallback(name))
            REAL.append(f"    {cfg}\n    pub use {alt};\n".replace("\n#", "\n    #"))
    return out

def conv_file(fname):
    s = open(os.path.join(SRC, fname)).read()
    out = []
    for m in re.finditer(r'((?:#\[rustversion::[^\]]+\]\s*)?)emit_traits!\s*\{', s):
        g = ""
        if m.group(1).strip():
            g = gate(m.group(1).strip()[2:-1])
            if g is None: continue
        i = m.end() - 1; j = match(s, i, '{', '}')
        out += conv_block(s[i+1:j], g, None)
    return out

mods = [f[:-3] for f in sorted(os.listdir(SRC)) if f.endswith(".rs") and f != "lib.rs"]
# Hand-written modules (clone, marker, default, any) live next to the
# generated ones and are not touched here.
std_only = {"io", "net", "process", "string", "task"}
count = 0
for mod in mods:
    REAL = []
    items = conv_file(mod + ".rs")
    if mod == "error":
        items = [x.replace("#[cfg(not(announce_std_rust_1_81))]", "#[cfg(not(announce_std_rust_1_81))]\n#[cfg(feature = \"std\")]") for x in items]
    if mod == "alloc":
        items.append('/// announce mock of [`::core::alloc::GlobalAlloc`].\n///\n/// The real trait is re-exported under the same path.\n#[announce(mock = ::core::alloc::GlobalAlloc)]\npub unsafe trait GlobalAlloc {\n    unsafe fn alloc(&self, layout: ::core::alloc::Layout) -> *mut ::core::primitive::u8;\n    unsafe fn dealloc(&self, ptr: *mut ::core::primitive::u8, layout: ::core::alloc::Layout);\n    unsafe fn alloc_zeroed(&self, layout: ::core::alloc::Layout) -> *mut ::core::primitive::u8 { ::core::unimplemented!() }\n    unsafe fn realloc(&self, ptr: *mut ::core::primitive::u8, layout: ::core::alloc::Layout, new_size: ::core::primitive::usize) -> *mut ::core::primitive::u8 { ::core::unimplemented!() }\n}\n#[doc(no_inline)]\npub use ::core::alloc::GlobalAlloc;\n')
    if REAL:
        items.append("// Real traits whose name is also a derive macro: explicit names shadow glob\n// imports per namespace, so this glob supplies the *trait* while the announce\n// trampoline above keeps the *macro* namespace. Before rustc 1.94 the glob\n// breaks the trampoline's resolution, so it is only emitted from 1.94 on.\n#[cfg(announce_std_rust_1_94)]\n#[doc(hidden)]\npub mod __real {\n" + "".join(REAL) + "}\n#[cfg(announce_std_rust_1_94)]\n#[doc(no_inline)]\npub use self::__real::*;\n")
    if mod == "borrow":
        items.append('#[cfg(feature = "std")]\n/// announce mock of [`::std::borrow::ToOwned`].\n#[announce(mock = ::std::borrow::ToOwned)]\npub trait ToOwned {\n    type Owned: ::core::borrow::Borrow<Self>;\n    fn to_owned(&self) -> Self::Owned;\n    fn clone_into(&self, target: &mut Self::Owned) { ::core::unimplemented!() }\n}\n#[cfg(feature = "std")]\n#[doc(no_inline)]\npub use ::std::borrow::ToOwned;\n')
    count += sum(1 for x in items if "#[announce(" in x)
    if not items: continue
    head = f"//! announce mocks of `{mod}` traits. Generated from newer-type-std's copies.\n\n#[allow(unused_imports)]\nuse announce::announce;\n\n"
    open(os.path.join(DST, mod + ".rs"), "w").write(head + "\n".join(items))
    print(mod, sum(1 for x in items if "#[announce(" in x))
print("total", count, "versions", sorted(versions))
versions.add(94)  # glob re-export of derive-named traits
open(os.path.join(DST, ".versions"), "w").write(" ".join(map(str, sorted(versions))))
open(os.path.join(DST, ".mods"), "w").write(" ".join(m for m in mods if os.path.exists(os.path.join(DST, m + ".rs"))))
