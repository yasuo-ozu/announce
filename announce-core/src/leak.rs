//! Type context of a definition: `#[announce(repeater = path)]` and the
//! type-leak table.
//!
//! On the cfg-evaluated item, type-leak finds the types that would not
//! resolve at a caller: relative paths such as `Local` or `super::X`. The
//! analysis is type-leak's own (`Leaker` and `LeakerResult`). Its rules allow
//! `crate::` paths, every absolute path (`::std::vec::Vec<u8>`, `::other::X`:
//! the same crate on both sides), primitives with `allow_primitive`, and with
//! `allow_std_prelude = true` the prelude names (type-leak's `Rules::prelude()`,
//! and the names in [`NEWER_PRELUDE`]). The one exception is `Sized`: `T: ?Sized`,
//! `where Self: Sized` and `trait T: Sized` are accepted bare, because the
//! language gives it a meaning of its own. For a trait, `Leaker::check_values` is
//! on, so the defaults of associated consts are read too; [`without_bodies`] takes
//! the bodies of provided methods off the trait it is given, as announce does not
//! analyse them. Inline modules are not analysed.
//!
//! The result of the analysis is a [`LeakerResult`]: the `Referrer`, which lists
//! the *entries* (the types that depend on where they are written), and the errors
//! for the parts that no table can carry. type-leak recovers from such an error:
//! the part is copied as written and the analysis goes on. The errors are not
//! announce's business. The definition never reports, rewords or records them: they
//! travel in the META inside the `LeakerResult`, and the consumer decides, by
//! reporting them ([`TypeLeak::try_apply`], `Entry::item_resolved`) or by allowing
//! them (`TypeLeak::allow`, `TypeLeak::ignore`).
//!
//! announce's own failures are these, and each is an error at the definition (or,
//! with `strict = false`, a `type_leak_error(..)` record, see [`Table::Failed`]):
//! - an entry without a `repeater = ..` ([`WITHOUT_REPEATER`]), or in a type alias
//!   ([`ALIAS`]; aliases get no table),
//! - an entry that has no impl: its type needs `Self`, or a const generic that is not
//!   of a primitive type ([`NOT_PRIMITIVE`]).
//!
//! The repeater is the trait that `define_repeater!` (type-leak's
//! `repeater_definition()`) defines in the definer's crate. Each entry `I` of the
//! `Referrer` gets an impl of it, written by type-leak
//! (`LeakerResult::repeater_impls`):
//!
//! ```text
//! impl<P..> REPEATER<(ARGS..), KEY> for () { type Type = T_I; }
//! ```
//!
//! where `KEY` is `Projection::key(I)` (the `id` of the table in its high 64 bits, `I` in
//! the low ones), `P..` are the generics of the *context of that entry* (the scope where
//! the type is written: the item, a method, a GAT) and `ARGS` encodes the ones the type
//! uses; const generics of every primitive type go through the repeater's const slots (a
//! `u128` is the `KEY` itself). The payload stays the original item. The META of the entry
//! is
//!
//! ```text
//! type_leak(repeater = .., id = .., context = .., result = [..])
//! ```
//!
//! `result` is the `LeakerResult` as type-leak serializes it: the `Referrer`, then the
//! errors that remain. `repeater` and `id` are there for a definition with `repeater = ..`.
//! Without one, and with no entry but some errors, the META is
//! `type_leak(context = .., result = [..])`, so that the consumer sees the errors. A
//! callback rewrites the payload with it (`announce::helper::TypeLeak::try_apply`).
//! An item that the analysis found nothing in has no META, unless it has a `repeater`
//! (then the table is empty).

use crate::helper::{LeakError, LeakFailure, TypeLeak, ALIAS, NOT_PRIMITIVE, WITHOUT_REPEATER};
use crate::options::Options;
use proc_macro2::{Ident, Span, TokenStream};
use std::borrow::Cow;
use syn::{Item, ItemImpl, Path, Result, Type};
use template_quote::ToTokens;
use type_leak::{Cause, ContextId, ImplSelf, LeakerResult, Place, Projection, Referrer, Rules};

pub struct Leaked {
    /// Impls emitted at the definition site.
    pub impls: TokenStream,
    /// `type_leak(repeater = .., id = .., context = .., result = [..])`; without the first two
    /// where there is no `repeater`.
    pub meta: TokenStream,
}

/// What [`table`] makes of an item that is analysed.
pub enum Table {
    /// The impls and the `type_leak(..)` META of an item with a `repeater`, or the META alone
    /// of an item without one that has errors of type-leak and no entry.
    Leaked(Leaked),
    /// Something in the item has no table here: an entry without a `repeater`, or without an
    /// impl. The META is `type_leak_error(..)`.
    Failed(LeakFailure),
}

/// The analysis of an item: what type-leak found, and the scope of the item.
pub struct Analysis {
    /// What type-leak found: the entries and the errors.
    pub result: LeakerResult,
    /// The `Referrer` of [`result`](Self::result), which has no accessor for it.
    pub referrer: Referrer,
    /// The top-level context of the item in `referrer`.
    pub context: ContextId,
}

impl Analysis {
    /// The entries that need a repeater impl, with their context: the live entries of the
    /// `Referrer`. One type text can be several entries, one for each context.
    pub fn entries(
        &self,
    ) -> impl Iterator<Item = (type_leak::EntryId, &Type, &type_leak::Context)> {
        self.referrer.iter()
    }
}

/// The repeater must be a local trait (`crate::..` / `$crate::..`): the table
/// implements it for `()`, which the orphan rule only allows for a local
/// trait. Callers resolve it through `$crate`.
pub fn check_repeater(repeater: &Path) -> Result<()> {
    let first = repeater.segments.first().map(|s| s.ident.to_string());
    if repeater.leading_colon.is_none()
        && matches!(first.as_deref(), Some("crate") | Some("$crate"))
    {
        Ok(())
    } else {
        Err(syn::Error::new_spanned(
            repeater,
            "`repeater` must be a trait of this crate, written `crate::path::to::Repeater` \
             (callers resolve it through `$crate`, and the table implements it for `()`, \
             which the orphan rule only allows for a local trait)",
        ))
    }
}

/// Std prelude names (types and traits) accepted bare with
/// `allow_std_prelude = true`.
pub(crate) const PRELUDE: &[&str] = &[
    "Box",
    "String",
    "Vec",
    "Option",
    "Result",
    "Copy",
    "Send",
    "Sized",
    "Sync",
    "Unpin",
    "Drop",
    "Fn",
    "FnMut",
    "FnOnce",
    "Clone",
    "ToOwned",
    "ToString",
    "PartialEq",
    "PartialOrd",
    "Eq",
    "Ord",
    "AsRef",
    "AsMut",
    "Into",
    "From",
    "Default",
    "Iterator",
    "Extend",
    "IntoIterator",
    "DoubleEndedIterator",
    "ExactSizeIterator",
    "TryFrom",
    "TryInto",
    "FromIterator",
];

/// The names of [`PRELUDE`] that type-leak's `Rules::prelude()` does not allow, as the copy may be
/// of an edition or a Rust whose prelude lacks them (2021, 1.56): announce accepts them with
/// `allow_std_prelude = true`.
const NEWER_PRELUDE: &[&str] = &["TryFrom", "TryInto", "FromIterator"];

/// Token streams print a space between every token (`inner :: G < Self :: A >`,
/// `T : :: core :: convert :: From < G < T > >`); text with such types in it
/// printed as one writes them.
pub(crate) fn tidy(text: &str) -> String {
    // `T : :: core` must not become `T :::core`.
    const LEADING: &str = "\u{1}";
    let mut text = text.replace(" : :: ", &format!(": {}", LEADING));
    for (from, to) in [
        (" :: ", "::"),
        (":: ", "::"),
        (" ::", "::"),
        (" <", "<"),
        ("< ", "<"),
        (" >", ">"),
        ("& ", "&"),
        (" ,", ","),
        (" : ", ": "),
        (" ! ", "!"),
        ("! (", "!("),
        ("! [", "!["),
        ("! {", "!{"),
        (",>", ">"),
    ] {
        text = text.replace(from, to);
    }
    text.replace(LEADING, "::")
}

