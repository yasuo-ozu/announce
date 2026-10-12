//! What `#[announce(..)]` and `#[derive(Announce)]` do, as functions that
//! another proc-macro crate can call.
//!
//! A library that builds or rewrites items inside its own proc macros can make
//! them announced without emitting the attribute `#[announce(..)]`: it calls
//! [`announce`] on the tokens of the item and returns the result. The result is
//! exactly what the attribute produces, so the trampoline works as for any other
//! announced item. For a struct, enum or union a derive can call
//! [`derive_announce`] or [`derive_announce_with`] instead.
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
//! # The repeater trait
//!
//! [`define_repeater`] does what `announce::define_repeater!` does: it returns
//! the definition of the repeater trait that `repeater = ..` needs (type-leak's
//! `repeater_definition()`), for a library whose own macro defines it in the
//! users' crates. It needs Rust 1.59 where it is expanded.
//!
//! # The path to the `announce` crate
//!
//! The generated code names the `announce` crate, for the hidden derive and
//! attribute that capture the item (`announce::__private`) and for the
//! [`Announce`](crate::Announce) impl. It is the path in the `crate = ..`
//! option ([`Options::krate()`]). Without that option it is read from the
//! `Cargo.toml` of the crate being compiled, the one that uses the library's
//! macro: the name it gives to its `announce` dependency, or `::announce` if
//! there is none.
//!
//! So a library whose users do not depend on `announce` themselves has to pass
//! the option, with a path that its users can reach. The library re-exports the
//! crate (`#[doc(hidden)] pub use announce;` in the library crate, next to the
//! re-export of its macros) and the macro passes `crate = ::my_lib::announce`.
//! If the users do depend on `announce`, nothing is needed.
//!
//! # Output
//!
//! It is the output of the attribute, token for token: the item as given
//! (unless `emit_item = false` or `mock`), followed by a hidden `#[derive]` on a
//! dummy enum that carries the item. The derive runs after rustc has evaluated
//! the `#[cfg]` attributes in the item, in the crate that expands the macro, and
//! then emits the trampoline `macro_rules!`. So the item may hold `#[cfg]`
//! attributes, and the macro has to be expanded in item position.
//!
//! # Checks and the attribute query
//!
//! With checks in the options (`my_lib::check`), the output is not that yet: it
//! is a call of the first check that carries the *query*, the question which
//! attributes each check owns (`announce_helper::attrs` has the protocol). The
//! last element of the query is `<crate path>::__private::finish!`, a hidden
//! macro of the `announce` crate that the path in `crate = ..` must reach (a
//! library that re-exports `announce` does). It expands to the output above,
//! without the attributes the checks own in the item it emits. [`finish`] is the
//! function behind that macro; a library does not call it. The same holds for
//! [`derive_announce`] and [`derive_announce_with`], where the item cannot be
//! changed: the library declares its attributes on its own derive, and the query
//! only lists them in `META`.
//!
//! ```
//! use announce::machinery::announce;
//! use proc_macro2::TokenStream;
//!
//! let attr: TokenStream = "crate = ::my_lib::announce, ::my_lib::check".parse().unwrap();
//! let item: TokenStream = "pub struct S { #[my_lib(skip)] pub a: u8 }".parse().unwrap();
//! let out = announce(attr, item).to_string();
//! assert!(out.starts_with(":: my_lib :: check ! { @ __announce_attrs"));
//! assert!(out.contains(":: my_lib :: announce :: __private :: finish !"));
//! ```
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
//! # Errors
//!
//! [`announce`] and [`derive_announce`] return an error as a `compile_error!`
//! invocation in the tokens, which a proc macro can return as it is. Their `try_`
//! forms return the `syn::Error`, to combine it with others or to report it
//! with another span. The API uses the types of `proc-macro2` 1 and `syn` 2.
pub use announce_core::{
    announce, define_repeater, derive_announce, derive_announce_with, finish, try_announce,
    try_define_repeater, try_derive_announce, try_finish, Check, Layout, Options,
    SINGLE_COPY_THRESHOLD,
};
