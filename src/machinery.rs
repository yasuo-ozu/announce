//! What `#[announce(..)]` does, as functions that another proc-macro crate can call.
//!
//! A library that builds or rewrites items in its own proc macros can announce
//! them without emitting the attribute `#[announce(..)]`. It calls [`announce`] on
//! the tokens of the item and returns the result, which is exactly what the
//! attribute produces. The options are those of the
//! [`#[announce]`](macro@crate::announce#options) attribute. A derive macro cannot do
//! this, because a derive cannot change its item: ask your users for `#[announce]`, or
//! make an attribute macro.
//!
//! This module needs the feature `proc-macro-api` of `announce`, which is on by
//! default (`default-features = false` leaves it out, for `no_std` definitions).
//!
//! ```toml
//! # Cargo.toml of the library's proc-macro crate
//! announce = "0.1"
//! ```
//!
//! ```ignore
//! #[proc_macro_attribute]
//! pub fn my_attr(attr: TokenStream, item: TokenStream) -> TokenStream {
//!     // Parse and rewrite the item ...
//!     let item: proc_macro2::TokenStream = rewrite(item.into());
//!
//!     // ... then announce it, with the options of `#[announce(..)]`.
//!     let mut opts: announce::machinery::Options = syn::parse(attr).unwrap();
//!     opts.krate = Some(syn::parse_quote!(::my_lib::announce));
//!     announce::machinery::announce(quote::ToTokens::to_token_stream(&opts), item).into()
//! }
//! ```
//!
//! # The path to the `announce` crate
//!
//! The generated code names the `announce` crate: for the hidden derive and
//! attribute that capture the item (`announce::__private`), and for the
//! [`Announce`](crate::Announce) impl. That path is the `crate = ..` option
//! ([`Options::krate()`]). Without the option it is read from the `Cargo.toml` of
//! the crate being compiled, which is the crate that uses the library's macro. It
//! is the name that crate gives to its `announce` dependency, or `::announce` if
//! there is none.
//!
//! So a library whose users do not depend on `announce` must pass the option, with
//! a path that its users can reach. The library re-exports the crate
//! (`#[doc(hidden)] pub use announce;`, next to the re-export of its macros), and
//! the macro passes `crate = ::my_lib::announce`. If the users do depend on
//! `announce`, nothing is needed.
//!
//! # Output
//!
//! The output is the output of the attribute, token for token. If the item has no other
//! outer attribute, nothing can expand after the attribute: the output is the item as given
//! (unless `emit_item = false` or `mock`), followed by a hidden `#[derive]` on a dummy
//! enum that carries the item and emits the trampoline. The derive runs after rustc has
//! evaluated the `#[cfg]` attributes in the item, in the crate that expands the macro. So
//! the item may hold `#[cfg]` attributes, and the macro must be expanded in item position.
//!
//! ```
//! use announce::machinery::announce;
//! use proc_macro2::TokenStream;
//!
//! let attr: TokenStream = "crate = ::my_lib::announce, macro_name = Hello".parse().unwrap();
//! let item: TokenStream = "pub struct S { pub a: u8 }".parse().unwrap();
//! let out = announce(attr, item).to_string();
//! assert!(out.contains(":: my_lib :: announce :: __private :: Capture"));
//! ```
//!
//! # Several announcements
//!
//! [`announce`] goes the way of the attribute in this as well. If the item has other outer
//! attributes, the output is the item with the hidden attribute
//! `#[<crate path>::__private::_actually_run_announce(..)]`, the *runner*, as its last
//! attribute, and nothing else. It holds the options (and the version of its encoding),
//! and when it expands, after everything else on the item, it makes the trampoline. If the
//! item has a runner already, because another announcement was expanded first, the options
//! are merged into it. The output of a macro that calls [`announce`] must be an item that
//! the compiler expands, so that the runner is expanded. An announcement with `mock` or
//! `emit_item = false` is made at once. See [Several announcements on one
//! item](macro@crate::announce#several-announcements-on-one-item).
//!
//! ```
//! use announce::machinery::announce;
//! use proc_macro2::TokenStream;
//!
//! let attr: TokenStream = "crate = ::my_lib::announce, meta = my_lib::info".parse().unwrap();
//! let item: TokenStream = "#[derive(Debug)] pub struct S;".parse().unwrap();
//! let out = announce(attr, item).to_string();
//! assert!(out.contains("__private ::_actually_run_announce"));
//! assert!(out.contains("_announce_format_version = \"1\""));
//! assert!(!out.contains("Capture"));
//! ```
//!
//! # Checks
//!
//! With checks in the options (`my_lib::check`), the output is not that yet. It is
//! a call of the first check, with the attribute *query* that asks each check
//! which attributes it owns (the protocol is in
//! [`announce::helper::attrs`][attrs]). The last element of the query's list is
//! `<crate path>::__private::finish!`, a hidden macro of the `announce` crate. The
//! path in `crate = ..` must reach it, which a library that re-exports `announce`
//! does. `finish!` expands to the output above, without the attributes that the
//! checks own. [`finish`] is the function behind that macro; a library does not
//! call it.
//!
//! ```
//! use announce::machinery::announce;
//! use proc_macro2::TokenStream;
//!
//! let attr: TokenStream = "crate = ::my_lib::announce, ::my_lib::check".parse().unwrap();
//! let item: TokenStream = "pub struct S { #[my_lib(skip)] pub a: u8 }".parse().unwrap();
//! let out = announce(attr, item).to_string();
//! assert!(out.starts_with(":: my_lib :: check ! { @ __announce_attrs"));
//! // The list starts with the macro that is called, and ends with `finish!`.
//! assert!(out.contains("[:: my_lib :: check ! , :: my_lib :: announce :: __private :: finish !]"));
//! ```
//!
//! # The repeater trait
//!
//! [`define_repeater`] does what [`announce::define_repeater!`](macro@crate::define_repeater)
//! does. Use it in a library whose own macro defines the repeater trait in the
//! users' crates. It needs Rust 1.59 where it is expanded.
//!
//! # Errors
//!
//! [`announce`] returns an error as a `compile_error!` invocation in the tokens, which a
//! proc macro can return as it is. [`try_announce`] returns the `syn::Error`, to combine
//! it with others or to report it with another span. The API uses the types of
//! `proc-macro2` 1 and `syn` 2.
//!
//! [attrs]: crate::helper::attrs
pub use announce_core::{
    announce, define_repeater, finish, try_announce, try_define_repeater, try_finish, Check,
    Layout, Options, SINGLE_COPY_THRESHOLD,
};
