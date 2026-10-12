//! Proc macros behind [`announce`](https://docs.rs/announce). Use them through the
//! `announce` crate; the paths here are not a stable interface.

mod cratepath;
mod generate;
mod leak;
mod options;

use proc_macro::TokenStream;
use proc_macro2::{Ident, Span, TokenStream as TokenStream2};
use std::sync::atomic::{AtomicU64, Ordering};
use template_quote::quote;

fn random_u64() -> u64 {
    use std::hash::{BuildHasher, Hasher};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let mut h = std::collections::hash_map::RandomState::new().build_hasher();
    h.write_u64(COUNTER.fetch_add(1, Ordering::Relaxed));
    h.finish()
}

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
/// evaluated in the defining crate and `crate::` reads `$crate::`. The
/// `announce` crate docs show how to chain several definitions into one callback.
///
/// # Options
///
/// Options go in the attribute, separated by commas:
/// `#[announce(macro_name = Name, my_lib::check)]`.
///
/// - `macro_name = Ident`: name of the emitted macro. Default: the item's name.
///   Use it when a derive or attribute macro owns that name, or to announce an
///   item twice.
/// - `crate = path`: path to the `announce` crate in generated code. Optional:
///   it is read from the caller's `Cargo.toml`, so a renamed dependency works.
/// - `meta = path(args)`: metadata for consumers. It is added to `META` as
///   `path(args)` and never called. May repeat. `(args)` is optional.
/// - `path` or `path(args)`, without `=`: a check. It runs at the definition,
///   as `path! { (args), ITEM }` (`()` for a bare `path`), so its
///   `compile_error!` shows up where the item is defined, not where it is used.
///   It is also listed in `META`.
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
///   Default: `true`.
///
/// ```ignore
/// #[announce(macro_name = ParseAnnounce, meta = my_lib::info(version = 1))]
/// pub trait Parse { /* .. */ }
///
/// // Runs `my_lib::no_tuple! { (named), struct S { .. } }` next to the item.
/// #[announce(my_lib::no_tuple(named))]
/// pub struct S { pub a: u8 }
/// ```
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
/// defining crate that carries the types to the callback. It must have exactly
/// this shape:
///
/// ```text
/// pub trait Repeater<const ID: u64, const I: usize, G: ?Sized> { type Type: ?Sized; }
/// ```
///
/// Write it as `crate::path::to::Repeater` in the option, and make it `pub` so
/// callers can reach it as `$crate::path::to::Repeater`. It has to be a trait of
/// the defining crate: announce implements it for `()`, and the orphan rule allows
/// that only for a local trait. A `repeater` works on a struct, enum, union and
/// trait.
///
/// ```ignore
/// mod inner { pub struct Local(pub u8); }
/// use inner::Local;
///
/// pub trait Repeater<const ID: u64, const I: usize, G: ?Sized> { type Type: ?Sized; }
///
/// // `Local` and `Vec<T>` are relative here.
/// #[announce(repeater = crate::Repeater)]
/// pub struct Holder<T> { pub a: Local, pub b: Vec<T>, pub c: u8 }
/// ```
///
/// The item is announced as written. For each relative type, announce emits an
/// impl of the repeater for `()` next to the item. The impl has only the generic
/// parameters that the type mentions, with their own bounds:
///
/// ```text
/// impl crate::Repeater<ID, 0, ()> for () { type Type = Local; }
/// impl<T> crate::Repeater<ID, 1, (T,)> for () { type Type = Vec<T>; }
/// ```
///
/// `ID` is a number chosen for the item. `META` gets one more element:
///
/// ```text
/// type_leak(repeater = $crate::Repeater, id = ID, generics = [(), (T,)], types = [Local, Vec<T>])
/// ```
///
/// `types` lists the relative types (`&Local`, also with an elided lifetime, lists
/// `Local`). `generics[i]` is the tuple of the generic
/// parameters that `types[i]` mentions, in order of first use: a type parameter
/// as itself (`T`), a lifetime as `&'a ()`, a const as `[(); N]`, and `()` if
/// there are none. Pair `generics[i]` with `types[i]`, and do not rely on the order
/// of the list. With a `repeater`, `META` always has `type_leak(..)`, with empty
/// lists when no type needed it.
///
/// A callback resolves the types with `announce_helper`. `Entry::type_leak()`
/// reads the `META` element, and `TypeLeak::apply` rewrites every listed type in
/// an item to `<() as $crate::Repeater<ID, I, G>>::Type`:
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
///     pub a: <() as $crate::Repeater<ID, 0, ()>>::Type,
///     pub b: <() as $crate::Repeater<ID, 1, (T,)>>::Type,
///     pub c: u8,
/// }
/// ```
///
/// That resolves in any crate, because `$crate` is the defining crate. `TypeLeak`
/// also has `lookup` (the index of a type) and `projection` (the tokens for an
/// index) for a callback that rewrites in its own way.
///
/// ## Errors a repeater does not fix
///
/// A repeater carries types only. The rest must resolve by itself:
///
/// - **Trait paths**: supertraits, bounds in generics and `where` clauses, and
///   `impl` and `dyn` bounds must be absolute. `trait Sub: Base` is an error; write
///   `crate::Base`. `T: std::fmt::Display` is an error too; write
///   `::core::fmt::Display`. Write `?Sized` as `?::core::marker::Sized`. With
///   `allow_std_prelude = true`, bare prelude traits (`Clone`, `Send`, `?Sized`,
///   ...) pass.
/// - **Value paths** in array lengths, enum discriminants, const generic arguments
///   and const defaults must be absolute (`crate::N`), start with `Self::`, or be a
///   const parameter. `[u8; N]` with a const parameter `N` is fine; `[u8; N]` with
///   a relative constant `N` is an error.
/// - **Type aliases** must use absolute paths on the right-hand side, and take no
///   `repeater`. `type A = Option<Local>` is an error; write
///   `::core::option::Option<crate::Local>`.
/// - A relative type that names a higher-ranked lifetime (`Local<'b>` in
///   `for<'b> fn(Local<'b>)`) cannot be carried either; write it with absolute
///   paths. Higher-ranked bounds in code that is already absolute are fine.
/// - A relative type may mention a const parameter only if it is a `usize`.
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
///   `type Idx = usize` is an error; write `usize`, `bool`, `char` or an
///   absolute path.
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
/// that handles the paths itself announces a copy wrapped in a `mod` of the same
/// name, with `emit_item = false`, so that only the trampoline is emitted. The
/// `announce` crate docs show this pattern.
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
    match announce_impl(attr.into(), item.into()) {
        Ok(ts) => ts.into(),
        Err(e) => e.to_compile_error().into(),
    }
}

