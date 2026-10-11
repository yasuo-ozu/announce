# announce benchmark

Measures how token counts and compile wall time of `#[announce]` grow with

- **S**, the definition size (fields per struct, or methods per trait), and
- **K**, the number of definitions referenced at once in one chain
  `D0! { [D0!, D1!, .., D{K-1}!, ::bench_cb::count!] all }`.

```sh
python3 bench/run.py --quick          # S in {1,10}, K in {1,4}, 1 run (~30 s)
python3 bench/run.py                  # S in {1,10,100,1000}, K in {1,4,16,64}, structs and traits, 3 runs (~3-5 min)
python3 bench/run.py --sizes 1,100 --counts 8 --kinds struct,trait --runs 5
python3 bench/run.py --help
```

Python 3 standard library only; cargo runs with `--offline`. The generated
crates live in `target/bench/ws` (a standalone cargo workspace with its own
`[workspace]`, so it never joins the repository workspace; `announce` and
`announce-helper` are path dependencies on this checkout), the build directory
is `target/bench/target` and is deleted at the end unless `--keep` is given.
Results go to `target/bench/results.{csv,json}`; the Markdown table goes to
stdout, progress to stderr. Latest recorded run: [RESULTS.md](RESULTS.md).

## What is generated, per grid point (kind, S, K)

| crate | contents |
|---|---|
| `def_*` | K items `#[::announce::announce] pub struct Di { pub f0: u32, .. }` (or `pub trait Di { fn m0(&self, x: u32) -> u32; .. }`) |
| `plain_*` | the same items without `#[announce]` (reference) |
| `use_chain_*` | one chain over all K definitions, ending in `::bench_cb::count!` |
| `use_sep_*` | K separate calls `Di! { [Di!, ::bench_cb::count!] ci }` (skip with `--no-separate`) |

`bench_cb::count!` is a proc macro that parses its input with
`announce_helper::Input` and emits `ENTRIES`, `INPUT_TOKENS` and `ITEM_TOKENS`
constants. Each user crate `const`-asserts `ENTRIES == K` and has a tiny bin
that prints the constants. `tokcount` counts tokens of Rust source with
`proc_macro2::TokenStream::from_str` (exact lexing, not an approximation).

Token counts are token trees counted recursively: a group `(..)`, `[..]`,
`{..}` counts as 1 plus its contents; `::` and `=>` count as 2 punctuation
tokens.

## Columns

| column | meaning |
|---|---|
| src tok | tokens of the definer's `lib.rs` as written (with the `#[announce]` attributes) |
| plain exp tok | tokens of `-Zunpretty=expanded` of the `plain_*` crate (includes ~17 tokens of std prelude boilerplate) |
| announce exp tok | tokens of `-Zunpretty=expanded` of the `def_*` crate: the items plus the generated `macro_rules!` trampolines (and `Announce` impls etc.). Each trampoline has one arm per routing case and each arm holds a copy of the item, so this grows as roughly 1200 + 7 x item tokens per definition (the item plus 6 copies; structs, at the time of writing) |
| cb item tok | sum of the `ITEM` payload tokens the callback received (same for chain and K calls) |
| cb input tok (chain) | all tokens the callback received in the single chain call (list, entry keys, META, items, REST) |
| cb input tok (K calls) | the same, summed over the K separate calls |
| plain def ms / announce def ms | wall time of rebuilding only the definer crate (`cargo build -p X --lib` after touching its `lib.rs`; deps already built) |
| user chain ms / user K calls ms | wall time of rebuilding only the user crate, its definer and `bench_cb` already built |

Times are the median of `--runs` runs (default 3) and include the fixed cargo
and rustc start-up cost: the run header reports the rebuild time of an empty
lib crate as a baseline to subtract mentally.

## Caveats

- `-Zunpretty=expanded` is unstable: the script sets `RUSTC_BOOTSTRAP=1`
  (needed on a stable toolchain). Its pretty-printed output is re-lexed, so
  the counts are those of the printed form, not of rustc's internal streams.
- The expansion count of `def_*` is measured on the definer only; trampoline
  expansion happens in the user crate and is captured by the user timings and
  the callback-side counts.
- Wall times are noisy (other load, CPU frequency, file system cache) and
  machine-specific; compare columns within one run, not across machines.
  `CARGO_INCREMENTAL=0` is set so that the rebuild redoes expansion each time.
- The user crates set `#![recursion_limit = "1024"]`: a chain over K
  definitions nests about one macro expansion per hop.
- Definitions use only primitive field types (`u32`), so no `repeater` / type
  leak information is involved.
