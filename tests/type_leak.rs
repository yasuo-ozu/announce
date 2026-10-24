//! Repeater tables as a consumer sees them: the META, and the items re-emitted
//! through `TypeLeak::apply` in modules where the relative types are not in scope.

fn norm(s: &str) -> String {
    s.split_whitespace().collect()
}

/// The META with the random `id = <digits>u64` replaced by `id=ID`.
fn without_id(meta: &str) -> String {
    let m = norm(meta);
    let start = m.find("id=").expect("no id in META") + "id=".len();
    let end = start + m[start..].find("u64").expect("id without u64 suffix") + "u64".len();
    assert!(
        m[start..end - 3].chars().all(|c| c.is_ascii_digit()),
        "{}",
        m
    );
    format!("{}ID{}", &m[..start], &m[end..])
}

// H4: a repeater, but only absolute types: the table is there and empty.
mod empty {
    pub mod re {
        ::announce_tests::defs::empty_table::NoLeaks! { [::announce_tests::defs::empty_table::NoLeaks!, ::announce_tests::derive::apply_leak!] M }
    }
    pub mod ty {
        ::announce_tests::defs::empty_table::NoLeaks! { [::announce_tests::defs::empty_table::NoLeaks!, ::announce_tests::derive::leak_types!] T }
    }
}

#[test]
fn repeater_with_only_absolute_types_has_an_empty_table() {
    assert_eq!(
        without_id(empty::re::M[0]),
        format!(
            "type_leak(repeater=$crate::empty_table::Repeater,id=ID,context=0,referrer=[\"type-leak-format-{}\"()])",
            announce::type_leak::Referrer::FORMAT_VERSION
        )
    );
    // `Entry::type_leak()` is `Some`, with empty lists.
    assert_eq!(empty::ty::T, ["[]"]);
    // The item is unchanged.
    let v = empty::re::NoLeaks {
        a: 1,
        b: vec![2],
        c: Some(announce_tests::defs::empty_table::NoLeaks2),
    };
    assert_eq!(v.b, [2]);
}

// M6: bounds and const generics of the item survive in the table and the item.
mod bounds {
    pub mod bounded {
        ::announce_tests::defs::table_bounds::Bounded! { [::announce_tests::defs::table_bounds::Bounded!, ::announce_tests::derive::apply_leak!] M }
    }
    pub mod bounded_ty {
        ::announce_tests::defs::table_bounds::Bounded! { [::announce_tests::defs::table_bounds::Bounded!, ::announce_tests::derive::leak_types!] T }
    }
    pub mod arr {
        ::announce_tests::defs::table_bounds::ArrOfLocal! { [::announce_tests::defs::table_bounds::ArrOfLocal!, ::announce_tests::derive::apply_leak!] M }
    }
    pub mod arr_ty {
        ::announce_tests::defs::table_bounds::ArrOfLocal! { [::announce_tests::defs::table_bounds::ArrOfLocal!, ::announce_tests::derive::leak_types!] T }
    }
    pub mod konst {
        ::announce_tests::defs::table_bounds::ConstArg! { [::announce_tests::defs::table_bounds::ConstArg!, ::announce_tests::derive::apply_leak!] M }
    }
    pub mod konst_ty {
        ::announce_tests::defs::table_bounds::ConstArg! { [::announce_tests::defs::table_bounds::ConstArg!, ::announce_tests::derive::leak_types!] T }
    }
    pub mod both {
        ::announce_tests::defs::table_bounds::Both! { [::announce_tests::defs::table_bounds::Both!, ::announce_tests::derive::apply_leak!] M }
    }
    pub mod both_ty {
        ::announce_tests::defs::table_bounds::Both! { [::announce_tests::defs::table_bounds::Both!, ::announce_tests::derive::leak_types!] T }
    }
}

