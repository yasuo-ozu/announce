//! Uses announce only as `ann_dot`, renamed with the dotted keys
//! `"ann-dot".package = "announce"`. Before, the scanner
//! of `Cargo.toml` missed the rename and the generated code named `::announce`.

pub mod inner {
    pub struct Local(pub u8);
}

#[ann_dot::announce]
pub struct Dotted {
    pub a: u8,
}

#[ann_dot::announce]
pub trait MarkedDotted {
    fn f(&self) -> u8;
}

ann_dot::define_repeater!(pub Repeater);
use inner::Local;

#[ann_dot::announce(repeater = crate::Repeater)]
pub struct WithRepeater(pub Local);

macro_rules! capture {
    (@announce_format "1" [$($l:tt)*] $($t:tt)*) => { pub const OUT: &str = stringify!($($t)*); };
}
pub mod check {
    crate::Dotted! { [crate::Dotted!, capture!] }
    pub mod repeater {
        crate::WithRepeater! { [crate::WithRepeater!, capture!] }
    }
}

fn _assert_marker<T: ?Sized + ann_dot::Announce>() {}
const _: fn() = _assert_marker::<Dotted>;
const _: fn() = _assert_marker::<dyn MarkedDotted>;

#[test]
fn the_dotted_rename_is_found() {
    assert!(check::OUT.contains("struct Dotted"), "{}", check::OUT);
    assert!(
        check::repeater::OUT.contains("type_leak"),
        "{}",
        check::repeater::OUT
    );
    assert_eq!(Dotted { a: 1 }.a, 1);
    let _ = WithRepeater(Local(1));
}
