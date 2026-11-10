//! A definition whose types do not resolve outside its module and that has no `repeater`,
//! announced with `strict = false`, used by a callback of another crate, and by
//! `announce::same_context!` through the re-export of a library (the way a crate that builds on
//! `announce` offers it).
//!
//! - `macros`: a proc-macro crate with a callback that emits the item through
//!   `Entry::item_resolved`;
//! - `lib`: re-exports `announce` as its module `announce`;
//! - `defs`: the defining crate. `Local` is private to its module `m`, and `#[announce]`
//!   records that (`type_leak_error(..)` in META) instead of failing. The module calls the
//!   callback of `macros` on its own items: the call is in the module of the item, so it
//!   compiles, and the copies are usable;
//! - this file is the calling crate. It reads the record; a call to the callback from here
//!   would not compile (`tests/ui/tests/ui/recorded_cross_crate.rs` checks that).

use subcrate::subcrate;

#[subcrate(proc_macro, ::announce, ::proc_macro2, ::syn, ::template_quote)]
pub mod macros {
    use proc_macro::TokenStream;
    use template_quote::quote;

    /// `resolved_copy! { [..] NAME }`: the struct of the entry, named `NAME`, as
    /// `Entry::item_resolved` gives it, with the check of the call.
    #[proc_macro]
    pub fn resolved_copy(input: TokenStream) -> TokenStream {
        announce::helper::callback(input.into(), |input| {
            let name: syn::Ident = input.rest_as()?;
            let krate: syn::Path = syn::parse_quote!(::announce);
            let mut out = proc_macro2::TokenStream::new();
            for e in input.entries_in_order() {
                let (mut item, check) = e.item_resolved(&krate)?;
                match &mut item {
                    syn::Item::Struct(s) => s.ident = name.clone(),
                    other => return Err(syn::Error::new_spanned(other, "expected a struct")),
                }
                out.extend(quote! { #item #check });
            }
            Ok(out)
        })
        .into()
    }
}

// A subcrate cannot `pub use` a crate it links, so the module re-exports everything the
// crate has (see `tests/machinery.rs`).
#[subcrate(ann = ::announce)]
pub mod lib {
    #[doc(hidden)]
    pub mod announce {
        pub use crate::ann::*;
    }

    /// A macro of a library that announces the item it is given, relaxed: the way a crate that
    /// builds on `announce` offers `strict = false` to its users.
    #[macro_export]
    macro_rules! announce_loose {
        ($item:item) => {
            #[$crate::announce::announce(crate = $crate::announce, strict = false)]
            $item
        };
    }
}

#[subcrate(::announce, macros, lib)]
pub mod defs {
    use announce::announce;

    pub mod m {
        use super::announce;

        pub mod inner {
            pub struct Local(pub u8);
        }
        use inner::Local;

        /// `Local` does not resolve outside `m`, and there is no `repeater`.
        #[announce(strict = false)]
        pub struct Holder {
            pub a: Local,
            pub b: u8,
        }

        // In the module of the item, with the callback of another crate: fine.
        Holder! { [Holder!, macros::resolved_copy!] HolderCopy }

        pub fn make() -> HolderCopy {
            HolderCopy { a: Local(1), b: 2 }
        }

        // Also in a function body of the module.
        pub fn make_in_fn() -> u8 {
            Holder! { [Holder!, macros::resolved_copy!] HolderInFn }
            let h = HolderInFn { a: Local(3), b: 4 };
            h.a.0 + h.b
        }

        /// The same through a re-export of `announce`: the check is `same_context!` of the
        /// library, and the marker next to the item names the path of the library.
        #[lib::announce::announce(crate = ::lib::announce, strict = false)]
        pub struct Recorded {
            pub a: Local,
        }

        Recorded! { [Recorded!, ::lib::announce::same_context!] }

        pub fn in_fn() -> u8 {
            Recorded! { [Recorded!, ::lib::announce::same_context!] }
            1
        }

        pub mod child {
            use super::*;

            Recorded! { [Recorded!, ::lib::announce::same_context!] }
        }

        // An item announced through the macro of the library, in the module that it is in: the
        // check of the library passes there.
        lib::announce_loose! {
            pub struct Wrapped {
                pub a: Local,
            }
        }

        Wrapped! { [Wrapped!, ::lib::announce::same_context!] }

        pub fn wrapped_in_fn() -> u8 {
            Wrapped! { [Wrapped!, ::lib::announce::same_context!, macros::resolved_copy!] WrappedCopy }
            WrappedCopy { a: Local(6) }.a.0
        }

        /// A struct with nothing to record passes `same_context!` from any crate.
        #[lib::announce::announce(crate = ::lib::announce, strict = false)]
        pub struct Plain {
            pub a: u8,
        }
    }
}

macro_rules! capture {
    ($(@announce_format "1")? [$($l:tt)*] $($t:tt)*) => {
        pub const OUT: &str = stringify!($($t)*);
    };
}

fn norm(s: &str) -> String {
    s.split_whitespace().collect()
}

#[test]
fn the_copies_made_in_the_module_work() {
    let h = defs::m::make();
    assert_eq!((h.a.0, h.b), (1, 2));
    assert_eq!(defs::m::make_in_fn(), 7);
}

mod record {
    defs::m::Holder! { [defs::m::Holder!, capture!] }
}

#[test]
fn the_calling_crate_reads_the_record() {
    let out = norm(record::OUT);
    // No table, and the record: the tokens as written, and the reason.
    assert!(!out.contains("type_leak("), "{}", out);
    assert!(
        out.contains("type_leak_error(id=") && out.contains("errors=[(Local)\"without`repeater`\""),
        "{}",
        out
    );
    // The item itself is as written.
    assert!(
        out.contains("pubstructHolder{puba:Local,pubb:u8,}"),
        "{}",
        out
    );
}

// `same_context!` of the library, from this crate: the chain passes the entries on.
mod passes {
    defs::m::Plain! { [defs::m::Plain!, announce::same_context!, capture!] }
}

#[test]
fn same_context_passes_an_item_without_a_failure() {
    assert!(
        norm(passes::OUT).starts_with("defs::m::Plain!=>[]{"),
        "{}",
        passes::OUT
    );
}

#[test]
fn same_context_works_in_the_module_of_the_item() {
    // The calls in `defs::m` compiled; the item is there.
    assert_eq!(defs::m::in_fn(), 1);
    let _ = defs::m::Recorded {
        a: defs::m::inner::Local(1),
    };
}

#[test]
fn an_item_announced_through_a_macro_of_a_library_is_recorded_and_passes_in_its_module() {
    assert_eq!(defs::m::wrapped_in_fn(), 6);
    // The record is in the call that this crate makes: the macro of the library wrote
    // `strict = false` for the item of the user.
    mod wrapped {
        defs::m::Wrapped! { [defs::m::Wrapped!, capture!] }
    }
    let out = norm(wrapped::OUT);
    assert!(
        out.contains("type_leak_error(id=") && out.contains("errors=[(Local)\"without`repeater`\""),
        "{}",
        out
    );
}
