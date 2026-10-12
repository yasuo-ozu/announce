//! Items above `SINGLE_COPY_THRESHOLD` tokens: their trampoline holds the item
//! once (`Layout::Single`, DESIGN.md section 4.1). Each one is well over 300
//! tokens; `tests/user/tests/large.rs` runs them through every route and
//! `tests/ui/tests/ui/large_errors.rs` pins the errors only that layout has.

use super::announce;

// A trait with 30 required methods.
#[announce]
pub trait BigTrait {
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

// Required and provided methods (for `signature_only!`).
#[announce]
pub trait BigSig {
    fn required(&self) -> u32;
    fn r0(&self, x: u32) -> u32;
    fn r1(&self, x: u32) -> u32;
    fn r2(&self, x: u32) -> u32;
    fn r3(&self, x: u32) -> u32;
    fn r4(&self, x: u32) -> u32;
    fn r5(&self, x: u32) -> u32;
    fn r6(&self, x: u32) -> u32;
    fn r7(&self, x: u32) -> u32;
    fn r8(&self, x: u32) -> u32;
    fn r9(&self, x: u32) -> u32;
    fn r10(&self, x: u32) -> u32;
    fn r11(&self, x: u32) -> u32;
    fn r12(&self, x: u32) -> u32;
    fn r13(&self, x: u32) -> u32;
    fn p0(&self) -> u32 {
        0
    }
    fn p1(&self) -> u32 {
        1
    }
    fn p2(&self) -> u32 {
        2
    }
    fn p3(&self) -> u32 {
        3
    }
    fn p4(&self) -> u32 {
        4
    }
    fn p5(&self) -> u32 {
        5
    }
    fn p6(&self) -> u32 {
        6
    }
    fn p7(&self) -> u32 {
        7
    }
    fn p8(&self) -> u32 {
        8
    }
    fn p9(&self) -> u32 {
        9
    }
    fn p10(&self) -> u32 {
        10
    }
    fn p11(&self) -> u32 {
        11
    }
    fn p12(&self) -> u32 {
        12
    }
    fn p13(&self) -> u32 {
        13
    }
    fn body(&self) -> u32 {
        42
    }
}

#[announce]
pub struct BigStruct {
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

#[announce]
pub enum BigEnum {
    V0(u32),
    V1(u32),
    V2(u32),
    V3(u32),
    V4(u32),
    V5(u32),
    V6(u32),
    V7(u32),
    V8(u32),
    V9(u32),
    V10(u32),
    V11(u32),
    V12(u32),
    V13(u32),
    V14(u32),
    V15(u32),
    V16(u32),
    V17(u32),
    V18(u32),
    V19(u32),
    V20(u32),
    V21(u32),
    V22(u32),
    V23(u32),
    V24(u32),
    V25(u32),
    V26(u32),
    V27(u32),
    V28(u32),
    V29(u32),
    V30(u32),
    V31(u32),
    V32(u32),
    V33(u32),
    V34(u32),
    V35(u32),
    V36(u32),
    V37(u32),
    V38(u32),
    V39(u32),
    V40(u32),
    V41(u32),
    V42(u32),
    V43(u32),
    V44(u32),
    V45(u32),
    V46(u32),
    V47(u32),
    V48(u32),
    V49(u32),
    V50(u32),
    V51(u32),
    V52(u32),
    V53(u32),
    V54(u32),
    V55(u32),
    V56(u32),
    V57(u32),
    V58(u32),
    V59(u32),
    V60(u32),
    V61(u32),
    V62(u32),
    V63(u32),
    V64(u32),
    V65(u32),
    V66(u32),
    V67(u32),
    V68(u32),
    V69(u32),
}

#[announce]
#[derive(Clone, Copy)]
pub union BigUnion {
    pub i: u32,
    pub f: f32,
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
}

#[announce]
pub type BigAlias = (
    ::std::vec::Vec<u8>,
    ::std::vec::Vec<u8>,
    ::std::vec::Vec<u8>,
    ::std::vec::Vec<u8>,
    ::std::vec::Vec<u8>,
    ::std::vec::Vec<u8>,
    ::std::vec::Vec<u8>,
    ::std::vec::Vec<u8>,
    ::std::vec::Vec<u8>,
    ::std::vec::Vec<u8>,
    ::std::vec::Vec<u8>,
    ::std::vec::Vec<u8>,
    ::std::vec::Vec<u8>,
    ::std::vec::Vec<u8>,
    ::std::vec::Vec<u8>,
    ::std::vec::Vec<u8>,
    ::std::vec::Vec<u8>,
    ::std::vec::Vec<u8>,
    ::std::vec::Vec<u8>,
    ::std::vec::Vec<u8>,
    ::std::vec::Vec<u8>,
    ::std::vec::Vec<u8>,
    ::std::vec::Vec<u8>,
    ::std::vec::Vec<u8>,
    ::std::vec::Vec<u8>,
    ::std::vec::Vec<u8>,
    ::std::vec::Vec<u8>,
    ::std::vec::Vec<u8>,
    ::std::vec::Vec<u8>,
    ::std::vec::Vec<u8>,
    ::std::vec::Vec<u8>,
    ::std::vec::Vec<u8>,
    ::std::vec::Vec<u8>,
    ::std::vec::Vec<u8>,
    ::std::vec::Vec<u8>,
    ::std::vec::Vec<u8>,
    ::std::vec::Vec<u8>,
    ::std::vec::Vec<u8>,
    ::std::vec::Vec<u8>,
    ::std::vec::Vec<u8>,
    ::std::vec::Vec<u8>,
    ::std::vec::Vec<u8>,
    ::std::vec::Vec<u8>,
    ::std::vec::Vec<u8>,
    ::std::vec::Vec<u8>,
);

// `$` in a large item: a module with a macro and a repetition.
#[announce]
pub mod big_mod {
    macro_rules! sum {
        ($($x:expr),*) => {
            0 $(+ $x)*
        };
    }
    pub fn six() -> i32 {
        sum!(1, 2, 3)
    }
    pub fn f0() -> u32 {
        0
    }
    pub fn f1() -> u32 {
        1
    }
    pub fn f2() -> u32 {
        2
    }
    pub fn f3() -> u32 {
        3
    }
    pub fn f4() -> u32 {
        4
    }
    pub fn f5() -> u32 {
        5
    }
    pub fn f6() -> u32 {
        6
    }
    pub fn f7() -> u32 {
        7
    }
    pub fn f8() -> u32 {
        8
    }
    pub fn f9() -> u32 {
        9
    }
    pub fn f10() -> u32 {
        10
    }
    pub fn f11() -> u32 {
        11
    }
    pub fn f12() -> u32 {
        12
    }
    pub fn f13() -> u32 {
        13
    }
    pub fn f14() -> u32 {
        14
    }
    pub fn f15() -> u32 {
        15
    }
    pub fn f16() -> u32 {
        16
    }
    pub fn f17() -> u32 {
        17
    }
    pub fn f18() -> u32 {
        18
    }
    pub fn f19() -> u32 {
        19
    }
    pub fn f20() -> u32 {
        20
    }
    pub fn f21() -> u32 {
        21
    }
    pub fn f22() -> u32 {
        22
    }
    pub fn f23() -> u32 {
        23
    }
    pub fn f24() -> u32 {
        24
    }
    pub fn f25() -> u32 {
        25
    }
    pub fn f26() -> u32 {
        26
    }
    pub fn f27() -> u32 {
        27
    }
    pub fn f28() -> u32 {
        28
    }
    pub fn f29() -> u32 {
        29
    }
}

#[announce]
pub mod big_mod2 {
    macro_rules! prod {
        ($($x:expr),*) => {
            1 $(* $x)*
        };
    }
    pub fn twenty_four() -> i32 {
        prod!(2, 3, 4)
    }
    pub fn f0() -> u32 {
        0
    }
    pub fn f1() -> u32 {
        1
    }
    pub fn f2() -> u32 {
        2
    }
    pub fn f3() -> u32 {
        3
    }
    pub fn f4() -> u32 {
        4
    }
    pub fn f5() -> u32 {
        5
    }
    pub fn f6() -> u32 {
        6
    }
    pub fn f7() -> u32 {
        7
    }
    pub fn f8() -> u32 {
        8
    }
    pub fn f9() -> u32 {
        9
    }
    pub fn f10() -> u32 {
        10
    }
    pub fn f11() -> u32 {
        11
    }
    pub fn f12() -> u32 {
        12
    }
    pub fn f13() -> u32 {
        13
    }
    pub fn f14() -> u32 {
        14
    }
    pub fn f15() -> u32 {
        15
    }
    pub fn f16() -> u32 {
        16
    }
    pub fn f17() -> u32 {
        17
    }
    pub fn f18() -> u32 {
        18
    }
    pub fn f19() -> u32 {
        19
    }
    pub fn f20() -> u32 {
        20
    }
    pub fn f21() -> u32 {
        21
    }
    pub fn f22() -> u32 {
        22
    }
    pub fn f23() -> u32 {
        23
    }
    pub fn f24() -> u32 {
        24
    }
    pub fn f25() -> u32 {
        25
    }
    pub fn f26() -> u32 {
        26
    }
    pub fn f27() -> u32 {
        27
    }
    pub fn f28() -> u32 {
        28
    }
    pub fn f29() -> u32 {
        29
    }
}

// For `signature_only!` on a large module.
#[announce]
pub mod big_sig_mod {
    pub fn plain() -> u32 {
        7
    }
    pub fn opaque() -> impl ::core::fmt::Display {
        8
    }
    pub fn f0() -> u32 {
        0
    }
    pub fn f1() -> u32 {
        1
    }
    pub fn f2() -> u32 {
        2
    }
    pub fn f3() -> u32 {
        3
    }
    pub fn f4() -> u32 {
        4
    }
    pub fn f5() -> u32 {
        5
    }
    pub fn f6() -> u32 {
        6
    }
    pub fn f7() -> u32 {
        7
    }
    pub fn f8() -> u32 {
        8
    }
    pub fn f9() -> u32 {
        9
    }
    pub fn f10() -> u32 {
        10
    }
    pub fn f11() -> u32 {
        11
    }
    pub fn f12() -> u32 {
        12
    }
    pub fn f13() -> u32 {
        13
    }
    pub fn f14() -> u32 {
        14
    }
    pub fn f15() -> u32 {
        15
    }
    pub fn f16() -> u32 {
        16
    }
    pub fn f17() -> u32 {
        17
    }
    pub fn f18() -> u32 {
        18
    }
    pub fn f19() -> u32 {
        19
    }
    pub fn f20() -> u32 {
        20
    }
    pub fn f21() -> u32 {
        21
    }
    pub fn f22() -> u32 {
        22
    }
    pub fn f23() -> u32 {
        23
    }
    pub fn f24() -> u32 {
        24
    }
    pub fn f25() -> u32 {
        25
    }
    pub fn f26() -> u32 {
        26
    }
    pub fn f27() -> u32 {
        27
    }
    pub fn f28() -> u32 {
        28
    }
    pub fn f29() -> u32 {
        29
    }
}

// The derive and attribute routes into a large trampoline: the derive
// `BigParse` and the attribute `big_parse_attr` redirect to
// `BigParseAnnounce!`.
#[announce(macro_name = BigParseAnnounce)]
pub trait BigParse {
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
pub use announce_test_derive::big_parse_attr;
pub use announce_test_derive::BigParse;

// A large item with a check and `meta`.
#[announce(announce_test_derive::no_tuple(big), meta = my_lib::info(big))]
pub struct BigChecked {
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

// The layout can be chosen: a small item held once ..
#[announce(layout = single)]
pub trait SmallSingle {
    fn f(&self) -> u8;
}

#[announce(layout = single)]
pub struct SmallSingleStruct<T>(pub T, pub u8);

#[announce(layout = single)]
pub mod small_single_mod {
    macro_rules! sum {
        ($($x:expr),*) => {
            0 $(+ $x)*
        };
    }
    pub fn six() -> i32 {
        sum!(1, 2, 3)
    }
}

// .. a small derive target held once ..
#[announce(macro_name = SmallSingleParseAnnounce, layout = single)]
pub trait SmallSingleParse {
    fn parse(s: &str) -> Self;
}
pub use announce_test_derive::SmallSingleParse;

// .. and large items copied into each arm.
#[announce(layout = direct)]
pub trait BigDirect {
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

#[announce(layout = direct)]
pub mod big_direct_mod {
    macro_rules! sum {
        ($($x:expr),*) => {
            0 $(+ $x)*
        };
    }
    pub fn six() -> i32 {
        sum!(1, 2, 3)
    }
    pub fn f0() -> u32 {
        0
    }
    pub fn f1() -> u32 {
        1
    }
    pub fn f2() -> u32 {
        2
    }
    pub fn f3() -> u32 {
        3
    }
    pub fn f4() -> u32 {
        4
    }
    pub fn f5() -> u32 {
        5
    }
    pub fn f6() -> u32 {
        6
    }
    pub fn f7() -> u32 {
        7
    }
    pub fn f8() -> u32 {
        8
    }
    pub fn f9() -> u32 {
        9
    }
    pub fn f10() -> u32 {
        10
    }
    pub fn f11() -> u32 {
        11
    }
    pub fn f12() -> u32 {
        12
    }
    pub fn f13() -> u32 {
        13
    }
    pub fn f14() -> u32 {
        14
    }
    pub fn f15() -> u32 {
        15
    }
    pub fn f16() -> u32 {
        16
    }
    pub fn f17() -> u32 {
        17
    }
    pub fn f18() -> u32 {
        18
    }
    pub fn f19() -> u32 {
        19
    }
    pub fn f20() -> u32 {
        20
    }
    pub fn f21() -> u32 {
        21
    }
    pub fn f22() -> u32 {
        22
    }
    pub fn f23() -> u32 {
        23
    }
    pub fn f24() -> u32 {
        24
    }
    pub fn f25() -> u32 {
        25
    }
    pub fn f26() -> u32 {
        26
    }
    pub fn f27() -> u32 {
        27
    }
    pub fn f28() -> u32 {
        28
    }
    pub fn f29() -> u32 {
        29
    }
}

// The derive form takes the option in its helper attribute, and the item is
// emitted as written.
#[derive(::announce::Announce, Debug)]
#[announce(layout = single, meta = crate::info(derived))]
pub struct DerivedSingle {
    pub a: u8,
    #[cfg(any())]
    pub gone: u8,
}
