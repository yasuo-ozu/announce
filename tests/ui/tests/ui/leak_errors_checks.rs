// The error at the definition is the same with checks of other crates, with an attribute that a
// check owns, and with a derive that the runner waits for. The item is still emitted without
// its owned attributes, so the errors are the only ones.
#[macro_export]
macro_rules! plain_check {
    (@announce_format "1" @__announce_attrs [ $($hp:ident)? $(:: $hs:ident)* ! , $($np:ident)? $(:: $ns:ident)* ! $(, $($rest:tt)*)? ] { $($state:tt)* }) => {
        $($np)? $(:: $ns)* ! { @announce_format "1" @__announce_attrs [ $($np)? $(:: $ns)* ! $(, $($rest)*)? ] { $($state)* } }
    };
    (@announce_format "1" ($($args:tt)*), $($item:tt)*) => {};
}

pub mod inner {
    #[derive(Clone)]
    pub struct UiLocal;
}

mod d {
    use super::inner::UiLocal;

    // A check of another crate that owns an attribute.
    #[announce::announce(announce_test_derive::lib_check)]
    pub struct Owned {
        #[my_lib(skip)]
        pub a: UiLocal,
    }

    // Between two checks, one a `macro_rules!` check; and with a derive, so that the runner waits
    // for the owned attribute.
    #[announce::announce(crate::plain_check, announce_test_derive::lib_check)]
    #[derive(Clone)]
    pub struct Between {
        #[my_lib(skip)]
        pub a: UiLocal,
    }

    // A check that does not own anything.
    #[announce::announce(crate::plain_check)]
    pub struct Plain {
        pub a: UiLocal,
    }

    // `strict = false` records the same items: no error here.
    #[announce::announce(announce_test_derive::lib_check, strict = false)]
    pub struct Recorded {
        #[my_lib(skip)]
        pub a: UiLocal,
    }
}

fn main() {
    // The items are there, without the attribute that the check owns.
    let _ = d::Owned { a: inner::UiLocal };
    let _ = d::Between { a: inner::UiLocal };
    let _ = d::Plain { a: inner::UiLocal };
    let _ = d::Recorded { a: inner::UiLocal };
}