#[test]
fn repeater_table_keeps_bounds() {
    use announce_tests::defs::table_bounds::inner::{Local, Wrap};
    // `T: Clone` is a bound of the item, and `Wrap<T>` needs it to be well-formed.
    let b = bounds::bounded::Bounded::<u8>(vec![1], Wrap(2), Local(3));
    assert_eq!((b.0[0], b.1 .0, b.2), (1, 2, Local(3)));
    // Only the relative types are carried; `::std::vec::Vec<T>` is left as written.
    assert_eq!(bounds::bounded_ty::T, ["[Local@(),Wrap<T>@(T,)]"]);
    let m = without_id(bounds::bounded::M[0]);
    assert!(
        m.starts_with("type_leak(repeater=$crate::table_bounds::Repeater,id=ID,context=0,referrer=[\"type-leak-format-"),
        "{}",
        m
    );
    assert!(!m.contains("Vec"), "{}", m);
}

#[test]
fn repeater_table_const_generics() {
    use announce_tests::defs::table_bounds::inner::{Buf, Local, Lt};
    // `[Local; N]`: `Local` is carried, the const parameter stays on the array.
    let a = bounds::arr::ArrOfLocal::<2>([Local(1), Local(2)]);
    assert_eq!(a.0[1], Local(2));
    assert_eq!(bounds::arr_ty::T, ["[Local@()]"]);
    // `Buf<N>` mentions the const parameter: `G` is `([(); N],)`.
    let c = bounds::konst::ConstArg::<3>(Buf([1, 2, 3]));
    assert_eq!(c.0 .0, [1, 2, 3]);
    assert_eq!(bounds::konst_ty::T, ["[Buf<N>@([();N],)]"]);
    // Lifetime, type and const parameters in one item.
    let x = 7u8;
    let b = bounds::both::Both::<u8, 2>(
        Lt(&x),
        announce_tests::defs::table_bounds::inner::Wrap(1u8),
        Buf([0, 0]),
    );
    assert_eq!(*b.0 .0, 7);
    assert_eq!(
        bounds::both_ty::T,
        ["[Buf<N>@([();N],),Lt<'a>@(&'a(),),Wrap<T>@(T,)]"]
    );
}

// M7: `allow_std_prelude = true` with a repeater.
mod prelude {
    pub mod re {
        ::announce_tests::defs::prelude_leak::Holder! { [::announce_tests::defs::prelude_leak::Holder!, ::announce_tests::derive::apply_leak!] M }
    }
    pub mod ty {
        ::announce_tests::defs::prelude_leak::Holder! { [::announce_tests::defs::prelude_leak::Holder!, ::announce_tests::derive::leak_types!] T }
    }
}

#[test]
fn allow_std_prelude_with_repeater() {
    use announce_tests::defs::prelude_leak::inner::Local;
    // The item is re-emitted by `apply_leak!` in a module where `Local` is not in
    // scope, and it compiles: that is the point. The prelude names themselves
    // (`Vec`, `Option`, `Box`, `String`) resolve through the caller's prelude;
    // `Local` has to come from the table.
    let h = prelude::re::Holder {
        a: vec![Local(1)],
        b: Some(Box::new(Local(2))),
        s: String::from("s"),
    };
    assert_eq!(h.a[0], Local(1));
    assert_eq!(*h.b.unwrap(), Local(2));
    // The relative `Local` is carried, in whichever types around it type-leak
    // interns (which prelude wrapper that is, is its business). `String` needs
    // nothing and is not carried.
    let row = match prelude::ty::T {
        [row] => row,
        t => panic!("one entry expected: {:?}", t),
    };
    assert!(row.contains("Local"), "{}", row);
    assert!(!row.contains("String"), "{}", row);
    let m = norm(prelude::re::M[0]);
    assert!(m.contains("type_leak(repeater="), "{}", m);
    assert!(m.contains("Local"), "{}", m);
    assert!(!m.contains("String"), "{}", m);
}

