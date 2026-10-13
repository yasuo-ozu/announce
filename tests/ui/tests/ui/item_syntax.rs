// `#[announce]` reads only the kind and the name of the item (DESIGN.md section 4.2);
// its syntax is checked once, by the hidden derive. The errors are the same as when
// the attribute parsed the item itself: rustc's own parse error of the source, and
// `syn`'s at the same token, one for each item.
#[announce::announce]
pub struct S {
    pub a: ,
}

#[announce::announce(crate = ::announce)]
pub trait T {
    fn f(&self) -> ;
}

fn main() {}
