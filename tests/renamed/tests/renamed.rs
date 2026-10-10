macro_rules! capture {
    ([$($l:tt)*] $($t:tt)*) => { pub const OUT: &str = stringify!($($t)*); };
}
mod a { ::announce_test_renamed::Plain! { [::announce_test_renamed::Plain!, capture!] } }
mod b { ::announce_test_renamed::WithRepeater! { [::announce_test_renamed::WithRepeater!, capture!] } }
mod c { ::announce_test_renamed::MarkedMock! { [::announce_test_renamed::MarkedMock!, capture!] } }

fn assert_marker<T: ?Sized + ann::Announce>() {}

#[test]
fn renamed_dependency_works() {
    assert!(a::OUT.contains("struct Plain"));
    assert!(b::OUT.contains("type_leak"));
    assert!(c::OUT.contains("mock"));
    assert_marker::<announce_test_renamed::Plain>();
    assert_marker::<dyn announce_test_renamed::Marked>();
}
