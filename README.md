# announce crate [![Latest Version]][crates.io] [![Documentation]][docs.rs] [![GitHub Actions]][actions]

[Latest Version]: https://img.shields.io/crates/v/announce.svg
[crates.io]: https://crates.io/crates/announce
[Documentation]: https://img.shields.io/docsrs/announce
[docs.rs]: https://docs.rs/announce/latest/announce/
[GitHub Actions]: https://github.com/yasuo-ozu/announce/actions/workflows/rust.yml/badge.svg
[actions]: https://github.com/yasuo-ozu/announce/actions/workflows/rust.yml

<p align="center"><img src="https://raw.githubusercontent.com/yasuo-ozu/announce/main/logo.png" alt="announce" width="200"></p>

Make a definition visible to proc macros in any crate.

A proc macro only sees the tokens it is applied to. Put `#[announce]` on a
struct, enum, union, trait, type alias or inline `mod`, and any crate can ask for
its tokens: `#[announce]` keeps the item and emits a `macro_rules!` with the
item's name, and calling that macro passes the definition to a callback macro.

```rust
#[announce::announce]
pub trait Greet { fn hi(&self) -> &'static str; }

macro_rules! show {
    ([$($list:tt)*] $($key:ident)::+ ! => [$($meta:tt)*] { $($item:tt)* } $($rest:tt)*) => {
        const SHOWN: &str = stringify!($($item)*);
    };
}
Greet! { [Greet!, show!] }
assert!(SHOWN.contains("trait Greet"));
```

From another crate the call reads `path::to::Greet! { [path::to::Greet!, show!] REST }`.
The callback receives the definition, and `REST` unchanged:

```text
show! {
    [show!]
    path::to::Greet! => [META] { pub trait Greet { fn hi(&self) -> &'static str; } }
    REST
}
```

`META` lists what the definer attached (checks, `meta` entries). Callers and
callbacks need no dependency on announce.

