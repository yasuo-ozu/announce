# announce

Definition-side trampolines for proc macros: `#[announce]` makes a definition
known to proc macros in any crate.

A proc macro only sees the tokens it is applied to. When `#[my_macro(Trait)]`
needs the *definition* of `Trait` from another crate, the definition has to
travel to it. `#[announce]` records the definition in a `macro_rules!` of the
same name, and hands it to whichever macro asks:

```rust
#[announce::announce]
pub trait Greet {
    fn hi(&self) -> &'static str;
}

// In any crate: ask `Greet!` to pass its definition on to `my_callback!`.
path::to::Greet! { [path::to::Greet!, my_callback!] extra tokens }

// expands to
my_callback! { [my_callback!] path::to::Greet! => [] { pub trait Greet { .. } } extra tokens }
```

Continuation lists chain: `[A!, B!, cb!]` collects the definitions of `A` and `B`
before calling `cb!`. Crates that call trampolines need **no dependency** on
announce, because the generated macros are plain `macro_rules!`.

## Crates

| crate | for | contents |
|---|---|---|
| `announce` | crates that define items | `#[announce]`, `Marker` |
| `announce-helper` | proc-macro crates that consume trampolines | parse `Input`/`Entry`, start chains (`ContList::invoke`), `fetch`/`forward`, derive `redirect`, check helpers |
| `announce-std` | anyone | announced copies of 72 std/core traits (`announce_std::iter::Iterator!`, ...) |
| `announce-macros` | (internal) | the proc macros behind `announce` |

## Features

- Struct, enum, union, trait, type alias and inline `mod` items.
- Nested `#[cfg]`/`#[cfg_attr]` are evaluated in the **defining** crate before
  the definition is recorded.
- `$` and `$(..)*` inside the item survive (e.g. a `mod` holding a `macro_rules!`).
- `crate::` paths in the item keep referring to the defining crate. Other
  relative types (e.g. `Local` from a `use`, or prelude names like `Vec`; write
  `::std::vec::Vec` instead) require a local `#[announce(repeater = crate::Repeater)]`:
  the entry then carries `type_leak(..)` information, and a callback resolves
  them from any crate with `announce_helper::TypeLeak::apply`.
  Libraries that handle type context themselves announce a copy wrapped in a
  same-named inline `mod` (`emit_item = false`), which is not checked (see the
  crate docs).
- Relative trait paths in a definition (supertraits, bounds) are errors: write
  them absolutely (`::core::fmt::Display`, `crate::Base`).
- Any position (item, expression, type, pattern): a chain expands into the
  final callback's output, also when the definition contains `$`.
- Checks run at the definition site: `#[announce(my_lib::no_tuple_structs)]`
  invokes `my_lib::no_tuple_structs! { (), ITEM }` where the item is defined,
  so errors surface there and not in a downstream expansion.
- Metadata for consumers: `#[announce(meta = my_lib::info(k = v))]`.
- Transformers chosen by the caller: `Partly! { [Partly!, ::announce::signature_only!,
  cb!] }` rewrites the definition on its way to `cb!`. Write your own with
  `announce_helper::transformer!`.
- Consumer helpers: typed `key = value` META/REST parsing (`KvArgs`),
  `Input::gather` ("fetch these, then call me once"), the `callback!` entry
  macro, and deterministic type-leak numbering (`leak::StableReferrer`,
  feature `type-leak`).
- Routing through derive or attribute macros when the macro-namespace name is
  already taken (`#[announce(macro_name = ParseAnnounce)]` plus an
  announce-aware derive that calls `announce_helper::redirect`).
- Options: `macro_name`, `export_name`, `crate`, `mock`, `repeater`,
  `allow_primitive`, `allow_std_prelude`, `emit_item`, `marker`, `meta`. See the `announce` crate docs.

MSRV: Rust 1.71.

## Related crates

- **[macro_magic](https://crates.io/crates/macro_magic)** uses the same
  underlying idea: `#[export_tokens]` records an item in a `macro_rules!`
  callback, and proc macros wrapped in `#[import_tokens_attr]` /
  `#[import_tokens_proc]` receive the tokens of an item named by path. announce
  builds on that pattern and differs in:
  - **Continuation lists:** a call names any number of definitions and
    transformers, `[A!, B!, ::announce::signature_only!, cb!]`, and the callback
    runs once with all of them. In macro_magic, a proc macro imports the tokens
    of one item.
  - **Definition-side processing:** nested `#[cfg]`/`#[cfg_attr]` are evaluated
    in the defining crate, `crate::` becomes `$crate::`, `$` in the item is
    escaped, and checks (`#[announce(my_lib::check)]`) report errors at the
    definition. macro_magic forwards the item's tokens as written.
  - **Type context:** relative types are rejected or carried through a repeater
    table (`repeater =`, `TypeLeak::apply`), so the definition resolves in the
    consuming crate.
  - **Plain protocol:** callers and callbacks need no dependency on announce;
    callbacks are ordinary `macro_rules!` or proc macros that parse a documented
    wire format (`announce-helper` is optional).
  - **Extras:** `meta =` for consumers, routing through derive or attribute
    macros when the name is taken, `mock =` and announced std traits
    (`announce-std`), and an error naming the missing `#[announce]` when a
    callee is not a trampoline.
- **[reify_types](https://crates.io/crates/reify_types)** takes a different
  route: `#[derive(Reify)]` generates `fn reify() -> syn::DeriveInput` for a
  struct or enum, so its definition is available when compiled code *runs*
  (build tools, code generators, tests). A proc macro cannot call that function
  during expansion, and traits, type aliases and modules are not covered.
  announce delivers definitions to macros at *compile time* instead.

## Layout

- `DESIGN.md`: the detailed design, decisions and verified facts.
- `measurement/`: results of porting sumtype, newer-type, decycle and flat_enum
  to announce (measurement only; those crates were not changed).
- `tests/`: integration tests (`defs`, `derive`, `user`) and trybuild UI tests
  (`tests/ui`, run separately because trybuild needs a newer rustc).
- `tools/gen_std.py`: regenerates announce-std from newer-type-std's trait copies.
