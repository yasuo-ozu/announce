//! type-leak with a large item: the `type_leak(..)` META travels in the single
//! copy of META (see `large.rs`).

use super::announce;

::announce::define_repeater!(pub Repeater);

pub mod inner {
    #[derive(Debug, PartialEq)]
    pub struct Local(pub u8);
}
use inner::Local;

#[announce(repeater = crate::large_leak::Repeater)]
pub struct BigLeaky<T> {
    pub a: Local,
    pub b: Vec<T>,
    pub f0: u32,
    pub f1: u32,
    pub f2: u32,
    pub f3: u32,
    pub f4: u32,
    pub f5: u32,
    pub f6: u32,
    pub f7: u32,
    pub f8: u32,
    pub f9: u32,
    pub f10: u32,
    pub f11: u32,
    pub f12: u32,
    pub f13: u32,
    pub f14: u32,
    pub f15: u32,
    pub f16: u32,
    pub f17: u32,
    pub f18: u32,
    pub f19: u32,
    pub f20: u32,
    pub f21: u32,
    pub f22: u32,
    pub f23: u32,
    pub f24: u32,
    pub f25: u32,
    pub f26: u32,
    pub f27: u32,
    pub f28: u32,
    pub f29: u32,
    pub f30: u32,
    pub f31: u32,
    pub f32: u32,
    pub f33: u32,
    pub f34: u32,
    pub f35: u32,
    pub f36: u32,
    pub f37: u32,
    pub f38: u32,
    pub f39: u32,
    pub f40: u32,
    pub f41: u32,
    pub f42: u32,
    pub f43: u32,
    pub f44: u32,
    pub f45: u32,
    pub f46: u32,
    pub f47: u32,
    pub f48: u32,
    pub f49: u32,
    pub f50: u32,
    pub f51: u32,
    pub f52: u32,
    pub f53: u32,
    pub f54: u32,
    pub f55: u32,
    pub f56: u32,
    pub f57: u32,
    pub f58: u32,
    pub f59: u32,
    pub f60: u32,
    pub f61: u32,
    pub f62: u32,
    pub f63: u32,
    pub f64: u32,
    pub f65: u32,
    pub f66: u32,
    pub f67: u32,
    pub f68: u32,
    pub f69: u32,
    pub f70: u32,
    pub f71: u32,
    pub f72: u32,
    pub f73: u32,
    pub f74: u32,
    pub f75: u32,
    pub f76: u32,
    pub f77: u32,
    pub f78: u32,
    pub f79: u32,
}

#[announce(repeater = crate::large_leak::Repeater)]
pub trait BigLeakTrait {
    fn make(&self) -> Local;
    fn m0(&self, x: u32) -> u32;
    fn m1(&self, x: u32) -> u32;
    fn m2(&self, x: u32) -> u32;
    fn m3(&self, x: u32) -> u32;
    fn m4(&self, x: u32) -> u32;
    fn m5(&self, x: u32) -> u32;
    fn m6(&self, x: u32) -> u32;
    fn m7(&self, x: u32) -> u32;
    fn m8(&self, x: u32) -> u32;
    fn m9(&self, x: u32) -> u32;
    fn m10(&self, x: u32) -> u32;
    fn m11(&self, x: u32) -> u32;
    fn m12(&self, x: u32) -> u32;
    fn m13(&self, x: u32) -> u32;
    fn m14(&self, x: u32) -> u32;
    fn m15(&self, x: u32) -> u32;
    fn m16(&self, x: u32) -> u32;
    fn m17(&self, x: u32) -> u32;
    fn m18(&self, x: u32) -> u32;
    fn m19(&self, x: u32) -> u32;
    fn m20(&self, x: u32) -> u32;
    fn m21(&self, x: u32) -> u32;
    fn m22(&self, x: u32) -> u32;
    fn m23(&self, x: u32) -> u32;
    fn m24(&self, x: u32) -> u32;
    fn m25(&self, x: u32) -> u32;
    fn m26(&self, x: u32) -> u32;
    fn m27(&self, x: u32) -> u32;
    fn m28(&self, x: u32) -> u32;
    fn m29(&self, x: u32) -> u32;
}
