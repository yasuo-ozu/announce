#![doc = include_str!("../README.md")]
#![doc(html_logo_url = "https://raw.githubusercontent.com/yasuo-ozu/announce/main/logo.png")]
#![doc(html_favicon_url = "https://raw.githubusercontent.com/yasuo-ozu/announce/main/logo.png")]
#![no_std]

pub use announce_macros::announce;
pub use announce_macros::define_repeater;

pub use announce_macros::signature_only;

/// Marks an announced item.
///
/// `#[announce]` implements it for structs, enums and unions, and for `dyn Trait`
/// of simple dyn-compatible traits (see
/// [The `Announce` impl](macro@crate::announce#the-announce-impl)). There is no
/// impl with `emit_item = false` or `mock`.
///
/// A consumer can require `T: Announce` next to a trampoline call. That gives a
/// clear error when the callee was not announced.
#[cfg_attr(
    feature = "proc-macro-api",
    doc = "[`helper::check_type`] and [`helper::check_trait`] write that bound."
)]
#[cfg_attr(
    not(feature = "proc-macro-api"),
    doc = "`helper::check_type` and `helper::check_trait` write that bound (they need the feature `proc-macro-api`)."
)]
#[cfg_attr(
    announce_diagnostic,
    diagnostic::on_unimplemented(
        message = "`{Self}` is not announced: its definition has no `#[announce]`",
        label = "not announced, so it has no trampoline to call",
        note = "add `#[announce]` to the definition of `{Self}`; for items of std and core, use the stand-ins of the `announce-std` crate"
    )
)]
pub trait Announce {}

/// Re-export of [`type-leak`](https://docs.rs/type-leak), the library that the
/// `repeater` option and [`helper::TypeLeak`] are built on.
///
/// It is the version that `announce-macros` and `announce::helper` use. A macro
/// crate that works with the types announce leaks (for example with
/// [`helper::leak::StableReferrer`]) can name it through `announce`, and so
/// cannot end up with a second, incompatible copy.
///
/// Needs the feature `proc-macro-api`, which is on by default.
///
/// ```
/// let _leaker = announce::type_leak::Leaker::new();
/// ```
#[cfg(feature = "proc-macro-api")]
pub use type_leak;

// The docs of the module are those of `announce_core::helper`, inlined here.
#[cfg(feature = "proc-macro-api")]
#[doc(inline)]
pub use announce_core::helper;

#[cfg(feature = "proc-macro-api")]
pub mod machinery;

#[doc(hidden)]
pub mod __private {
    pub use announce_macros::{__announce_runner, finish, remove, same_context_impl, Capture};

    // The parts of the check of `same_context!` and `LeakFailure::assume_success`. It calls
    // `__announce_ctx` on a `Probe<R>`: inside the item's module the definition's private trait
    // answers `ThisCallInTheModule`; elsewhere only `ContextFallback` is found, after an autoref,
    // and answers `ThisCall`.

    /// The value that the check calls the method on.
    #[derive(Clone, Copy)]
    pub struct Probe<const R: u64>;

    /// The answer of the method of the definition's trait: the call is in the module of the item.
    pub struct ThisCallInTheModule;

    /// The answer of the fallback: it is not.
    pub struct ThisCall;

    /// In scope at every call, and found after the trait of the definition.
    pub trait ContextFallback {
        fn __announce_ctx(self) -> ThisCall;
    }

    impl<const R: u64> ContextFallback for &Probe<R> {
        fn __announce_ctx(self) -> ThisCall {
            ThisCall
        }
    }
}

