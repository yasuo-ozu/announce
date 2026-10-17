fn takes_debug<T: announce_std::fmt::Debug>(_: T) {}

macro_rules! got {
    (@announce_format "1" [$($l:tt)*] $($p:ident)? $(:: $s:ident)* ! => [mock($($m:tt)*)] { $($item:tt)* }) => {
        const MOCK: &str = stringify!($($m)*);
    };
}
announce_std::fmt::Debug! { [announce_std::fmt::Debug!, got!] }

#[test]
fn derive_named_trait_in_type_namespace() {
    takes_debug(1u8);
    assert_eq!(MOCK, ":: core :: fmt :: Debug");
}

#[allow(unused_macros)]
macro_rules! grab {
    (@announce_format "1" [$($l:tt)*] $($p:ident)? $(:: $s:ident)* ! => [$($m:tt)*] { $($item:tt)* }) => {
        pub const ITEM: &str = stringify!($($item)*);
    };
}
mod iter_item {
    announce_std::iter::Iterator! { [announce_std::iter::Iterator!, grab!] }
}

#[test]
fn provided_methods_have_placeholder_bodies() {
    let s: String = iter_item::ITEM.split_whitespace().collect();
    assert!(
        s.contains("fnnext(&mutself)->::core::option::Option<Self::Item>;"),
        "{}",
        s
    );
    assert!(s.contains("{::core::unimplemented!()}"), "{}", s);
}

fn _new_mocks<
    T: announce_std::clone::Clone
        + announce_std::marker::Copy
        + announce_std::default::Default
        + announce_std::any::Any,
>() {
}

#[test]
fn hand_written_mocks() {
    _new_mocks::<u8>();
}