For a struct, enum or union you write yourself, derive it instead. The derive
also implements the [`Announce`](https://docs.rs/announce/latest/announce/trait.Announce.html)
trait, and takes the same options in a helper attribute `#[announce(..)]`:

```rust
#[derive(announce::Announce)]
pub struct Point { pub x: i32, pub y: i32 }
```

## Calling a trampoline

One callback can take many definitions. The list names the trampolines, then
the callback, which runs once. A long chain may need a higher `recursion_limit`
([details](https://docs.rs/announce/latest/announce/attr.announce.html#size-and-recursion)).

```text
path::Item! { [path::Item!, other::Def!, cb!] REST }
```

## Features

- Works in any position: item, statement, expression, type, pattern.
- Evaluates `#[cfg]` in the defining crate; `crate::` paths keep pointing there.
- Leaves docs out of the item the callback receives (`docs = true` keeps them).
- Checks at the definition, which can own attributes of their own inside the item:
  [`#[announce(my_lib::check)]`](https://docs.rs/announce/latest/announce/attr.announce.html#checks-and-attributes-of-your-own).
- Metadata for consumers: `#[announce(meta = my_lib::info(k = v))]`.
- Types that are relative to the defining crate: [`repeater`](https://docs.rs/announce/latest/announce/attr.announce.html#type-context).
- Transformers chosen by the caller, such as
  [`signature_only!`](https://docs.rs/announce/latest/announce/macro.signature_only.html).
- A derive or attribute may own the name: rename the trampoline with `macro_name`.

All options of `#[announce(..)]` are in the
[API docs](https://docs.rs/announce/latest/announce/attr.announce.html).

## Crates

| crate | use it to |
|---|---|
| [`announce`](https://docs.rs/announce) | define items: `#[announce]`, `#[derive(Announce)]`, trait `Announce` |
| [`announce-helper`](https://docs.rs/announce-helper) | write callbacks and checks, start chains, redirect derives |
| [`announce-std`](https://docs.rs/announce-std) | announce std/core traits (`announce_std::iter::Iterator!`) |

A proc-macro crate can announce items it builds itself with
[`announce::machinery`](https://docs.rs/announce/latest/announce/machinery/index.html).
Minimum Rust version: 1.57.

## Performance

Measured with the criterion benchmark in `bench/` (run `cargo bench --offline`
inside `bench/`; the full tables, with the numbers from before the single-copy
layout for comparison, are in `bench/RESULTS.md`), on traits, with rustc 1.101
nightly on x86_64 and cargo's dev profile. S is the number of methods per trait,
K the number of definitions passed to one callback, which only consumes its
input. Sizes are of the `.rmeta` files cargo writes: item definitions and
`macro_rules!` bodies live in the metadata. Build times are the median rebuild of
that crate alone (10 samples, on a shared machine, so rough).

Lines: color = S (darker is larger), solid = announced or one chain, dashed =
plain or K separate calls. Both axes are logarithmic. Made with
`bench/plot.py --svg bench/plots`.

<img src="https://raw.githubusercontent.com/yasuo-ozu/announce/main/bench/plots/defining-rmeta.svg" alt="Defining crate: .rmeta size against K" width="49%"> <img src="https://raw.githubusercontent.com/yasuo-ozu/announce/main/bench/plots/defining-time.svg" alt="Defining crate: rebuild time against K" width="49%">
<img src="https://raw.githubusercontent.com/yasuo-ozu/announce/main/bench/plots/calling-rmeta.svg" alt="Calling crate: .rmeta size against K" width="49%"> <img src="https://raw.githubusercontent.com/yasuo-ozu/announce/main/bench/plots/calling-time.svg" alt="Calling crate: rebuild time against K" width="49%">

| S | K | definer, plain | definer, announced | calling crate, one chain | calling crate, K calls | definer build, plain / announced | calling crate build, one chain |
|---:|---:|---:|---:|---:|---:|---:|---:|
| 1 | 1 | 2.5 KiB | 10.0 KiB | 4.0 KiB | 4.0 KiB | 97 / 99 ms | 99 ms |
| 10 | 16 | 42 KiB | 194 KiB | 5.3 KiB | 10.9 KiB | 95 / 155 ms | 117 ms |
| 200 | 16 | 865 KiB | 1,357 KiB | 6.9 KiB | 12.6 KiB | 159 / 475 ms | 143 ms |
| 200 | 96 | 5,562 KiB | 8,629 KiB | 22.7 KiB | 58.9 KiB | 429 / 2,365 ms | 462 ms |

- The definer's `.rlib` is 2.4 KiB with or without `#[announce]`, because
  definitions contain no code. The trampoline is metadata: about 6 KiB per
  definition (11 KiB before), plus its copies of the item, three of them (six
  before). An item of more than 300 tokens (about 21 trait methods) is held once
  in its macro: 7 KiB plus one copy. At S = 200 the definer's metadata is 2.3
  times the plain crate's with three copies (3.4 times before) and 1.6 times
  with one, so S = 200, K = 96 is 8,629 KiB, not 19,046 KiB.
- The price is expansions: one call of such a trampoline nests three macro
  expansions instead of one (see "Calling a trampoline" for the chain length).
  A calling crate's `.rlib` stays 2.4 KiB. Its `.rmeta` does not depend on S and
  grows by about 0.1 KiB per definition in one chain (0.2 for a large one) and
  about 0.5 KiB per separate call (0.6).
- The benchmark traits have no documentation. Doc attributes are not recorded, so
  documented items cost less than their size suggests: a trait of 40 methods with
  five lines of docs each has a 36.8 KiB `.rmeta` (44.7 KiB with `docs = true`),
  and one of 5 methods 14.1 KiB (17.4 KiB).
- An executable that uses both crates has the same size at every point
  measured (8,658 KiB in the dev profile, 356 KiB stripped; an empty program
  is 4,495 and 331 KiB): the trampolines do not reach it.
- The announced definer rebuilds in about the same time as before (5 % less at
  S = 200): its time is the expansion of `#[announce]`, mostly the proc macro that
  reads the item, not the size of the macro it writes. It costs tens of
  milliseconds more than the plain one for small definitions and about five times
  as much at S = 200, K = 96. A chain over 96 large definitions rebuilds 17 %
  slower in the calling crate and 96 separate calls 40 % faster (interleaved
  runs, `bench/RESULTS.md`).

## Related crates

- **[macro_magic](https://crates.io/crates/macro_magic)** has the same core idea.
  announce adds continuation lists, definition-side cfg evaluation and checks,
  and type context handling.
- **[reify_types](https://crates.io/crates/reify_types)** makes a definition
  available when compiled code *runs*. announce delivers it at *compile time*.
