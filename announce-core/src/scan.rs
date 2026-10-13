//! A scan of the first tokens of an item, which tells its kind and name without
//! parsing it (DESIGN.md section 4.2).
//!
//! `#[announce]` has to refuse an item it does not support (`fn`, `impl`, `mod
//! m;`, ..) at once, with the error that says what is supported. Everything
//! else about the item, its syntax included, is validated once, by `syn`, in the
//! `Capture` derive. So the attribute reads only the top-level tokens up to the
//! name: the outer attributes, the visibility, `unsafe` / `auto`, the keyword and
//! the identifier. It never looks inside a group, apart from the first token of
//! `pub( .. )`, and it never reads the generics, the fields or the body.

use proc_macro2::{Delimiter, Ident, TokenStream, TokenTree};

/// The kinds of item that `#[announce]` supports.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Kind {
    Struct,
    Enum,
    Union,
    Trait,
    Type,
    /// An inline module (`mod m { .. }`).
    Mod,
}

/// What the scan of an item found.
#[derive(Clone, Debug)]
pub(crate) struct Head {
    pub kind: Kind,
    pub ident: Ident,
}

const UNSUPPORTED: &str =
    "#[announce] supports struct, enum, union, trait, type alias and inline mod items";

const MOD_FILE: &str = "#[announce] supports only inline modules (`mod m { .. }`), not `mod m;`";

/// The top-level tokens of a stream, with the contents of the groups that have
/// no delimiter (`$item`, `$vis` fragments of a `macro_rules!` that calls
/// `#[announce]`) read in place, as `syn` does.
struct Flat {
    stack: Vec<proc_macro2::token_stream::IntoIter>,
}

impl Flat {
    fn new(ts: TokenStream) -> Self {
        Flat {
            stack: vec![ts.into_iter()],
        }
    }
}

impl Iterator for Flat {
    type Item = TokenTree;

    fn next(&mut self) -> Option<TokenTree> {
        loop {
            match self.stack.last_mut()?.next() {
                None => {
                    self.stack.pop();
                }
                Some(TokenTree::Group(g)) if g.delimiter() == Delimiter::None => {
                    self.stack.push(g.stream().into_iter())
                }
                Some(t) => return Some(t),
            }
        }
    }
}

fn is_ident(t: Option<&TokenTree>, word: &str) -> bool {
    matches!(t, Some(TokenTree::Ident(i)) if i == word)
}

fn is_punct(t: Option<&TokenTree>, c: char) -> bool {
    matches!(t, Some(TokenTree::Punct(p)) if p.as_char() == c)
}

/// Whether `inside` (the contents of the group after `pub`) is a restriction:
/// `crate`, `self`, `super` or `in path`.
fn is_restriction(inside: TokenStream) -> bool {
    match inside.into_iter().next() {
        Some(TokenTree::Ident(i)) => i == "crate" || i == "self" || i == "super" || i == "in",
        _ => false,
    }
}

