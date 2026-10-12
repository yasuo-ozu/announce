macro_rules! capture {
    ([$($l:tt)*] $($t:tt)*) => { pub const OUT: &str = stringify!($($t)*); };
}
mod a {
    ::announce_test_renamed::Plain! { [::announce_test_renamed::Plain!, capture!] }
}
mod b {
    ::announce_test_renamed::WithRepeater! { [::announce_test_renamed::WithRepeater!, capture!] }
}
mod c {
    ::announce_test_renamed::MarkedMock! { [::announce_test_renamed::MarkedMock!, capture!] }
}

fn assert_marker<T: ?Sized + ann::Announce>() {}

#[test]
fn renamed_dependency_works() {
    assert!(a::OUT.contains("struct Plain"));
    assert!(b::OUT.contains("type_leak"));
    assert!(c::OUT.contains("mock"));
    assert_marker::<announce_test_renamed::Plain>();
    assert_marker::<dyn announce_test_renamed::Marked>();
}

mod explicit {
    ::announce_test_renamed::Explicit! { [::announce_test_renamed::Explicit!, capture!] }
    pub mod t {
        ::announce_test_renamed::ExplicitTrait! { [::announce_test_renamed::ExplicitTrait!, capture!] }
    }
    pub mod d {
        ::announce_test_renamed::DerivedExplicit! { [::announce_test_renamed::DerivedExplicit!, capture!] }
    }
}

#[test]
fn crate_option_with_a_renamed_dependency() {
    assert_marker::<announce_test_renamed::Explicit>();
    assert_marker::<dyn announce_test_renamed::ExplicitTrait>();
    assert_marker::<announce_test_renamed::DerivedExplicit>();
    assert!(explicit::OUT.contains("struct Explicit"));
    assert!(explicit::t::OUT.contains("trait ExplicitTrait"));
    assert!(explicit::d::OUT.contains("struct DerivedExplicit"));
    // An explicit path wins over the one read from Cargo.toml.
    fn spy<T: ?Sized + announce_test_renamed::spy::Announce>() {}
    spy::<announce_test_renamed::ViaSpy>();
}

mod checked {
    ::announce_test_renamed::WithCheck! { [::announce_test_renamed::WithCheck!, capture!] }
    pub mod e {
        ::announce_test_renamed::WithCheckExplicit! { [::announce_test_renamed::WithCheckExplicit!, capture!] }
    }
}

#[test]
fn the_attribute_query_goes_through_the_renamed_dependency() {
    let out: String = checked::OUT.split_whitespace().collect();
    assert!(
        out.contains("[$crate::rename_check,attrs(renamed_attr)]")
            || out.contains("[rename_check,attrs(renamed_attr)]"),
        "{}",
        out
    );
    assert!(out.contains("#[renamed_attr(x)]"), "{}", out);
    let out: String = checked::e::OUT.split_whitespace().collect();
    assert!(out.contains("attrs(renamed_attr)"), "{}", out);
    // The items were emitted without the attribute.
    let w = announce_test_renamed::WithCheck { a: 1 };
    assert_eq!(w.a + announce_test_renamed::WithCheckExplicit { a: 2 }.a, 3);
}
