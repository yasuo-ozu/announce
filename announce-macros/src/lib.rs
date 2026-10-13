//! The proc macros of [`announce`](https://docs.rs/announce). Use them through the
//! `announce` crate: the paths here are not a stable interface. The code behind
//! the macros is in `announce-core`.

use proc_macro::TokenStream;

/// Makes an item readable by macros in any crate.
///
/// `#[announce]` keeps the item as written and emits a `macro_rules!` with the
/// item's name next to it: the *trampoline*. Calling the trampoline passes the item,
/// as tokens, to a callback macro. It works on a struct, enum, union, trait, type
/// alias and inline `mod`.
///
/// ```ignore
/// #[announce::announce]
/// pub trait Greet { fn hi(&self) -> &'static str; }
///
/// // In any crate: pass the definition of `Greet` to `my_callback!`.
/// path::to::Greet! { [path::to::Greet!, my_callback!] REST }
/// ```
///
/// `my_callback!` receives the definition, and `REST` unchanged:
///
/// ```text
/// my_callback! {
///     [my_callback!]
///     path::to::Greet! => [META] { pub trait Greet { fn hi(&self) -> &'static str; } }
///     REST
/// }
/// ```
///
/// `META` lists what the definer attached: checks, `meta` entries and so on. It is
/// empty without options. In the item, `#[cfg]` is already evaluated in the defining
/// crate, `crate::` reads `$crate::`, and doc attributes are gone (see
/// [Docs](#docs)). The [wire format] is the same for every item. Write callbacks
/// with `announce-helper`.
///
/// # Options
///
/// Options go in the attribute, separated by commas:
/// `#[announce(macro_name = Name, my_lib::check)]`.
///
/// - `macro_name = Ident`: name of the trampoline. Default: the item's name. Set it
///   when a derive or attribute macro owns that name: the derive forwards to the
///   trampoline with `announce_helper::redirect`. Also needed to announce an item
///   twice.
/// - `crate = path`: path to the `announce` crate in generated code. Default: read
///   from the `Cargo.toml` of the crate being compiled, so a renamed dependency
///   works. A proc-macro library whose users do not depend on `announce` must set
///   it (see [`announce::machinery`]).
/// - `meta = path(args)`: adds `path(args)` to `META` for consumers. It is never
///   called. May repeat. `(args)` is optional.
/// - `path` or `path(args)`, without `=`: a check. It runs at the definition, as
///   `path! { (args), ITEM }` (`()` for a bare `path`), so its `compile_error!` shows
///   where the item is defined, not where it is used. It is also listed in `META`.
///   A check can own attributes: see
///   [Checks and attributes of your own](#checks-and-attributes-of-your-own).
/// - `mock = path`: the item stands in for the item at `path` (`announce-std` uses
///   this for std items). It is kept in a hidden module, so it is still
///   type-checked. `META` gets `mock(path)`.
/// - `repeater = crate::Repeater`: lets the item use types that do not resolve in
///   other crates. See [Type context](#type-context).
/// - `allow_primitive = bool`: primitive types (`u8`, `bool`, `str`, ...) need no
///   repeater. Default: `true`.
/// - `allow_std_prelude = bool`: bare std prelude names (`String`, `Vec<u8>`,
///   `Option<T>`, `Clone`, ...) need no repeater. Default: `false`.
/// - `emit_item = bool`: with `false`, emit only the trampoline, not the item.
///   Default: `true`. Its main use is in [Inline modules](#inline-modules).
/// - `layout = auto | single | direct` (advanced): how the trampoline holds the
///   item. Default: `auto`. See [Size and recursion](#size-and-recursion).
/// - `docs = bool`: with `true`, the item that callbacks and checks receive keeps
///   its doc attributes. Default: `false`. See [Docs](#docs).
///
/// ```ignore
/// #[announce(macro_name = ParseAnnounce, meta = my_lib::info(version = 1))]
/// pub trait Parse { /* .. */ }
///
/// // Runs `my_lib::no_tuple! { (named), struct S { .. } }` next to the item
/// // (after the query of "Checks and attributes of your own").
/// #[announce(my_lib::no_tuple(named))]
/// pub struct S { pub a: u8 }
/// ```
///
/// # Calling a trampoline
///
/// ```text
/// path::Item! { [path::Item!, other::Def!, cb!] REST }
/// // cb! { [cb!] other::Def! => [META] { DEF } path::Item! => [META] { ITEM } REST }
/// ```
///
/// The bracketed list names the trampolines to call, then the callback. Each
/// trampoline adds its entry and calls the next element, so `cb!` runs once, with
/// every definition. The newest entry comes first and `REST` is never touched
/// ([wire format]).
///
/// - A list element is `path!`, `#[derive(path)] Name` or `#[attr] Name`. The last
///   two reach a trampoline through a derive or attribute macro that owns the name
///   (see `macro_name`). That route works in item position only.
/// - A leading `(..)` group of global options is forwarded unchanged by every hop.
/// - A transformer is a list element right after the definition. It rewrites the
///   entry and forwards to the rest, as in `[Item!, ::announce::signature_only!, cb!]`
///   (see [`signature_only!`](macro@signature_only), or write your own with
///   `announce_helper::transformer!`). The caller chooses transformers, never the
///   definition.
/// - The call works in any other position too: statement, expression, type and
///   pattern, also for items containing `$`.
/// - A long chain can reach rustc's `recursion_limit`: see
///   [Size and recursion](#size-and-recursion).
///
/// # Checks and attributes of your own
///
/// A check can own attribute names. A library can then put attributes of its own
/// inside the item. Rustc would reject them ("cannot find attribute"), because no
/// derive declares them:
///
/// ```ignore
/// #[announce(my_lib::check)]
/// pub struct S {
///     #[my_lib(skip)]
///     pub a: u8,
///     pub b: u8,
/// }
/// ```
///
/// - `#[announce]` **removes** the owned attributes from the item it emits, so rustc
///   never sees them.
/// - The payload **keeps** them. Callbacks see them, and `META` lists the names as
///   one element, `attrs(my_lib, other)` (`announce_helper::Entry::owned_attrs`). A
///   callback that emits the item itself must remove them first
///   (`announce_helper::Entry::item_without_owned_attrs`).
/// - The check gets the item **with** them and can validate them. Its
///   `compile_error!` points at the attribute where the item is defined.
///
/// Put `#[announce]` above the item's own owned attributes. Rustc resolves
/// attributes from the top, so an attribute above `#[announce]` is rejected before
/// `#[announce]` can remove it.
///
/// An owned name `my_lib` matches `#[my_lib]`, `#[my_lib(..)]`, `#[my_lib = ..]` and
/// `#[my_lib::..]`, anywhere in the item: fields, variants, methods, an inline `mod`
/// and more (exact rules: [`announce_helper::attrs`]). A name that rustc or a
/// standard tool uses (`cfg`, `doc`, `derive`, `allow`, `rustfmt`, `rustc_*`, ..)
/// cannot be owned: that is an error where `#[announce]` is used. Two checks must not
/// own the same name for different purposes. An owned name must not be read by
/// another macro on the item: a derive's helper attribute would be removed before the
/// derive runs.
///
/// **The query.** With checks, `#[announce]` first asks each of them which attribute
/// names it owns. **Every check must answer**, also one that owns no attribute.
/// `announce_helper::check` runs a proc-macro check and answers for you. A
/// `macro_rules!` check needs one more arm. The protocol is in
/// [`announce_helper::attrs`]. Without checks there is no query.
///
/// **`#[cfg]` and `#[cfg_attr]`.** Owned attributes are removed after the item's
/// cfgs are evaluated in the defining crate. An owned attribute inside
/// `#[cfg_attr(feature = "x", my_lib(skip))]` is there when the feature is on, and is
/// removed like any other. (For this, an item with owned attributes is emitted by the
/// hidden derive that evaluates the cfgs, not by `#[announce]` itself. An item whose
/// checks own nothing is emitted as written.)
///
/// **`#[derive(Announce)]`** cannot remove attributes, because a derive does not emit
/// its input. A library that owns attributes on a derive declares them with its own
/// derive (`#[proc_macro_derive(MyDerive, attributes(my_lib))]`) and calls
/// `announce::machinery::derive_announce_with`. The derive sends the same query, so
/// the checks run and can validate the attributes, and `META` lists `attrs(..)`. The
/// item keeps the attributes. A callback that re-emits it strips them with
/// `Entry::item_without_owned_attrs`.
///
/// # Docs
///
/// Callbacks do not need documentation, so by default **the item that callbacks and
/// checks receive has no doc attributes**. The item that `#[announce]` emits keeps
/// all of them: rustdoc shows the definition as you wrote it, and
/// `#![deny(missing_docs)]` still passes. Dropping docs also makes the trampoline
/// smaller, which is part of the defining crate's metadata (an item with a few lines
/// of documentation on each method is about a sixth smaller).
///
/// - Removed: every `#[doc = ..]` attribute. That is what `///`, `//!`, `/** .. */`
///   and `#[doc = "text"]` all are, also `#[doc = include_str!(..)]`. They go from
///   every place where owned attributes are found (see
///   [Checks and attributes of your own](#checks-and-attributes-of-your-own)), so also
///   from fields, variants, methods and an inline `mod` with its inner attributes. Not
///   from inside macro invocations and `macro_rules!` bodies.
/// - Kept: the list forms `#[doc(hidden)]`, `#[doc(alias = "x")]`, `#[doc(inline)]`,
///   `#[doc(cfg(..))]` and the like. They steer rustdoc and are not text, and a
///   callback that re-emits the item may want them.
/// - Docs are removed after `#[cfg]` and `#[cfg_attr]` are evaluated, so
///   `#[cfg_attr(feature = "x", doc = "..")]` goes whether or not the feature is on,
///   and `#[cfg_attr(feature = "x", doc(hidden))]` stays when it is.
/// - A check gets the same item as the payload: without docs, with its owned
///   attributes.
/// - `docs = true` keeps the doc attributes in the payload and in what checks get.
///   It is also valid in the `#[announce(..)]` helper attribute of
///   `#[derive(Announce)]` and as `announce::machinery::Options::docs`.
/// - With `mock`, the hidden copy that rustc compiles keeps its docs like any
///   emitted item.
///
/// ```ignore
/// /// Says hello.
/// #[announce]                    // callbacks get `pub trait Greet { fn hi(&self); }`
/// pub trait Greet {
///     /// Says hi.
///     fn hi(&self);
/// }
///
/// #[announce(docs = true)]       // callbacks get the `#[doc = ".."]` attributes too
/// pub struct Point { /** x */ pub x: i32 }
/// ```
///
/// # Size and recursion
///
/// The item and its `META` live in the trampoline's body, so they are part of the
/// defining crate's metadata, once for every copy the trampoline makes. `layout`
/// chooses how the trampoline holds the item. Callbacks see the same tokens either
/// way.
///
/// - `direct`: the item is copied into each of the three arms. A call is one macro
///   expansion.
/// - `single`: the item is held once, so the metadata is about a third of `direct`.
///   A call takes three nested expansions (two through the derive and attribute
///   route).
/// - `auto` (the default): `single` for an item of more than 300 tokens with its
///   `META` (`announce::machinery::SINGLE_COPY_THRESHOLD`; about 21 trait methods),
///   `direct` for a smaller one.
///
/// Most definers need no other value than `auto`.
///
/// Rustc's `recursion_limit` (128 by default) counts nested expansions, and a chain
/// nests all of its macros. A chain fits the default limit with up to 127 small
/// definitions, 63 small ones that contain `$`, or 42 large ones. A longer chain
/// needs `#![recursion_limit = "256"]` (or more) in the crate that makes the call.
///
/// # Type context
///
/// The item's tokens are used again in another crate, so every path in them must mean
/// the same thing there. `#[announce]` checks this at the definition and reports an
/// error at a path that would not resolve. Most items need nothing.
///
/// ## What resolves everywhere
///
/// These need no action:
///
/// - primitive types (`u8`, `bool`, `str`, ...), unless `allow_primitive = false`;
/// - `crate::..`, `$crate::..` and absolute `::..` paths such as
///   `::std::vec::Vec<u8>` or `::core::fmt::Display` (a callback sees `$crate::..`
///   for `crate::..`);
/// - the item's own generic parameters, and the generics of its methods and generic
///   associated types;
/// - `Self`, and associated types of `Self` or of a parameter (`Self::Out`,
///   `T::Output`), on their own or inside an absolute type
///   (`::std::boxed::Box<Self::Out>`). Inside a *relative* type, see
///   "`Self` in a relative type" below;
/// - std prelude names (`String`, `Vec<u8>`, `Option<T>`, `Clone`, ...), only with
///   `allow_std_prelude = true`. This goes by the bare name, with or without generic
///   arguments. It is off by default, because the caller's prelude may differ.
///
/// A type built only from these is fine too: `(u8, bool)`, `&'static str`,
/// `[u8; 4]`, `fn(u8) -> bool`, `::std::boxed::Box<T>`.
///
/// Everything else is *relative*: a `Local` imported by a `use`, `super::X`, and
/// `std::vec::Vec<u8>` without the leading `::`. A bare `Vec<u8>` is relative unless
/// `allow_std_prelude = true`.
///
/// ## Relative types: `repeater`
///
/// A relative type is an error ("this type does not resolve outside the defining
/// crate"), unless the definer gives a `repeater`. The repeater is a trait of the
/// defining crate that carries the types to the callback. Create it with
/// [`define_repeater!`], and name it as `crate::path::to::Repeater` in the option.
/// It works on a struct, enum, union and trait.
///
/// ```ignore
/// mod inner { pub struct Local(pub u8); }
/// use inner::Local;
///
/// announce::define_repeater!(pub Repeater);
///
/// // `Local` and `Vec<T>` are relative here.
/// #[announce(repeater = crate::Repeater)]
/// pub struct Holder<T> { pub a: Local, pub b: Vec<T>, pub c: u8 }
/// ```
///
/// The item is announced as written. type-leak analyses it and numbers the relative
/// types it finds: the *entries* `0, 1, ..`. For each entry, `#[announce]` emits an
/// impl of the repeater for `()` next to the item. The generics that the type uses
/// are encoded in the first argument, and `ID` is a number chosen for the item:
///
/// ```text
/// impl crate::Repeater<(), ID, 0> for () { type Type = Local; }
/// impl<T> crate::Repeater<(T,), ID, 1> for () { type Type = Vec<T>; }
/// ```
///
/// With a `repeater`, `META` always has one more element, `type_leak(..)`, with no
/// entries when no type needed it. A callback reads it with
/// `announce_helper::Entry::type_leak`. Its format and the encoding of the generics
/// are in [`announce_helper::TypeLeak`]. `TypeLeak::apply` rewrites every entry in an
/// item to `<() as $crate::Repeater<ARGS, ID, I>>::Type`:
///
/// ```ignore
/// for entry in input.entries_in_order() {
///     let mut item = entry.item()?;           // the definition, as written
///     if let Some(tl) = entry.type_leak()? {  // `None` without a repeater
///         tl.apply(&mut item);
///     }
///     // emit `item`
/// }
/// ```
///
/// `Holder` then reads:
///
/// ```text
/// pub struct Holder<T> {
///     pub a: <() as $crate::Repeater<(), ID, 0>>::Type,
///     pub b: <() as $crate::Repeater<(T,), ID, 1>>::Type,
///     pub c: u8,
/// }
/// ```
///
/// That resolves in any crate, because `$crate` is the defining crate. A callback
/// that writes the item at another place, such as the `impl Tr<u8> for X` of a
/// trait, uses `TypeLeak::try_apply_at` with a `UseSite`.
///
/// ## Errors a repeater does not fix
///
/// A repeater carries types only. The rest must resolve by itself:
///
/// - **Trait paths**: supertraits, bounds in generics and `where` clauses, and
///   `impl` and `dyn` bounds must be absolute. `trait Sub: Base` is an error; write
///   `crate::Base`. `T: std::fmt::Display` is an error too; write
///   `::core::fmt::Display`. `Sized` and `?Sized` may be written bare (`T: ?Sized`,
///   `where Self: Sized`). With `allow_std_prelude = true`, the other bare prelude
///   traits (`Clone`, `Send`, ...) pass.
/// - **Value paths** in array lengths, enum discriminants, const generic arguments
///   and const defaults must be absolute (`crate::N`), start with `Self::`, or be a
///   const parameter. `[u8; N]` with a const parameter `N` is fine; with a relative
///   constant `N` it is an error.
/// - **Type aliases** must use absolute paths on the right-hand side, and take no
///   `repeater`. `type A = Option<Local>` is an error; write
///   `::core::option::Option<crate::Local>`.
/// - A relative type may mention a const parameter only if the parameter has a
///   primitive type (`bool`, `char`, any integer). A const parameter of a type alias
///   (`const N: Idx`, even an absolute alias) cannot be encoded.
///
/// The type analysis rejects more, at the offending type ("use absolute path: ..."),
/// whatever the `repeater`. In each case, write the relative type with an absolute
/// path (`crate::..`, `::other_crate::..`), and nothing needs to be carried:
///
/// - **`Self` in a relative type.** `Self` has no meaning in the repeater's impl, so
///   `fn f(&self) -> Local<Self>` and `Local<Self::Out>` are errors, while
///   `::std::boxed::Box<Self::Out>` and a bare `Self::Out` are fine. In a struct,
///   enum or union, `Local<Self::A>` must be written out
///   (`Local<<Self as crate::Trait>::A>`). `Local<T::Output>` is fine, `T` being a
///   parameter of the item.
/// - **A bound cycle.** A relative type in a bound or `where` clause that mentions
///   the bounded parameter, as in `struct S<T: ::core::convert::From<Local<T>>>(T)`,
///   is an error: the rewritten bound would have to hold before it can be checked.
///   `T: ::core::convert::From<Local<u8>>` and `U: ::core::convert::From<Local<T>>`
///   are fine.
/// - **A relative type with `'_`** among its own arguments (`Local<'_>`). An elided
///   lifetime of a reference around it (`&Local`) is fine.
/// - **A macro call in an array length**, unless it is absolute: `[u8; two!()]` is an
///   error, `[u8; crate::two!()]` is fine.
/// - **A relative type as the type of a const parameter**: `const N: Idx` with
///   `type Idx = usize` is an error. Write the primitive type (`usize`, `bool`,
///   `char`, ..).
/// - **A method lifetime that only a relative type keeps** (E0195). In a trait,
///   `fn f<'a>(&self, a: Local<'a>) -> &'a u8` is an error, because the copy would
///   have `'a` only inside a projection. Keep `'a` in another argument (`&'a self`,
///   `b: &'a u8`) or write `Local` with an absolute path.
///
/// ## What is not checked
///
/// - Bodies of provided trait methods. `fn g(&self) -> u8 { helper() }` passes, but a
///   callback that re-emits the body needs `helper` to resolve.
///   [`signature_only!`](macro@signature_only) replaces such bodies.
/// - Types named in the default of an associated const. `const N: usize =
///   ::core::mem::size_of::<Local>();` passes the value-path check, but `Local` is not
///   read, so it is not carried. (Array lengths, const arguments, discriminants and
///   the defaults of const parameters are read.)
/// - Attribute arguments, such as `#[serde(with = "m")]`. They are neither checked nor
///   rewritten.
///
/// ## Inline modules
///
/// Inline modules are not analysed, and a `repeater` on one is an error. A library
/// that handles the paths itself announces a copy wrapped in an inline `mod` of the
/// same name, with `emit_item = false`:
///
/// ```ignore
/// #[announce(emit_item = false, macro_name = MyTraitCopy, meta = my_lib::info(..))]
/// pub mod MyTrait { pub trait MyTrait: Base { fn f(&self) -> Local; } }
/// ```
///
/// Only the trampoline is emitted, not the `mod`, so the copy does not clash with the
/// real `MyTrait`. The library makes every path in the module resolve for its
/// callbacks, and a callback unwraps the module. Announcing one item twice needs a
/// different `macro_name` each time.
///
/// # The `Announce` impl
///
/// `#[announce]` also implements the `Announce` trait, so a consumer can ask for a
/// clear error when an item is not announced. It does so for structs, enums and
/// unions, and for `dyn Trait` of simple dyn-compatible traits (no generics, `&self`
/// or `&mut self` methods only, at most `Send` and `Sync` as supertraits). With
/// `emit_item = false` or `mock` there is no impl.
///
/// For a struct, enum or union that you write yourself, you can use
/// [`#[derive(Announce)]`](derive@Announce) instead.
///
/// [wire format]: https://docs.rs/announce-helper/latest/announce_helper/struct.Input.html#the-wire-format
/// [`announce_helper::attrs`]: https://docs.rs/announce-helper/latest/announce_helper/attrs/index.html
/// [`announce_helper::TypeLeak`]: https://docs.rs/announce-helper/latest/announce_helper/struct.TypeLeak.html
/// [`announce::machinery`]: https://docs.rs/announce/latest/announce/machinery/index.html
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
/// See [Type context](macro@announce#type-context) for what a `repeater` does.
#[proc_macro]
pub fn define_repeater(input: TokenStream) -> TokenStream {
    announce_core::define_repeater(input.into()).into()
}

