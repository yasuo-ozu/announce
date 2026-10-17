// An owned attribute name must not be an attribute that rustc or a tool uses.
// A proc-macro check (`announce::helper::check`) reports it where the check is used.
#[announce::announce(announce_test_derive::doc_owner)]
pub struct A {
    pub a: u8,
}

// A `macro_rules!` check reports it at `finish!`, at the name it wrote.
macro_rules! cfg_owner {
    (@announce_format "1" @__announce_attrs [ $($hp:ident)? $(:: $hs:ident)* ! , $($np:ident)? $(:: $ns:ident)* ! $(, $($rest:tt)*)? ] { $($state:tt)* }) => {
        $($np)? $(:: $ns)* ! { @announce_format "1" @__announce_attrs [ $($np)? $(:: $ns)* ! $(, $($rest)*)? ] { $($state)* attrs(cfg) } }
    };
    (@announce_format "1" ($($args:tt)*), $($item:tt)*) => {};
}

#[announce::announce(cfg_owner)]
pub struct B {
    pub b: u8,
}

macro_rules! announce_owner {
    (@announce_format "1" @__announce_attrs [ $($hp:ident)? $(:: $hs:ident)* ! , $($np:ident)? $(:: $ns:ident)* ! $(, $($rest:tt)*)? ] { $($state:tt)* }) => {
        $($np)? $(:: $ns)* ! { @announce_format "1" @__announce_attrs [ $($np)? $(:: $ns)* ! $(, $($rest)*)? ] { $($state)* attrs(rustc_thing) } }
    };
    (@announce_format "1" ($($args:tt)*), $($item:tt)*) => {};
}

#[announce::announce(announce_owner)]
pub struct C {
    pub c: u8,
}

fn main() {}
