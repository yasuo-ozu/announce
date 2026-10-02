//! announce mocks of `fmt` traits. Generated from newer-type-std's copies.

#[allow(unused_imports)]
use announce::announce;

/// announce mock of [`::core::fmt::Display`]: invoke `Display!` to receive its definition.
///
/// The real trait is re-exported under the same path.
#[announce(mock = ::core::fmt::Display)]
pub trait Display {
    fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result;
}

#[doc(no_inline)]
pub use ::core::fmt::Display;

/// announce mock of [`::core::fmt::Debug`]: invoke `Debug!` to receive its definition.
///
/// The real trait is available under the same path in the type namespace
/// (from rustc 1.94, through a glob re-export, since the macro namespace holds this
/// trampoline).
#[announce(mock = ::core::fmt::Debug)]
pub trait Debug {
    fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result;
}

#[cfg(not(announce_std_rust_1_94))]
/// Fallback for rustc < 1.94: a blanket subtrait of [`::core::fmt::Debug`], usable in bounds.
pub trait Debug: ::core::fmt::Debug {}
#[cfg(not(announce_std_rust_1_94))]
impl<__T: ?::core::marker::Sized + ::core::fmt::Debug> Debug for __T {}

/// announce mock of [`::core::fmt::Binary`]: invoke `Binary!` to receive its definition.
///
/// The real trait is re-exported under the same path.
#[announce(mock = ::core::fmt::Binary)]
pub trait Binary {
    fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result;
}

#[doc(no_inline)]
pub use ::core::fmt::Binary;

/// announce mock of [`::core::fmt::Octal`]: invoke `Octal!` to receive its definition.
///
/// The real trait is re-exported under the same path.
#[announce(mock = ::core::fmt::Octal)]
pub trait Octal {
    fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result;
}

#[doc(no_inline)]
pub use ::core::fmt::Octal;

/// announce mock of [`::core::fmt::LowerHex`]: invoke `LowerHex!` to receive its definition.
///
/// The real trait is re-exported under the same path.
#[announce(mock = ::core::fmt::LowerHex)]
pub trait LowerHex {
    fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result;
}

#[doc(no_inline)]
pub use ::core::fmt::LowerHex;

/// announce mock of [`::core::fmt::UpperHex`]: invoke `UpperHex!` to receive its definition.
///
/// The real trait is re-exported under the same path.
#[announce(mock = ::core::fmt::UpperHex)]
pub trait UpperHex {
    fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result;
}

#[doc(no_inline)]
pub use ::core::fmt::UpperHex;

/// announce mock of [`::core::fmt::LowerExp`]: invoke `LowerExp!` to receive its definition.
///
/// The real trait is re-exported under the same path.
#[announce(mock = ::core::fmt::LowerExp)]
pub trait LowerExp {
    fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result;
}

#[doc(no_inline)]
pub use ::core::fmt::LowerExp;

/// announce mock of [`::core::fmt::UpperExp`]: invoke `UpperExp!` to receive its definition.
///
/// The real trait is re-exported under the same path.
#[announce(mock = ::core::fmt::UpperExp)]
pub trait UpperExp {
    fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result;
}

#[doc(no_inline)]
pub use ::core::fmt::UpperExp;

/// announce mock of [`::core::fmt::Pointer`]: invoke `Pointer!` to receive its definition.
///
/// The real trait is re-exported under the same path.
#[announce(mock = ::core::fmt::Pointer)]
pub trait Pointer {
    fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result;
}

#[doc(no_inline)]
pub use ::core::fmt::Pointer;

/// announce mock of [`::core::fmt::Write`]: invoke `Write!` to receive its definition.
///
/// The real trait is re-exported under the same path.
#[announce(mock = ::core::fmt::Write)]
pub trait Write {
    fn write_str(&mut self, s: &::core::primitive::str) -> ::core::fmt::Result;
    fn write_char(&mut self, c: ::core::primitive::char) -> ::core::fmt::Result {
        ::core::unimplemented!()
    }
    fn write_fmt(&mut self, args: ::core::fmt::Arguments<'_>) -> ::core::fmt::Result {
        ::core::unimplemented!()
    }
}

#[doc(no_inline)]
pub use ::core::fmt::Write;

// Real traits whose name is also a derive macro: explicit names shadow glob
// imports per namespace, so this glob supplies the *trait* while the announce
// trampoline above keeps the *macro* namespace. Before rustc 1.94 the glob
// breaks the trampoline's resolution, so it is only emitted from 1.94 on.
#[cfg(announce_std_rust_1_94)]
#[doc(hidden)]
pub mod __real {

    pub use ::core::fmt::Debug;
}
#[cfg(announce_std_rust_1_94)]
#[doc(no_inline)]
pub use self::__real::*;
