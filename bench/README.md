# announce benchmark

A [criterion](https://crates.io/crates/criterion) benchmark of the cost of
`#[announce]`: the artifact sizes (`.rlib`, `.rmeta`, executable) and the
rebuild time of the defining crate and of the calling crate, as functions of

- **S**, the definition size (fields per struct, or methods per trait), and
- **K**, the number of definitions referenced at once in one chain
  `D0! { [D0!, D1!, .., D{K-1}!, ::bench_cb::count!] all }`.

```sh
cd bench
cargo bench --offline                                  # the default grid, see below
ANNOUNCE_BENCH_QUICK=1 cargo bench --offline           # S in {1,10}, K in {1,4}: about 1 min
ANNOUNCE_BENCH_SIZES=100 ANNOUNCE_BENCH_COUNTS=8,64 ANNOUNCE_BENCH_KINDS=struct,trait cargo bench --offline
# the large grid (structs and traits, S up to 1000, K up to 128; about 30 minutes):
ANNOUNCE_BENCH_KINDS=struct,trait ANNOUNCE_BENCH_SIZES=1,10,100,1000 ANNOUNCE_BENCH_COUNTS=1,2,4,8,16,32,64,128 cargo bench --offline
cargo bench --offline -- 'definer_announce/trait'      # criterion's own filter (a regex on the id)
cargo bench --offline -- --test                        # one rebuild per benchmark, no statistics
ANNOUNCE_BENCH_NO_TIMING=1 cargo bench --offline       # artifact sizes only, no criterion run
```

`bench/` is a standalone cargo package (its own `[workspace]`, not a member of
the repository workspace), so run cargo from inside `bench/`. Criterion owns
the command line, so the grid is configured with environment variables:

| variable | default | meaning |
|---|---|---|
| `ANNOUNCE_BENCH_SIZES` | `1,10,100,200` | S values |
| `ANNOUNCE_BENCH_COUNTS` | `1,2,4,8,16,32,48,64,96` | K values |
| `ANNOUNCE_BENCH_KINDS` | `trait` | definition kinds, `struct` and/or `trait` |
| `ANNOUNCE_BENCH_QUICK` | unset | `1`: default S = `1,10` and K = `1,4` (explicit variables still win) |
| `ANNOUNCE_BENCH_SAMPLES` | `10` | criterion samples per benchmark (criterion's minimum is 10) |
| `ANNOUNCE_BENCH_NO_TIMING` | unset | `1`: only build every crate once and report the sizes |
| `ANNOUNCE_BENCH_LAYOUT` | unset | `single`, `direct` or `auto`: adds `layout = ..` to every `#[announce]` (the trampoline layout, DESIGN.md section 4.1). Unset: no option, so the layout follows the size of the item |
| `ANNOUNCE_BENCH_KEEP` | unset | `1`: keep the generated crates and their build directory |
| `ANNOUNCE_BENCH_REPO` | the parent of `bench/` | checkout of `announce`, `announce-helper` and the `Cargo.lock` to benchmark, e.g. a `git archive` of a commit |
| `ANNOUNCE_BENCH_REV` | `git rev-parse` of the checkout | revision to print in the report, for a checkout without `.git` |

The default grid is traits only, S in {1, 10, 100, 200} and K in {1, 2, 4, 8,
16, 32, 48, 64, 96}: 36 points, each with four timed crates, 10 samples per
benchmark. It takes about 13 minutes (see [RESULTS.md](RESULTS.md)). The large
grid in the example above (structs too, S up to 1000, K up to 128) takes about
30 minutes; the points with S = 1000 and K >= 64 are more than half of that.
Everything runs offline against the cargo cache; the generated crates depend on
`announce` and `announce-helper` by path.

## Where the results go

All paths are below `bench/target/` (or `$CARGO_TARGET_DIR`).

| file | content |
|---|---|
| `criterion/<group>/<kind>/<param>/new/estimates.json` | criterion's estimates (ns) per benchmark; `<param>` is `s<S>_k<K>` |
| `criterion/announce-sizes.csv` | artifact sizes in bytes, one row per grid point |
| `criterion/announce-sizes.md` | the same, as Markdown tables in KiB |
| `criterion/announce-report.md` | sizes and rebuild times side by side (what [RESULTS.md](RESULTS.md) is made from); also printed at the end of the run |
| `announce-bench/{ws,target}` | the generated crates and their build directory; deleted at the end of the run unless `ANNOUNCE_BENCH_KEEP=1` |

The report only contains rebuild times written during the current run
(estimates of earlier runs are ignored), so a filtered run reports only what it
ran. The size table has a row for every crate that was built.

## What is generated, per grid point (kind, S, K)

| crate | contents |
|---|---|
| `def_*` | K items `#[::announce::announce] pub struct Di { pub f0: u32, .. }` (or `pub trait Di { fn m0(&self, x: u32) -> u32; .. }`) |
| `plain_*` | the same items without `#[announce]` (reference) |
| `use_chain_*` | one chain over all K definitions, ending in `::bench_cb::count!` |
| `use_calls_*` | K separate calls `Di! { [Di!, ::bench_cb::count!] ci }` |

`bench_cb::count!` is a proc macro callback. It parses its whole input with
`announce_helper::Input` (so it consumes every entry) and emits one constant,
`ENTRIES`; each user crate `const`-asserts `ENTRIES == K`. Each user crate also
has a tiny executable that prints `ENTRIES` and the size of the first
definition (`size_of::<Di>()`, or of a `&dyn Di`), so that the user crate and
the definer are linked into it.

## Benchmarks (criterion groups)

Each benchmark is `<group>/<kind>/s<S>_k<K>`. One iteration touches the
crate's `lib.rs` and times `cargo build --offline -p <crate> --lib` (wall
time, spawn included; dependencies are already built, `CARGO_INCREMENTAL=0`).
The first call builds the crate and its dependencies and measures the sizes,
outside the timed part. Sampling is `Flat` with 10 samples of one rebuild each
(plus one untimed warm-up rebuild). The benchmarks run point by point (the four
crates of one grid point one after the other), so that they are measured under
similar load.

| group | crate rebuilt |
|---|---|
| `definer_plain` | `plain_*` |
| `definer_announce` | `def_*` |
| `user_chain` | `use_chain_*` |
| `user_calls` | `use_calls_*` |
| `baseline/empty_lib_start`, `baseline/empty_lib_end` | an empty library, for the fixed cargo and rustc start-up cost; measured before and after the grid, so a drift of the machine's load shows |

## Columns (all builds use cargo's dev profile)

Sizes are bytes of the files in cargo's build directory, shown as KiB (1024
bytes). They are not timings: each is measured once, after the crate was built
for the first time. They are deterministic; after the timed rebuilds the
benchmark checks that the `.rlib` and `.rmeta` sizes did not change and the
report says so.

