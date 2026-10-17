// An item announced by two versions of the `announce` crate is one error. The sentinel that
// the announcement emitted next to the item is satisfied, so that no second error follows.
// (`other_version` puts the runner of a version `7` below the attributes, as another
// `announce` in the build would.)
#[announce::announce]
#[announce_test_derive::other_version]
pub struct Plain(pub u8);

#[announce::announce]
#[announce_test_derive::other_version]
pub trait Tr {}

fn main() {}
