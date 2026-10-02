//! announce mocks of `string` traits. Generated from newer-type-std's copies.

#[allow(unused_imports)]
use announce::announce;

/// announce mock of [`::std::string::ToString`]: invoke `ToString!` to receive its definition.
///
/// The real trait is re-exported under the same path.
#[announce(mock = ::std::string::ToString)]
pub trait ToString {
    fn to_string(&self) -> ::std::string::String;
}

#[doc(no_inline)]
pub use ::std::string::ToString;
