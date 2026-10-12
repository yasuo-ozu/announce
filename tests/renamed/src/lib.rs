//! Uses announce only as `ann` (renamed in Cargo.toml).

pub mod inner {
    pub struct Local(pub u8);
}

#[ann::announce]
pub struct Plain {
    pub a: u8,
}

/// The `Announce` impl and the capture derive must resolve through `::ann`.
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

/// `crate = path` names the (renamed) dependency explicitly.
#[ann::announce(crate = ::ann)]
pub struct Explicit {
    pub a: u8,
}

#[ann::announce(crate = ::ann)]
pub trait ExplicitTrait {
    fn f(&self) -> u8;
}

#[derive(ann::Announce)]
#[announce(crate = ::ann, meta = info(2))]
pub struct DerivedExplicit {
    pub a: u8,
}

/// The option wins over the name read from `Cargo.toml`: the `Announce` impl
/// names this module's trait, not `ann::Announce`.
pub mod spy {
    pub use ::ann::__private;
    pub trait Announce {}
}

#[ann::announce(crate = crate::spy)]
pub struct ViaSpy;