/// Announces a struct, enum or union as written, and implements `Announce` for it.
///
/// Options go in a helper attribute, `#[announce(..)]`. They are the
/// [options](macro@announce#options) of the `#[announce]` attribute, except `mock`
/// and `emit_item`. By default the announced item has no doc attributes: see
/// [Docs](macro@announce#docs).
///
/// ```ignore
/// #[derive(announce::Announce)]
/// #[announce(macro_name = PointAnnounce)]
/// pub struct Point { pub x: i32 }
/// ```
#[proc_macro_derive(Announce, attributes(announce))]
pub fn derive_announce(input: TokenStream) -> TokenStream {
    announce_core::derive_announce(input.into()).into()
}

/// The end of the attribute query of `#[announce(check, ..)]`: the last element of the
/// list that the checks forward. Not part of the API, never written by hand
/// (`announce_helper::attrs` describes the query).
#[doc(hidden)]
#[proc_macro]
pub fn finish(input: TokenStream) -> TokenStream {
    announce_core::finish(input.into()).into()
}

#[doc(hidden)]
#[proc_macro_attribute]
pub fn remove(_attr: TokenStream, _item: TokenStream) -> TokenStream {
    TokenStream::new()
}

announce_helper::transformer! {
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
    /// To write your own transformer, use `announce_helper::transformer!`. The function
    /// `announce_helper::signature_only` does this rewrite on a `syn::Item`.
    pub fn signature_only(entry) {
        let mut item: syn::Item = syn::parse2(entry.item.clone())?;
        announce_helper::signature_only(&mut item);
        entry.item = template_quote::ToTokens::to_token_stream(&item);
        Ok(entry)
    }
}

#[doc(hidden)]
#[proc_macro_derive(Capture, attributes(__announce))]
pub fn capture(input: TokenStream) -> TokenStream {
    match announce_core::capture(input.into()) {
        Ok(ts) => ts.into(),
        Err(e) => e.to_compile_error().into(),
    }
}
