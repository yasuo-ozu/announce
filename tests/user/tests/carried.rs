//! Trait objects, array lengths and const arguments of a type are carried by a `repeater`
//! (`tests/defs/src/carried.rs`): the copies below are made in another crate, where `Base`, `N`
//! and `Wrap` do not resolve, and use them through the table.

use announce_test_defs::carried::m::{inner::Wrap, Base};

mod copy {
    announce_test_defs::carried::m::Item! {
        [announce_test_defs::carried::m::Item!, announce_test_derive::resolved_copy!] ItemCopy
    }
    announce_test_defs::carried::m::Make! {
        [announce_test_defs::carried::m::Make!, announce_test_derive::resolved_copy!] MakeCopy
    }
}

struct Three;

impl Base for Three {
    fn n(&self) -> u8 {
        3
    }
}

#[test]
fn the_item_is_used_with_a_trait_object_and_constants_of_its_module() {
    let item = copy::ItemCopy {
        a: Box::new(Three),
        b: [1, 2, 3],
        c: Wrap([4, 5, 6]),
    };
    assert_eq!(item.a.n(), 3);
    assert_eq!(item.b.iter().sum::<u8>(), 6);
    assert_eq!(item.c.0, [4, 5, 6]);
    // The length is the constant of the defining module.
    assert_eq!(item.b.len(), announce_test_defs::carried::m::N);
}

#[test]
fn the_trait_is_implemented_with_the_carried_types() {
    struct Maker;
    impl copy::MakeCopy for Maker {
        fn object(&self) -> Box<dyn Base + Send> {
            Box::new(Three2)
        }
        fn array(&self) -> [u8; 3] {
            [7, 8, 9]
        }
        fn wrap(&self) -> Wrap<3> {
            Wrap([1, 1, 1])
        }
    }
    struct Three2;
    impl Base for Three2 {
        fn n(&self) -> u8 {
            30
        }
    }
    let m = Maker;
    assert_eq!(copy::MakeCopy::object(&m).n(), 30);
    assert_eq!(copy::MakeCopy::array(&m), [7, 8, 9]);
    assert_eq!(copy::MakeCopy::wrap(&m).0, [1, 1, 1]);
}
