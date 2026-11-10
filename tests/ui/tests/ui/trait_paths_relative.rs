pub trait Rel {}
pub struct Local;

// A relative trait path in a `where` clause: a repeater only carries types.
#[announce::announce]
pub struct WhereRel<T>
where
    T: Rel,
{
    pub t: T,
}

// ... in `impl Trait` argument position ...
#[announce::announce]
pub trait ImplArg {
    fn f(&self, x: impl Rel);
}

// ... in `impl Trait` return position ...
#[announce::announce]
pub trait ImplRet {
    fn f(&self) -> impl Rel;
}

// ... and in `dyn`, a relative type (a `repeater` carries it: tests/defs/src/carried.rs).
#[announce::announce]
pub trait DynRel {
    fn g(&self) -> ::std::boxed::Box<dyn Rel>;
}

// ... and a repeater does not help, whatever else it carries.
::announce::define_repeater!(pub Repeater);
#[announce::announce(repeater = crate::Repeater)]
pub struct WithRepeater<T: Rel>(pub T, pub Local);

fn main() {}
