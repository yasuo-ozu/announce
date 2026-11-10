// A relative trait path in a bound is a cause of its own, at the path in the definition, in a
// bound of a parameter, in a `where` clause and in the bound of a method. The check fails when
// the call is outside the module, and no type of the item is involved.
mod m {
    pub trait UiBase {}

    #[announce::announce(strict = false)]
    pub struct InBound<T: UiBase> {
        pub t: T,
    }

    #[announce::announce(strict = false)]
    pub struct InWhere<T>
    where
        T: UiBase,
    {
        pub t: T,
    }

    #[announce::announce(strict = false)]
    pub trait InMethod {
        fn f<U: UiBase>(&self, u: U);
    }
}

m::InBound! { [m::InBound!, announce::same_context!] }
m::InWhere! { [m::InWhere!, announce::same_context!] }
m::InMethod! { [m::InMethod!, announce::same_context!] }

fn main() {}
