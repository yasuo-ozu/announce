//! Uses announce only as `ann` (renamed in Cargo.toml).

pub mod inner {
    pub struct Local(pub u8);
}

#[ann::announce]
pub struct Plain {
    pub a: u8,
}

/// The `Marker` impl and the capture derive must resolve through `::ann`.
#[ann::announce]
pub trait Marked {
    fn f(&self) -> u8;
}

pub trait Repeater<const ID: u64, const I: usize, G: ?Sized> {
    type Type: ?Sized;
}
use inner::Local;

#[ann::announce(repeater = crate::Repeater, meta = info(1))]
pub struct WithRepeater<T>(pub T, pub Local);

#[ann::announce(mock = crate::Marked)]
pub trait MarkedMock {
    fn f(&self) -> u8;
}
