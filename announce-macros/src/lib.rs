//! Proc macros behind [`announce`](https://docs.rs/announce). Use them through the
//! `announce` crate; the paths here are not a stable interface. The code behind
//! the macros is in `announce-core`.

use proc_macro::TokenStream;

/// Announces an item: lets macros in any crate read its definition.
///
/// `#[announce]` keeps the item as written and emits a `macro_rules!` with the
/// item's name next to it. Calling that macro passes the item, as tokens, to a
/// callback macro. It works on a struct, enum, union, trait, type alias and
/// inline `mod`.
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
/// `META` is the list of what the definer attached: checks, `meta` entries and
/// so on. It is empty without options. In the item, `#[cfg]` is already
/// evaluated in the defining crate, `crate::` reads `$crate::`, and the doc
/// attributes are gone (see [Docs](#docs)). See
/// [Calling a trampoline](#calling-a-trampoline) for chaining several
/// definitions into one callback.
///
/// # Options
///
/// Options go in the attribute, separated by commas:
/// `#[announce(macro_name = Name, my_lib::check)]`.
///
/// - `macro_name = Ident`: name of the emitted macro. Default: the item's name.
///   Use it when a derive or attribute macro owns that name: announce the item as
///   `macro_name = ParseAnnounce`, and the derive forwards to it with
///   `announce_helper::redirect`. Also needed to announce an item twice.
/// - `crate = path`: path to the `announce` crate in generated code. Optional:
///   it is read from the `Cargo.toml` of the crate being compiled, so a renamed
///   dependency works. A proc-macro library whose users do not depend on
///   `announce` re-exports it and passes `crate = ::my_lib::announce`
///   (`announce::machinery` has the details).
/// - `meta = path(args)`: metadata for consumers. It is added to `META` as
///   `path(args)` and never called. May repeat. `(args)` is optional.
/// - `path` or `path(args)`, without `=`: a check. It runs at the definition,
///   as `path! { (args), ITEM }` (`()` for a bare `path`), so its
///   `compile_error!` shows up where the item is defined, not where it is used.
///   It is also listed in `META`. A check also answers a query, and can own
///   attributes of its own: see [Checks](#checks-and-attributes-of-your-own).
/// - `mock = path`: the item is a stand-in for the item at `path` (this is how
///   `announce-std` announces std items). It is kept in a hidden module, so it
///   is still type-checked, and `META` gets `mock(path)`.
/// - `repeater = crate::Repeater`: lets the item use types that do not resolve
///   in other crates. See [Type context](#type-context).
/// - `allow_primitive = bool`: primitive types (`u8`, `bool`, `str`, ...) need
///   no repeater. Default: `true`.
/// - `allow_std_prelude = bool`: bare std prelude names (`String`, `Vec<u8>`,
///   `Option<T>`, `Clone`, ...) need no repeater. Default: `false`.
/// - `emit_item = bool`: with `false`, only the macro is emitted, not the item.
///   Default: `true`. See [Inline modules](#inline-modules) for its main use.
/// - `layout = auto | single | direct` (advanced): how the macro holds the item.
///   See [Size and recursion](#size-and-recursion). Default: `auto`.
/// - `docs = bool`: with `true`, the item that callbacks and checks receive keeps
///   its doc attributes. Default: `false`, they are removed from it
///   (the emitted item keeps them). See [Docs](#docs).
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
/// The list names the trampolines to call, then the callback. Each macro adds its
/// entry and calls the next, so `cb!` runs once, with every definition. The
/// newest entry comes first and `REST` is never touched. Parsers read entries
/// greedily: wrap a `REST` that starts like one.
///
/// - A list element is `path!`, `#[derive(path)] Name` or `#[attr] Name`. The
///   last two reach a trampoline through a derive or attribute macro that owns
///   the name (see `macro_name`); that route works in item position only.
/// - A leading `(..)` group of global options is forwarded unchanged by every
///   hop.
/// - A transformer is a list element right after the definition. It rewrites the
///   entry and forwards to the rest, for example
///   `[Item!, ::announce::signature_only!, cb!]`
///   ([`signature_only!`](macro@signature_only), or your own with
///   `announce_helper::transformer!`). Transformers are chosen by the caller,
///   never by an option of the definition.
/// - The call works in any position: item, statement, expression, type and
///   pattern, also for items containing `$`.
/// - A long chain can reach rustc's `recursion_limit`: see
///   [Size and recursion](#size-and-recursion).
///
/// # Checks and attributes of your own
///
/// A check can own attribute names. A library then puts attributes of its own
/// inside the item, which rustc would reject ("cannot find attribute"), because
/// no derive declares them:
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
/// - The attributes of the owned names are **removed from the item** that
///   `#[announce]` emits, so rustc never sees them.
/// - They are **kept in the payload**: callbacks see them, and `META` lists the
///   names as one element `attrs(my_lib, other)` (`announce_helper::Entry::owned_attrs`).
///   A callback that emits the item itself removes them first
///   (`announce_helper::Entry::item_without_owned_attrs`).
/// - The check receives the item **with** them, and can validate them: its
///   `compile_error!` points at the attribute where the item is defined.
///
/// Put `#[announce]` above the owned attributes of the item itself: rustc
/// resolves attributes from the top, and an attribute above `#[announce]` is
/// rejected before `#[announce]` can remove it.
///
/// An owned name `my_lib` matches `#[my_lib]`, `#[my_lib(..)]`, `#[my_lib = ..]`
/// and `#[my_lib::..]` (the first segment of the path; `#[::my_lib::x]` is not
/// matched). It matches wherever syn parses attributes inside the item: the item,
/// its fields, variants, generic parameters, trait items, function parameters
/// and statements, and everything inside an inline `mod`, also inner attributes
/// (`#![..]`). It does not look inside macro invocations and `macro_rules!`
/// bodies. A name that rustc or a standard tool uses (`cfg`, `doc`, `derive`,
/// `allow`, `rustfmt`, `rustc_*`, ..) cannot be owned: that is an error where
/// `#[announce]` is used. Two checks must not own the same name for different
/// purposes, and an owned name must not be read by another macro on the item (a
/// derive's helper attribute would be removed before the derive runs).
///
/// **How a check says what it owns: the query.** With checks, `#[announce]` does
/// not emit the item at once. It calls the first check with a query that passes
/// through all of them and ends at a hidden macro of `announce`:
///
/// ```text
/// a::check! { @__announce_attrs [ b::check!, ::announce::__private::finish! ] { STATE } }
/// ```
///
/// A check **answers** by calling the first element of the list, with the rest
/// of the list and `STATE` unchanged, plus `attrs(name, ..)` if it owns names:
///
/// ```text
/// b::check! { @__announce_attrs [ ::announce::__private::finish! ] { STATE attrs(my_lib) } }
/// ```
///
/// `finish!` reads the names and expands to what `#[announce]` expands to
/// without checks (with the owned attributes removed, see below). The state is
/// opaque to the checks. **Every check must answer, also one that owns no
/// attribute.** A proc-macro check is written with `announce_helper::check!`,
/// which does this; a `macro_rules!` check has one more arm (see
/// `announce_helper::attrs`):
///
/// ```ignore
/// macro_rules! my_check {
///     (@__announce_attrs [ $($np:ident)? $(:: $ns:ident)* ! $(, $($rest:tt)*)? ] { $($state:tt)* }) => {
///         $($np)? $(:: $ns)* ! { @__announce_attrs [ $($($rest)*)? ] { $($state)* attrs(my_lib) } }
///     };
///     (($($args:tt)*), $($item:tt)*) => { /* the check */ };
/// }
/// ```
///
/// A check that does not answer stops the item from being emitted. A proc-macro
/// check that parses `announce_helper::CheckInput` by hand reports it. A
/// `macro_rules!` check without the arm gets rustc's "no rules expected `@`" at
/// the `#[announce]`. Without checks there is no query, and the expansion is
/// direct.
///
/// **`#[cfg]` and `#[cfg_attr]`.** The owned attributes are removed after the
/// item's cfgs are evaluated in the defining crate: an owned attribute inside
/// `#[cfg_attr(feature = "x", my_lib(skip))]` is there when the feature is on, and
/// is removed like any other. For that, an item with owned attributes is not
/// emitted by `#[announce]` itself but by the hidden derive that evaluates the
/// cfgs (the rest of the item is unchanged). An item whose checks own nothing is
/// emitted as written.
///
/// **`#[derive(Announce)]`** cannot remove attributes: a derive does not emit its
/// input. A library that owns attributes on a derive declares them with its own
/// derive (`#[proc_macro_derive(MyDerive, attributes(my_lib))]`) and calls
/// `announce::machinery::derive_announce_with`. The derive sends the same
/// query, so the checks run, can validate the attributes and `META` lists
/// `attrs(..)`, but the item keeps them. A callback that re-emits it elsewhere
/// strips them with `Entry::item_without_owned_attrs`.
///
/// # Docs
///
/// Documentation is for readers of the definition, and callbacks that use the
/// tokens do not need it, so by default **the item that callbacks and checks
/// receive has no doc attributes**. The item that is emitted keeps all of them:
/// rustdoc shows the definition as you wrote it, and `#![deny(missing_docs)]`
/// still passes. Dropping them also makes the trampoline smaller, which is part
/// of the defining crate's metadata (an item with a few lines of documentation
/// on each method is about a sixth smaller).
///
/// - Removed: every `#[doc = ..]` attribute, which is what `///`, `//!`,
///   `/** .. */` and an explicit `#[doc = "text"]` all are, also
///   `#[doc = include_str!(..)]`. It is removed wherever syn parses attributes
///   inside the item: the item, its fields, variants, generic parameters,
///   trait items, function parameters and statements, and everything inside an
///   inline `mod`, with its inner attributes (`//!`, `#![doc = ..]`). Not inside
///   macro invocations and `macro_rules!` bodies, which are only tokens.
/// - Kept: the list forms `#[doc(hidden)]`, `#[doc(alias = "x")]`,
///   `#[doc(inline)]`, `#[doc(cfg(..))]` and the like. They steer rustdoc
///   (what is shown, found or inlined), they are not text, and a callback that
///   re-emits the item may want them.
/// - This is done after `#[cfg]` and `#[cfg_attr]` are evaluated in the
///   defining crate, so `#[cfg_attr(feature = "x", doc = "..")]` goes whether
///   or not the feature is on, and `#[cfg_attr(feature = "x", doc(hidden))]`
///   stays when it is.
/// - **Checks** get exactly the same item as the payload (without docs, with
///   the attributes they own), so one check sees what callbacks see.
/// - `docs = true` keeps the doc attributes in the payload and in what checks
///   get. The same option is valid in the `#[announce(..)]` helper attribute of
///   `#[derive(Announce)]` and in `announce::machinery::Options::docs`.
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
/// The item and its `META` live in the `macro_rules!` body, so they are part of
/// the defining crate's metadata, once for every copy the macro makes. A macro
/// copies a small item into each of its three arms and expands once per call
/// (`layout = direct`). An item of more than 300 tokens with its `META`
/// (`announce::machinery::SINGLE_COPY_THRESHOLD`; about 21 trait methods) is held
/// once, which takes the metadata of the macro down to a third of that
/// (`layout = single`). The wire format that callbacks see is the same.
///
/// The price of the single copy is expansions: a call takes three nested
/// macro expansions, two through the derive and attribute route, instead of
/// one. Rustc's `recursion_limit` (128 by default) counts nested expansions,
/// and a chain nests all of its macros. A chain fits the default limit with up
/// to 127 small definitions, 63 small ones that contain `$`, or 42 large ones;
/// a longer chain needs `#![recursion_limit = "256"]` (or more) in the crate
/// that makes the call. `layout = direct` keeps a large item at one expansion,
/// at the cost of its metadata; `layout = single` does the opposite for a small
/// one. Most definers need neither.
///
/// # Type context
///
/// The item's tokens are used again in another crate, so every path in them has
/// to mean the same thing there. `#[announce]` checks this at the definition and
/// reports an error at the path that would not resolve. Most items need nothing.
///
/// ## What resolves everywhere
///
/// These need no action:
///
/// - primitive types (`u8`, `bool`, `str`, ...), unless `allow_primitive = false`;
/// - `crate::..` paths (the callback sees `$crate::..`), `$crate::..` paths and
///   absolute `::..` paths such as `::std::vec::Vec<u8>` or `::core::fmt::Display`;
/// - the item's own generic parameters, and the generic parameters of its methods
///   and generic associated types;
/// - `Self`, and associated types of `Self` or of a parameter (`Self::Out`,
///   `T::Output`), on their own or inside an absolute type
///   (`::std::boxed::Box<Self::Out>`). Inside a *relative* type, `Self` and
///   `Self::Out` are errors; `T::Output` is fine (see below);
/// - std prelude names (`String`, `Vec<u8>`, `Option<T>`, `Clone`, ...), only with
///   `allow_std_prelude = true`. This goes by the bare name, with or without
///   generic arguments. It is off by default, because the caller's prelude may
///   differ.
///
/// A type built only from these is fine too: `(u8, bool)`, `&'static str`,
/// `[u8; 4]`, `fn(u8) -> bool`, `::std::boxed::Box<T>`.
///
/// Everything else is relative: `Local` imported by a `use`, `super::X`, and also
/// `std::vec::Vec<u8>` without the leading `::`. A bare `Vec<u8>` is relative
/// unless `allow_std_prelude = true`.
///
/// ## Relative types: `repeater`
///
/// A relative type is an error ("this type does not resolve outside the defining
/// crate"), unless the definer gives a `repeater`. The repeater is a trait of the
/// defining crate that carries the types to the callback. Do not write it:
/// [`define_repeater!`] emits it, once per crate, from `type-leak`'s
/// `repeater_definition()`.
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
/// Name it as `crate::path::to::Repeater` in the option, and make it `pub` and
/// reachable from the crate root, so callers can reach it as
/// `$crate::path::to::Repeater`. It has to be a trait of the defining crate:
/// announce implements it for `()`, and the orphan rule allows that only for a
/// local trait. A `repeater` works on a struct, enum, union and trait.
///
/// The trait has a type parameter for the generics an entry uses, the id of the
/// item and the number of the entry, then one defaulted const parameter for each
/// primitive type, and the associated type `Type`. The crate that expands
/// `define_repeater!` needs Rust 1.59 (const parameters with defaults) and the
/// 2018 edition or later; `announce` itself needs 1.57, and so does a crate that
/// only uses a repeater of another crate.
///
/// The item is announced as written. type-leak analyses it and numbers the
/// relative types it finds, the *entries* `0, 1, ..`. Each entry is a type in
/// the *context* where it is written (the item, a method, a generic associated
/// type), with that context's generics: the same text in two methods with
/// different generics is two entries, and a lifetime that a `for<'b>` of the item
/// binds (`Local<'b>` in `for<'b> fn(Local<'b>)`) belongs to the context of its
/// type. For each entry announce emits an impl of the repeater for `()` next to
/// the item, with the generics of that context, their bounds and where clauses,
/// and the generics the type uses encoded in the first argument:
///
/// ```text
/// impl crate::Repeater<(), ID, 0> for () { type Type = Local; }
/// impl<T> crate::Repeater<(T,), ID, 1> for () { type Type = Vec<T>; }
/// ```
///
/// `ID` is a number chosen for the item. A type parameter is encoded as itself,
/// a lifetime as `&'a ()`, a `usize` const as `[(); N]`, and a const of any other
/// primitive type (`bool`, `char`, `u8` .. `u128`, `i8` .. `i128`, `isize`) through
/// the const parameter that the repeater has for that type. A lifetime bound
/// (`T: 'a`) is encoded too (`&'a T`), and the impl does not declare it.
/// `META` gets one more element:
///
/// ```text
/// type_leak(repeater = $crate::Repeater, id = ID, context = 1, referrer = [ .. ])
/// ```
///
/// `referrer` is the `type_leak::Referrer` of the item, in type-leak's own token
/// format (do not parse it yourself; its first token is a format tag, and a
/// version of type-leak that does not match is an error). It holds the entries
/// (`&Local`, also with an elided lifetime, is the entry `Local`) and the context
/// of each, and `context` is the context of the item itself, where a callback
/// rewrites it. The entry numbers are the `I` of the impls. With a `repeater`,
/// `META` always has `type_leak(..)`, with no entries when no type needed it.
/// Readers ignore keys they do not know.
///
/// A callback resolves the types with `announce_helper`. `Entry::type_leak()`
/// reads the `META` element, and `TypeLeak::apply` rewrites every entry in an
/// item to `<() as $crate::Repeater<ARGS, ID, I>>::Type`, with the `ARGS` that
/// `type_leak::Referrer::entry_args_with` gives for the item's own scope (every
/// generic by its own name), the encoding of the impl:
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
/// That resolves in any crate, because `$crate` is the defining crate. A
/// callback that writes the item at another place, such as the `impl Tr<u8> for
/// X` of a trait, uses `TypeLeak::try_apply_at` with a `UseSite`, which gives
/// the arguments type-leak would write there (`TypeLeak::entry_args`), and
/// `TypeLeak::lookup` and `projection` are available for a callback that
/// rewrites in its own way.
///
/// ## Errors a repeater does not fix
///
/// A repeater carries types only. The rest must resolve by itself:
///
/// - **Trait paths**: supertraits, bounds in generics and `where` clauses, and
///   `impl` and `dyn` bounds must be absolute. `trait Sub: Base` is an error; write
///   `crate::Base`. `T: std::fmt::Display` is an error too; write
///   `::core::fmt::Display`. `Sized` and `?Sized` may be written bare (`T:
///   ?Sized`, `where Self: Sized`). With `allow_std_prelude = true`, the other
///   bare prelude traits (`Clone`, `Send`, ...) pass.
/// - **Value paths** in array lengths, enum discriminants, const generic arguments
///   and const defaults must be absolute (`crate::N`), start with `Self::`, or be a
///   const parameter. `[u8; N]` with a const parameter `N` is fine; `[u8; N]` with
///   a relative constant `N` is an error.
/// - **Type aliases** must use absolute paths on the right-hand side, and take no
///   `repeater`. `type A = Option<Local>` is an error; write
///   `::core::option::Option<crate::Local>`.
/// - A relative type may mention a const parameter only if its type is a
///   primitive one (`bool`, `char`, any integer): a const parameter of a type
///   alias (`const N: Idx`, even an absolute alias) cannot be encoded.
///
/// The type analysis rejects more, and says so at the offending type
/// ("use absolute path: ..."), whatever the `repeater`. In each case, writing
/// the relative type with an absolute path (`crate::..`, `::other_crate::..`)
/// makes it something that resolves everywhere, so nothing is carried:
///
/// - **`Self` in a relative type.** `Self` has no meaning in the impl of the
///   repeater, so `fn f(&self) -> Local<Self>` and `Local<Self::Out>` are errors,
///   while `::std::boxed::Box<Self::Out>` and a bare `Self::Out` are fine. In a
///   struct, enum or union, `Local<Self::A>` asks for the trait to be written
///   out (`Local<<Self as crate::Trait>::A>`). `Local<T::Output>` is fine, `T`
///   being a parameter of the item.
/// - **A bound cycle.** A relative type in a bound or `where` clause that
///   mentions the bounded parameter, as in
///   `struct S<T: ::core::convert::From<Local<T>>>(T)`, is an error: the
///   rewritten bound would have to hold before it can be checked.
///   `T: ::core::convert::From<Local<u8>>` and
///   `U: ::core::convert::From<Local<T>>` are fine.
/// - **A relative type with `'_`** among its own arguments (`Local<'_>`); an
///   elided lifetime of a reference around it (`&Local`) is fine.
/// - **A macro call in an array length**, unless it is absolute:
///   `[u8; two!()]` is an error, `[u8; crate::two!()]` is fine.
/// - **The type of a const parameter**, if it is relative: `const N: Idx` with
///   `type Idx = usize` is an error; write the primitive type (`usize`, `bool`,
///   `char`, ..).
/// - **A method lifetime that only a relative type keeps** (E0195). In a trait,
///   `fn f<'a>(&self, a: Local<'a>) -> &'a u8` is an error, because the copy
///   would have `'a` only inside a projection. Keep `'a` in another argument
///   (`&'a self`, `b: &'a u8`) or write `Local` with an absolute path.
///
/// ## What is not checked
///
/// - Bodies of provided trait methods. `fn g(&self) -> u8 { helper() }` passes,
///   but a callback that re-emits the body needs `helper` to resolve.
///   `signature_only!` replaces such bodies.
/// - Types named in the default of an associated const. `const N: usize =
///   ::core::mem::size_of::<Local>();` passes the value-path check, but `Local`
///   is not read, so it is not carried (array lengths, const arguments,
///   discriminants and the defaults of const parameters are read).
/// - Attribute arguments, such as `#[serde(with = "m")]`. They are neither
///   checked nor rewritten.
///
/// ## Inline modules
///
/// Inline modules are not analysed, and a `repeater` on one is an error. A library
/// that handles the paths itself announces a copy wrapped in an inline `mod` of
/// the same name, with `emit_item = false`:
///
/// ```ignore
/// #[announce(emit_item = false, macro_name = MyTraitCopy, meta = my_lib::info(..))]
/// pub mod MyTrait { pub trait MyTrait: Base { fn f(&self) -> Local; } }
/// ```
///
/// Only the trampoline is emitted, not the `mod`, so the copy does not clash with
/// the real `MyTrait`. The library makes every path in the module resolve for its
/// callbacks, and a callback unwraps the module. Announcing one item twice needs a
/// different `macro_name` each time.
///
/// # The `Announce` impl
///
/// `#[announce]` also implements the `Announce` trait, so a consumer can ask
/// for a clear error when an item is not announced. It does so for structs,
/// enums and unions, and for `dyn Trait` of simple dyn-compatible traits (no
/// generics, `&self` or `&mut self` methods only, at most `Send` and `Sync` as
/// supertraits). With `emit_item = false` or `mock` there is no impl.
///
/// For a struct, enum or union you write yourself, derive it instead:
/// [`#[derive(Announce)]`](derive@Announce) emits the item as written. Options
/// go in an `#[announce(..)]` helper attribute, except `mock` and `emit_item`.
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
/// The output is the trait that `type_leak::repeater_definition()` returns: a
/// trait with the type, the item's id, the entry's number and defaulted const
/// parameters for the const generics of every primitive type. Do not write that
/// trait yourself, and do not implement it: `#[announce]` writes the impls and
/// the callbacks of other crates write the paths through it.
///
/// The trait needs Rust 1.59 (const parameters with defaults) and the 2018
/// edition or later in the crate that expands this macro; `announce` itself
/// needs only 1.57. It must be `pub` and reachable from the crate root, so
/// that callers name it as `$crate::path::to::Repeater`, and a trait of the
/// defining crate, because announce implements it for `()`.
#[proc_macro]
pub fn define_repeater(input: TokenStream) -> TokenStream {
    announce_core::define_repeater(input.into()).into()
}

