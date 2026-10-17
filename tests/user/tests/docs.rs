//! Doc attributes in the payload: removed by default, kept with `docs = true`,
//! in the item that is emitted always. The definitions are in
//! `announce_test_defs::docs` (`#![deny(missing_docs)]`); the feature `x` is
//! off there and on here.

use announce_test_defs::docs as d;

macro_rules! capture {
    ([$($l:tt)*] $($t:tt)*) => {
        pub const OUT: &str = stringify!($($t)*);
    };
}

fn norm(s: &str) -> String {
    s.split_whitespace().collect()
}

mod plain {
    use super::d;
    d::Plain! { [d::Plain!, capture!] }
}
mod choice {
    use super::d;
    d::Choice! { [d::Choice!, capture!] }
}
mod tr {
    use super::d;
    d::Tr! { [d::Tr!, capture!] }
}
mod inner {
    use super::d;
    d::inner! { [d::inner!, capture!] }
}
mod kept {
    use super::d;
    d::Kept! { [d::Kept!, capture!] }
}
mod kept_mod {
    use super::d;
    d::kept_mod! { [d::kept_mod!, capture!] }
}
mod derived {
    use super::d;
    d::Derived! { [d::Derived!, capture!] }
}
mod derived_kept {
    use super::d;
    d::DerivedKept! { [d::DerivedKept!, capture!] }
}
mod owned {
    use super::d;
    d::Owned! { [d::Owned!, capture!] }
}

/// No string doc anywhere in `out`; the list forms of `doc(..)` are not docs.
fn has_no_docs(out: &str) -> bool {
    !out.contains("#[doc=") && !out.contains("#![doc=")
}

#[test]
fn struct_payload_has_no_docs() {
    let out = norm(plain::OUT);
    assert!(has_no_docs(&out), "{}", out);
    for text in ["Docof", "cfg_attrdoc", "doc of"] {
        assert!(!out.contains(text), "{}", out);
    }
    // The shape is intact; the `cfg_attr` of the definer is evaluated (feature
    // `x` is off there), so its `doc = ..` is a doc and goes, its `doc(hidden)` stays.
    assert!(
        out.contains(
            "pubstructPlain<T>{puba:T,#[doc(hidden)]pubb:u8,pubc:u8,#[doc(hidden)]pubd:u8,}"
        ),
        "{}",
        out
    );
    // `doc(alias = ..)` on the item stays. `#[derive]` is not there: it is below
    // `#[announce]` in the source, and the runner that makes the announcement is below
    // it, so it has expanded (and rustc has taken it off the item) when the announcement
    // is made.
    assert!(
        out.contains("{#[doc(alias=\"plain_alias\")]pubstructPlain"),
        "{}",
        out
    );
    assert!(!out.contains("#[derive"), "{}", out);
    assert!(!out.contains("#[cfg"), "{}", out);
}

#[test]
fn enum_payload_has_no_docs() {
    let out = norm(choice::OUT);
    assert!(has_no_docs(&out), "{}", out);
    assert!(out.ends_with("{pubenumChoice{A,B(u8),}}"), "{}", out);
}

#[test]
fn trait_payload_has_no_docs() {
    let out = norm(tr::OUT);
    assert!(has_no_docs(&out), "{}", out);
    assert!(
        out.ends_with(
            "{pubtraitTr{constK:u8;typeA;#[doc(alias=\"method_alias\")]fnf(&self)->u8{1}}}"
        ),
        "{}",
        out
    );
}

#[test]
fn inline_mod_payload_has_no_docs_also_inner_ones() {
    let out = norm(inner::OUT);
    assert!(has_no_docs(&out), "{}", out);
    assert!(!out.contains("Inner"), "{}", out);
    assert!(!out.contains("Blockdoc"), "{}", out);
    // The inner `#![doc(alias = ..)]` is not a string doc: it stays.
    assert!(
        out.ends_with(
            "{pubmodinner{#![doc(alias=\"inner_alias\")]pubstructNested{pubn:u8,}pubfnthree()->u8{3}}}"
        ),
        "{}",
        out
    );
}

#[test]
fn owned_attributes_and_docs() {
    // The payload keeps `#[rules(skip)]` and loses the docs.
    let out = norm(owned::OUT);
    assert!(has_no_docs(&out), "{}", out);
    assert!(
        out.ends_with("{pubstructOwned{#[rules(skip)]puba:u8,pubb:u8,}}"),
        "{}",
        out
    );
}

#[test]
fn docs_true_keeps_the_docs() {
    let out = norm(kept::OUT);
    assert!(
        out.ends_with(
            "{#[doc=\"DocofKept,whosedocsstayinthepayload.\"]pubstructKept{#[doc=\"Docofk.\"]pubk:u8,}}"
        ),
        "{}",
        out
    );
    // Every kind of position: item, inner attribute, nested item.
    let out = norm(kept_mod::OUT);
    for text in [
        "#[doc=\"DocofKeptMod.\"]",
        "#![doc=\"Innerdocofkept_mod.\"]",
        "#[doc=\"DocofInside.\"]",
    ] {
        assert!(out.contains(text), "{} in {}", text, out);
    }
}

#[test]
fn derive_announce_strips_docs_unless_asked() {
    let out = norm(derived::OUT);
    assert!(has_no_docs(&out), "{}", out);
    assert!(out.ends_with("{pubstructDerived{puba:u8,}}"), "{}", out);
    // The helper attribute `#[announce(docs = true)]` is not in the payload either.
    let out = norm(derived_kept::OUT);
    assert!(
        out.ends_with(
            "{#[doc=\"DocofDerivedKept.\"]pubstructDerivedKept{#[doc=\"Docofa.\"]puba:u8,}}"
        ),
        "{}",
        out
    );
    assert!(!out.contains("#[announce"), "{}", out);
}

#[test]
fn checks_get_the_payload() {
    let plain = norm(d::checked::plain::CHECKED);
    assert_eq!(plain, "pubstructChecked{puba:u8,}");
    // The doc comments reach the check as the compiler wrote them, `r"..."`.
    let kept = norm(d::checked::kept::CHECKED).replace("r\"", "\"");
    assert_eq!(
        kept,
        "#[doc=\"DocofChecked.\"]pubstructChecked{#[doc=\"Docofa.\"]puba:u8,}"
    );
}

#[test]
fn the_emitted_items_keep_their_docs() {
    // `#![deny(missing_docs)]` in the definitions already makes the crate fail to
    // build when a doc is lost. Here the items are used, so they are emitted.
    let p = d::Plain {
        a: 1u8,
        b: 2,
        c: 3,
        d: 4,
    };
    assert_eq!((p.a, p.b, p.c, p.d), (1, 2, 3, 4));
    assert!(matches!(d::Choice::B(1), d::Choice::B(1)));
    assert_eq!(d::inner::three(), 3);
    assert_eq!(d::inner::Nested { n: 1 }.n, 1);
    assert_eq!(d::Owned { a: 1, b: 2 }.a, 1);
    assert_eq!(d::Kept { k: 1 }.k, 1);
    let _ = (
        d::kept_mod::Inside,
        d::Derived { a: 1 },
        d::DerivedKept { a: 1 },
    );
}
