#[announce::announce]
fn f() {}

#[announce::announce(macro_nam = X)]
struct S;

#[announce::announce]
mod m;

fn main() {}
