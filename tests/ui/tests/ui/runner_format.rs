// The arguments of the runner (the hidden attribute that `#[announce]` puts last) have the
// version of their encoding. An item that was announced by another version of `announce`
// is one error, which lists the versions, and the item is still there.

// The runner itself: another version, no version, two versions.
#[announce::__private::_actually_run_announce(_announce_format_version = "0")]
pub struct Other(pub u8);

#[announce::__private::_actually_run_announce(meta = my_lib::info)]
pub struct Unknown(pub u8);

#[announce::__private::_actually_run_announce(_announce_format_version = "1", ?? !!, _announce_format_version = "2")]
pub struct Two(pub u8);

// An announcement that finds runners of two versions, at two crate paths.
#[announce::announce(meta = my_lib::info)]
#[announce::__private::_actually_run_announce(_announce_format_version = "7", ?? !!)]
#[::announce::__private::_actually_run_announce(_announce_format_version = "1")]
pub struct Merged(pub u8);

fn main() {
    let _ = (Other(0), Unknown(0), Two(0), Merged(0));
}
