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

ann::define_repeater!(pub Repeater);
use inner::Local;

#[ann::announce(repeater = crate::Repeater, meta = info(1))]
pub struct WithRepeater<T>(pub T, pub Local);

/// A relative type without a `repeater` is recorded with `strict = false`: the marker next to
/// the item names the renamed dependency (`::ann::__private`), and the check of
/// `ann::same_context!` passes in the module of the item.
#[ann::announce(strict = false)]
pub struct Recorded {
    pub a: Local,
}

#[ann::announce(crate = ::ann, strict = false)]
pub struct RecordedExplicit {
    pub a: Local,
}

Recorded! { [Recorded!, ann::same_context!] }
RecordedExplicit! { [RecordedExplicit!, ann::same_context!] }

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

/// With a derive of another trait, the item goes through the runner, which names the
/// renamed dependency (`::ann::__private::__announce_runner`).
#[ann::announce(crate = ::ann, meta = info(2))]
#[derive(Debug)]
pub struct ExplicitWithDerive {
    pub a: u8,
}

/// The same without `crate = ..`: the path is read from `Cargo.toml`.
#[ann::announce(meta = info(3))]
#[derive(Debug)]
pub struct RenamedWithDerive {
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

/// A check that owns an attribute: the query ends at `finish!`, which is reached
/// through the renamed dependency (`::ann::__private::finish!`).
macro_rules! rename_check {
    (@announce_format "1" @__announce_attrs [ $($hp:ident)? $(:: $hs:ident)* ! , $($np:ident)? $(:: $ns:ident)* ! $(, $($rest:tt)*)? ] { $($state:tt)* }) => {
        $($np)? $(:: $ns)* ! { @announce_format "1" @__announce_attrs [ $($np)? $(:: $ns)* ! $(, $($rest)*)? ] { $($state)* attrs(renamed_attr) } }
    };
    (@announce_format "1" ($($args:tt)*), $($item:tt)*) => {};
}

#[ann::announce(rename_check)]
pub struct WithCheck {
    #[renamed_attr(x)]
    pub a: u8,
}

#[ann::announce(crate = ::ann, rename_check)]
pub struct WithCheckExplicit {
    #[renamed_attr]
    pub a: u8,
}
