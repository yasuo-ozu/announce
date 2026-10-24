//! The proc macros of [`announce`](https://docs.rs/announce). Use them through the
//! `announce` crate: the paths here are not a stable interface. The code behind
//! the macros is in `announce-core`. The reference of
//! [`#[announce]`](https://docs.rs/announce/latest/announce/attr.announce.html) is in the
//! `announce` crate.

use proc_macro::TokenStream;

#[proc_macro_attribute]
pub fn announce(attr: TokenStream, item: TokenStream) -> TokenStream {
    announce_core::announce(attr.into(), item.into()).into()
}

/// Defines the repeater trait that `#[announce(repeater = ..)]` needs.
///
/// ```ignore
/// // In the defining crate, at a path that callers can reach (here the crate root).
/// announce::define_repeater!(pub Repeater);
///
/// #[announce::announce(repeater = crate::Repeater)]
/// pub struct Holder { pub a: Local }
/// ```
///
/// The input is a visibility and a name (`pub Repeater`, `pub(crate) Rep`, `Rep`).
/// The output is the trait that `type_leak::repeater_definition()` returns: a trait
/// with a type parameter for the generics, the item's id and the entry's number,
/// defaulted const parameters for the const generics of every primitive type, and
/// the associated type `Type`. Do not write or implement it: `#[announce]` writes the
/// impls, and callbacks in other crates write the paths through it.
///
/// The trait must be:
///
/// - `pub` and reachable from the crate root, so that callers name it as
///   `$crate::path::to::Repeater`;
/// - a trait of the defining crate, because announce implements it for `()`.
///
/// The crate that expands this macro needs Rust 1.59 (const parameters with
/// defaults) and the 2018 edition or later. `announce` itself needs 1.57, and so does
/// a crate that only uses a repeater of another crate.
///
/// See [Type context](https://docs.rs/announce/latest/announce/attr.announce.html#type-context)
/// for what a `repeater` does.
#[proc_macro]
pub fn define_repeater(input: TokenStream) -> TokenStream {
    announce_core::define_repeater(input.into()).into()
}

/// The runner: the attribute that `#[announce]` puts last among the attributes of an item,
/// which makes the trampoline when it expands, after the other macros on the item. Not
/// part of the API, never written by hand.
#[doc(hidden)]
#[proc_macro_attribute]
pub fn __announce_runner(attr: TokenStream, item: TokenStream) -> TokenStream {
    announce_core::run_announce(attr.into(), item.into()).into()
}

/// The end of the attribute query of `#[announce(check, ..)]`: the last element of the
/// list that the checks forward. Not part of the API, never written by hand
/// (`announce::helper::attrs` describes the query).
#[doc(hidden)]
#[proc_macro]
pub fn finish(input: TokenStream) -> TokenStream {
    announce_core::finish(input.into()).into()
}

/// The body of `announce::same_context!`, a callback that a caller puts in the chain. Not part
/// of the API: the `macro_rules!` of `announce` calls it with its `$crate`.
#[doc(hidden)]
#[proc_macro]
pub fn same_context_impl(input: TokenStream) -> TokenStream {
    announce_core::helper::same_context(input.into()).into()
}

#[doc(hidden)]
#[proc_macro_attribute]
pub fn remove(_attr: TokenStream, _item: TokenStream) -> TokenStream {
    TokenStream::new()
}

/// Transformer that strips the newest entry's item down to signatures.
///
/// Put it in the list right after the definition. The callback then gets the
/// definition with the function bodies replaced, not the code itself.
///
/// ```text
/// Item! { [Item!, ::announce::signature_only!, cb!] REST }
/// ```
///
/// For a trait `Greet` and a module `util`, the callback gets the second form
/// instead of the first:
///
/// ```text
/// // before
/// pub trait Greet {
///     fn name(&self) -> u32;
///     fn hi(&self) -> u32 { 42 }
/// }
/// pub mod util {
///     pub fn plain() -> u32 { 7 }
///     pub const fn zero() -> u32 { 0 }
///     pub fn evens() -> impl Iterator<Item = u32> { (0..4).step_by(2) }
/// }
///
/// // after
/// pub trait Greet {
///     fn name(&self) -> u32;
///     fn hi(&self) -> u32 { ::core::unimplemented!() }
/// }
/// pub mod util {
///     pub fn plain() -> u32 { ::core::unimplemented!() }
///     pub const fn zero() -> u32 { 0 }                                   // kept: const fn
///     pub fn evens() -> impl Iterator<Item = u32> { (0..4).step_by(2) } // kept: impl Trait
/// }
/// ```
///
/// The rules:
///
/// - Provided trait methods, fns in modules (also nested ones) and methods in impl
///   blocks get the body `{ ::core::unimplemented!() }`.
/// - Required trait methods stay as they are, so a callback can still tell them from
///   provided ones.
/// - The body of a `const fn` and of a fn that returns `impl Trait` is kept, because a
///   placeholder would not compile there if the callback emits the item.
/// - Structs, enums, unions and type aliases are unchanged.
///
/// To write your own transformer, use `announce::helper::transformer`. The function
/// `announce::helper::signature_only` does this rewrite on a `syn::Item`.
#[proc_macro]
pub fn signature_only(input: TokenStream) -> TokenStream {
    announce_core::helper::transformer(input.into(), |mut entry| {
        let mut item: syn::Item = syn::parse2(entry.item.clone())?;
        announce_core::helper::signature_only(&mut item);
        entry.item = template_quote::ToTokens::to_token_stream(&item);
        Ok(entry)
    })
    .into()
}

#[doc(hidden)]
#[proc_macro_derive(Capture, attributes(__announce))]
pub fn capture(input: TokenStream) -> TokenStream {
    match announce_core::capture(input.into()) {
        Ok(ts) => ts.into(),
        Err(e) => e.to_compile_error().into(),
    }
}
