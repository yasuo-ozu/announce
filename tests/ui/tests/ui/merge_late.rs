// A macro that announces what it produces must put `#[announce]` first among the
// attributes of its output. This one (`late_announce`) puts it below the others, after the
// attribute that makes the trampoline has expanded, so it would make a second one. It is
// one error, at the attribute that was put there, and the item is still there.
#[announce::announce(meta = my_lib::first)]
#[announce_test_derive::passthrough]
#[announce_test_derive::late_announce(meta = my_lib::late)]
pub struct Late(pub u8);

fn main() {
    let _ = Late(0);
}
