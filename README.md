# announce crate [![Latest Version]][crates.io] [![Documentation]][docs.rs] [![GitHub Actions]][actions] [![MSRV]][msrv]

[Latest Version]: https://img.shields.io/crates/v/announce.svg
[crates.io]: https://crates.io/crates/announce
[Documentation]: https://img.shields.io/docsrs/announce
[docs.rs]: https://docs.rs/announce/latest/announce/
[GitHub Actions]: https://github.com/yasuo-ozu/announce/actions/workflows/rust.yml/badge.svg
[actions]: https://github.com/yasuo-ozu/announce/actions/workflows/rust.yml
[MSRV]: https://img.shields.io/badge/MSRV-1.57-blue.svg
[msrv]: https://github.com/yasuo-ozu/announce/blob/main/Cargo.toml

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
    (@announce_format "1" [$($list:tt)*] $($key:ident)::+ ! => [$($meta:tt)*] { $($item:tt)* } $($rest:tt)*) => {
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
    @announce_format "1"
    [show!]
    path::to::Greet! => [META] { pub trait Greet { fn hi(&self) -> &'static str; } }
    REST
}
```

`META` lists what the definer attached (checks, `meta` entries). Callers and
callbacks need no dependency on announce.

Every call that announce makes to the next macro of a chain starts with
`@announce_format "1"`, the version of the wire format, before `[LIST]`. A macro of
yours can match it to know that announce called it. A call you write by hand
(`Greet! { [Greet!, show!] }`) needs no prefix:

```rust
#[announce::announce]
pub trait Greet { fn hi(&self) -> &'static str; }

macro_rules! who {
    // Called by announce.
    (@announce_format "1" $($call:tt)*) => { const CALLER: &str = "announce"; };
    // Called by hand.
    ($($call:tt)*) => { const CALLER: &str = "hand"; };
}
Greet! { [Greet!, who!] }
assert_eq!(CALLER, "announce");
```

The prefix is part of the
[wire format](https://docs.rs/announce/latest/announce/helper/struct.Input.html#the-wire-format).
An item and a macro built with different, incompatible versions of announce stop with
one error that names both versions.

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
| [`announce`](https://docs.rs/announce) | everything below |
| [`announce-std`](https://docs.rs/announce-std) | announce std/core traits (`announce_std::iter::Iterator!`) |

In `announce`:

- `#[announce]` and the trait `Announce` define announced items.
- [`announce::helper`](https://docs.rs/announce/latest/announce/helper/index.html)
  writes callbacks and checks, starts chains and redirects derives.
- [`announce::machinery`](https://docs.rs/announce/latest/announce/machinery/index.html)
  announces items that your own proc macro builds.
- `helper`, `machinery` and `type_leak` need the feature `proc-macro-api`, on by default; a
  `#![no_std]` crate that only defines items uses `default-features = false`.

`announce-core` and `announce-macros` are internal; depend on `announce`.
Minimum Rust version: 1.57.

## Performance

Traits with S methods each, K of them passed to one callback; rustc nightly,
dev profile. Lines: color = S (darker is larger), solid = announced or one
chain, dashed = plain or K separate calls; both axes logarithmic. Details and
how to run it: `bench/`.

<img src="https://raw.githubusercontent.com/yasuo-ozu/announce/main/bench/plots/defining-rmeta.svg" alt="Defining crate: .rmeta size against K" width="49%"> <img src="https://raw.githubusercontent.com/yasuo-ozu/announce/main/bench/plots/defining-time.svg" alt="Defining crate: rebuild time against K" width="49%">
<img src="https://raw.githubusercontent.com/yasuo-ozu/announce/main/bench/plots/calling-rmeta.svg" alt="Calling crate: .rmeta size against K" width="49%"> <img src="https://raw.githubusercontent.com/yasuo-ozu/announce/main/bench/plots/calling-time.svg" alt="Calling crate: rebuild time against K" width="49%">

- The cost is metadata of the defining crate: about 6 KiB per definition plus
  copies of the item (one copy for items above about 21 trait methods). Doc
  comments are not recorded.
- Executables are unaffected, and calling crates stay small (12–60 KiB of
  `.rmeta` at K = 96).
- `[profile.dev.build-override] opt-level = 3` in your workspace builds the
  proc macros optimized: 1,117 ms becomes 763 ms at S = 200, K = 96.

## Related crates

- **[macro_magic](https://crates.io/crates/macro_magic)** has the same core idea.
  announce adds continuation lists, definition-side cfg evaluation and checks,
  and type context handling.
- **[reify_types](https://crates.io/crates/reify_types)** makes a definition
  available when compiled code *runs*. announce delivers it at *compile time*.
