//! announce mocks of `task` traits. Generated from newer-type-std's copies.

#[allow(unused_imports)]
use announce::announce;

/// announce mock of [`::std::task::Wake`]: invoke `Wake!` to receive its definition.
///
/// The real trait is re-exported under the same path.
#[announce(mock = ::std::task::Wake)]
pub trait Wake {
    fn wake(self: ::std::sync::Arc<Self>);
    fn wake_by_ref(self: &::std::sync::Arc<Self>) {
        ::core::unimplemented!()
    }
}

#[doc(no_inline)]
pub use ::std::task::Wake;
