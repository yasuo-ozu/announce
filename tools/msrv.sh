#!/bin/sh
# Builds and tests announce on its minimum Rust versions, in a scratch copy:
#   1.57   announce, announce-core (with `announce::helper`), announce-macros and
#          announce-std; announce without its default feature `proc-macro-api`. Not
#          the tests (announce-tests, nostd, renamed): their definers use `repeater`s
#   1.59   the whole workspace, without the UI tests: `tests/ui_*.rs` are emptied in
#          the copy, as they assert the texts of a current rustc
#   1.78   announce and announce-core, and the texts of a failing `announce::same_context!`:
#          1.78 has `#[diagnostic::on_unimplemented]` and not yet `do_not_recommend` (1.85).
#          The other steps here have neither, and stable and nightly have both
# Needs the rustup toolchains 1.57.0, 1.59.0 and 1.78.0 (`rustup toolchain install
# 1.57.0 --profile minimal`), and a stable cargo of 1.84 or newer. Set
# CARGO_NET_OFFLINE=true to use only the local cargo cache.
#
# Why a copy: cargo 1.57 cannot read a lock file of the current format, nor the
# current registry cache. The copy gets a lock file that the MSRV-aware resolver
# of a recent cargo makes (`fallback`), and its sources vendored. (`unicode-width`
# declares no `rust-version`, so that resolver cannot know that 0.1.14 needs rustc
# 1.66: announce-core caps it below 0.1.13 itself.)
set -eu

root=$(cd "$(dirname "$0")/.." && pwd)
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
echo "msrv: working in $work"

(cd "$root" && tar cf - --exclude=target --exclude=.git --exclude=bench --exclude=Cargo.lock .) |
    (cd "$work" && tar xf -)
cd "$work"

CARGO_RESOLVER_INCOMPATIBLE_RUST_VERSIONS=fallback cargo +stable generate-lockfile
cargo +stable vendor vendor >/dev/null
mkdir -p .cargo
cat >.cargo/config.toml <<'CONFIG'
[source.crates-io]
replace-with = "vendored-sources"

[source.vendored-sources]
directory = "vendor"
CONFIG

run() {
    echo "msrv: cargo +$*"
    cargo "+$@" --offline
}

run 1.57.0 test -p announce -p announce-core -p announce-macros -p announce-std
run 1.57.0 build -p announce-std --no-default-features
run 1.57.0 build -p announce --no-default-features
for f in tests/ui_*.rs; do
    : >"$f"
done
run 1.59.0 test --workspace
run 1.78.0 test -p announce -p announce-core

# The doctests only see that a failing `same_context!` fails. On 1.78 it must also say why
# (`on_unimplemented`) and must not trip over `do_not_recommend`, which 1.78 does not know.
mkdir texts
cat >texts/Cargo.toml <<'MANIFEST'
[package]
name = "texts"
version = "0.0.0"
edition = "2021"

[workspace]

[dependencies]
announce = { path = ".." }
MANIFEST
mkdir texts/src
cat >texts/src/lib.rs <<'SOURCE'
mod m {
    pub struct Local(pub u8);

    #[announce::announce(strict = false)]
    pub struct Item {
        pub a: Local,
    }
}

m::Item! { [m::Item!, announce::same_context!] }
SOURCE
echo "msrv: cargo +1.78.0 build (a failing same_context!)"
if cargo +1.78.0 build --offline --manifest-path texts/Cargo.toml --target-dir target 2>texts/stderr; then
    echo "msrv: the failing same_context! built" >&2
    exit 1
fi
grep -q "use absolute path: 'Local'" texts/stderr || { cat texts/stderr; echo "msrv: no texts on 1.78" >&2; exit 1; }
! grep -q "^warning" texts/stderr || { cat texts/stderr; echo "msrv: a warning on 1.78" >&2; exit 1; }
echo "msrv: ok"
