//! What `#[announce(..)]` and `#[derive(Announce)]` do, as functions that another
//! proc-macro crate can call.
//!
//! A library that builds or rewrites items in its own proc macros can announce
//! them without emitting the attribute `#[announce(..)]`. It calls [`announce`] on
//! the tokens of the item and returns the result, which is exactly what the
//! attribute produces. For a struct, enum or union, a derive can call
//! [`derive_announce`] or [`derive_announce_with`] instead. The options are those
//! of the [`#[announce]`](macro@crate::announce#options) attribute.
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
//! The output is the output of the attribute, token for token: the item as given
//! (unless `emit_item = false` or `mock`), followed by a hidden `#[derive]` on a
//! dummy enum that carries the item. The derive runs after rustc has evaluated the
//! `#[cfg]` attributes in the item, in the crate that expands the macro. Then it
//! emits the trampoline. So the item may hold `#[cfg]` attributes, and the macro
//! must be expanded in item position.
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
//! # Checks
//!
//! With checks in the options (`my_lib::check`), the output is not that yet. It is
//! a call of the first check, with the attribute *query* that asks each check
//! which attributes it owns (the protocol is in
//! [`announce_helper::attrs`][attrs]). The last element of the query's list is
//! `<crate path>::__private::finish!`, a hidden macro of the `announce` crate. The
//! path in `crate = ..` must reach it, which a library that re-exports `announce`
//! does. `finish!` expands to the output above, without the attributes that the
//! checks own. [`finish`] is the function behind that macro; a library does not
//! call it.
//!
//! [`derive_announce`] and [`derive_announce_with`] send the same query. A derive
//! cannot change its item, so the library declares the attributes on its own
//! derive, and the query only lists them in `META`.
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
//! [`announce`] and [`derive_announce`] return an error as a `compile_error!`
//! invocation in the tokens, which a proc macro can return as it is. Their `try_`
//! forms return the `syn::Error`, to combine it with others or to report it with
//! another span. The API uses the types of `proc-macro2` 1 and `syn` 2.
//!
//! [attrs]: https://docs.rs/announce-helper/latest/announce_helper/attrs/index.html
pub use announce_core::{
    announce, define_repeater, derive_announce, derive_announce_with, finish, try_announce,
    try_define_repeater, try_derive_announce, try_finish, Check, Layout, Options,
    SINGLE_COPY_THRESHOLD,
};
