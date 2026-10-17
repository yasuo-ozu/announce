// `#[derive(Announce)]` that the author writes is deprecated: it warns, and says to write
// `#[announce]`. Here the warning is an error, to show it.
#![deny(deprecated)]

#[derive(announce::Announce)]
#[announce(meta = my_lib::info)]
pub struct Warned(pub u8);

// It warns when it stands aside for an announcement below it as well.
#[derive(announce::Announce)]
#[announce::announce]
pub struct Aside(pub u8);

fn main() {}