// M13: new type-leak 0.8 behavior.
mod leak08 {
    pub mod elided {
        ::announce_tests::defs::leak08::Elided! { [::announce_tests::defs::leak08::Elided!, ::announce_tests::derive::apply_leak!] M }
    }
    pub mod elided_ty {
        ::announce_tests::defs::leak08::Elided! { [::announce_tests::defs::leak08::Elided!, ::announce_tests::derive::leak_types!] T }
    }
    pub mod hr {
        ::announce_tests::defs::leak08::Hr! { [::announce_tests::defs::leak08::Hr!, ::announce_tests::derive::apply_leak!] M }
    }
    pub mod hr_ty {
        ::announce_tests::defs::leak08::Hr! { [::announce_tests::defs::leak08::Hr!, ::announce_tests::derive::leak_types!] T }
    }
    pub mod tup {
        ::announce_tests::defs::leak08::Tup! { [::announce_tests::defs::leak08::Tup!, ::announce_tests::derive::apply_leak!] M }
    }
    pub mod tup_ty {
        ::announce_tests::defs::leak08::Tup! { [::announce_tests::defs::leak08::Tup!, ::announce_tests::derive::leak_types!] T }
    }
}

#[test]
fn elided_reference_to_a_relative_type_is_carried() {
    use announce_tests::defs::leak08::inner::Local;
    struct X;
    impl leak08::elided::Elided for X {
        fn get(&self, l: &Local) -> u8 {
            l.0
        }
        fn pick<'a>(&'a self, l: &'a Local) -> &'a Local {
            l
        }
    }
    let l = Local(5);
    assert_eq!(leak08::elided::Elided::get(&X, &l), 5);
    assert_eq!(leak08::elided::Elided::pick(&X, &l), &Local(5));
    // `Local` is carried, `&` and its elided lifetime are not part of the type.
    assert_eq!(leak08::elided_ty::T, ["[Local@()]"]);
}

#[test]
fn higher_ranked_fn_bound_over_a_relative_type() {
    use announce_tests::defs::leak08::inner::Local;
    let h = leak08::hr::Hr {
        f: Box::new(|l: &Local| l.0 + 1),
    };
    assert_eq!((h.f)(&Local(4)), 5);
    assert_eq!(leak08::hr_ty::T, ["[Local@()]"]);
}

#[test]
fn tuple_carries_only_the_relative_member() {
    use announce_tests::defs::leak08::inner::Local;
    let t = leak08::tup::Tup { t: (Local(1), 2) };
    assert_eq!(t.t, (Local(1), 2));
    assert_eq!(leak08::tup_ty::T, ["[Local@()]"]);
    let m = without_id(leak08::tup::M[0]);
    assert!(m.contains(",referrer=[\"type-leak-format-"), "{}", m);
    assert!(m.ends_with("(Local)])"), "{}", m);
}

#[test]
fn method_lifetime_kept_by_another_argument() {
    use announce_tests::defs::leak08::wrap::Wrap;
    mod c {
        ::announce_tests::defs::leak08::Constrained! { [::announce_tests::defs::leak08::Constrained!, ::announce_tests::derive::apply_leak!] M }
        pub mod ty {
            ::announce_tests::defs::leak08::Constrained! { [::announce_tests::defs::leak08::Constrained!, ::announce_tests::derive::leak_types!] T }
        }
    }
    struct X;
    impl c::Constrained for X {
        fn by_self<'a>(&'a self, a: Wrap<'a>) -> &'a u8 {
            a.0
        }
        fn by_arg<'a>(&self, a: Wrap<'a>, _b: &'a u8) -> &'a u8 {
            a.0
        }
    }
    let v = 3u8;
    assert_eq!(*c::Constrained::by_self(&X, Wrap(&v)), 3);
    assert_eq!(*c::Constrained::by_arg(&X, Wrap(&v), &v), 3);
    assert_eq!(c::ty::T, ["[Wrap<'a>@(&'a(),)]"]);
}
