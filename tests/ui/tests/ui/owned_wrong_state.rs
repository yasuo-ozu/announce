// A check that answers wrongly: it adds something that is not `attrs(..)` to
// the state.
macro_rules! bad_state {
    (@__announce_attrs [ $($hp:ident)? $(:: $hs:ident)* ! , $($np:ident)? $(:: $ns:ident)* ! $(, $($rest:tt)*)? ] { $($state:tt)* }) => {
        $($np)? $(:: $ns)* ! { @__announce_attrs [ $($np)? $(:: $ns)* ! $(, $($rest)*)? ] { $($state)* oops(my_lib) } }
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
    (@__announce_attrs [ $($hp:ident)? $(:: $hs:ident)* ! , $($np:ident)? $(:: $ns:ident)* ! $(, $($rest:tt)*)? ] { $($state:tt)* }) => {
        $($np)? $(:: $ns)* ! { @__announce_attrs [] { $($state)* } }
    };
    (($($args:tt)*), $($item:tt)*) => {};
}

#[announce::announce(drops_list, announce_test_derive::no_tuple)]
pub struct T {
    pub a: u8,
}

// The same, as the last check: `finish!` receives an empty list.
#[announce::announce(drops_list)]
pub struct U {
    pub a: u8,
}

// A check that forwards its list without dropping its own element (the head): the
// list that the next macro receives is not headed by that macro. `finish!` wants
// exactly `[ finish! ]` ...
macro_rules! keeps_head {
    (@__announce_attrs [ $($hp:ident)? $(:: $hs:ident)* ! , $($np:ident)? $(:: $ns:ident)* ! $(, $($rest:tt)*)? ] { $($state:tt)* }) => {
        $($np)? $(:: $ns)* ! { @__announce_attrs [ $($hp)? $(:: $hs)* ! , $($np)? $(:: $ns)* ! $(, $($rest)*)? ] { $($state)* } }
    };
    (($($args:tt)*), $($item:tt)*) => {};
}

#[announce::announce(keeps_head)]
pub struct V {
    pub a: u8,
}

// ... and a check that looks at its head finds the wrong macro there.
#[announce::announce(keeps_head, announce_test_derive::head_probe)]
pub struct W {
    pub a: u8,
}

fn main() {}
