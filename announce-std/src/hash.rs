//! announce mocks of `hash` traits. Generated from newer-type-std's copies.

#[allow(unused_imports)]
use announce::announce;

/// announce mock of [`::core::hash::Hash`]: invoke `Hash!` to receive its definition.
///
/// The real trait is available under the same path in the type namespace
/// (from rustc 1.94, through a glob re-export, since the macro namespace holds this
/// trampoline).
#[announce(mock = ::core::hash::Hash)]
pub trait Hash {
    fn hash<H>(&self, state: &mut H)
    where
        H: ::core::hash::Hasher;
}

#[cfg(not(announce_std_rust_1_94))]
/// Fallback for rustc < 1.94: a blanket subtrait of [`::core::hash::Hash`], usable in bounds.
pub trait Hash: ::core::hash::Hash {}
#[cfg(not(announce_std_rust_1_94))]
impl<__T: ?::core::marker::Sized + ::core::hash::Hash> Hash for __T {}

/// announce mock of [`::core::hash::Hasher`]: invoke `Hasher!` to receive its definition.
///
/// The real trait is re-exported under the same path.
#[announce(mock = ::core::hash::Hasher)]
pub trait Hasher {
    fn finish(&self) -> ::core::primitive::u64;
    fn write(&mut self, bytes: &[::core::primitive::u8]);
    fn write_u8(&mut self, i: ::core::primitive::u8) {
        ::core::unimplemented!()
    }
    fn write_u16(&mut self, i: ::core::primitive::u16) {
        ::core::unimplemented!()
    }
    fn write_u32(&mut self, i: ::core::primitive::u32) {
        ::core::unimplemented!()
    }
    fn write_u64(&mut self, i: ::core::primitive::u64) {
        ::core::unimplemented!()
    }
    fn write_u128(&mut self, i: ::core::primitive::u128) {
        ::core::unimplemented!()
    }
    fn write_usize(&mut self, i: ::core::primitive::usize) {
        ::core::unimplemented!()
    }
    fn write_i8(&mut self, i: ::core::primitive::i8) {
        ::core::unimplemented!()
    }
    fn write_i16(&mut self, i: ::core::primitive::i16) {
        ::core::unimplemented!()
    }
    fn write_i32(&mut self, i: ::core::primitive::i32) {
        ::core::unimplemented!()
    }
    fn write_i64(&mut self, i: ::core::primitive::i64) {
        ::core::unimplemented!()
    }
    fn write_i128(&mut self, i: ::core::primitive::i128) {
        ::core::unimplemented!()
    }
    fn write_isize(&mut self, i: ::core::primitive::isize) {
        ::core::unimplemented!()
    }
}

#[doc(no_inline)]
pub use ::core::hash::Hasher;

/// announce mock of [`::core::hash::BuildHasher`]: invoke `BuildHasher!` to receive its definition.
///
/// The real trait is re-exported under the same path.
#[announce(mock = ::core::hash::BuildHasher)]
pub trait BuildHasher {
    type Hasher: ::core::hash::Hasher;
    fn build_hasher(&self) -> Self::Hasher;

    fn hash_one<T: ::core::hash::Hash>(&self, x: T) -> ::core::primitive::u64
    where
        Self: ::core::marker::Sized,
        Self::Hasher: ::core::hash::Hasher,
    {
        ::core::unimplemented!()
    }
}

#[doc(no_inline)]
pub use ::core::hash::BuildHasher;

// Real traits whose name is also a derive macro: explicit names shadow glob
// imports per namespace, so this glob supplies the *trait* while the announce
// trampoline above keeps the *macro* namespace. Before rustc 1.94 the glob
// breaks the trampoline's resolution, so it is only emitted from 1.94 on.
#[cfg(announce_std_rust_1_94)]
#[doc(hidden)]
pub mod __real {

    pub use ::core::hash::Hash;
}
#[cfg(announce_std_rust_1_94)]
#[doc(no_inline)]
pub use self::__real::*;
