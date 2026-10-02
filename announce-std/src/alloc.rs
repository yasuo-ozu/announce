//! announce mocks of `alloc` traits. Generated from newer-type-std's copies.

#[allow(unused_imports)]
use announce::announce;

/// announce mock of [`::core::alloc::GlobalAlloc`].
///
/// The real trait is re-exported under the same path.
#[announce(mock = ::core::alloc::GlobalAlloc)]
pub unsafe trait GlobalAlloc {
    unsafe fn alloc(&self, layout: ::core::alloc::Layout) -> *mut ::core::primitive::u8;
    unsafe fn dealloc(&self, ptr: *mut ::core::primitive::u8, layout: ::core::alloc::Layout);
    unsafe fn alloc_zeroed(&self, layout: ::core::alloc::Layout) -> *mut ::core::primitive::u8 {
        ::core::unimplemented!()
    }
    unsafe fn realloc(
        &self,
        ptr: *mut ::core::primitive::u8,
        layout: ::core::alloc::Layout,
        new_size: ::core::primitive::usize,
    ) -> *mut ::core::primitive::u8 {
        ::core::unimplemented!()
    }
}
#[doc(no_inline)]
pub use ::core::alloc::GlobalAlloc;