fn announce_impl(attr: TokenStream2, item: TokenStream2) -> syn::Result<TokenStream2> {
    let opts: options::Options = syn::parse2(attr.clone())?;
    let parsed: syn::Item = syn::parse2(item.clone())?;
    generate::ident_vis(&parsed)?;
    let krate = opts.krate();
    let random = random_u64();
    let cap = Ident::new(
        &format!("__AnnounceCap_{}", generate::hex(random)),
        Span::call_site(),
    );
    let random_lit = syn::LitInt::new(&format!("{}u64", random), Span::call_site());
    let emit_item = opts.mock.is_none() && opts.emit_item;
    // DESIGN.md section 4.2: the derive sees the copy with every nested cfg
    // evaluated in this crate, then `remove` deletes the dummy.
    Ok(quote! {
        #(if emit_item) { #item }
        #[derive(#krate::__private::Capture)]
        #[__announce(#random_lit; #attr)]
        #[#krate::__private::remove]
        #[allow(dead_code, non_camel_case_types)]
        enum #cap { V = { #item 0 } }
    })
}

/// Announces a struct, enum or union as written, and implements `Announce` for it.
///
/// Options go in a helper attribute, `#[announce(..)]`. They are the options of
/// the `#[announce]` attribute, except `mock` and `emit_item`.
///
/// ```ignore
/// #[derive(announce::Announce)]
/// #[announce(macro_name = PointAnnounce)]
/// pub struct Point { pub x: i32 }
/// ```
#[proc_macro_derive(Announce, attributes(announce))]
pub fn derive_announce(input: TokenStream) -> TokenStream {
    match derive_announce_impl(input.into()) {
        Ok(ts) => ts.into(),
        Err(e) => e.to_compile_error().into(),
    }
}

fn derive_announce_impl(input: TokenStream2) -> syn::Result<TokenStream2> {
    let mut item: syn::Item = syn::parse2(input)?;
    let attrs = match &mut item {
        syn::Item::Struct(s) => &mut s.attrs,
        syn::Item::Enum(e) => &mut e.attrs,
        syn::Item::Union(u) => &mut u.attrs,
        other => {
            return Err(syn::Error::new_spanned(
                other,
                "`#[derive(Announce)]` supports struct, enum and union items",
            ))
        }
    };
    let mut opts = options::Options::default();
    let mut seen = false;
    for a in attrs.iter().filter(|a| a.path().is_ident("announce")) {
        if seen {
            return Err(syn::Error::new_spanned(
                a,
                "announce: only one `#[announce(..)]` helper attribute is allowed",
            ));
        }
        seen = true;
        if let syn::Meta::List(l) = &a.meta {
            opts = syn::parse2(l.tokens.clone())?;
        }
    }
    attrs.retain(|a| !a.path().is_ident("announce"));
    for key in ["mock", "emit_item"] {
        if opts.given.iter().any(|g| g == key) {
            return Err(syn::Error::new(
                Span::call_site(),
                format!(
                    "`{}` does not apply to `#[derive(Announce)]`: the item is emitted as written",
                    key
                ),
            ));
        }
    }
    // The derive input is already cfg-evaluated by rustc.
    opts.emit_item = false;
    opts.derived = true;
    generate::generate(item, &opts, random_u64())
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
    match capture_impl(input.into()) {
        Ok(ts) => ts.into(),
        Err(e) => e.to_compile_error().into(),
    }
}

fn capture_impl(input: TokenStream2) -> syn::Result<TokenStream2> {
    let di: syn::DeriveInput = syn::parse2(input)?;
    let attr = di
        .attrs
        .iter()
        .find(|a| a.path().is_ident("__announce"))
        .ok_or_else(|| {
            syn::Error::new_spanned(
                &di.ident,
                "announce: missing #[__announce] on capture dummy",
            )
        })?;
    let args: options::CaptureArgs = attr.parse_args()?;
    let expr = match &di.data {
        syn::Data::Enum(e) => e
            .variants
            .first()
            .and_then(|v| v.discriminant.as_ref())
            .map(|d| &d.1),
        _ => None,
    };
    let stmts = match expr {
        Some(syn::Expr::Block(b)) => &b.block.stmts,
        _ => {
            return Err(syn::Error::new_spanned(
                &di.ident,
                "announce: malformed capture dummy",
            ))
        }
    };
    // The copy is gone when the item itself was `#[cfg]`'d out.
    let item = stmts.iter().find_map(|s| match s {
        syn::Stmt::Item(i) => Some(i.clone()),
        _ => None,
    });
    match item {
        Some(item) => generate::generate(item, &args.opts, args.random),
        None => Ok(TokenStream2::new()),
    }
}
