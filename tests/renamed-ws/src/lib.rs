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
pub mod check {
    crate::ViaWorkspace! { [crate::ViaWorkspace!, capture!] }
}

fn _assert_marker<T: ?Sized + ann_ws::Announce>() {}
const _: fn() = _assert_marker::<ViaWorkspace>;
const _: fn() = _assert_marker::<dyn MarkedWs>;

macro_rules! ws_check {
    (@__announce_attrs [ $($np:ident)? $(:: $ns:ident)* ! $(, $($rest:tt)*)? ] { $($state:tt)* }) => {
        $($np)? $(:: $ns)* ! { @__announce_attrs [ $($($rest)*)? ] { $($state)* attrs(ws_attr) } }
    };
    (($($args:tt)*), $($item:tt)*) => {};
}

/// The query ends at `finish!`, reached through the workspace-inherited rename.
#[ann_ws::announce(ws_check)]
pub struct WithCheckWs {
    #[ws_attr]
    pub a: u8,
}

pub mod checked {
    crate::WithCheckWs! { [crate::WithCheckWs!, capture!] }
}

#[test]
fn the_attribute_query_goes_through_the_workspace_rename() {
    let out: String = checked::OUT.split_whitespace().collect();
    assert!(
        out.contains("attrs(ws_attr)") && out.contains("#[ws_attr]puba:u8"),
        "{}",
        out
    );
    assert_eq!(WithCheckWs { a: 1 }.a, 1);
}