/// Analyses `item` with type-leak. `None` for items that are not analysed
/// (inline modules: the documented wrapper pattern).
///
/// In a test build every answer is checked against the fast path
/// ([`crate::fastpath`]), which must never accept more than this does.
pub fn analyze(item: &Item, opts: &Options) -> Option<Analysis> {
    let result = analyze_full(item, opts);
    #[cfg(test)]
    crate::fastpath::check_analysis(item, opts, &result);
    result
}

pub(crate) fn analyze_full(item: &Item, opts: &Options) -> Option<Analysis> {
    if !matches!(
        item,
        Item::Struct(_) | Item::Enum(_) | Item::Union(_) | Item::Trait(_) | Item::Type(_)
    ) {
        return None;
    }
    Some(intern(item, opts))
}

/// The trait without the bodies of its provided methods. `Leaker::check_values` reads
/// the default values of the associated consts, which is what announce wants, and the bodies
/// with them, which it does not analyse (a relative path in a body is not an error).
fn without_bodies(item: &syn::ItemTrait) -> Cow<'_, syn::ItemTrait> {
    let has_body = |i: &syn::TraitItem| matches!(i, syn::TraitItem::Fn(f) if f.default.is_some());
    if !item.items.iter().any(has_body) {
        return Cow::Borrowed(item);
    }
    let mut bodiless = item.clone();
    for i in &mut bodiless.items {
        if let syn::TraitItem::Fn(f) = i {
            if f.default.take().is_some() {
                f.semi_token = Some(Default::default());
            }
        }
    }
    Cow::Owned(bodiless)
}

/// Runs type-leak over `item`, which `analyze_full` has found to be analysed. The errors that it
/// recovers from stay in the result: they are the consumer's to report.
fn intern(item: &Item, opts: &Options) -> Analysis {
    // `crate::` paths, primitives (optionally) and the prelude (optionally) are allowed, and
    // every absolute path: `::std` and `::alloc`, and the other crates, are the same crate on both
    // sides, so a type of them is no entry, and only the relative types inside it are carried.
    let mut rules = Rules::new().crate_paths().external_crates();
    if opts.allow_primitive {
        rules = rules.primitive();
    }
    let mut allowed: Vec<Path> = Vec::new();
    if opts.allow_std_prelude {
        // type-leak's prelude is that of every edition as of Rust 1.56, with `Some`, `None`, `Ok`,
        // `Err` and `drop`. The names of `PRELUDE` that it leaves out as newer are kept.
        rules = rules.prelude();
        allowed.extend(
            NEWER_PRELUDE
                .iter()
                .map(|n| Path::from(Ident::new(n, Span::call_site()))),
        );
    } else {
        // Bare `Sized` (`Self: Sized`, `trait T: Sized`, `T: ?Sized + Sized`) is
        // accepted without the option.
        allowed.push(Path::from(Ident::new("Sized", Span::call_site())));
    }
    let rules = rules
        .or_if(move |cause: &Cause, _: Place| allowed.iter().any(|path| cause.starts_with(path)));
    let mut leaker = type_leak::Leaker::with_rule(rules);
    leaker.check_values = matches!(item, Item::Trait(_));
    let context = match item {
        Item::Struct(s) => leaker.intern_struct(s),
        Item::Enum(e) => leaker.intern_enum(e),
        Item::Union(u) => leaker.intern_union(u),
        Item::Trait(t) => leaker.intern_trait(&without_bodies(t)),
        Item::Type(a) => {
            leaker
                .enter_generics(&a.generics, |view| syn::visit::visit_item_type(view, a))
                .0
        }
        _ => unreachable!("`analyze_full` passes the analysed items only"),
    };
    // The same type text is one entry per context (method generics with different bounds, a
    // generic named like a type): each has its own impl and its own arguments. The Referrer is of
    // what type-leak could intern, whatever it refused.
    let result = leaker.result();
    let referrer = result.clone().into_referrer();
    Analysis {
        result,
        referrer,
        context,
    }
}

/// Whether the errors are at the same tokens, also where they are written: `{:?}` of tokens has
/// the spans of a proc macro (and none in a test build, where the text alone tells them apart).
fn at_same_tokens(a: &LeakError, b: &LeakError) -> bool {
    format!("{:?}", a.tokens) == format!("{:?}", b.tokens)
}

/// Why an entry is an error where no repeater carries it.
fn unrepeated(item: &Item) -> &'static str {
    if matches!(item, Item::Type(_)) {
        ALIAS
    } else {
        WITHOUT_REPEATER
    }
}

/// An error of type-leak's encoders, as a recorded thing. `entry` is the type of the entry that
/// has no impl.
fn encode_error(e: type_leak::EncodeError, entry: &Type) -> LeakError {
    use type_leak::EncodeError;
    match &e {
        EncodeError::UnsupportedConstType(ty) => LeakError::new(ty, NOT_PRIMITIVE),
        EncodeError::UnexpectedArgument(arg) => {
            LeakError::new(arg, "with a generic argument that cannot be encoded")
        }
        EncodeError::MissingArgument(param) => {
            LeakError::new(param, "with a generic parameter that has no argument")
        }
        EncodeError::UnboundedTraitObject(ty) => {
            LeakError::new(ty, "with a trait object that has no lifetime bound")
        }
        EncodeError::NeedsSelf(_) => LeakError::new(entry, "with `Self` in a carried type"),
        _ => LeakError::new(entry, format!("with a type that cannot be encoded ({})", e)),
    }
}

/// The repeater `repeater` as the projection of the impls: for `()`, with the `id` of the
/// announcement, and `()` is what a consumer's projection names. An entry that names `Self` has
/// no impl for `()`, so it is a cause of [`encode_errors`].
fn projection(repeater: &Path, random: u64) -> Projection {
    Projection::new(
        repeater.clone(),
        random,
        ImplSelf::Marker(syn::parse_quote!(())),
    )
}

/// The causes that keep an entry from having an impl, each once: a const generic that several
/// entries use stops them all. Every entry is looked at, so the causes are all there is, not the
/// first.
fn encode_errors(analysis: &Analysis, projection: &Projection) -> Vec<LeakError> {
    let unit: Type = syn::parse_quote!(());
    let mut errors = Vec::<LeakError>::new();
    for (entry, ty, context) in analysis.entries() {
        if let Err(e) = projection.of(&unit, context, entry) {
            let e = encode_error(e, ty);
            if !errors
                .iter()
                .any(|x| x.reason == e.reason && at_same_tokens(x, &e))
            {
                errors.push(e);
            }
        }
    }
    errors
}

/// The record of `errors` for `item`, with the texts that a call says about each; `None`
/// without errors.
fn failed(item: &Item, random: u64, errors: Vec<LeakError>) -> Result<Option<Table>> {
    if errors.is_empty() {
        return Ok(None);
    }
    let (ident, _) = crate::generate::ident_vis(item)?;
    let id = syn::LitInt::new(&format!("{}u64", random), Span::call_site());
    let name = syn::ext::IdentExt::unraw(&ident).to_string();
    let errors = errors.into_iter().map(|e| e.described(&name)).collect();
    let mut failure = LeakFailure::new(ident, id, errors);
    // The crate that `#[announce]` compiles is the definer, which the texts name for `$crate`.
    failure.krate = std::env::var("CARGO_CRATE_NAME").ok();
    Ok(Some(Table::Failed(failure)))
}

