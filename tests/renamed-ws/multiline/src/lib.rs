//! Uses announce only as `ann_multi`, renamed with an inline table over
//! several lines (ALGORITHM_FINDINGS #10). Before, the scanner of `Cargo.toml`
//! saw `ann-multi = {` only and the generated code named `::announce`.

#[ann_multi::announce]
pub struct Multi {
    pub a: u8,
}

#[ann_multi::announce]
pub trait MarkedMulti {
    fn f(&self) -> u8;
}

macro_rules! capture {
    ([$($l:tt)*] $($t:tt)*) => { pub const OUT: &str = stringify!($($t)*); };
}
pub mod check {
    crate::Multi! { [crate::Multi!, capture!] }
}

fn _assert_marker<T: ?Sized + ann_multi::Announce>() {}
const _: fn() = _assert_marker::<Multi>;
const _: fn() = _assert_marker::<dyn MarkedMulti>;

#[test]
fn the_multi_line_rename_is_found() {
    assert!(check::OUT.contains("struct Multi"), "{}", check::OUT);
    assert_eq!(Multi { a: 1 }.a, 1);
}
