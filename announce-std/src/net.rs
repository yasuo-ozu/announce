//! announce mocks of `net` traits. Generated from newer-type-std's copies.

#[allow(unused_imports)]
use announce::announce;

/// announce mock of [`::std::net::ToSocketAddrs`]: invoke `ToSocketAddrs!` to receive its definition.
///
/// The real trait is re-exported under the same path.
#[announce(mock = ::std::net::ToSocketAddrs)]
pub trait ToSocketAddrs {
    type Iter: ::core::iter::Iterator<Item = ::std::net::SocketAddr>;
    fn to_socket_addrs(&self) -> ::std::io::Result<Self::Iter>;
}

#[doc(no_inline)]
pub use ::std::net::ToSocketAddrs;
