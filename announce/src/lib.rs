//! Definition-side trampolines for proc macros: `#[announce]` makes a
//! definition known to proc macros in any crate.
//!
//! `#[announce]` on a struct, enum, union, trait, type alias or inline module
//! records the definition in a `macro_rules!` of the same name. Invoking that
//! macro with a continuation list passes the definition on to the next macro
//! in the list, as tokens:
//!
//! ```
//! #[announce::announce]
//! pub trait Greet { fn hi(&self) -> &'static str; }
//!
//! macro_rules! show {
//!     ([$($list:tt)*] $($key:ident)::+ ! => [$($meta:tt)*] { $($item:tt)* } $($rest:tt)*) => {
//!         const SHOWN: &str = stringify!($($item)*);
//!     };
//! }
//! Greet! { [Greet!, show!] }
//! assert!(SHOWN.contains("trait Greet"));
//! ```
//!
//! # Wire protocol
//!
//! ```text
//! INPUT  := ('(' OPTION,* ')')? '[' CONT (',' CONT)* ','? ']' ENTRY* REST
//! OPTION := expr                 (global: forwarded by every hop; see Position)
//! CONT   := path '!' | '#[derive(' path ')]' IDENT? | '#[' path ... ']' IDENT?
//!           (a bang element must include the `!`)
//! ENTRY  := CONT '=>' '[' META,* ']' '{' ITEM '}'
//! ```
//!
//! Consumers parse `ENTRY*` greedily, so a `REST` that itself starts like an
//! entry (`path! => [..] {..}`) must be wrapped, e.g. in parentheses.
//!
//! The head of the list is always the macro being invoked. It drops itself,
//! prepends `HEAD => [META] { ITEM }` right after the list and invokes the
//! next element. `REST` is never touched. Entries are therefore newest first.
//!
//! # Options
//!
//! `#[announce(key = value, ..)]` sets options; any other entry is a *check*:
//!
//! - `macro_name = Ident`: name of the emitted macro (default: the item's).
//! - `export_name = Ident`: name of the hidden `#[macro_export]` macro.
//! - `crate = path`: path to this crate for generated code. By default it is
//!   read from the caller's `Cargo.toml`, so a renamed dependency
//!   (`ann = { package = "announce", .. }`, also workspace-inherited) works
//!   without this option; it falls back to `::announce`. Library wrappers that
//!   re-export announce pass their re-export here.
//! - `repeater = crate::path::Repeater`: required when the item has types that do
//!   not resolve outside this crate (relative paths such as `Local`). Your
//!   trait `pub trait Repeater<const ID: u64, const I: usize, G: ?Sized> { type Type: ?Sized; }`
//!   gets one impl per such type, and the entry's META gets
//!   `type_leak(repeater = .., id = .., generics = .., types = [..])`; callbacks
//!   rewrite the (original) payload with `announce_helper::TypeLeak::apply`.
//! - Trait paths in the item (supertraits, bounds) must be absolute
//!   (`::core::fmt::Display`, `crate::Base`): a repeater covers types only.
//! - `allow_primitive = <bool>` (default `true`), `allow_std_prelude = <bool>`
//!   (default `false`): whether primitives / std prelude names count as
//!   resolving everywhere when deciding whether a `repeater` is needed (the
//!   type-leak `LeakerConfig`). type-leak 0.7 matches prelude names only
//!   without generic arguments: `String` passes, `Vec<u8>` does not.
//! - `mock = path`: the item is a copy of `path` (see `announce-std`); the copy is
//!   type-checked in a hidden module and the entry gets `mock(path)`.
//! - `emit_item = false`: do not re-emit the item itself, only the
//!   trampoline. For a derive that wants to announce its input: emit
//!   `#[announce(emit_item = false, ..)] #input` from the derive.
//! - `marker = true|false`: force or suppress the [`Marker`] impl.
//! - `meta = path(args)`: metadata for consumers. Listed in the entry's META
//!   like a check, but never invoked. May be given more than once.
//! - `path` / `path(args)`: a check. `path! { (args), ITEM }` is invoked at the
//!   definition site, and the entry's META lists it.
//!
//! Nested `#[cfg]`/`#[cfg_attr]` in the item are evaluated in the defining
//! crate before the payload is recorded.
//!
//! # Position
//!
//! A bang chain expands hop by hop into the final callback's output, so the
//! first call can be anywhere the callback's output fits:
//!
//! | position | bang chain (any payload) | derive/attribute route |
//! |---|---|---|
//! | item / statement | yes | yes |
//! | expression, type, pattern | yes | no |
//!
//! A payload containing `$` (e.g. a module holding a `macro_rules!`) is
//! un-escaped by the trampoline itself (it re-invokes itself once with a `$`
//! argument), so no helper items are emitted. The derive/attribute route emits
//! an item (the dummy enum) and has no expression form; the global option
//! `(expr)` turns that into a clear error and otherwise has no effect.
//!
//! # Several announcements of one item
//!
//! Each announcement re-exports a macro under a name and, when it emits the
//! item, implements [`Marker`]. A second announcement of the same item (for
//! example `#[announce]` plus a library derive that announces its input with
//! `emit_item = false`) must use a different `macro_name`; `emit_item = false`
//! implies `marker = false`, so the `Marker` impls do not conflict.
//!
//! # Libraries with their own type-context handling
//!
//! `#[announce]` rejects items with relative types (unless `repeater = ..`)
//! and relative trait paths, because a caller in another crate could not
//! resolve them. A library that already makes such paths resolve in its own
//! way (its own repeater or interning scheme, types used only as lookup keys,
//! ..) announces a **wrapped copy** instead. That is the supported pattern:
//!
//! ```text
//! // what the library's attribute emits for `pub trait MyTrait: Base { fn f(&self) -> Local; }`
//! pub trait MyTrait: Base { fn f(&self) -> Local; }      // the real item, unchanged
//!
//! #[::my_lib::__private::announce::announce(
//!     crate = ::my_lib::__private::announce,
//!     emit_item = false,                                  // only the trampoline (implies marker = false)
//!     meta = ::my_lib::__private::info(..),               // the library's own type context
//! )]
//! pub mod MyTrait {                                       // same name and visibility as the item
//!     pub trait MyTrait: Base { fn f(&self) -> Local; }   // the copy
//! }
//! ```
//!
//! Rules:
//! - The wrapper is an **inline module with the item's name and visibility**,
//!   containing exactly that item. The trampoline is therefore named like the
//!   item (`MyTrait!`).
//! - Inline modules are not analysed, so no type-context check runs and no
//!   repeater table is built. **The library is responsible** for every path in
//!   the copy resolving where its callbacks use it.
//! - `emit_item = false` is required: the real item is emitted separately (so
//!   it is not wrapped for users), and no `Marker` is implemented for it.
//! - The callback receives `mod MyTrait { <item> }` and unwraps it (the only
//!   item of the module). cfgs are evaluated and `crate::` paths rewritten as
//!   for any payload.
//!
//! sumtype, newer-type and flat_enum use this pattern in the measurement
//! ports (`measurement/` in the repository). Items without a library behind
//! them should use absolute paths or `repeater = ..`.
//!
//! # Transformers
//!
//! Rewriting a definition is the *caller's* choice: a transformer is a list
//! element placed right after the definition it rewrites, and it forwards to
//! the rest of the list. [`signature_only!`] is provided; write your own with
//! `announce_helper::transformer!`.
//!
//! ```text
//! Partly! { [Partly!, ::announce::signature_only!, my_callback!] REST }
//! ```

#![no_std]

pub use announce_macros::announce;

/// Transformer: strips the newest entry's item down to its signatures.
/// Put it in the continuation list right after the definition:
/// `F! { [F!, ::announce::signature_only!, callback!] .. }`.
pub use announce_macros::signature_only;

/// Implemented by `#[announce]` for its structs, enums and unions, and for traits
/// that are certainly dyn-compatible (`impl Marker for dyn Trait`).
///
/// Consumers may check `T: Marker` next to a trampoline call to get a clearer
/// error when the callee is not `#[announce]` (best-effort; see
/// `announce_helper::check_type` / `check_trait`).
#[cfg_attr(
    announce_diagnostic,
    diagnostic::on_unimplemented(
        message = "`{Self}` is not marked `#[announce]`",
        label = "no `#[announce]` trampoline for this item",
        note = "add `#[announce]` to its definition; for std/core items use the `announce-std` crate"
    )
)]
pub trait Marker {}

#[doc(hidden)]
pub mod __private {
    pub use announce_macros::{remove, Capture};
}