| column | meaning |
|---|---|
| plain / announce rlib, rmeta | `lib<crate>-<hash>.rlib` and `.rmeta` of `plain_*` and of `def_*` (definition side) |
| chain / K calls rlib, rmeta | the same for `use_chain_*` and `use_calls_*` (calling side) |
| chain / K calls exe | the executable of that user crate (dev profile: unoptimized, with debuginfo) |
| exe stripped | the same executable after `strip -o` (the size with `strip = true`) |
| plain / announce rebuild ms | criterion median of the rebuild of `plain_*` / `def_*` in ms, with the half-width of its 95% confidence interval (`±`). The median, because a few rebuilds are slowed by other load |
| chain / K calls rebuild ms | the same for `use_chain_*` / `use_calls_*` |

## Caveats

- Where things live. Item definitions and `macro_rules!` bodies are metadata,
  so the copies of each item inside the generated trampoline show up in the
  definer's `.rmeta`. On the toolchain used (recent nightly cargo) rustc is run
  with `-C embed-metadata=no`, so the `.rlib` holds only compiled code, not
  metadata: it stays the size of an empty object for definitions without code,
  and the on-disk size of a library is rlib + rmeta.
- The executable mostly reflects runtime code (the dev-profile `std` and its
  debuginfo). Trampolines and definitions without functions are not in it, so
  it should not depend on S, K or `#[announce]`; it is reported to show that.
- The callback emits only a constant. The calling-side sizes therefore measure
  what the protocol itself leaves in the user crate; a real callback that
  re-emits or implements the definitions adds its own output on top.
- Rebuild times are noisy (other load, CPU frequency, file system cache) and
  machine-specific; compare columns within one run, and rows of one point
  rather than far apart rows. The empty-crate benchmarks give the fixed cost of
  one cargo invocation (cargo reads the manifests of all generated crates
  on each call, which is part of it). Criterion prints
  `Warning: Unable to complete 10 samples in 1.0ms` for every benchmark: the
  warm-up and target times are set to their minimum (1 ms), so that criterion
  runs one warm-up rebuild and then exactly one rebuild per sample. This is
  expected.
- A chain over K definitions nests the expansions of all its macros, and rustc's
  default recursion limit is 128. A definition takes one expansion if it is
  small and three above 300 tokens (DESIGN.md section 4.1), so for K > 16 the
  generator adds `#![recursion_limit = "1024"]` to the chain crates (it has no
  effect on build time). Until 2026-10-13 it added `"512"` for K > 100 only.
- Definitions use only primitive field types (`u32`), so no `repeater` / type
  leak information is involved.
- The generated workspace copies the repository's `Cargo.lock`, to use only
  crate versions that are in the offline cache.

## Plots

`python3 plot.py` (standard library only) reads the results of the last
`cargo bench` and writes `target/criterion/announce-plots.html`: rebuild time
and `.rmeta` size against K, with one line per S for plain vs announced
(defining crate) and one chain vs K separate calls (calling crate). Criterion's
own HTML reports are in `target/criterion/report/index.html`.
