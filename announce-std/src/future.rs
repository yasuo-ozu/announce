//! announce mocks of `future` traits. Generated from newer-type-std's copies.

#[allow(unused_imports)]
use announce::announce;

/// announce mock of [`::core::future::Future`]: invoke `Future!` to receive its definition.
///
/// The real trait is re-exported under the same path.
#[announce(mock = ::core::future::Future)]
pub trait Future {
    type Output;
    fn poll(
        self: ::core::pin::Pin<&mut Self>,
        cx: &mut ::core::task::Context<'_>,
    ) -> ::core::task::Poll<Self::Output>;
}

#[doc(no_inline)]
pub use ::core::future::Future;

/// announce mock of [`::core::future::IntoFuture`]: invoke `IntoFuture!` to receive its definition.
///
/// The real trait is re-exported under the same path.
#[announce(mock = ::core::future::IntoFuture)]
pub trait IntoFuture {
    type Output;
    type IntoFuture: ::core::future::Future<Output = Self::Output>;
    fn into_future(self) -> Self::IntoFuture;
}

#[doc(no_inline)]
pub use ::core::future::IntoFuture;
