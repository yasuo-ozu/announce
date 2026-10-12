//! The repeater of type-leak 0.8 as a consumer sees it: each leaked type is
//! rewritten with the generics of its own scope, and the re-emitted items use
//! them from a module where the relative types are not in scope.
use announce_test_defs::contexts::inner::{Arr, Flag, Local, LocalC, LocalG, Wrap, G, GC, W};

macro_rules! reemit {
    ($($name:ident = $leaf:ident;)*) => {$(
        mod $name {
            ::announce_test_defs::contexts::$leaf! {
                [::announce_test_defs::contexts::$leaf!, ::announce_test_derive::apply_leak!] M
            }
        }
    )*};
}

reemit! {
    same_text = SameTextTwice;
    generic_like_type = GenericNamedLikeAType;
    bound_other = BoundNamesOther;
    method_bound = MethodBoundNamesTrait;
    outlives = OutlivesBound;
    where_clause = WhereClause;
    name_elsewhere = NameOnlyElsewhere;
    kind_clash = KindClash;
    const_params = ConstParams;
    const_trait = ConstTrait;
    higher_ranked = HigherRanked;
}

struct X;

impl same_text::SameTextTwice for X {
    fn f<X2: Clone>(&self, x: X2) -> GC<X2> {
        GC(x)
    }
    fn g<X2>(&self, x: X2) -> G<X2> {
        G(x)
    }
}

impl generic_like_type::GenericNamedLikeAType for X {
    fn f<L>(&self, x: L) -> L {
        x
    }
    fn g(&self) -> Local {
        Local(4)
    }
}

impl method_bound::MethodBoundNamesTrait<u8> for X {
    fn f<U: Into<u8>>(&self, x: LocalG<U>) -> u8 {
        x.0.into()
    }
}

impl name_elsewhere::NameOnlyElsewhere for X {
    fn f<U>(&self, x: LocalG<U>) -> U {
        x.0
    }
    fn g(&self) -> announce_test_defs::contexts::inner::U {
        announce_test_defs::contexts::inner::U
    }
}

impl kind_clash::KindClash for X {
    fn f<N>(&self, x: LocalG<N>) -> N {
        x.0
    }
    fn g<const N: usize>(&self, x: Arr<N>) -> usize {
        x.0.len()
    }
}

impl const_trait::ConstTrait<true> for X {
    fn flag(&self) -> Flag<true> {
        Flag
    }
    fn pick<const C: char>(&self) -> W<0, true, C, 1> {
        W
    }
}

#[test]
fn a_same_text_in_two_contexts() {
    use same_text::SameTextTwice as _;
    assert_eq!(X.f(3u8), GC(3u8));
    assert_eq!(X.g("s"), G("s"));
}

#[test]
fn a_generic_named_like_a_type() {
    use generic_like_type::GenericNamedLikeAType as _;
    assert_eq!(X.f(1u32), 1);
    assert_eq!(X.g(), Local(4));
}

#[test]
fn bound_that_names_another_parameter() {
    let b = bound_other::BoundNamesOther::<u8, u32>(LocalG(1u8), 2u32);
    assert_eq!((b.0 .0, b.1), (1, 2));
}

#[test]
fn bounds_that_name_the_traits_parameters() {
    use method_bound::MethodBoundNamesTrait as _;
    assert_eq!(X.f(LocalG(5u8)), 5);
    let inner = LocalG(7u8);
    let o = outlives::OutlivesBound::<u8>(&inner);
    assert_eq!(o.0 .0, 7);
}

#[test]
fn where_clause_of_the_item() {
    let w = where_clause::WhereClause::<u8>(LocalC(5));
    assert_eq!(w.0 .0, 5);
}

#[test]
fn a_name_that_is_a_generic_only_elsewhere() {
    use name_elsewhere::NameOnlyElsewhere as _;
    assert_eq!(X.f(LocalG(7u8)), 7);
    assert_eq!(X.g(), announce_test_defs::contexts::inner::U);
}

#[test]
fn a_parameter_kind_clash_between_methods() {
    use kind_clash::KindClash as _;
    assert_eq!(X.f(LocalG(1u8)), 1);
    assert_eq!(X.g(Arr([0; 3])), 3);
}

#[test]
fn const_generics_of_every_primitive_type() {
    let c = const_params::ConstParams::<7, true, 'x', -5> {
        w: W,
        local: Local(1),
    };
    assert_eq!(c.local, Local(1));
    let _: W<7, true, 'x', -5> = c.w;
    // A trait with a `bool` generic, and a method with a `char` one.
    use const_trait::ConstTrait as _;
    let _: Flag<true> = X.flag();
    let _: W<0, true, 'q', 1> = X.pick::<'q'>();
}

#[test]
fn a_lifetime_that_the_item_binds() {
    fn show(w: Wrap<'_>) -> u8 {
        *w.0
    }
    let h = higher_ranked::HigherRanked { f: show };
    assert_eq!((h.f)(Wrap(&9)), 9);
}
