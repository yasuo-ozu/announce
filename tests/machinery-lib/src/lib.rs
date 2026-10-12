//! The public crate of a stand-in library whose macros call
//! `announce::machinery`. Its users depend on this crate only.

pub use announce_test_machinery::{owner, pair, tracked, Plain, Tracked};

/// The macros pass `crate = ::announce_test_machinery_lib::announce`, so the
/// generated code reaches `announce` through here.
#[doc(hidden)]
pub use announce;
