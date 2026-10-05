//! Uses announce only as `ann_ws`, renamed in the workspace root.

#[ann_ws::announce]
pub struct ViaWorkspace {
    pub a: u8,
}

#[ann_ws::announce]
pub trait MarkedWs {
    fn f(&self) -> u8;
}

macro_rules! capture {
    ([$($l:tt)*] $($t:tt)*) => { pub const OUT: &str = stringify!($($t)*); };
}
pub mod check { crate::ViaWorkspace! { [crate::ViaWorkspace!, capture!] } }

fn _assert_marker<T: ?Sized + ann_ws::Marker>() {}
const _: fn() = _assert_marker::<ViaWorkspace>;
const _: fn() = _assert_marker::<dyn MarkedWs>;