/// Announces a struct, enum or union as written, and implements `Announce` for it.
///
/// Options go in a helper attribute, `#[announce(..)]`. They are the options of
/// the `#[announce]` attribute, except `mock` and `emit_item`. Doc attributes
/// are left out of the announced item unless the helper has `docs = true`
/// (see "Docs" in the docs of the `#[announce]` attribute).
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

/// The end of the attribute query of `#[announce(check, ..)]`: the last element
/// of the list that the checks forward. Not part of the API, never written by
/// hand (`announce_helper::attrs` describes the query).
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
    /// Put it in the list right after the definition. The callback then gets
    /// the definition with the function bodies replaced, not the code itself.
    ///
    /// ```text
    /// Item! { [Item!, ::announce::signature_only!, cb!] REST }
    /// ```
    ///
    /// For a trait `Greet` and a module `util`, the callback gets the second
    /// form instead of the first:
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
    /// Provided trait methods, fns in modules and methods in their impl blocks
    /// get the body `{ ::core::unimplemented!() }`. Required trait methods stay
    /// as they are, so a callback can still tell them from provided ones. The
    /// body of a `const fn` and of a fn returning `impl Trait` is kept, because
    /// a placeholder would not compile there if the callback emits the item.
    /// Structs, enums, unions and type aliases are unchanged. To write your own
    /// transformer, see `announce_helper::signature_only`.
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
