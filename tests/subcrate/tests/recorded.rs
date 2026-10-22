//! A library that re-exports `announce` (the way a crate that builds on it does) and offers
//! `strict = false` to its users: an item announced through the re-export and through a macro of
//! the library, called with `same_context!` of the library in the module of the item and, for an
//! item without a record, from this crate. The calls that fail are in `tests/ui`.

use subcrate::subcrate;

// A subcrate cannot `pub use` a crate it links, so the module re-exports everything the
// crate has (see `tests/machinery.rs`).
#[subcrate(ann = ::announce)]
pub mod lib {
    #[doc(hidden)]
    pub mod announce {
        pub use crate::ann::*;
    }

    /// Announces the item it is given, relaxed.
    #[macro_export]
    macro_rules! announce_loose {
        ($item:item) => {
            #[$crate::announce::announce(crate = $crate::announce, strict = false)]
            $item
        };
    }
}

#[subcrate(::announce, lib)]
pub mod defs {
    pub mod m {
        pub mod inner {
            pub struct Local(pub u8);
        }
        use inner::Local;

        /// `Local` does not resolve outside `m`: the check is `same_context!` of the library, and
        /// the marker next to the item names the path of the library.
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

        // The macro of the library writes `strict = false` for the item of the user.
        lib::announce_loose! {
            pub struct Wrapped {
                pub a: Local,
            }
        }

        Wrapped! { [Wrapped!, ::lib::announce::same_context!] }

        /// Nothing to record.
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

#[test]
fn same_context_works_in_the_module_of_the_item() {
    assert_eq!(defs::m::in_fn(), 1);
    let _ = defs::m::Recorded {
        a: defs::m::inner::Local(1),
    };
}

// From this crate, `same_context!` passes an item without a record on to the next callback.
mod passes {
    defs::m::Plain! { [defs::m::Plain!, announce::same_context!, capture!] }
}

#[test]
fn same_context_passes_an_item_without_a_failure() {
    let out: String = passes::OUT.split_whitespace().collect();
    assert!(out.starts_with("defs::m::Plain!=>[]{"), "{}", out);
}
