//! The wire format example in the docs of `announce::helper::Input` (section "A real
//! example"): the chain below gives exactly the documented block, and the block is
//! read from the source of the docs, so the two cannot drift.
//!
//! The definitions are in `announce_tests::defs::wire`. When the output changes on
//! purpose, update the ```text block in `announce-core/src/helper/mod.rs`.

use announce_tests::defs::wire as defs;

/// The callback: it prints its whole input.
macro_rules! show {
    ($($t:tt)*) => { stringify!($($t)*) };
}

/// The chain in the docs, as written there.
const WIRE: &str = defs::Point! { (expr) [defs::Point!, defs::Length!, show!] some rest };

/// The documented block: the ```text block after `<!-- wire-example -->` in the docs
/// of `Input`, without the `///` of the doc comment.
fn documented() -> String {
    const SRC: &str = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../announce-core/src/helper/mod.rs"
    ));
    let mut lines = SRC.lines().map(str::trim_start);
    lines
        .by_ref()
        .find(|l| *l == "/// <!-- wire-example -->")
        .expect("no `<!-- wire-example -->` marker in the docs of `Input`");
    assert_eq!(
        lines.next(),
        Some("/// ```text"),
        "the marker is followed by a text block"
    );
    let block: Vec<&str> = lines
        .take_while(|l| *l != "/// ```")
        .map(|l| {
            l.strip_prefix("/// ")
                .or_else(|| l.strip_prefix("///"))
                .unwrap_or(l)
        })
        .collect();
    block.join("\n")
}

fn norm(s: &str) -> String {
    s.split_whitespace().collect()
}

/// Replaces the digits between `prefix` and `suffix` (the first occurrence, in a
/// text without whitespace) by `with`, and returns the new text and the digits.
fn mask(n: &str, prefix: &str, suffix: &str, with: &str) -> (String, String) {
    let start = n
        .find(prefix)
        .unwrap_or_else(|| panic!("no `{}` in the wire format: {}", prefix, n))
        + prefix.len();
    let len = n[start..]
        .find(suffix)
        .unwrap_or_else(|| panic!("no `{}` after `{}`: {}", suffix, prefix, n));
    let digits = &n[start..start + len];
    assert!(
        !digits.is_empty() && digits.chars().all(|c| c.is_ascii_digit()),
        "`{}` is not a number after `{}`: {}",
        digits,
        prefix,
        n
    );
    (
        format!("{}{}{}", &n[..start], with, &n[start + len..]),
        digits.to_string(),
    )
}

/// The format tag of the type table: `"type-leak-format-<N>"`. `N` is the format
/// version of the `type-leak` that is linked, which a patch release of `type-leak`
/// may change (the docs show the one of the checked-in `Cargo.lock`).
const TAG: &str = "\"type-leak-format-";

/// `id = <digits>u64` (random for each build) becomes `id = ID`, as in the docs;
/// the version of the format tag becomes `N` (and is the current one).
fn masked_chain(s: &str) -> String {
    let (n, _) = mask(&norm(s), "id=", "u64", "ID");
    let n = n.replacen("id=IDu64", "id=ID", 1);
    let (n, version) = mask(&n, TAG, "\"", "N");
    assert_eq!(
        version,
        announce::type_leak::Referrer::FORMAT_VERSION.to_string(),
        "the type table of the chain has the format tag of the linked `type-leak`"
    );
    n
}

#[test]
fn the_documented_wire_format_is_what_a_callback_gets() {
    let documented = norm(&documented());
    assert!(documented.contains("id=ID,"), "{}", documented);
    // The documented format version is some version of `type-leak`; see `TAG`.
    let (documented, _) = mask(&documented, TAG, "\"", "N");
    assert_eq!(
        masked_chain(WIRE),
        documented,
        "the docs of `announce::helper::Input` show another wire format than the chain gives"
    );
}
