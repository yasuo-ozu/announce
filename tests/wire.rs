//! The wire format example in the docs of `announce::helper::Input` (section "A real
//! example"), checked both ways. The chain below gives exactly the documented block, and
//! the block parses as `Input` and `Entry` read it, as the annotations of the docs say.
//! The block is read from the source of the docs, so the docs, the chain and the parser
//! cannot drift.
//!
//! The definitions are in `announce_tests::defs::wire`. When the output changes on
//! purpose, update the ```text block in `announce-core/src/helper/mod.rs`.

use announce::helper::Input;
use announce_tests::defs::wire as defs;
use template_quote::ToTokens;

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

fn norm(s: impl ToString) -> String {
    s.to_string().split_whitespace().collect()
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
    let documented = norm(documented());
    assert!(documented.contains("id=ID,"), "{}", documented);
    // The documented format version is some version of `type-leak`; see `TAG`.
    let (documented, _) = mask(&documented, TAG, "\"", "N");
    assert_eq!(
        masked_chain(WIRE),
        documented,
        "the docs of `announce::helper::Input` show another wire format than the chain gives"
    );
}

#[test]
fn the_documented_example_parses_as_annotated() {
    // `$crate` is one identifier in the tokens of a macro, but not in text.
    let text = documented().replace("$crate", "crate");
    let input: Input = syn::parse_str(&text).expect("the documented example parses");

    assert_eq!(
        norm(input.options.as_ref().unwrap().to_token_stream()),
        "expr"
    );
    assert!(input.has_option("expr"));
    assert_eq!(input.list.0.len(), 1);
    assert_eq!(norm(input.list.0[0].to_token_stream()), "show!");
    assert_eq!(norm(&input.rest), "somerest");

    // Newest first.
    assert_eq!(input.entries.len(), 2);
    let (length, point) = (&input.entries[0], &input.entries[1]);
    assert_eq!(norm(length.key.to_token_stream()), "defs::Length!");
    assert_eq!(norm(point.key.to_token_stream()), "defs::Point!");
    assert_eq!(
        input
            .entries_in_order()
            .map(|e| norm(e.key.to_token_stream()))
            .collect::<Vec<_>>(),
        ["defs::Point!", "defs::Length!"]
    );

    // `Length`: the type table of a repeater.
    assert_eq!(length.meta.len(), 1);
    assert!(length.find_meta("type_leak").is_some());
    assert_eq!(norm(&length.item), "pubstructLength(pubMeters);");

    // `Point`: a check, a `meta = ..` entry and the owned attribute names.
    assert!(point.has_check("lib_check"));
    assert_eq!(
        norm(point.find_meta("info").unwrap().to_token_stream()),
        "crate::info(1)"
    );
    assert_eq!(point.owned_attrs().names().len(), 1);
    assert_eq!(point.owned_attrs().names()[0], "my_lib");
    // No docs; the owned attribute is in the payload, and not in the emitted item.
    let item = point.item().unwrap();
    assert!(!norm(item.to_token_stream()).contains("doc"));
    assert_eq!(point.owned_attrs().find(&item).len(), 1);
    assert_eq!(
        point
            .item_without_owned_attrs()
            .unwrap()
            .to_token_stream()
            .to_string(),
        "pub struct Point { pub x : u8 , pub y : u8 , }"
    );
}
