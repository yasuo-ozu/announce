pub mod inner { pub struct Local; }
use inner::Local;
pub const N: usize = 3;

// #7: the repeater must be local (`crate::..`).
#[announce::announce(repeater = ::core::marker::PhantomData)]
pub struct NonLocal(pub Local);

// #5: relative value paths cannot be covered by a repeater.
#[announce::announce]
pub struct RelLen(pub [u8; N]);

fn main() {}
