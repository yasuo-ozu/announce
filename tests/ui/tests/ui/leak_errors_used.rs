// With `strict = true` (the default, and the same when it is written) the definition is checked
// and nothing calls a trampoline: the type that cannot be named outside its module is the only
// error. The item is still emitted, with its derive (so the runner makes the trampoline), and
// the rest of the crate uses it: no error follows from the one at the definition.
mod m {
    #[derive(Clone)]
    pub struct UiLocal(pub u8);

    #[announce::announce]
    #[derive(Clone)]
    pub struct Item {
        pub a: UiLocal,
    }

    #[announce::announce(strict = true)]
    #[derive(Clone)]
    pub struct Explicit {
        pub a: UiLocal,
    }

    // `strict = true` in both announcements, or in one and not written in the other.
    #[announce::announce(strict = true)]
    #[announce::announce]
    #[derive(Clone)]
    pub struct Twice {
        pub a: UiLocal,
    }

    // `strict = false` is the only thing that makes the same item a record: it has no error.
    #[announce::announce(strict = false)]
    #[derive(Clone)]
    pub struct Relaxed {
        pub a: UiLocal,
    }
}

fn main() {
    let local = m::UiLocal(1);
    let _ = m::Item { a: local.clone() }.clone().a.0;
    let _ = m::Explicit { a: local.clone() }.clone().a.0;
    let _ = m::Twice { a: local.clone() }.clone().a.0;
    let _ = m::Relaxed { a: local }.clone().a.0;
}
