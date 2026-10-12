// A check that answers wrongly: it adds something that is not `attrs(..)` to
// the state.
macro_rules! bad_state {
    (@__announce_attrs [ $($np:ident)? $(:: $ns:ident)* ! $(, $($rest:tt)*)? ] { $($state:tt)* }) => {
        $($np)? $(:: $ns)* ! { @__announce_attrs [ $($($rest)*)? ] { $($state)* oops(my_lib) } }
    };
    (($($args:tt)*), $($item:tt)*) => {};
}

#[announce::announce(bad_state)]
pub struct S {
    pub a: u8,
}

// A check that drops the rest of the list: `finish!` is never reached, and the
// next check says so.
macro_rules! drops_list {
    (@__announce_attrs [ $($np:ident)? $(:: $ns:ident)* ! $(, $($rest:tt)*)? ] { $($state:tt)* }) => {
        $($np)? $(:: $ns)* ! { @__announce_attrs [] { $($state)* } }
    };
    (($($args:tt)*), $($item:tt)*) => {};
}

#[announce::announce(drops_list, announce_test_derive::no_tuple)]
pub struct T {
    pub a: u8,
}

fn main() {}
