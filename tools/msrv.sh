#!/bin/sh
# Builds and tests announce on its minimum Rust versions, in a scratch copy:
#   1.57   announce, announce-core (with `announce::helper`), announce-macros,
#          announce-std and the tests without a `repeater` (derive, machinery*)
#   1.59   the whole workspace
# Needs the rustup toolchains 1.57.0 and 1.59.0 (`rustup toolchain install
# 1.57.0 --profile minimal`), and a stable cargo of 1.84 or newer. Set
# CARGO_NET_OFFLINE=true to use only the local cargo cache.
#
# Why a copy: cargo 1.57 cannot read a lock file of the current format, nor the
# current registry cache. The copy gets a lock file that the MSRV-aware resolver
# of a recent cargo makes (`fallback`), with `unicode-width` pinned (it declares
# no `rust-version`, but 0.1.14 needs rustc 1.66), and its sources vendored.
set -eu

root=$(cd "$(dirname "$0")/.." && pwd)
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
echo "msrv: working in $work"

(cd "$root" && tar cf - --exclude=target --exclude=.git --exclude=bench --exclude=Cargo.lock .) |
    (cd "$work" && tar xf -)
cd "$work"

CARGO_RESOLVER_INCOMPATIBLE_RUST_VERSIONS=fallback cargo +stable generate-lockfile
cargo +stable update -p unicode-width --precise 0.1.12
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

run 1.57.0 test -p announce -p announce-core -p announce-macros -p announce-std \
    -p announce-test-derive -p announce-test-machinery -p announce-test-machinery-lib \
    -p announce-test-machinery-user
run 1.57.0 build -p announce-std --no-default-features
run 1.59.0 test --workspace
echo "msrv: ok"
