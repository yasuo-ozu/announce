//! A callback built with announce 0.2 ignores the record of `#[announce(strict = false)]`
//! (`type_leak_error(..)` in the META): it takes the item for one that resolves everywhere,
//! which works in the module of the item.

pub mod m {
    pub mod inner {
        pub struct Local(pub u8);
    }
    use inner::Local;

    #[announce::announce(strict = false)]
    pub struct Holder {
        pub a: Local,
        pub b: u8,
    }

    // Reads the item with announce 0.2.0. The copy uses `Local`, which is in scope here.
    Holder! { [Holder!, ::announce_test_compat_02_callback::copy!] }

    pub fn make() -> HolderCopy02 {
        HolderCopy02 { a: Local(1), b: 2 }
    }
}

#[test]
fn a_0_2_callback_ignores_the_record() {
    // The call carries the record, but `type_leak()` of 0.2 does not read it as a table.
    assert!(m::metas().contains("type_leak_error"), "{}", m::metas());
    assert!(!m::has_type_leak());
    // The item is the one written, and it compiled where the call is written.
    let h = m::make();
    assert_eq!((h.a.0, h.b), (1, 2));
}
