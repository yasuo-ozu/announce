//! announce mocks of `any` traits (hand-written).

#[allow(unused_imports)]
use announce::announce;

/// announce mock of [`::core::any::Any`]: invoke `Any!` to receive its definition.
///
/// The real trait is re-exported under the same path.
#[announce(mock = ::core::any::Any)]
pub trait Any: 'static {
    fn type_id(&self) -> ::core::any::TypeId;
}

#[doc(no_inline)]
pub use ::core::any::Any;