/// Enforces `repeater` when it is needed, and builds the table, or records why there is none.
pub fn table(item: &Item, opts: &crate::options::Options, random: u64) -> Result<Option<Table>> {
    let repeater = opts.repeater.as_ref();
    // Type aliases are not a type-leak target: their right-hand side must be
    // written with paths that resolve everywhere.
    if let (Item::Type(_), Some(a)) = (item, repeater) {
        return Err(syn::Error::new_spanned(
            a,
            "`repeater` is not supported on type aliases; write the right-hand side with absolute paths",
        ));
    }
    let analysis = match (analyze(item, opts), repeater) {
        (Some(analysis), _) => analysis,
        (None, Some(a)) => {
            return Err(syn::Error::new_spanned(
                a,
                "`repeater` is supported on struct, enum, union and trait items",
            ))
        }
        (None, None) => return Ok(None),
    };
    let repeater = match repeater {
        Some(repeater) => repeater,
        None => {
            // Every entry is an error without a repeater, which a type alias has none of.
            let reason = unrepeated(item);
            let mut errors = Vec::<LeakError>::new();
            for (_, ty, _) in analysis.entries() {
                let e = LeakError::new(ty, reason);
                if !errors.iter().any(|x| at_same_tokens(x, &e)) {
                    errors.push(e);
                }
            }
            if !errors.is_empty() {
                return failed(item, random, errors);
            }
            // Nothing for a table to carry. The errors of type-leak are the consumer's.
            if analysis.result.errors().is_empty() {
                return Ok(None);
            }
            let table = TypeLeak::new(None, analysis.context, analysis.result);
            let meta = table.meta();
            check_round_trip(&meta, &table)?;
            return Ok(Some(Table::Leaked(Leaked {
                impls: TokenStream::new(),
                meta,
            })));
        }
    };
    let projection = projection(repeater, random);
    let errors = encode_errors(&analysis, &projection);
    if !errors.is_empty() {
        return failed(item, random, errors);
    }
    let written = match analysis.result.repeater_impls(&projection) {
        Ok(written) => written,
        Err(e) => {
            // Every cause is in `encode_errors` already; this is no more than a guard.
            let ty: Type = syn::parse_quote!(());
            return failed(item, random, vec![encode_error(e, &ty)]);
        }
    };
    let mut impls = TokenStream::new();
    for r in written {
        let mut imp: ItemImpl = r.into();
        imp.attrs.push(syn::parse_quote!(#[doc(hidden)]));
        imp.to_tokens(&mut impls);
    }
    let table = TypeLeak::new(Some(projection), analysis.context, analysis.result);
    // The consumer reads the META back and uses the same indices as the impls
    // above, so the table must survive its own serialization unchanged.
    let meta = table.meta();
    check_round_trip(&meta, &table)?;
    Ok(Some(Table::Leaked(Leaked { impls, meta })))
}

/// The table that a consumer parses from `meta` is the one the impls were
/// written from (same entries, same indices, same contexts, same errors).
fn check_round_trip(meta: &TokenStream, table: &TypeLeak) -> Result<()> {
    let read: TypeLeak = syn::parse2(meta.clone())?;
    let same = |a: &Projection, b: &Projection| {
        a.id == b.id && a.path.to_token_stream().to_string() == b.path.to_token_stream().to_string()
    };
    let projection = match (&read.projection, &table.projection) {
        (Some(a), Some(b)) => same(a, b),
        (None, None) => true,
        _ => false,
    };
    // The referrer and the errors, which the result does not compare as a whole.
    let (a, b) = (read.result.into_parts(), table.result.clone().into_parts());
    if projection && read.context == table.context && a == b {
        Ok(())
    } else {
        Err(syn::Error::new(
            Span::call_site(),
            "announce: internal error: the type-leak table does not read back as written",
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::helper::TypeLeak;
    use crate::options::Options;
    use template_quote::quote;

    fn item(ts: TokenStream) -> Item {
        syn::parse2(ts).unwrap()
    }

    fn opts() -> Options {
        Options::default()
    }

    fn with_repeater() -> Options {
        Options {
            repeater: Some(syn::parse_quote!(crate::R)),
            ..Options::default()
        }
    }

    fn prelude() -> Options {
        Options {
            allow_std_prelude: true,
            ..Options::default()
        }
    }

    fn squash(ts: impl ToTokens) -> String {
        ts.to_token_stream()
            .to_string()
            .split_whitespace()
            .collect()
    }

    /// The entries that type-leak finds in `it`, as text.
    fn analysed(it: TokenStream, o: &Options) -> Vec<String> {
        analyze(&item(it), o)
            .unwrap()
            .entries()
            .map(|(_, t, _)| squash(t))
            .collect()
    }

    /// The errors that type-leak finds in `it`: the text of each cause.
    fn errors_of(it: TokenStream, o: &Options) -> Vec<String> {
        analyze(&item(it), o)
            .unwrap()
            .result
            .errors()
            .iter()
            .map(|e| squash(&e.cause))
            .collect()
    }

    /// Whether type-leak finds nothing in `it`: no entry and no error.
    fn clean(it: TokenStream, o: &Options) -> bool {
        let a = analyze(&item(it), o).unwrap();
        let clean = a.entries().next().is_none() && a.result.errors().is_empty();
        clean
    }

    /// How many entries and errors type-leak finds in `it`.
    fn found(it: TokenStream, o: &Options) -> usize {
        let a = analyze(&item(it), o).unwrap();
        a.entries().count() + a.result.errors().len()
    }

    /// A reason in a word, to write the causes of an item in a line.
    fn tag(reason: &str) -> &str {
        match reason {
            WITHOUT_REPEATER => "relative",
            ALIAS => "alias",
            NOT_PRIMITIVE => "const",
            "with `Self` in a carried type" => "self",
            other => other.trim_start_matches("with "),
        }
    }

    /// `ty` is the one relative type of `it`: an error without a repeater, an entry and an
    /// impl with one.
    fn carried(it: TokenStream, ty: &str) {
        assert_eq!(causes(it.clone()), [format!("{}: relative", ty)], "{}", it);
        assert_eq!(analysed(it.clone(), &with_repeater()), [ty], "{}", it);
        let impls = squash(&leaked(it.clone()).impls);
        assert!(
            impls.contains(&format!("typeType={};", ty)),
            "{}: {}",
            it,
            impls
        );
    }

    /// What `table` records for `it`, as `tokens: reason` for each cause; nothing if it
    /// builds a table or finds nothing.
    fn causes_with(it: TokenStream, o: &Options) -> Vec<String> {
        match table(&item(it), o, 1).unwrap() {
            Some(Table::Failed(f)) => f
                .errors
                .iter()
                .map(|e| format!("{}: {}", squash(&e.tokens), tag(&e.reason)))
                .collect(),
            _ => Vec::new(),
        }
    }

    fn causes(it: TokenStream) -> Vec<String> {
        causes_with(it, &opts())
    }

    /// The record of `it`, or a panic.
    fn record(it: TokenStream, o: &Options) -> LeakFailure {
        match table(&item(it), o, 99).unwrap() {
            Some(Table::Failed(f)) => f,
            _ => panic!("a record"),
        }
    }

    /// The rows of a table, `item => text`, a line each.
    fn pairs(table: &str) -> impl Iterator<Item = (&str, &str)> {
        table.lines().filter_map(|l| l.split_once(" => "))
    }

    fn rows(table: &str) -> impl Iterator<Item = (TokenStream, &str)> {
        pairs(table).map(|(it, text)| (it.parse().unwrap(), text))
    }

    /// The causes of each row, `item => cause | cause`.
    fn check(table: &str, o: &Options) {
        for (it, want) in rows(table) {
            assert_eq!(causes_with(it.clone(), o).join(" | "), want, "{}", it);
        }
    }

    /// The table of `it` with the repeater `crate::R` and the id 42.
    fn leaked(it: TokenStream) -> Leaked {
        match table(&item(it), &with_repeater(), 42).unwrap() {
            Some(Table::Leaked(l)) => l,
            _ => panic!("a table"),
        }
    }

    /// The `KEY` argument of the repeater for the entry `nth` of [`leaked`], as `squash` prints it.
    fn key(nth: usize) -> String {
        let projection = TypeLeak::projection_of(syn::parse_quote!(crate::R), 42);
        let entry: type_leak::EntryId = syn::parse_str(&nth.to_string()).unwrap();
        format!("{}u128", projection.key(entry))
    }

    /// The META of `it` that has no repeater, read back; `None` where it has no META.
    fn meta_of(it: TokenStream, o: &Options) -> Option<TypeLeak> {
        match table(&item(it), o, 42).unwrap() {
            Some(Table::Leaked(l)) => Some(syn::parse2(l.meta).unwrap()),
            Some(Table::Failed(_)) => panic!("a record"),
            None => None,
        }
    }

    #[test]
    fn tidy_prints_types_as_written() {
        assert_eq!(tidy("'inner :: G < Self :: A >'"), "'inner::G<Self::A>'");
        assert_eq!(tidy("Box < Wrap < '_ > >"), "Box<Wrap<'_>>");
        assert_eq!(tidy("Vec < & 'a u8 >"), "Vec<&'a u8>");
        assert_eq!(tidy("(A , B)"), "(A, B)");
        assert_eq!(
            tidy("T : :: core :: convert :: From < inner :: G < T > >"),
            "T: ::core::convert::From<inner::G<T>>"
        );
        assert_eq!(tidy("const N : inner :: Ty"), "const N: inner::Ty");
        // Tokens print differently inside the compiler and outside of it.
        assert_eq!(tidy("two ! ()"), "two!()");
        assert_eq!(tidy("two! ()"), "two!()");
        assert_eq!(tidy("vec! [1 , 2]"), "vec![1, 2]");
        // Prose and text that is tidy already stay as they are.
        let prose = "write `<Self as Tr>::A`, or use `crate::X` (e.g. `&'a self`)";
        assert_eq!(tidy(prose), prose);
    }

    #[test]
    fn check_repeater_accepts_crate_paths_only() {
        for ok in ["crate::R", "crate::m::R"] {
            let p: Path = syn::parse_str(ok).unwrap();
            assert!(check_repeater(&p).is_ok(), "{}", ok);
        }
        for bad in [
            "::other::R",
            "R",
            "other::R",
            "self::R",
            "super::R",
            "::crate::R",
            "::R",
        ] {
            let p: Path = syn::parse_str(bad).unwrap();
            let e = check_repeater(&p).unwrap_err().to_string();
            assert!(
                e.contains("`repeater` must be a trait of this crate"),
                "{}: {}",
                bad,
                e
            );
        }
    }

    #[test]
    fn options_parse_runs_check_repeater() {
        let o: Options = syn::parse_quote!(repeater = crate::R);
        assert!(o.repeater.is_some());
        let e = syn::parse2::<Options>(quote! { repeater = ::other::R })
            .err()
            .unwrap();
        assert!(
            e.to_string().contains("must be a trait of this crate"),
            "{}",
            e
        );
    }

    #[test]
    fn type_alias_with_a_relative_type_is_recorded() {
        assert_eq!(causes(quote! { type A = Local; }), ["Local: alias"]);
        assert_eq!(causes(quote! { type B<T> = Wrap<T>; }), ["Wrap<T>: alias"]);
        // A prelude name is relative too.
        assert_eq!(causes(quote! { type C = Vec<u8>; }), ["Vec<u8>: alias"]);
    }

    #[test]
    fn type_alias_with_absolute_types_is_fine() {
        for it in [
            quote! { type A = ::std::vec::Vec<u8>; },
            quote! { type A = crate::X; },
            quote! { type A = u8; },
            quote! { type A<T> = ::std::vec::Vec<T>; },
        ] {
            assert!(table(&item(it), &opts(), 1).unwrap().is_none());
        }
    }

    #[test]
    fn repeater_on_a_type_alias_is_an_error() {
        let e = table(&item(quote! { type A = crate::X; }), &with_repeater(), 1)
            .err()
            .unwrap()
            .to_string();
        assert!(
            e.contains("`repeater` is not supported on type aliases"),
            "{}",
            e
        );
        // Before any analysis: also with a relative right-hand side.
        let e = table(&item(quote! { type A = Local; }), &with_repeater(), 1)
            .err()
            .unwrap()
            .to_string();
        assert!(
            e.contains("`repeater` is not supported on type aliases"),
            "{}",
            e
        );
    }

    #[test]
    fn modules_and_functions_are_not_analysed() {
        let m = item(quote! { mod m { pub struct S { a: Local } } });
        assert!(analyze(&m, &opts()).is_none());
        assert!(table(&m, &opts(), 1).unwrap().is_none());
        let e = table(&m, &with_repeater(), 1).err().unwrap().to_string();
        assert!(
            e.contains("`repeater` is supported on struct, enum, union and trait items"),
            "{}",
            e
        );
        let f = item(quote! { fn f(a: Local) {} });
        assert!(analyze(&f, &opts()).is_none());
    }

    #[test]
    fn analyze_lists_the_relative_types() {
        let o = opts();
        assert_eq!(
            analysed(
                quote! { struct S { a: Local, b: u8, c: crate::X, d: ::std::string::String } },
                &o
            ),
            ["Local"]
        );
        assert!(analysed(
            quote! { struct S { a: u8, c: crate::X, d: ::std::string::String } },
            &o
        )
        .is_empty());
        // The same type twice is one entry.
        assert_eq!(
            analysed(quote! { struct S { a: Local, b: Local } }, &o),
            ["Local"]
        );
        // Enums, unions and traits are analysed too.
        assert_eq!(analysed(quote! { enum E { A(Local), B } }, &o), ["Local"]);
        assert_eq!(analysed(quote! { union U { a: Local } }, &o), ["Local"]);
        assert_eq!(
            analysed(quote! { trait T { fn f(&self) -> Local; } }, &o),
            ["Local"]
        );
    }

    #[test]
    fn a_type_of_another_crate_is_no_entry() {
        // Every absolute path is the same crate on both sides: `::std` as well as `::other`.
        let o = opts();
        assert!(clean(
            quote! { struct S { a: ::other::X<u8>, b: ::std::vec::Vec<crate::Y> } },
            &o
        ));
        // A relative type in it is carried, with the absolute type around it.
        let it = quote! { struct S { a: ::std::vec::Vec<Local>, b: ::other::X<u8> } };
        assert_eq!(
            analysed(it.clone(), &with_repeater()),
            ["::std::vec::Vec<Local>"]
        );
        let impls = squash(&leaked(it).impls);
        assert_eq!(impls.matches("for()").count(), 1, "{}", impls);
        assert!(
            impls.contains("typeType=::std::vec::Vec<Local>;"),
            "{}",
            impls
        );
    }

    #[test]
    fn analyze_honours_the_primitive_and_prelude_options() {
        let it = quote! { struct S { a: u8, b: Vec<u8> } };
        let strict = Options {
            allow_primitive: false,
            ..Options::default()
        };
        assert!(analysed(it.clone(), &strict).iter().any(|t| t == "u8"));
        let default = analysed(it.clone(), &opts());
        assert_eq!(default, ["Vec<u8>"]);
        assert!(analysed(it, &prelude()).is_empty());
    }

    #[test]
    fn types_in_associated_const_defaults_are_carried() {
        // type-leak reads the default of an associated const (`check_values`), so the type
        // in it is an entry like any other.
        let it = quote! { trait Tr { const N: usize = ::core::mem::size_of::<inner::Local>(); } };
        carried(it.clone(), "inner::Local");
        // The callback's copy has the projection in the value.
        let table: TypeLeak = syn::parse2(leaked(it.clone()).meta).unwrap();
        let mut copy = item(it);
        table.apply(&mut copy);
        let key: TokenStream = key(0).parse().unwrap();
        assert_eq!(
            squash(&copy),
            squash(quote! {
                trait Tr {
                    const N: usize =
                        ::core::mem::size_of::<<() as crate::R<(), #key>>::Type>();
                }
            })
        );
        // A relative path that is not a type is an error of type-leak, for the consumer: it is
        // not a cause here.
        let it = quote! { trait Tr { const N: usize = LEN; fn f(&self) -> Local; } };
        assert_eq!(causes(it.clone()), ["Local: relative"]);
        assert_eq!(errors_of(it, &opts()), ["LEN"]);
    }

    #[test]
    fn a_prelude_value_needs_the_prelude_option() {
        // `check_values` reads the default of an associated const, where `None` is a relative
        // path, and the rule of the prelude accepts it like the name of a type.
        for (value, name) in [
            ("None", "None"),
            ("Some(1)", "Some"),
            ("Ok(1)", "Ok"),
            ("Err(1)", "Err"),
            ("drop(1)", "drop"),
        ] {
            let it = format!(
                "trait T {{ const X: ::core::option::Option<u8> = {}; }}",
                value
            );
            let it: TokenStream = it.parse().unwrap();
            assert_eq!(errors_of(it.clone(), &opts()), [name], "{}", it);
            // It is no cause at the definition: the consumer reports it.
            assert!(causes(it.clone()).is_empty(), "{}", it);
            assert!(clean(it.clone(), &prelude()), "{}", it);
        }
        // Only the bare name is the prelude's.
        let it = quote! { trait T { const X: ::core::option::Option<u8> = inner::None; } };
        assert_eq!(errors_of(it, &prelude()), ["inner::None"]);
        let it = quote! { trait T { const X: ::core::option::Option<u8> = ::core::option::Option::None; } };
        assert!(clean(it, &opts()));
    }

    #[test]
    fn the_prelude_option_takes_the_names_of_the_tables() {
        // type-leak's `Rules::prelude()` is the prelude of Rust 1.56: it has `drop`, and leaves
        // out `TryFrom`, `TryInto` and `FromIterator`, which announce takes bare as well (its
        // fast path does too, and must not accept more than the analysis). This pins the set,
        // so that a change of type-leak's list is announce's to decide.
        let types = PRELUDE.iter().map(|n| format!("struct S({});", n));
        let values = ["Some", "None", "Ok", "Err", "drop"]
            .iter()
            .map(|n| format!("trait T {{ const X: () = {}; }}", n));
        for src in types.chain(values) {
            let it: TokenStream = src.parse().unwrap();
            // `analyze` compares with the fast path as well.
            assert!(clean(it.clone(), &prelude()), "{}", src);
            // Only `Sized` is accepted bare without the option.
            if !src.contains("Sized") {
                assert_eq!(found(it.clone(), &opts()), 1, "{}", src);
            }
        }
        for name in NEWER_PRELUDE {
            assert!(PRELUDE.contains(name), "{}", name);
        }
        // What a later prelude added is not taken, as type-leak leaves it out.
        for src in [
            "struct S(Future);",
            "trait T { const X: usize = size_of::<u8>(); }",
        ] {
            let it: TokenStream = src.parse().unwrap();
            assert_eq!(found(it, &prelude()), 1, "{}", src);
        }
    }

    #[test]
    fn the_bodies_of_provided_methods_are_not_analysed() {
        // `check_values` would read them, so the trait is given without them.
        let it = quote! {
            trait Tr {
                const N: usize = 4;
                fn f(&self) { helper(); let x: Local = Local; }
                fn g(&self) -> u8 { Self::N as u8 }
            }
        };
        assert!(clean(it.clone(), &opts()));
        assert!(analysed(it, &with_repeater()).is_empty());
        // The signature is read all the same.
        let it = quote! { trait Tr { fn f(&self, a: Local) { helper(); } } };
        carried(it, "Local");
    }

    #[test]
    fn a_relative_trait_path_is_an_error_for_the_consumer() {
        for bad in [
            quote! { trait T: Base {} },
            quote! { struct S<T: Base>(T); },
            quote! { trait T { fn f(&self, x: impl Base); } },
            quote! { trait T { fn f<U: Base>(&self, u: U); } },
        ] {
            assert_eq!(errors_of(bad.clone(), &opts()), ["Base"], "{}", bad);
            // It is no cause at the definition, with or without a repeater.
            assert!(causes(bad.clone()).is_empty(), "{}", bad);
            assert!(causes_with(bad, &with_repeater()).is_empty());
        }
        assert!(clean(quote! { trait T: crate::Base {} }, &opts()));
        assert!(clean(
            quote! { struct S<T: ::core::fmt::Debug>(T); },
            &opts()
        ));
        // Bare prelude traits only with `allow_std_prelude`.
        let it = quote! { struct S<T: Clone>(T); };
        assert_eq!(errors_of(it.clone(), &opts()), ["Clone"]);
        assert!(clean(it, &prelude()));
    }

    #[test]
    fn a_trait_object_is_a_type_that_is_carried() {
        for (it, ty) in rows(
            "
            struct S { a: ::std::boxed::Box<dyn Base> } => dynBase
            struct S { a: ::std::boxed::Box<dyn Base + Send> } => dynBase+Send
            struct S<T> { a: ::std::boxed::Box<dyn Base<T>>, t: T } => dynBase<T>
            trait T { fn f(&self, a: &(dyn Base + 'static)); } => dynBase+'static
            struct S { a: ::std::boxed::Box<dyn Fn(Local) -> u8> } => dynFn(Local)->u8
        ",
        ) {
            assert!(table(&item(it.clone()), &opts(), 1).is_ok());
            carried(it, ty);
        }
        // An absolute trait object is no entry.
        let it = quote! { struct S { a: ::std::boxed::Box<dyn crate::Base> } };
        assert!(analysed(it, &with_repeater()).is_empty());
    }

    #[test]
    fn bare_sized_needs_no_prelude_option() {
        // `Sized` cannot be meaningfully shadowed.
        for ok in [
            quote! { struct S<T: ?Sized>(::std::boxed::Box<T>); },
            quote! { struct S<T>(T) where T: ?Sized; },
            quote! { struct S<T: Sized>(T); },
            quote! { trait T { fn f(&self) where Self: Sized; } },
            quote! { trait T { fn f<U: ?Sized>(&self, u: &U); } },
            quote! { trait T: Sized {} },
            quote! { trait T: crate::Base + Sized {} },
            quote! { trait T { fn f(&self) -> ::std::boxed::Box<dyn crate::Base + Sized>; } },
            quote! { trait T { fn f(&self, x: impl Sized); } },
            quote! { struct S<T: ?Sized + ::core::fmt::Debug>(::std::boxed::Box<T>); },
            quote! { struct S<T: ?::core::marker::Sized>(::std::boxed::Box<T>); },
        ] {
            assert!(clean(ok.clone(), &opts()), "{}", ok);
        }
        // The relative types next to it are carried as before.
        let impls = squash(
            &leaked(quote! {
                trait T { fn f<U: ?Sized>(&self, u: &U) -> Local<U>; fn g() -> Local2 where Self: Sized; }
            })
            .impls,
        );
        assert!(
            impls.contains(&format!("impl<U:?Sized>crate::R<(*constU,),{}>", key(0))),
            "{}",
            impls
        );
        assert!(impls.contains("typeType=Local<U>;"), "{}", impls);
        assert!(impls.contains("typeType=Local2;"), "{}", impls);
        // Every other bare prelude trait still needs `allow_std_prelude`, and the
        // names around `Sized` are not the prelude's.
        for bad in [
            quote! { struct S<T: Clone>(T); },
            quote! { struct S<T: Copy>(T); },
            quote! { struct S<T: Send>(T); },
            quote! { trait T: Clone {} },
            quote! { trait T { fn f(&self) where Self: Unpin; } },
            quote! { struct S<T: marker::Sized>(T); },
            quote! { struct S<T: self::Sized>(T); },
            quote! { struct S<T: Sized2>(T); },
        ] {
            assert_eq!(found(bad.clone(), &opts()), 1, "{}", bad);
        }
        // With the option, the prelude traits are accepted, `Sized` as well.
        for ok in [
            quote! { struct S<T: Clone + ?Sized>(::std::boxed::Box<T>); },
            quote! { trait T { fn f(&self) where Self: Sized; } },
            quote! { trait T: Sized {} },
        ] {
            assert!(clean(ok.clone(), &prelude()), "{}", ok);
        }
    }

    #[test]
    fn value_paths_are_errors_for_the_consumer() {
        for bad in [
            quote! { enum E { A = N } },
            quote! { struct S<const M: usize = N>; },
            quote! { trait T { const M: usize = N; } },
            quote! { struct S<T: ::x::Tr<{ N }>>(T); },
            quote! { struct S<T>(T) where T: ::x::Tr<{ N }>; },
        ] {
            assert_eq!(errors_of(bad.clone(), &opts()), ["N"], "{}", bad);
            // A repeater does not carry a value, and it is no cause at the definition.
            assert_eq!(errors_of(bad.clone(), &with_repeater()), ["N"]);
            assert!(causes_with(bad.clone(), &with_repeater()).is_empty());
            assert!(causes(bad).is_empty());
        }
        for ok in [
            quote! { struct S([u8; crate::N]); },
            quote! { struct S<const N: usize>([u8; N]); },
            quote! { enum E { A = ::core::mem::size_of::<u8>() as isize } },
            // A primitive type is allowed (`allow_primitive`) in a value too.
            quote! { enum E { A = u8::MAX as isize } },
            quote! { enum E { A = crate::N, B = 2 } },
            quote! { struct S<const M: usize = { crate::N }>; },
            quote! { struct S<T: ::x::Tr<{ crate::N }>>(T); },
        ] {
            assert!(clean(ok.clone(), &opts()), "{}", ok);
        }
    }

    #[test]
    fn a_value_path_is_a_const_parameter_of_its_own_scope() {
        // The parameter of a method or of a generic associated type is not one of the item,
        // nor of another method.
        for bad in [
            quote! { trait T { const M: usize = N; fn f<const N: usize>(&self); } },
            quote! { trait T { fn f<const N: usize>(&self); const M: usize = N; } },
            quote! { trait T { fn f<const N: usize>(&self); fn g<const M: usize = N>(&self); } },
            quote! { trait T { fn f<const N: usize>(&self); fn g<U: ::x::Tr<{ N }>>(&self); } },
            quote! { trait T { type A<const N: usize>; const M: usize = N; } },
            quote! { trait T { type A<const N: usize>; type B<U: ::x::Tr<{ N }>>; } },
            quote! { trait T { type A<const N: usize>; fn g<U: ::x::Tr<{ N }>>(&self); } },
        ] {
            assert_eq!(errors_of(bad.clone(), &opts()), ["N"], "{}", bad);
        }
        // Its own scope, and the item's everywhere.
        for ok in [
            quote! { trait T { fn f<const N: usize, const M: usize = N>(&self); } },
            quote! { trait T { fn f<const N: usize>(&self) where Self: ::x::Tr<{ N }>; } },
            quote! { trait T { type A<const N: usize>: ::x::Tr<{ N }>; } },
            quote! { trait T<const N: usize> { const M: usize = N; fn f(&self) where Self: ::x::Tr<{ N }>; } },
            quote! { trait T<const N: usize> { fn f<const M: usize>(&self) where Self: ::x::Tr<{ N }>; } },
            quote! { trait T<const N: usize> { type A<const M: usize = N>; } },
            quote! { struct S<const N: usize, const M: usize = N>; },
        ] {
            assert!(clean(ok.clone(), &opts()), "{}", ok);
        }
    }

    #[test]
    fn values_inside_a_type_are_carried_with_it() {
        for (it, ty) in rows(
            "
            struct S([u8; N]); => [u8;N]
            struct S(crate::Buf<{ N }>); => crate::Buf<{N}>
            struct S(Wrap<{ N }>); => Wrap<{N}>
            struct S { a: ::std::boxed::Box<[u8; N]> } => [u8;N]
            struct S<T: ::core::convert::From<[u8; N]>>(T); => [u8;N]
            trait T { fn f(&self, a: [u8; N]) -> u8; } => [u8;N]
            struct S { a: fn([u8; N]) -> u8 } => [u8;N]
        ",
        ) {
            carried(it, ty);
        }
        // Constants that are fine on their own stay so inside a type.
        for ok in [
            quote! { struct S<const N: usize>([u8; N]); },
            quote! { struct S([u8; crate::N]); },
            quote! { struct S(::std::boxed::Box<[u8; Self::N]>); },
        ] {
            assert!(analysed(ok, &with_repeater()).is_empty());
        }
        // The values outside of types are errors of type-leak all the same.
        let it = quote! { enum E { A = M, B([u8; N]) } };
        assert_eq!(causes(it.clone()), ["[u8;N]: relative"]);
        assert_eq!(errors_of(it.clone(), &with_repeater()), ["M"]);
        assert!(causes_with(it, &with_repeater()).is_empty());
        // The types named in a value are carried like the others.
        let it = quote! { enum E { A = ::core::mem::size_of::<[u8; N]>() as isize } };
        assert_eq!(analysed(it, &with_repeater()), ["[u8;N]"]);
    }

    #[test]
    fn higher_ranked_lifetime_in_a_leaked_type_is_carried() {
        // The entry's context declares `'b`, so the impl does and the consumer's
        // projection (inside the `for<'b>`) passes it.
        let impls = squash(&leaked(quote! { struct S { f: for<'b> fn(Local<'b>) } }).impls);
        assert!(
            impls.contains(&format!(
                "impl<'b>crate::R<(&'b(),),{}>for(){{typeType=Local<'b>;}}",
                key(0)
            )),
            "{}",
            impls
        );
    }

    #[test]
    fn relative_type_without_repeater_is_recorded() {
        assert_eq!(
            causes(quote! { struct S { a: Local } }),
            ["Local: relative"]
        );
        // Every type that needed a table, once each.
        assert_eq!(
            causes(quote! { struct S { a: Local, b: Other, c: Local, d: u8 } }),
            ["Local: relative", "Other: relative"]
        );
        // The record is for the item, with the id of the announcement.
        let f = record(quote! { pub struct S { a: Local } }, &opts());
        assert_eq!(f.ident.to_string(), "S");
        assert_eq!(f.id.base10_digits(), "99");
        // The texts of the cause are in the META that the callbacks read.
        let meta = squash(f.meta());
        assert!(meta.starts_with("type_leak_error(id=99u64,"), "{}", meta);
        assert!(
            meta.contains("message=\"useabsolutepath:'Local'\""),
            "{}",
            meta
        );
    }

    #[test]
    fn a_cause_carries_the_texts_that_a_call_says() {
        // A relative type: the path in it, written absolutely whatever stands around it.
        for (it, path) in rows(
            "
            pub struct S { pub a: Local } => Local
            pub struct S { pub a: &'static Local } => Local
            pub struct S { pub a: *const inner::Other } => inner::Other
            pub struct S { pub a: &'static mut inner::Third } => inner::Third
            pub struct S { pub a: ::std::boxed::Box<dyn Fourth> } => Fourth
            pub struct S { pub a: (u8, Local) } => Local
            pub struct S { pub a: [u8; N] } => N
            pub struct S(pub crate::Buf<{ N }>); => N
        ",
        ) {
            let e = &record(it.clone(), &opts()).errors[0];
            let message = format!("use absolute path: '{}'", path);
            let help = format!(
                "write `{p}` with an absolute path (e.g. `crate::{p}`), or add `repeater = ..` \
                 to the `#[announce]` of `S`",
                p = path
            );
            assert_eq!(e.message.as_deref(), Some(&*message), "{}", it);
            assert_eq!(e.help.as_deref(), Some(&*help), "{}", it);
            assert!(e.notes.is_empty());
        }
        // A prelude name: the help says what the name costs.
        let f = record(quote! { trait T { fn f(&self, a: Vec<Local>); } }, &opts());
        assert_eq!(
            f.errors[0].help.as_deref(),
            Some(
                "write `Vec` with an absolute path (it is a name of the std prelude, which a caller \
                 may shadow), or add `allow_std_prelude = true` to the `#[announce]` of `T`"
            )
        );
        // A const generic that is not of a primitive type.
        let it = quote! { struct S<const N: crate::Idx> { a: Wrap<N> } };
        let e = &record(it, &with_repeater()).errors[0];
        assert_eq!(
            e.message.as_deref(),
            Some("const parameter of a non-primitive type: 'crate::Idx'")
        );
        assert_eq!(
            e.help.as_deref(),
            Some("use a primitive type for the const parameter")
        );
    }

    #[test]
    fn nothing_to_carry_gives_no_table_without_repeater() {
        let it = item(quote! { struct S { a: u8, b: crate::X, c: ::std::string::String } });
        assert!(table(&it, &opts(), 1).unwrap().is_none());
    }

    #[test]
    fn repeater_table_has_one_impl_per_type() {
        let leaked = leaked(quote! { struct S { a: Local, b: Other, c: Local } });
        let impls = squash(&leaked.impls);
        assert_eq!(impls.matches("for()").count(), 2, "{}", impls);
        // The arguments come first: `Repeater<ARGS, KEY>`.
        for nth in 0..2 {
            let path = format!("crate::R<(),{}>for()", key(nth));
            assert!(impls.contains(&path), "{}", impls);
        }
        assert!(
            impls.contains("typeType=Local;") && impls.contains("typeType=Other;"),
            "{}",
            impls
        );
        let meta = squash(&leaked.meta);
        assert!(
            meta.starts_with(
                "type_leak(repeater=crate::R,id=42u64,context=1,result=[\"type-leak-format-"
            ),
            "{}",
            meta
        );
    }

    #[test]
    fn repeater_table_carries_only_the_generics_of_the_entry() {
        let leaked = leaked(quote! {
            struct S<'a, T: ::core::fmt::Debug, U, const N: usize> { a: Wrap<T>, b: Ref<'a>, c: Arr<N>, d: Plain }
        });
        let impls = squash(&leaked.impls);
        // `T` keeps its own bound; `U` is never mentioned.
        assert!(
            impls.contains(&format!(
                "impl<T:::core::fmt::Debug>crate::R<(T,),{}>",
                key(0)
            )),
            "{}",
            impls
        );
        assert!(impls.contains("typeType=Wrap<T>;"), "{}", impls);
        assert!(!impls.contains("U"), "{}", impls);
        // Lifetimes appear as `&'a ()`, consts as `[(); N]`.
        for (args, nth) in [("&'a()", 1), ("[();N]", 2)] {
            let path = format!("crate::R<({},),{}>", args, key(nth));
            assert!(impls.contains(&path), "{}: {}", path, impls);
        }
        assert!(impls.contains("impl<constN:usize>"), "{}", impls);
        assert!(
            impls.contains(&format!("crate::R<(),{}>", key(3))),
            "{}",
            impls
        );
    }

    #[test]
    fn repeater_table_encodes_const_generics_of_every_primitive_type() {
        let leaked = leaked(quote! {
            struct S<const A: u8, const B: bool, const C: char, const D: i64, const E: u64, const F: isize, const K: u128>(
                inner::W<A, B, C, D, E, F, K>
            );
        });
        let impls = squash(&leaked.impls);
        assert!(
            impls.starts_with(
                "#[doc(hidden)]impl<constA:u8,constB:bool,constC:char,constD:i64,constE:u64,constF:isize,constK:u128>crate::R<("
            ),
            "{}",
            impls
        );
        // Every parameter in its own slot of the repeater; `usize` is a length, and `u128` is
        // the `KEY` slot.
        for slot in [
            "*constdyncrate::R<(),0,false,'\\0',A,Type=()>",
            "*constdyncrate::R<(),0,B,Type=()>",
            "*constdyncrate::R<(),0,false,C,Type=()>",
            "*constdyncrate::R<(),K,Type=()>",
        ] {
            assert!(impls.contains(slot), "{}: {}", slot, impls);
        }
        assert!(impls.contains("D,Type=()>"), "{}", impls);
        assert!(impls.contains("E,Type=()>"), "{}", impls);
        assert!(impls.contains("F,Type=()>"), "{}", impls);
    }

    #[test]
    fn repeater_table_records_an_alias_as_the_type_of_a_const_generic() {
        // An absolute alias is fine for the analysis, but cannot be encoded.
        let it = quote! { struct S<const N: crate::Idx> { a: Wrap<N> } };
        assert_eq!(causes_with(it, &with_repeater()), ["crate::Idx: const"]);
        // Not used by a carried type: nothing to encode.
        leaked(quote! { struct S<const N: crate::Idx> { a: Local, b: [u8; 4] } });
    }

    #[test]
    fn every_encode_cause_is_reported_at_once() {
        // The causes are announce's: the entries that have no impl. The errors of type-leak
        // (`T: Base`, `M`, `Wrap<'_>`) are the consumer's, and are no cause here.
        check("
            struct S<T: Base, const N: crate::Idx> { a: Local<N>, t: T } => crate::Idx: const
            struct S<T: Base, const N: crate::Idx> { a: Local<N>, b: Wrap<'_>, t: T } => crate::Idx: const
            enum E<const N: crate::Idx> { A = M, B(Local<N>) } => crate::Idx: const
            struct S<const N: crate::Idx, const M: crate::Idx2> { a: Local<N>, b: Other<M> } => crate::Idx: const | crate::Idx2: const
            struct S<T: Base, const N: crate::Idx> { a: Local, t: T } =>
            struct S { a: inner::G<Self>, b: Other<Self> } => Other<Self>: self | inner::G<Self>: self
            struct S<T> { a: Local<T>, b: inner::G<Self> } => inner::G<Self>: self
            struct S<const N: crate::Idx> { a: Local<N>, b: inner::G<Self> } => crate::Idx: const
        ", &with_repeater());
        // One cause makes several entries fail, and is one error.
        check("
            struct S<T: Base, const N: crate::Idx> { a: Local<N>, b: Other<N>, t: T } => crate::Idx: const
        ", &with_repeater());
        // Without a repeater there is no table to write: every entry is a cause of its own.
        check(
            "
            struct S<T: Base, const N: crate::Idx> { a: Local<N>, t: T } => Local<N>: relative
            struct S { a: inner::G<Self>, b: Local } => inner::G<Self>: relative | Local: relative
        ",
            &opts(),
        );
        let record = record(
            quote! { struct S<const N: crate::Idx, const M: crate::Idx2> { a: Local<N>, b: Other<M> } },
            &with_repeater(),
        );
        assert_eq!(record.errors.len(), 2);
    }

    #[test]
    fn an_entry_that_names_self_has_no_impl() {
        // The impls are for `()`, which `Self` is not: the entry cannot be written.
        let f = record(quote! { struct S { a: inner::G<Self> } }, &with_repeater());
        assert_eq!(f.errors.len(), 1);
        assert_eq!(f.errors[0].reason, "with `Self` in a carried type");
        assert_eq!(squash(&f.errors[0].tokens), "inner::G<Self>");
        // A trait: `Self` is the implementor.
        check(
            "
            trait Tr { fn f(&self) -> inner::G<Self>; } => inner::G<Self>: self
            trait Tr { type A; fn f(&self) -> inner::G<Self::A>; } => inner::G<Self::A>: self
        ",
            &with_repeater(),
        );
        // `Self` alone, and an absolute type that names it, are no entry.
        for ok in [
            quote! { struct S(::other::G<Self>); },
            quote! { trait Tr { type Out; fn f(&self) -> Self::Out; } },
            quote! { struct S<T: ::core::ops::Add>(inner::G<T::Output>); },
        ] {
            assert!(
                !causes_with(ok.clone(), &with_repeater())
                    .iter()
                    .any(|c| c.ends_with(": self")),
                "{}",
                ok
            );
        }
    }

    #[test]
    fn same_text_in_two_contexts_is_two_entries_with_their_own_generics() {
        // The bound of `X` in `f` must not reach `g`.
        let leaked = leaked(quote! {
            trait T {
                fn f<X: ::core::clone::Clone>(&self) -> inner::GC<X>;
                fn g<X>(&self) -> inner::G<X>;
                fn h<X>(&self) -> inner::GC<X>;
            }
        });
        let impls = squash(&leaked.impls);
        assert_eq!(impls.matches("for()").count(), 3, "{}", impls);
        assert!(
            impls.contains("impl<X:::core::clone::Clone>crate::R<(X,),"),
            "{}",
            impls
        );
        assert!(impls.contains("impl<X>crate::R<(X,),"), "{}", impls);
        assert_eq!(
            impls.matches("typeType=inner::GC<X>;").count(),
            2,
            "{}",
            impls
        );
    }

    #[test]
    fn a_method_generic_named_like_a_type_does_not_make_it_a_generic() {
        // `Local` is a generic only inside `f`.
        let leaked = leaked(quote! {
            trait U { fn f<Local>(&self, x: Local); fn g(&self) -> Local; }
        });
        let impls = squash(&leaked.impls);
        assert!(
            impls.contains(&format!(
                "implcrate::R<(),{}>for(){{typeType=Local;}}",
                key(0)
            )),
            "{}",
            impls
        );
    }

    #[test]
    fn bounds_naming_other_parameters_stay_in_the_context() {
        // `T: Into<V>` needs `V`, which comes with it.
        let impls = squash(
            &leaked(quote! {
                struct BF<T: ::core::convert::Into<V>, V>(pub inner::LocalG<T>, pub V);
            })
            .impls,
        );
        assert!(
            impls.contains("impl<T:::core::convert::Into<V>,V>crate::R<(T,V,),")
                || impls.contains("impl<T:::core::convert::Into<V>,V>crate::R<(T,V),"),
            "{}",
            impls
        );
    }

    #[test]
    fn where_clauses_reach_the_impl() {
        // The `where` clause of the item is carried to the impl too.
        let impls = squash(
            &leaked(quote! {
                struct WC<T>(pub inner::LocalC<T>) where T: ::core::clone::Clone;
            })
            .impls,
        );
        assert!(
            impls.contains(&format!(
                "impl<T>crate::R<(T,),{}>for()where T:::core::clone::Clone",
                key(0)
            )) || impls.contains(&format!(
                "impl<T:::core::clone::Clone>crate::R<(T,),{}>for()",
                key(0)
            )),
            "{}",
            impls
        );
    }

    #[test]
    fn a_name_that_is_a_parameter_only_elsewhere_is_not_one() {
        // `U` is a parameter of `f` only: the `inner::U` of `g` is a type, not that parameter.
        let impls = squash(
            &leaked(quote! {
                trait MD { fn f<U>(&self, x: inner::LocalG<U>); fn g(&self) -> inner::U; }
            })
            .impls,
        );
        assert!(
            impls.contains(&format!("impl<U>crate::R<(U,),{}>for()", key(0))),
            "{}",
            impls
        );
        assert!(
            impls.contains(&format!("crate::R<(),{}>for()", key(1))),
            "{}",
            impls
        );
        let impls = squash(
            &leaked(quote! {
                trait K { fn f<N>(&self, x: inner::LocalG<N>); fn g<const N: usize>(&self, x: inner::Arr<N>); }
            })
            .impls,
        );
        assert!(impls.contains("impl<N>crate::R<(N,),"), "{}", impls);
        assert!(
            impls.contains("impl<constN:usize>crate::R<([();N],),"),
            "{}",
            impls
        );
    }

    #[test]
    fn repeater_without_relative_types_is_an_empty_table() {
        let leaked = leaked(quote! { struct S { a: u8 } });
        assert!(leaked.impls.is_empty());
        let meta = squash(&leaked.meta);
        assert!(
            meta.starts_with("type_leak(repeater=crate::R,id=42u64,context=0,result=["),
            "{}",
            meta
        );
        // The consumer reads it back, with nothing to replace.
        let read: TypeLeak = syn::parse2(leaked.meta).unwrap();
        assert_eq!(read.entries().len(), 0);
        assert!(read.errors().is_empty());
    }

    #[test]
    fn the_meta_reads_back_with_the_indices_of_the_impls() {
        let leaked = leaked(quote! {
            struct S<T> { a: Local, b: Wrap<T>, c: ::other::X<u8> }
        });
        let read: TypeLeak = syn::parse2(leaked.meta).unwrap();
        let impls = squash(&leaked.impls);
        // The impls are for the entries; `::other::X<u8>` is no entry.
        assert_eq!(read.entries().len(), 2);
        for (ix, ty) in read.entries() {
            let nth = format!("{}>for()", key(ix.index()));
            assert!(impls.contains(&nth), "{}: {}", nth, impls);
            assert!(
                impls.contains(&format!("typeType={};", squash(&ty))),
                "{}",
                impls
            );
        }
    }

    // --- the errors of type-leak travel in the META ---

    #[test]
    fn the_errors_of_type_leak_are_no_cause_at_the_definition() {
        // The part is copied as written and the analysis goes on: the entries of the rest are
        // there, and so is each error.
        for it in [
            quote! { struct S { a: Wrap<'_>, b: u8 } },
            quote! { struct S<T: Base>(T); },
            quote! { struct S { a: Vec<&'_ u8> } },
            quote! { enum E { A = N } },
            quote! { struct S([u8; two!()]); },
            quote! { struct S<const N: inner::Ty>; },
            quote! { trait Tr { fn f<'a>(&self, a: Wrap<'a>) -> &'a u8; } },
            quote! { type A = Self; },
            quote! { type A = Wrap<'_>; },
        ] {
            assert!(causes(it.clone()).is_empty(), "{}", it);
            if !matches!(item(it.clone()), Item::Type(_)) {
                assert!(
                    causes_with(it.clone(), &with_repeater()).is_empty(),
                    "{}",
                    it
                );
            }
            let read = meta_of(it.clone(), &opts()).unwrap_or_else(|| panic!("a META: {}", it));
            assert!(!read.errors().is_empty(), "{}", it);
        }
    }

    #[test]
    fn errors_without_entries_are_a_meta_without_a_repeater() {
        let it = quote! { struct S<T: Base>(T); };
        let l = match table(&item(it), &opts(), 42).unwrap() {
            Some(Table::Leaked(l)) => l,
            _ => panic!("a table"),
        };
        // No impl, and no `repeater` or `id`: only the context and the result.
        assert!(l.impls.is_empty());
        let meta = squash(&l.meta);
        assert!(
            meta.starts_with("type_leak(context=1,result=[\"type-leak-format-"),
            "{}",
            meta
        );
        assert!(meta.contains("@["), "{}", meta);
        let read: TypeLeak = syn::parse2(l.meta).unwrap();
        assert!(read.projection.is_none());
        assert_eq!(read.errors().len(), 1);
        assert_eq!(squash(&read.errors()[0].cause), "Base");
        // The consumer reports it, at the cause.
        let e = read
            .try_apply(&mut item(quote! { struct S<T: Base>(T); }))
            .unwrap_err();
        assert!(e.to_string().contains("use absolute path: 'Base'"), "{}", e);
    }

    #[test]
    fn a_type_alias_with_errors_has_a_meta_without_a_repeater() {
        for it in [quote! { type A = Self; }, quote! { type A = Wrap<'_>; }] {
            let read = meta_of(it.clone(), &opts()).unwrap();
            assert!(read.projection.is_none(), "{}", it);
            assert_eq!(read.errors().len(), 1, "{}", it);
        }
        // An alias with a relative type has the record, and no META.
        assert_eq!(causes(quote! { type A = Local; }), ["Local: alias"]);
        // A clean alias has nothing.
        assert!(meta_of(quote! { type A = u8; }, &opts()).is_none());
    }

    #[test]
    fn a_repeater_has_the_errors_in_the_table_with_the_impls() {
        let it = quote! { struct S { a: Local, b: Wrap<'_> } };
        let l = leaked(it.clone());
        let impls = squash(&l.impls);
        assert!(impls.contains("typeType=Local;"), "{}", impls);
        let read: TypeLeak = syn::parse2(l.meta).unwrap();
        assert!(read.projection.is_some());
        assert_eq!(read.entries().len(), 1);
        assert_eq!(read.errors().len(), 1);
        assert_eq!(squash(&read.errors()[0].cause), "Wrap<'_>");
        // The consumer allows the part (it promises that it compiles as written), then the table
        // is ready.
        let allowed = read
            .clone()
            .allow(|cause: &Cause, _: Place| cause.starts_with(&syn::parse_quote!(Wrap)));
        assert!(allowed.errors().is_empty());
        let mut copy = item(it);
        allowed.try_apply(&mut copy).unwrap();
        let key: TokenStream = key(0).parse().unwrap();
        assert_eq!(
            squash(&copy),
            squash(quote! { struct S { a: <() as crate::R<(), #key>>::Type, b: Wrap<'_> } })
        );
        assert!(read.try_apply(&mut copy.clone()).is_err());
    }

    #[test]
    fn every_error_of_type_leak_is_reported_by_the_consumer_at_once() {
        let it = quote! {
            trait T<const N: usize = { Self::X }> {
                fn f(&self, a: *const dyn Tr<'_>);
                fn g<'x>(&self, b: Wrap<'x>) -> &'x u8;
                fn h(&self) -> [u8; two!()];
                fn z(&self) -> Local;
            }
        };
        // Without a repeater the entry is a cause at the definition, and there is no META.
        assert_eq!(causes(it.clone()), ["Local: relative"]);
        let l = leaked(it.clone());
        let read: TypeLeak = syn::parse2(l.meta).unwrap();
        let e = read.try_apply(&mut item(it)).unwrap_err();
        // One error each, combined.
        let all: Vec<String> = e.into_iter().map(|e| e.to_string()).collect();
        assert_eq!(all.len(), 3, "{:?}", all);
        // type-leak's own words, with its notes and helps.
        assert!(all[0].contains("use absolute path: 'Tr<'_>'"), "{:?}", all);
        assert!(all[1].contains("(E0195)"), "{:?}", all);
        assert!(all[2].contains("use absolute path: 'two'"), "{:?}", all);
    }
}
