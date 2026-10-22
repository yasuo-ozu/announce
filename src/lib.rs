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
    pub struct Probe<const R: u64>;
    pub struct ThisCallInTheModule;
    pub struct ThisCall;

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
/// can be used as written only there: in its module, its function bodies and its submodules that
/// `use super::*`. A caller that wants a clear error elsewhere puts `same_context!` in the chain,
/// in front of the macro that uses the items. It emits a check for each item whose definition
/// recorded a failure and calls the next element with its input unchanged:
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
/// What decides is the module where the call is written, not how the name of the trampoline got
/// there. A module that imports `Item` with `use` is still outside, even when `Local` is in scope
/// in it:
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
/// mod other {
///     use super::m::*;
///
///     Item! { [Item!, announce::same_context!] }
/// }
/// # fn main() {}
/// ```
///
/// There is one error for each cause, at the type in the definition (all of it underlined),
/// worded like the errors of type-leak, with the call in rustc's note about the bound. The texts
/// need Rust 1.78 (`#[diagnostic::on_unimplemented]`); 1.85 shortens the error:
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
/// The check is a `const _` item, so it never goes inside an `impl` or a `trait` block. Put it at
/// module level beside the impl (its answer depends only on the module of the call), and call
/// the callback in the impl without it:
///
/// ```text
/// Item! { [Item!, announce::same_context!] }          // module level: the check
/// impl S { Item! { [Item!, my::method_callback!] } }  // the real call, no check
/// ```
///
/// The check sees the items that have arrived at its place in the chain, not ones that a later
/// callback fetches (`Input::gather`), so list the trampolines in front of it. A callback of your
/// own that wants the same check uses `announce::helper::Entry::item_resolved`.
#[macro_export]
macro_rules! same_context {
    ($($input:tt)*) => {
        $crate::__private::same_context_impl! { $crate, $($input)* }
    };
}
