// With `strict = true` (the default) every cause that keeps a name from resolving outside its
// module is an error at its tokens, all of an item at once, and the item is still emitted.
// `strict = false` records the causes instead (same_context.rs).
#[macro_export]
macro_rules! plain_check {
    (@announce_format "1" @__announce_attrs [ $($hp:ident)? $(:: $hs:ident)* ! , $($np:ident)? $(:: $ns:ident)* ! $(, $($rest:tt)*)? ] { $($state:tt)* }) => {
        $($np)? $(:: $ns)* ! { @announce_format "1" @__announce_attrs [ $($np)? $(:: $ns)* ! $(, $($rest)*)? ] { $($state)* } }
    };
    (@announce_format "1" ($($args:tt)*), $($item:tt)*) => {};
}
::announce::define_repeater!(pub Repeater);
pub mod inner {
    #[derive(Clone)]
    pub struct UiLocal;
    pub struct UiOther;
    pub trait UiBase {}
    pub struct UiWrap<'a>(pub &'a u8);
    pub type UiIdx = usize;
    pub struct UiArr<const N: usize>(pub [u8; N]);
}

mod d {
    use super::inner::{UiArr, UiBase, UiIdx, UiLocal, UiOther, UiWrap};

    const LEN: usize = 4;

    #[announce::announce]
    pub struct Plain {
        pub a: UiLocal,
    }

    // With a derive, which the runner waits for; `strict = true` is the default written out.
    #[announce::announce(strict = true)]
    #[derive(Clone)]
    pub struct Runner {
        pub a: UiLocal,
    }

    // Two announcements make one error, not two.
    #[announce::announce]
    #[announce::announce(meta = my_lib::info)]
    pub struct Twice {
        pub a: UiLocal,
    }

    // Checks of other crates, and one that owns an attribute: the item is emitted without it.
    #[announce::announce(crate::plain_check, announce_test_derive::lib_check)]
    #[derive(Clone)]
    pub struct Checked {
        #[my_lib(skip)]
        pub a: UiLocal,
    }

    // Trait path, value path, a type that cannot be carried, and relative types: an array length
    // makes the type relative.
    #[announce::announce]
    pub trait Sub: UiBase {
        const M: usize = LEN;
        fn f(&self, a: UiWrap<'_>, b: UiLocal, c: [u8; LEN]);
    }

    // The right-hand side of a type alias.
    #[announce::announce]
    pub type Alias<T: UiBase> = Option<(UiArr<{ LEN }>, T)>;

    // A `repeater` carries relative types, but not trait paths or values.
    #[announce::announce(repeater = crate::Repeater)]
    pub trait WithRepeater: UiBase {
        const M: usize = LEN;
        fn f(&self, a: UiOther, c: [u8; LEN]);
    }

    // A const generic that cannot be encoded.
    #[announce::announce(repeater = crate::Repeater)]
    pub struct ConstTy<const N: UiIdx>(pub UiArr<N>);

    // A macro of a library announces the user's item: the error is at the user's tokens.
    announce_test_ui_lib::announced! {
        pub struct FromLibrary {
            pub a: UiLocal,
        }
    }

    // Fine: a `repeater` that carries everything, absolute paths.
    #[announce::announce(repeater = crate::Repeater)]
    pub struct Carried {
        pub a: UiLocal,
    }

    #[announce::announce]
    pub struct Absolute {
        pub a: crate::inner::UiLocal,
    }
}

fn main() {
    // The items are there, without the attribute that a check owns.
    let _ = d::Runner { a: inner::UiLocal }.clone();
    let _ = d::Checked { a: inner::UiLocal }.clone();
}

// Every cause is at its own place: not at the first node that says the same, and not one error
// for a type written in two places. A const generic that cannot be encoded is a cause too, and
// a value path names a const parameter only where the parameter is in scope.
mod e {
    use super::inner::{UiArr, UiBase, UiLocal, UiWrap};

    const N: usize = 4;

    // Only `f` has a lifetime that is in its return type alone.
    #[announce::announce(repeater = crate::Repeater)]
    pub trait Lifetimes {
        fn g<'a>(&self, a: UiWrap<'a>);
        fn f<'a>(&self, a: UiWrap<'a>) -> &'a u8;
    }

    // `UiLocal` is a parameter of `f`, and a type in the return type of `g`.
    #[announce::announce]
    pub trait Shadow: UiBase {
        fn f<UiLocal>(&self, x: UiLocal);
        fn g(&self) -> UiLocal;
    }

    // The same type in two methods: two errors.
    #[announce::announce]
    pub trait Places {
        fn a(&self, x: UiWrap<'_>);
        fn b(&self, y: UiWrap<'_>);
    }

    // The trait path, and the const generic that no table carries.
    #[announce::announce(repeater = crate::Repeater)]
    pub struct Encoded<T: UiBase, const N: crate::inner::UiIdx>(pub UiArr<N>, pub T);

    // The `N` of the default is the constant, not the parameter of `f`.
    #[announce::announce(repeater = crate::Repeater)]
    pub trait Scoped {
        const M: usize = N;
        fn f<const N: usize>(&self);
    }

    macro_rules! ty {
        ($($t:tt)*) => {
            u8
        };
    }

    // The `Self` in the macro is underlined where it is written.
    #[announce::announce(repeater = crate::Repeater)]
    pub struct InMacro<T = ty!(Self)>(pub T, pub UiLocal);
}
