//! announce mocks of `io` traits. Generated from newer-type-std's copies.

#[allow(unused_imports)]
use announce::announce;

/// announce mock of [`::std::io::Read`]: invoke `Read!` to receive its definition.
///
/// The real trait is re-exported under the same path.
#[announce(mock = ::std::io::Read)]
pub trait Read {
    fn read(
        &mut self,
        buf: &mut [::core::primitive::u8],
    ) -> ::std::io::Result<::core::primitive::usize>;
}

#[doc(no_inline)]
pub use ::std::io::Read;

/// announce mock of [`::std::io::Write`]: invoke `Write!` to receive its definition.
///
/// The real trait is re-exported under the same path.
#[announce(mock = ::std::io::Write)]
pub trait Write {
    fn write(
        &mut self,
        buf: &[::core::primitive::u8],
    ) -> ::std::io::Result<::core::primitive::usize>;
    fn flush(&mut self) -> ::std::io::Result<()>;
    fn write_all(&mut self, buf: &[::core::primitive::u8]) -> ::std::io::Result<()> {
        ::core::unimplemented!()
    }
}

#[doc(no_inline)]
pub use ::std::io::Write;

/// announce mock of [`::std::io::BufRead`]: invoke `BufRead!` to receive its definition.
///
/// The real trait is re-exported under the same path.
#[announce(mock = ::std::io::BufRead)]
pub trait BufRead: ::std::io::Read {
    fn fill_buf(&mut self) -> ::std::io::Result<&[::core::primitive::u8]>;
    fn consume(&mut self, amt: ::core::primitive::usize);
}

#[doc(no_inline)]
pub use ::std::io::BufRead;

#[cfg(announce_std_rust_1_80)]
/// announce mock of [`::std::io::Seek`]: invoke `Seek!` to receive its definition.
///
/// The real trait is re-exported under the same path.
#[announce(mock = ::std::io::Seek)]
pub trait Seek {
    fn seek(&mut self, pos: ::std::io::SeekFrom) -> ::std::io::Result<::core::primitive::u64>;
    fn rewind(&mut self) -> ::std::io::Result<()> {
        ::core::unimplemented!()
    }
    fn stream_position(&mut self) -> ::std::io::Result<::core::primitive::u64> {
        ::core::unimplemented!()
    }
    fn seek_relative(&mut self, offset: ::core::primitive::i64) -> ::std::io::Result<()> {
        ::core::unimplemented!()
    }
}

#[cfg(announce_std_rust_1_80)]
#[doc(no_inline)]
pub use ::std::io::Seek;

#[cfg(not(announce_std_rust_1_80))]
/// announce mock of [`::std::io::Seek`]: invoke `Seek!` to receive its definition.
///
/// The real trait is re-exported under the same path.
#[announce(mock = ::std::io::Seek)]
pub trait Seek {
    fn seek(&mut self, pos: ::std::io::SeekFrom) -> ::std::io::Result<::core::primitive::u64>;
    fn rewind(&mut self) -> ::std::io::Result<()> {
        ::core::unimplemented!()
    }
    fn stream_position(&mut self) -> ::std::io::Result<::core::primitive::u64> {
        ::core::unimplemented!()
    }
}

#[cfg(not(announce_std_rust_1_80))]
#[doc(no_inline)]
pub use ::std::io::Seek;