/// Reads the kind and the name of the item `tokens`, or says why the item is not
/// supported. The errors are those of `generate::ident_vis`, at the whole item.
///
/// This is not a validation of the syntax: an item that this accepts can still
/// be rejected by the parse of the `Capture` derive.
pub(crate) fn head(tokens: &TokenStream) -> syn::Result<Head> {
    let unsupported = || syn::Error::new_spanned(tokens, UNSUPPORTED);
    let mut it = Flat::new(tokens.clone()).peekable();

    // Outer attributes: `#` and a bracketed group.
    while matches!(it.peek(), Some(TokenTree::Punct(p)) if p.as_char() == '#') {
        it.next();
        match it.next() {
            Some(TokenTree::Group(g)) if g.delimiter() == Delimiter::Bracket => {}
            _ => return Err(unsupported()),
        }
    }
    // Visibility: `pub`, `pub(crate)`, `pub(self)`, `pub(super)`, `pub(in path)`.
    if is_ident(it.peek(), "pub") {
        it.next();
        let restricted = matches!(
            it.peek(),
            Some(TokenTree::Group(g))
                if g.delimiter() == Delimiter::Parenthesis && is_restriction(g.stream())
        );
        if restricted {
            it.next();
        }
    }
    // `unsafe` (a trait, and a module in recent syntax), `auto` (a trait).
    let mut word = it.next();
    let mut qualified = false;
    if is_ident(word.as_ref(), "unsafe") {
        qualified = true;
        word = it.next();
    }
    if is_ident(word.as_ref(), "auto") {
        qualified = true;
        word = it.next();
        if !is_ident(word.as_ref(), "trait") {
            return Err(unsupported());
        }
    }
    let kind = match &word {
        Some(TokenTree::Ident(i)) => match i.to_string().as_str() {
            "struct" if !qualified => Kind::Struct,
            "enum" if !qualified => Kind::Enum,
            "trait" => Kind::Trait,
            "type" if !qualified => Kind::Type,
            "mod" => Kind::Mod,
            // `union` is a keyword only before a name: `union! { .. }` is a macro call.
            "union" if !qualified && matches!(it.peek(), Some(TokenTree::Ident(_))) => Kind::Union,
            _ => return Err(unsupported()),
        },
        _ => return Err(unsupported()),
    };
    let ident = match it.next() {
        Some(TokenTree::Ident(i)) => i,
        // `struct` without a name is a syntax error, which the parse of the derive reports.
        _ => return Err(unsupported()),
    };
    if kind == Kind::Mod
        && !matches!(it.peek(), Some(TokenTree::Group(g)) if g.delimiter() == Delimiter::Brace)
    {
        // `mod m;` is not supported. Anything else after the name is a syntax error
        // that the parse of the derive reports.
        if is_punct(it.peek(), ';') || it.peek().is_none() {
            return Err(syn::Error::new_spanned(tokens, MOD_FILE));
        }
    }
    Ok(Head { kind, ident })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ts(s: &str) -> TokenStream {
        s.parse().unwrap()
    }

    fn ok(s: &str) -> (Kind, String) {
        let h = head(&ts(s)).unwrap_or_else(|e| panic!("{}: {}", s, e));
        (h.kind, h.ident.to_string())
    }

    fn err(s: &str) -> String {
        head(&ts(s))
            .err()
            .unwrap_or_else(|| panic!("{}", s))
            .to_string()
    }

    #[test]
    fn kinds_and_names() {
        assert_eq!(ok("struct S;"), (Kind::Struct, "S".into()));
        assert_eq!(ok("pub struct S { a: u8 }"), (Kind::Struct, "S".into()));
        assert_eq!(ok("pub(crate) struct S<T>(T);"), (Kind::Struct, "S".into()));
        assert_eq!(ok("pub(in a::b) enum E {}"), (Kind::Enum, "E".into()));
        assert_eq!(
            ok("pub(super) union U { a: u8 }"),
            (Kind::Union, "U".into())
        );
        assert_eq!(ok("pub(self) trait T {}"), (Kind::Trait, "T".into()));
        assert_eq!(ok("unsafe trait T {}"), (Kind::Trait, "T".into()));
        assert_eq!(ok("pub unsafe auto trait T {}"), (Kind::Trait, "T".into()));
        assert_eq!(ok("auto trait T {}"), (Kind::Trait, "T".into()));
        assert_eq!(ok("type A = u8;"), (Kind::Type, "A".into()));
        assert_eq!(ok("pub type A<T> = Vec<T>;"), (Kind::Type, "A".into()));
        assert_eq!(ok("pub mod m { }"), (Kind::Mod, "m".into()));
        assert_eq!(ok("mod m { fn f() {} }"), (Kind::Mod, "m".into()));
    }

    #[test]
    fn attributes_come_first() {
        assert_eq!(
            ok("#[derive(Clone)] #[doc = \"x\"] /// y\n pub struct S;"),
            (Kind::Struct, "S".into())
        );
        assert_eq!(ok("#[a] #[b(c)] trait T {}"), (Kind::Trait, "T".into()));
    }

    #[test]
    fn union_is_contextual() {
        assert_eq!(ok("union U { a: u8 }"), (Kind::Union, "U".into()));
        assert!(err("union! { }").contains("supports struct"));
        assert!(err("union::f();").contains("supports struct"));
    }

    #[test]
    fn raw_identifiers_and_keywords_as_names() {
        assert_eq!(ok("struct r#match;"), (Kind::Struct, "r#match".into()));
    }

    #[test]
    fn other_items_are_unsupported() {
        for src in [
            "fn f() {}",
            "pub fn f() {}",
            "const C: u8 = 0;",
            "static S: u8 = 0;",
            "impl S {}",
            "use a::b;",
            "extern crate a;",
            "macro_rules! m { () => {} }",
            "m!();",
            "unsafe impl T for S {}",
            "unsafe fn f() {}",
            "unsafe struct S;",
            "auto struct S;",
            "async fn f() {}",
            "union",
            "",
            "struct",
            "#[a] ",
        ] {
            let e = err(src);
            assert!(
                e.contains("supports struct, enum, union, trait, type alias and inline mod"),
                "{}: {}",
                src,
                e
            );
        }
    }

    #[test]
    fn a_module_must_be_inline() {
        for src in ["mod m;", "pub mod m;", "#[a] mod m;"] {
            assert!(err(src).contains("only inline modules"), "{}", src);
        }
        assert!(head(&ts("mod m { }")).is_ok());
    }

    #[test]
    fn groups_without_delimiters_are_read_in_place() {
        use proc_macro2::Group;
        // `$vis struct S;` where `$vis` is `pub`, as a `macro_rules!` fragment.
        let vis = TokenTree::Group(Group::new(Delimiter::None, ts("pub")));
        let mut tokens = TokenStream::new();
        tokens.extend([vis]);
        tokens.extend(ts("struct S;"));
        let h = head(&tokens).unwrap();
        assert_eq!((h.kind, h.ident.to_string()), (Kind::Struct, "S".into()));
        // The whole item in one fragment.
        let all = TokenTree::Group(Group::new(Delimiter::None, ts("pub trait T {}")));
        let mut tokens = TokenStream::new();
        tokens.extend([all]);
        assert_eq!(head(&tokens).unwrap().kind, Kind::Trait);
    }

    #[test]
    fn the_error_spans_the_whole_item() {
        // The same span as `Error::new_spanned(item, ..)` of the parsed item had.
        let e = head(&ts("fn f() {}")).err().unwrap();
        let again = syn::Error::new_spanned(ts("fn f() {}"), UNSUPPORTED);
        assert_eq!(e.to_string(), again.to_string());
    }
}
