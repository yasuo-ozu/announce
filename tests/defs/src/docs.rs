//! Doc attributes are removed from the payload (and from what checks get), not
//! from the emitted item. missing_docs is denied in this module: an item that
//! lost its docs on the way would not build. The feature x is off here and on
//! in announce-test-user.
#![deny(missing_docs)]
#![allow(unused_doc_comments, unknown_lints, clippy::mixed_attributes_style)]

use announce::announce;

/// Doc of Plain.
#[doc(alias = "plain_alias")]
#[announce]
#[derive(Clone, Debug)]
pub struct Plain<#[doc = "doc of T"] T> {
    /// Doc of a.
    pub a: T,
    /// Doc of b, which is hidden.
    #[doc(hidden)]
    pub b: u8,
    #[cfg_attr(not(feature = "x"), doc = "cfg_attr doc, without x")]
    #[cfg_attr(feature = "x", doc = "cfg_attr doc, with x")]
    pub c: u8,
    #[cfg_attr(not(feature = "x"), doc(hidden))]
    /// Doc of d.
    pub d: u8,
}

/// Doc of Choice.
#[announce]
pub enum Choice {
    /// Doc of A.
    A,
    /// Doc of B.
    B(#[doc = "doc of the tuple field"] u8),
}

/// Doc of Tr.
#[announce]
pub trait Tr {
    /// Doc of K.
    const K: u8;
    /// Doc of A.
    type A;
    /// Doc of f.
    #[doc(alias = "method_alias")]
    fn f(&self) -> u8 {
        1
    }
}

/// Doc of inner.
#[announce]
pub mod inner {
    //! Inner doc of inner.
    #![doc(alias = "inner_alias")]

    /// Doc of Nested.
    pub struct Nested {
        /// Doc of n.
        pub n: u8,
    }

    /**
     * Block doc of three.
     */
    pub fn three() -> u8 {
        3
    }
}

/// Doc of Kept, whose docs stay in the payload.
#[announce(docs = true)]
pub struct Kept {
    /// Doc of k.
    pub k: u8,
}

/// Doc of KeptMod.
#[announce(docs = true)]
pub mod kept_mod {
    //! Inner doc of kept_mod.

    /// Doc of Inside.
    pub struct Inside;
}

/// Doc of WithDerive.
#[announce]
#[derive(Debug)]
pub struct WithDerive {
    /// Doc of a.
    pub a: u8,
}

/// Doc of KeptWithDerive.
#[announce(docs = true)]
#[derive(Debug)]
pub struct KeptWithDerive {
    /// Doc of a.
    pub a: u8,
}

/// A check that records the item it receives, next to the item.
#[macro_export]
#[doc(hidden)]
macro_rules! record_item {
    (@announce_format "1" @__announce_attrs [ $($hp:ident)? $(:: $hs:ident)* ! , $($np:ident)? $(:: $ns:ident)* ! $(, $($rest:tt)*)? ] { $($state:tt)* }) => {
        $($np)? $(:: $ns)* ! { @announce_format "1" @__announce_attrs [ $($np)? $(:: $ns)* ! $(, $($rest)*)? ] { $($state)* } }
    };
    (@announce_format "1" ($($args:tt)*), $($item:tt)*) => {
        /// What the check received.
        pub const CHECKED: &str = stringify!($($item)*);
    };
}

/// The checks get the payload: no docs by default, with docs = true the docs.
pub mod checked {
    use super::announce;

    /// Item of a check without docs.
    pub mod plain {
        use super::announce;

        /// Doc of Checked.
        #[announce(crate::record_item)]
        pub struct Checked {
            /// Doc of a.
            pub a: u8,
        }
    }

    /// Item of a check with docs = true.
    pub mod kept {
        use super::announce;

        /// Doc of Checked.
        #[announce(docs = true, crate::record_item)]
        pub struct Checked {
            /// Doc of a.
            pub a: u8,
        }
    }
}

/// With owned attributes the derive emits the item (not #[announce]): it keeps its docs.
#[announce(crate::rules_check)]
pub struct Owned {
    /// Doc of a.
    #[rules(skip)]
    pub a: u8,
    /// Doc of b.
    pub b: u8,
}
