// The definition is checked and nothing calls a trampoline: the type that cannot be named
// outside its module is the only error. The item is still emitted, with its derive (so the
// runner makes the trampoline), and the rest of the crate uses it: no error follows from the one
// at the definition.
mod m {
    #[derive(Clone)]
    pub struct UiLocal(pub u8);

    #[announce::announce]
    #[derive(Clone)]
    pub struct Item {
        pub a: UiLocal,
    }
}

fn main() {
    let local = m::UiLocal(1);
    let _ = m::Item { a: local }.clone().a.0;
}
