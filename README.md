# announce

Make a definition visible to proc macros in any crate.

A proc macro only sees the tokens it is applied to. Put `#[announce]` on a
definition, and any crate can ask for its tokens. `#[announce]` keeps the item
and emits a `macro_rules!` with the item's name. Calling that macro passes the
definition, as tokens, to a callback macro. It works on a struct, enum, union,
trait, type alias or inline `mod`.

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

`META` lists what the definer attached (checks, `meta` entries). It is empty
without options. In the item, `#[cfg]` is already evaluated in the defining crate
and `crate::` reads `$crate::`. Callers and callbacks need no dependency on
announce.

For a struct, enum or union you write yourself, derive it instead. The derive
also implements the [`Announce`](https://docs.rs/announce/latest/announce/trait.Announce.html)
trait. Options go in a helper attribute (not `mock` or `emit_item`):

```rust
#[derive(announce::Announce)]
#[announce(meta = my_lib::info(version = 1))]
pub struct Point { pub x: i32, pub y: i32 }
```

## Crates

| crate | use it to |
|---|---|
| `announce` | define items: `#[announce]`, `#[derive(Announce)]`, trait `Announce` |
| `announce-helper` | write callbacks, start chains, redirect derives, check input |
| `announce-std` | announce std/core traits (`announce_std::iter::Iterator!`) |

## Calling a trampoline

```text
path::Item! { [path::Item!, other::Def!, cb!] REST }
// cb! { [cb!] other::Def! => [META] { DEF } path::Item! => [META] { ITEM } REST }
```

The list names the trampolines to call, then the callback. Each macro adds its
entry and calls the next, so `cb!` runs once, with every definition. The newest
entry comes first and `REST` is never touched. Parsers read entries greedily:
wrap a `REST` that starts like one. A list element is `path!`,
`#[derive(path)] Name` or `#[attr] Name`. A leading `(..)` group of global
options is forwarded unchanged by every hop.

## Features

- Works in any position: item, statement, expression, type, pattern, also for
  items containing `$`. The derive and attribute route works in item position only.
- Evaluates `#[cfg]` in the defining crate. `crate::` paths keep pointing there.
- Checks at the definition: `#[announce(my_lib::no_tuple_structs)]`.
- Metadata for consumers: `#[announce(meta = my_lib::info(k = v))]`.
- Transformers chosen by the caller. A transformer is a list element right after
  the definition. It rewrites the entry and forwards to the rest, for example
  `[Partly!, ::announce::signature_only!, cb!]`. Use
  [`signature_only!`](https://docs.rs/announce/latest/announce/macro.signature_only.html)
  or write your own with `announce_helper::transformer!`.
- A derive or attribute may own the name: rename the trampoline with
  `#[announce(macro_name = ParseAnnounce)]`. That derive then forwards to it with
  `announce_helper::redirect`.

## Type context

The item's tokens are used again in another crate, so its paths must resolve
there. `crate::` and absolute paths (`::std::vec::Vec<u8>`) do. A relative type
such as `Local` needs `repeater = crate::Repeater`, a trait of the defining
crate that carries the type to the callback (`announce_helper::TypeLeak::apply`).
Trait paths (supertraits, bounds) must be absolute (`::core::fmt::Display`). The
[`#[announce]` docs](https://docs.rs/announce/latest/announce/attr.announce.html#type-context)
list what resolves, what a repeater is, and what is an error.

A library with its own type handling announces a copy wrapped in a same-named
inline `mod`, with `emit_item = false`. Only the trampoline is emitted, not the
`mod`, so it does not clash with the real item. Modules are not analysed, so the
library makes every path resolve. The callback unwraps the module. Announcing one
item twice needs a different `macro_name` each time.

```text
#[announce(emit_item = false, meta = my_lib::info(..))]
pub mod MyTrait { pub trait MyTrait: Base { fn f(&self) -> Local; } }
```

All options are listed in the
[`#[announce]` docs](https://docs.rs/announce/latest/announce/attr.announce.html).

## Performance

Measured with `bench/run.py` (median of 5 runs, x86_64). S is the number of
methods per trait, K the number of definitions passed to one callback. Token
counts are of the expanded crate; times are rebuilds of that crate alone.

| S | K | plain tokens | announced tokens | callback receives | plain build | announced build | calling crate |
|---:|---:|---:|---:|---:|---:|---:|---:|
| 1 | 1 | 48 | 1,335 | 37 | 165 ms | 125 ms | 154 ms |
| 100 | 64 | 83,487 | 660,063 | 84,169 | 225 ms | 907 ms | 283 ms |
| 1000 | 16 | 208,095 | 1,475,439 | 208,249 | 412 ms | 1,965 ms | 364 ms |
| 1000 | 64 | 832,287 | 5,901,663 | 832,969 | 1,821 ms | 8,417 ms | 1,328 ms |

- Each definition adds about 1,200 tokens plus about 7 times the item to the
  defining crate (the generated macro holds 6 copies of the item). Build time
  stays close to the plain crate except for very large definitions.
- The callback receives only the item plus about 11 tokens per definition.
- The calling crate stays near its baseline until the definitions reach about
  100,000 tokens.

## Related crates

- **[macro_magic](https://crates.io/crates/macro_magic)** has the same core idea:
  record an item in a macro, import its tokens by path. announce adds
  continuation lists (one callback gets many definitions), definition-side cfg
  evaluation and checks, type context handling, and a plain protocol with no
  dependency on announce.
- **[reify_types](https://crates.io/crates/reify_types)** makes a definition
  available when compiled code *runs*, not during macro expansion. announce
  delivers it at *compile time*.