/// A callback for the chain of a trampoline call: the call must be in the type context of the
/// items it passes, or it does not compile.
///
/// An item announced with `strict = false`, whose types do not resolve outside its module (see
/// [Items that resolve in their module only](macro@crate::announce#items-that-resolve-in-their-module-only)),
/// can only be used as written in that module, its function bodies and its submodules that
/// `use super::*`. A caller that wants a clear error when it is not puts `same_context!` in
/// the chain, in front of the macro that uses the items:
///
/// ```text
/// m::Item! { [m::Item!, announce::same_context!, my::callback!] ARGS }
/// ```
///
/// It emits a check for each item whose definition recorded a failure, and calls the next
/// element of the chain with its input unchanged. The check compiles where the names of the
/// item resolve, and anywhere else it is an error:
///
/// ```
/// mod m {
///     pub struct Local(pub u8);
///
///     #[announce::announce(strict = false)]
///     pub struct Item {
///         pub a: Local,
///     }
///
///     // In the module of `Item`: passes. (`same_context!` is the whole chain here.)
///     Item! { [Item!, announce::same_context!] }
/// }
/// ```
///
/// ```compile_fail,E0277
/// mod m {
///     pub struct Local(pub u8);
///
///     #[announce::announce(strict = false)]
///     pub struct Item {
///         pub a: Local,
///     }
/// }
///
/// // Outside the module: `Local` is not in scope here.
/// m::Item! { [m::Item!, announce::same_context!] }
/// ```
///
/// There is one error for each cause, at the type in the definition (all of it is underlined,
/// as in the error of a strict definition), worded like the errors of type-leak. The call is in
/// rustc's note about the bound:
///
/// ```text
/// error[E0277]: use absolute path: 'Local'
///   --> src/main.rs:6:16
///    |
///  6 |         pub a: Local,
///    |                ^^^^^ relative type, not in scope where `m::Item!` is called
///    |
///    = note: `m::Item` is used outside its module `m`
///    = note: write `Local` with an absolute path (e.g. `crate::Local`), or add `repeater = ..` to the `#[announce]` of `Item`
/// note: required by a bound in `Check::used_outside_its_module`
///   --> src/main.rs:11:4
///    |
/// 11 | m::Item! { [m::Item!, announce::same_context!] }
///    |    ^^^^ required by this bound in `Check::used_outside_its_module`
/// ```
///
/// When the item is in another crate, the main location is in that crate's source, and an
/// editor may show only the note with your call.
///
/// A callback later in the chain that emits the item (`Entry::item_resolved`) outside its
/// module gets rustc's own "cannot find type" errors for the names of the item first, at the
/// same tokens, and the error of this check follows. When the item has a relative trait path in
/// a bound (`trait Sub: Base`, `T: Base`), rustc stops at its "cannot find trait" error (E0405)
/// before it reaches this check, so only rustc's error appears. `same_context!` on its own,
/// without a callback that emits the item, is not affected.
///
/// The check is a `const _` item, so it never goes inside an `impl` or a `trait` block
/// ("`const` items in this context need a name"). Put it at module level beside the impl; its
/// answer depends only on the module of the call. Call the callback in the impl without it:
///
/// ```
/// mod m {
///     pub struct Local(pub u8);
///
///     #[announce::announce(strict = false)]
///     pub struct Item {
///         pub a: Local,
///     }
///
///     pub struct S;
///
///     // A callback of the plain kind that makes a method.
///     macro_rules! method {
///         (@announce_format "1" [$($list:tt)*] $($rest:tt)*) => {
///             pub fn made_from_item(&self) -> u8 {
///                 7
///             }
///         };
///     }
///
///     // At module level: the check.
///     Item! { [Item!, announce::same_context!] }
///
///     // The real call, in the impl: no check.
///     impl S {
///         Item! { [Item!, method!] }
///     }
/// }
///
/// assert_eq!(m::S.made_from_item(), 7);
/// ```
///
/// The check sees the items that have arrived at its place in the chain, not ones that a later
/// callback fetches (`Input::gather`), so list the trampolines in front of it. A callback of
/// your own that wants the same check uses `announce::helper::Entry::item_resolved`.
#[macro_export]
macro_rules! same_context {
    ($($input:tt)*) => {
        $crate::__private::same_context_impl! { $crate, $($input)* }
    };
}
