//! The example of the docs of `Input` ("A real example") parses as the docs say.
//! The chain that produces it is run by `tests/wire.rs`, which compares
//! it with the same block.

use announce_core::helper::Input;
use template_quote::ToTokens;

/// The ```text block after `<!-- wire-example -->` in the docs of `Input`.
fn documented() -> String {
    const SRC: &str = include_str!("../src/helper/mod.rs");
    let mut lines = SRC.lines().map(str::trim_start);
    lines
        .by_ref()
        .find(|l| *l == "/// <!-- wire-example -->")
        .expect("no `<!-- wire-example -->` marker in the docs of `Input`");
    assert_eq!(lines.next(), Some("/// ```text"));
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
