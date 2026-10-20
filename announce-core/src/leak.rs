//! Type context of a definition: `#[announce(repeater = path)]` and the
//! type-leak table.
//!
//! On the cfg-evaluated item, type-leak finds the types that would not
//! resolve at a caller: relative paths such as `Local` or `super::X`.
//! The decision is type-leak's own: `Referrer::required()` after
//! `Leaker::finish()`, with primitives and `crate::` paths allowed. Std items
//! must be written absolutely (`::std::vec::Vec<u8>`); prelude names such as
//! `Vec` or `Option` count as relative unless `allow_std_prelude = true`
//! (type-leak's `allowed_paths`, with the names in [`PRELUDE`]). The one
//! exception is `Sized`: `T: ?Sized`, `where Self: Sized` and `trait T: Sized`
//! are accepted bare, because the language gives it a meaning of its own.
//!
//! type-leak sees the item as written. `Self` is not internable
//! (`allow_self = false`): it has no meaning in an `impl .. for ()`, and it
//! resolves everywhere. On top of type-leak, announce checks
//! - trait paths (supertraits, bounds, `impl`/`dyn` bounds): absolute only,
//! - value paths in array lengths, enum discriminants, const generic arguments
//!   and const defaults: absolute, `Self::..` or a const parameter (a repeater
//!   only carries types; the types named inside such an expression, as in
//!   `size_of::<Local>()`, are carried like any other: type-leak reads array
//!   lengths, const arguments, discriminants and the defaults of const
//!   parameters, not the defaults of associated consts).
//!
//! If any relative type remains, `repeater = ..` carries it (for type aliases,
//! which are not a type-leak target, it is an error instead). The repeater is the
//! trait that `define_repeater!` (type-leak's `repeater_definition()`) defines in
//! the definer's crate. Each required entry `I` of the `Referrer` then gets an
//! impl of it, written by type-leak (`Referrer::repeater_impl_with`):
//!
//! ```text
//! impl<P..> REPEATER<(ARGS..), ID, I> for () { type Type = T_I; }
//! ```
//!
//! where `P..` are the generics of the *context of that entry* (the scope
//! where the type is written: the item, a method, a GAT) and `ARGS` encodes the
//! ones the type uses; const generics of every primitive type go through the
//! repeater's slots (`RepeaterSlots`). The payload stays the original item; the
//! entry's META gets `type_leak(repeater = .., id = .., context = .., referrer =
//! [..])`, the `Referrer` as type-leak serializes it, and a callback rewrites
//! the payload with that `Referrer` (`announce::helper::TypeLeak::apply`), so the
//! consumer's arguments are type-leak's too.
//!
//! What a table cannot carry is a failure ([`Table::Failed`]): relative types without a
//! `repeater`, a relative path in a type alias, a type that type-leak refuses, a relative
//! trait or value path, a const generic that cannot be encoded. By default it is an error at
//! the definition ([`LeakFailure::to_definition_error`]). With `strict = false` it is
//! **recorded**, not refused: a call in the module of the item can use it as written, and the
//! call is where it fails otherwise. There is no table then: the META gets
//! `type_leak_error(..)` ([`LeakFailure`]) instead, and the definition emits a marker that
//! tells a call in its module from any other. Option misuse stays an error: a `repeater` on a
//! type alias or on an item that is not analysed, and a `repeater` of another crate.

use crate::helper::{LeakError, LeakFailure, TypeLeak};
use proc_macro2::{Ident, Span, TokenStream};
use syn::visit::Visit;
use syn::{Item, Path, Result, Type};
use template_quote::quote;
use type_leak::{ContextId, ImplSelf, Referrer, RepeaterSlots};

pub struct Leaked {
    /// Impls emitted at the definition site.
    pub impls: TokenStream,
    /// `type_leak(repeater = .., id = .., context = .., referrer = [..])`.
    pub meta: TokenStream,
}

/// What [`table`] makes of an item that is analysed.
pub enum Table {
    /// The item has a `repeater`: its impls and the `type_leak(..)` META.
    Leaked(Leaked),
    /// Something in the item does not resolve outside its module: there is no table, and
    /// the META is `type_leak_error(..)` ([`LeakFailure::meta`]).
    Failed(LeakFailure),
}

/// The analysis of an item: what type-leak found, and the scope of the item.
#[derive(Debug)]
pub struct Analysis {
    pub referrer: Referrer,
    /// The top-level context of the item in `referrer`.
    pub context: ContextId,
}

impl Analysis {
    /// The types that need a repeater impl (type-leak's required entries), with
    /// their index. One type text can be several entries, one for each context.
    pub fn required(&self) -> impl Iterator<Item = (usize, &Type)> {
        self.referrer.required()
    }
}

/// What in an item cannot be carried to another module: the analysis found no table for
/// it. Each error is a recorded thing ([`LeakError`]).
#[derive(Debug)]
pub struct Unleakable(pub Vec<LeakError>);

impl std::fmt::Display for Unleakable {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for (i, e) in self.0.iter().enumerate() {
            if i > 0 {
                f.write_str("; ")?;
            }
            write!(f, "`{}` {}", tidy(&e.tokens.to_string()), e.reason)?;
        }
        Ok(())
    }
}

/// The reason of a relative type in a definition without a `repeater`.
const REASON_WITHOUT_REPEATER: &str = crate::helper::WITHOUT_REPEATER;
/// The reason of a relative trait path.
const REASON_TRAIT_PATH: &str = crate::helper::TRAIT_PATH;
/// The reason of a relative value path.
const REASON_VALUE_PATH: &str = crate::helper::VALUE_PATH;
/// The reason of a relative path on the right-hand side of a type alias.
const REASON_ALIAS: &str = crate::helper::ALIAS;
/// The reason of a type that type-leak refuses to carry (`Wrap<'_>`, `Local<Self>`, ..).
const REASON_REFUSED: &str = crate::helper::REFUSED;

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

fn is_absolute(p: &Path) -> bool {
    let first = p
        .segments
        .first()
        .map(|s| s.ident.to_string())
        .unwrap_or_default();
    p.leading_colon.is_some() || first == "crate" || first == "$crate"
}

/// `Sized`, as written in `T: ?Sized`, `where Self: Sized` and `T: Sized`: the
/// single segment `Sized` without arguments or a leading `::`. It is the one
/// prelude trait that is accepted bare without `allow_std_prelude`: the
/// language gives `?Sized` (and `Sized`) a meaning of their own, so it cannot
/// be meaningfully shadowed, and it is written in nearly every generic API
/// that takes unsized types.
fn is_bare_sized(p: &Path) -> bool {
    p.leading_colon.is_none()
        && p.segments.len() == 1
        && p.segments[0].ident == "Sized"
        && p.segments[0].arguments.is_none()
}

/// Trait paths (supertraits, generic and where-clause bounds, `impl`/`dyn`
/// bounds) must be absolute: a repeater only carries *types*, so a relative
/// trait path could never resolve at a caller (decided 2026-10-01). Bare
/// `Sized` and `?Sized` are accepted ([`is_bare_sized`]); with
/// `allow_std_prelude = true`, bare prelude traits are accepted too. The relative
/// ones are returned.
fn check_trait_paths(item: &Item, allow_prelude: bool) -> Vec<LeakError> {
    struct V {
        allow_prelude: bool,
        errors: Vec<LeakError>,
    }
    impl<'ast> syn::visit::Visit<'ast> for V {
        fn visit_trait_bound(&mut self, b: &'ast syn::TraitBound) {
            let p = &b.path;
            let first = p
                .segments
                .first()
                .map(|s| s.ident.to_string())
                .unwrap_or_default();
            let prelude =
                self.allow_prelude && p.segments.len() == 1 && PRELUDE.contains(&first.as_str());
            if !(is_absolute(p) || prelude || is_bare_sized(p)) {
                self.errors.push(LeakError::new(p, REASON_TRAIT_PATH));
            }
            syn::visit::visit_trait_bound(self, b);
        }
    }
    let mut v = V {
        allow_prelude,
        errors: Vec::new(),
    };
    syn::visit::Visit::visit_item(&mut v, item);
    v.errors
}

/// Value expressions inside types and signatures (array lengths, enum
/// discriminants, const generic arguments, const defaults) are copied to
/// callers like the types around them, but a repeater cannot carry values:
/// every path in them must be absolute, `Self::..`, or a const parameter. A
/// relative array length would make type-leak carry the whole
/// array type. The types named inside a value (`size_of::<Local>()`) are
/// type-leak's business, not checked here. The relative paths are returned.
fn check_values(item: &Item, const_params: &[String]) -> Vec<LeakError> {
    struct V<'a> {
        consts: &'a [String],
        errors: Vec<LeakError>,
    }
    impl V<'_> {
        fn check(&mut self, e: &syn::Expr) {
            struct P<'b, 'a>(&'b mut V<'a>);
            impl<'ast> Visit<'ast> for P<'_, '_> {
                fn visit_expr_path(&mut self, p: &'ast syn::ExprPath) {
                    let path = &p.path;
                    let first = path
                        .segments
                        .first()
                        .map(|s| s.ident.to_string())
                        .unwrap_or_default();
                    let ok = p.qself.is_some()
                        || is_absolute(path)
                        || first == "Self"
                        || (path.segments.len() == 1 && self.0.consts.contains(&first));
                    if !ok {
                        self.0.errors.push(LeakError::new(path, REASON_VALUE_PATH));
                    }
                    syn::visit::visit_expr_path(self, p);
                }
            }
            Visit::visit_expr(&mut P(self), e);
        }
    }
    impl<'ast> Visit<'ast> for V<'_> {
        fn visit_type_array(&mut self, a: &'ast syn::TypeArray) {
            self.check(&a.len);
            syn::visit::visit_type_array(self, a);
        }
        fn visit_variant(&mut self, v: &'ast syn::Variant) {
            if let Some((_, e)) = &v.discriminant {
                self.check(e);
            }
            syn::visit::visit_variant(self, v);
        }
        fn visit_trait_item_const(&mut self, c: &'ast syn::TraitItemConst) {
            if let Some((_, e)) = &c.default {
                self.check(e);
            }
            syn::visit::visit_trait_item_const(self, c);
        }
        fn visit_const_param(&mut self, c: &'ast syn::ConstParam) {
            if let Some(e) = &c.default {
                self.check(e);
            }
            syn::visit::visit_const_param(self, c);
        }
        fn visit_generic_argument(&mut self, a: &'ast syn::GenericArgument) {
            if let syn::GenericArgument::Const(e) = a {
                self.check(e);
            }
            syn::visit::visit_generic_argument(self, a);
        }
        // Provided-method bodies are not analysed (documented gap).
        fn visit_block(&mut self, _: &'ast syn::Block) {}
    }
    let mut v = V {
        consts: const_params,
        errors: Vec::new(),
    };
    Visit::visit_item(&mut v, item);
    v.errors
}

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

/// The std prelude name that heads the offending type, when it is relative here
/// (`allow_std_prelude` is off). type-leak then names the whole type, says why it
/// cannot be leaked (`Self::Out`, `'_`, ..) and shows the absolute path of the
/// prelude name: "use `::core::option::Option` instead of `Option`". The reason
/// to act on is the relative name, which this finds.
fn prelude_name(item: &type_leak::ErrorItem, opts: &crate::options::Options) -> Option<String> {
    if opts.allow_std_prelude || !item.help_text()?.starts_with("use `") {
        return None;
    }
    let name = item.tokens.clone().into_iter().next()?.to_string();
    if PRELUDE.contains(&name.as_str()) {
        Some(name)
    } else {
        None
    }
}

/// The reason to record for one refusal of type-leak. For a prelude name it says that the
/// name is relative, which is what the definer can change.
fn refusal_reason(item: &type_leak::ErrorItem, opts: &crate::options::Options) -> String {
    if let Some(name) = prelude_name(item, opts) {
        return format!("with the relative prelude name `{}`", name);
    }
    if item.message.contains("has no meaning here") {
        // `Self` in a type alias.
        return "with `Self` in a type alias".to_string();
    }
    REASON_REFUSED.to_string()
}

/// The refusals of type-leak, one error for each offending path or type, at the tokens
/// as written.
fn refusals(e: type_leak::NotInternableError, opts: &crate::options::Options) -> Unleakable {
    Unleakable(
        e.items()
            .map(|item| LeakError::new(item.tokens.clone(), refusal_reason(&item, opts)))
            .collect(),
    )
}

/// Every const generic parameter declared anywhere in `item` (item, methods,
/// generic associated types): the names that `check_values` accepts in a value
/// path. A name is enough there: what is carried is decided by type-leak.
fn const_param_names(item: &Item) -> Vec<String> {
    struct V(Vec<String>);
    impl<'ast> Visit<'ast> for V {
        fn visit_const_param(&mut self, c: &'ast syn::ConstParam) {
            self.0.push(c.ident.to_string());
            syn::visit::visit_const_param(self, c);
        }
    }
    let mut v = V(Vec::new());
    Visit::visit_item(&mut v, item);
    v.0
}

/// Analyses `item` with type-leak. `None` for items that are not analysed
/// (inline modules: the documented wrapper pattern). The error is what the table cannot
/// carry ([`Unleakable`]).
///
/// In a test build every answer is checked against the fast path
/// ([`crate::fastpath`]), which must never accept more than this does.
pub fn analyze(
    item: &Item,
    opts: &crate::options::Options,
) -> std::result::Result<Option<Analysis>, Unleakable> {
    let result = analyze_full(item, opts);
    #[cfg(test)]
    crate::fastpath::check_analysis(item, opts, &result);
    result
}

fn analyze_full(
    item: &Item,
    opts: &crate::options::Options,
) -> std::result::Result<Option<Analysis>, Unleakable> {
    if !matches!(
        item,
        Item::Struct(_) | Item::Enum(_) | Item::Union(_) | Item::Trait(_) | Item::Type(_)
    ) {
        return Ok(None);
    }
    let mut errors = check_trait_paths(item, opts.allow_std_prelude);
    errors.extend(check_values(item, &const_param_names(item)));
    if !errors.is_empty() {
        return Err(Unleakable(errors));
    }
    // Primitives (optionally), `crate::` paths and (optionally) the prelude
    // are allowed.
    let mut config = type_leak::LeakerConfig::new();
    config.allow_crate();
    // `::std` and `::alloc` are the same crate on both sides. Left optional,
    // each `::std::vec::Vec<T>` would be an entry of the table, which travels in
    // every META; allowed, only the relative types inside it are carried.
    config
        .allowed_paths
        .extend([syn::parse_quote!(::std), syn::parse_quote!(::alloc)]);
    if opts.allow_primitive {
        config.allow_primitive();
    }
    if opts.allow_std_prelude {
        config.allowed_paths.extend(
            PRELUDE
                .iter()
                .map(|n| Path::from(Ident::new(n, Span::call_site()))),
        );
    } else {
        // Bare `Sized` (`Self: Sized`, `trait T: Sized`, `T: ?Sized + Sized`) is
        // accepted without the option, like in `check_trait_paths`.
        config
            .allowed_paths
            .push(Path::from(Ident::new("Sized", Span::call_site())));
    }
    let mut leaker = type_leak::Leaker::from_config(config);
    // `Self` has no meaning in an `impl .. for ()`, and resolves everywhere:
    // `allow_self = false`.
    let analysed = match item {
        Item::Struct(s) => leaker.intern_struct(s, false),
        Item::Enum(e) => leaker.intern_enum(e, false),
        Item::Union(u) => leaker.intern_union(u, false),
        Item::Trait(t) => leaker.intern_trait(t, false),
        Item::Type(a) => leaker
            .enter_generics(&a.generics, |view| syn::visit::visit_item_type(view, a))
            .map(|(context, ())| context),
        _ => return Ok(None),
    };
    let context = analysed.map_err(|e| refusals(e, opts))?;
    // Only the required entries need a repeater; absolute paths of other
    // crates (`::other::Wrap<u8>`) are optional entries, left as written by a
    // consumer's Referrer. The order is type-leak's own, which is
    // deterministic. The same type text is one entry per context (method
    // generics with different bounds, a generic named like a type): each has
    // its own impl and its own arguments.
    let referrer = leaker.finish();
    Ok(Some(Analysis { referrer, context }))
}

/// An error of type-leak's encoders, as a recorded thing.
fn encode_error(e: &type_leak::EncodeError) -> LeakError {
    use type_leak::EncodeError;
    match e {
        EncodeError::UnsupportedConstType(ty) => LeakError::new(ty, crate::helper::NOT_PRIMITIVE),
        EncodeError::UnexpectedArgument(arg) => {
            LeakError::new(arg, "with a generic argument that cannot be encoded")
        }
        EncodeError::MissingArgument(param) => {
            LeakError::new(param, "with a generic parameter that has no argument")
        }
        EncodeError::UnboundedTraitObject(ty) => {
            LeakError::new(ty, "with a trait object that has no lifetime bound")
        }
        EncodeError::NeedsSelf(ty) => LeakError::new(ty, "with `Self` in a carried type"),
        _ => LeakError::new(
            TokenStream::new(),
            format!("with a type that cannot be encoded ({})", e),
        ),
    }
}

/// The failure of `errors`.
fn failed(errors: Vec<LeakError>) -> Result<Option<Table>> {
    Ok(Some(Table::Failed(LeakFailure::new(errors))))
}

/// Builds the table, or records why there is none ([`Table::Failed`]). It is an error only
/// for a `repeater` that is misused: on a type alias, or on an item that is not analysed.
///
/// `None` when there is nothing to carry and nothing to record.
pub fn table(item: &Item, opts: &crate::options::Options, random: u64) -> Result<Option<Table>> {
    let repeater = opts.repeater.as_ref();
    // Type aliases are not a type-leak target: their right-hand side must be
    // written with paths that resolve everywhere.
    if let Item::Type(_) = item {
        if let Some(a) = repeater {
            return Err(syn::Error::new_spanned(
                a,
                "`repeater` is not supported on type aliases; write the right-hand side with absolute paths",
            ));
        }
        return match analyze(item, opts) {
            Err(Unleakable(errors)) => failed(errors),
            Ok(None) => Ok(None),
            Ok(Some(analysis)) => {
                let errors: Vec<LeakError> = analysis
                    .required()
                    .map(|(_, ty)| LeakError::new(ty, REASON_ALIAS))
                    .collect();
                if errors.is_empty() {
                    Ok(None)
                } else {
                    failed(errors)
                }
            }
        };
    }
    let analysis = match analyze(item, opts) {
        Ok(analysis) => analysis,
        Err(Unleakable(errors)) => return failed(errors),
    };
    let (repeater, analysis) = match (repeater, analysis) {
        (Some(a), None) => {
            return Err(syn::Error::new_spanned(
                a,
                "`repeater` is supported on struct, enum, union and trait items",
            ))
        }
        (None, None) => return Ok(None),
        (None, Some(analysis)) => {
            let errors: Vec<LeakError> = analysis
                .required()
                .map(|(_, ty)| LeakError::new(ty, REASON_WITHOUT_REPEATER))
                .collect();
            return if errors.is_empty() {
                Ok(None)
            } else {
                failed(errors)
            };
        }
        (Some(a), Some(analysis)) => (a, analysis),
    };
    let id = syn::LitInt::new(&format!("{}u64", random), Span::call_site());
    let referrer = &analysis.referrer;
    // One impl per required entry, written by type-leak from the context of
    // the entry: its own generics with their bounds and where predicates, and
    // the trait arguments that encode the ones the type uses (const generics
    // through the slots of the repeater). The impl is for `()`: no entry names
    // `Self` (`allow_self = false`), so any marker will do, and `()` is what a
    // consumer's projection names.
    let slots = RepeaterSlots::new(repeater.clone());
    let marker: Type = syn::parse_quote!(());
    let mut impls = TokenStream::new();
    let mut errors = Vec::new();
    for (ix, _) in analysis.required() {
        let r = match referrer.repeater_impl_with(ix, ImplSelf::Marker(&marker), |value, param| {
            slots.encode_const(value, &param.ty)
        }) {
            Ok(r) => r,
            Err(e) => {
                errors.push(encode_error(&e));
                continue;
            }
        };
        let (impl_generics, _, where_clause) = r.generics.split_for_impl();
        let (args, self_ty, ty) = (&r.args, &r.self_ty, &r.ty);
        impls.extend(quote! {
            #[doc(hidden)]
            impl #impl_generics #repeater<#args, #id, #ix> for #self_ty #where_clause {
                type Type = #ty;
            }
        });
    }
    if !errors.is_empty() {
        return failed(errors);
    }
    let table = TypeLeak::new(repeater.clone(), id, referrer.clone(), analysis.context);
    // The consumer reads the META back and uses the same indices as the impls
    // above, so the table must survive its own serialization unchanged.
    let meta = table.meta();
    check_round_trip(&meta, referrer)?;
    Ok(Some(Table::Leaked(Leaked { impls, meta })))
}

/// The `Referrer` that a consumer parses from `meta` is the one the impls were
/// written from (same entries, same indices, same contexts).
fn check_round_trip(meta: &TokenStream, referrer: &Referrer) -> Result<()> {
    let element: crate::helper::Meta = syn::parse2(meta.clone())?;
    let read = TypeLeak::from_kv(&mut element.kv()?)?;
    if read.referrer == *referrer {
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
    use crate::options::Options;
    use template_quote::ToTokens;

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

    fn squash(ts: impl ToTokens) -> String {
        ts.to_token_stream()
            .to_string()
            .split_whitespace()
            .collect()
    }

    fn analysed(it: TokenStream, o: &Options) -> Vec<String> {
        analyze(&item(it), o)
            .unwrap()
            .unwrap()
            .required()
            .map(|(_, t)| squash(t))
            .collect()
    }

    /// What `table` records for `it`, as `(tokens, reason)`; it is an error if it builds a
    /// table or finds nothing.
    fn recorded_with(it: TokenStream, o: &Options) -> Vec<(String, String)> {
        match table(&item(it), o, 1).unwrap() {
            Some(Table::Failed(f)) => f
                .errors
                .iter()
                .map(|e| (squash(&e.tokens), e.reason.clone()))
                .collect(),
            Some(Table::Leaked(_)) => panic!("a table"),
            None => panic!("nothing recorded"),
        }
    }

    fn recorded(it: TokenStream) -> Vec<(String, String)> {
        recorded_with(it, &opts())
    }

    /// What the analysis refuses, as `(tokens, reason)`.
    fn refused_with(it: TokenStream, o: &Options) -> Vec<(String, String)> {
        analyze(&item(it), o)
            .expect_err("a refusal")
            .0
            .iter()
            .map(|e| (squash(&e.tokens), e.reason.clone()))
            .collect()
    }

    fn refused(it: TokenStream) -> Vec<(String, String)> {
        refused_with(it, &opts())
    }

    /// The reasons of `refused(it)`, without the tokens.
    fn reasons(it: TokenStream) -> Vec<String> {
        refused(it).into_iter().map(|(_, r)| r).collect()
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
        assert_eq!(
            recorded(quote! { type A = Local; }),
            [("Local".to_string(), REASON_ALIAS.to_string())]
        );
        assert_eq!(
            recorded(quote! { type B<T> = Wrap<T>; }),
            [("Wrap<T>".to_string(), REASON_ALIAS.to_string())]
        );
        // A prelude name is relative too; a type is recorded whole.
        assert_eq!(
            recorded(quote! { type C = (A, ::core::option::Option<B>, Vec<u8>); }),
            [(
                "(A,::core::option::Option<B>,Vec<u8>)".to_string(),
                REASON_ALIAS.to_string()
            )]
        );
        // There is no table for the alias.
        match table(&item(quote! { type A = Local; }), &opts(), 7).unwrap() {
            Some(Table::Failed(f)) => assert_eq!(f.errors.len(), 1),
            _ => panic!("a failure"),
        }
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
        assert!(analyze(&m, &opts()).unwrap().is_none());
        assert!(table(&m, &opts(), 1).unwrap().is_none());
        let e = table(&m, &with_repeater(), 1).err().unwrap().to_string();
        assert!(
            e.contains("`repeater` is supported on struct, enum, union and trait items"),
            "{}",
            e
        );
        let f = item(quote! { fn f(a: Local) {} });
        assert!(analyze(&f, &opts()).unwrap().is_none());
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
    fn analyze_honours_the_primitive_and_prelude_options() {
        let it = quote! { struct S { a: u8, b: Vec<u8> } };
        let strict = Options {
            allow_primitive: false,
            ..Options::default()
        };
        assert!(analysed(it.clone(), &strict).iter().any(|t| t == "u8"));
        let default = analysed(it.clone(), &opts());
        assert_eq!(default, ["Vec<u8>"]);
        let prelude = Options {
            allow_std_prelude: true,
            ..Options::default()
        };
        assert!(analysed(it, &prelude).is_empty());
    }

    #[test]
    fn types_in_associated_const_defaults_are_not_read() {
        // A known limit: type-leak does not read the
        // default of an associated const, so nothing is carried.
        let it = quote! { trait Tr { const N: usize = ::core::mem::size_of::<inner::Local>(); } };
        assert!(analysed(it, &opts()).is_empty());
    }

    #[test]
    fn trait_paths_must_be_absolute() {
        assert_eq!(
            refused(quote! { trait T: Base {} }),
            [("Base".to_string(), REASON_TRAIT_PATH.to_string())]
        );
        assert_eq!(
            refused(quote! { struct S<T: Base>(T); }),
            [("Base".to_string(), REASON_TRAIT_PATH.to_string())]
        );
        // Every relative one is recorded.
        assert_eq!(
            {
                let mut all: Vec<String> =
                    refused(quote! { trait T: Base + inner::Other<u8> where Self: Third {} })
                        .into_iter()
                        .map(|(t, _)| t)
                        .collect();
                all.sort();
                all
            },
            ["Base", "Third", "inner::Other<u8>"]
        );
        // And the definition records them.
        assert_eq!(
            recorded(quote! { trait T: Base {} }),
            [("Base".to_string(), REASON_TRAIT_PATH.to_string())]
        );
        assert!(analyze(&item(quote! { trait T: crate::Base {} }), &opts()).is_ok());
        assert!(analyze(
            &item(quote! { struct S<T: ::core::fmt::Debug>(T); }),
            &opts()
        )
        .is_ok());
        // Bare prelude traits only with `allow_std_prelude`, and then the
        // trait-path error is the one that goes away.
        let it = item(quote! { struct S<T: Clone>(T); });
        let e = analyze(&it, &opts()).err().unwrap();
        assert_eq!(e.0[0].reason, REASON_TRAIT_PATH);
        assert!(analyze(
            &it,
            &Options {
                allow_std_prelude: true,
                ..Options::default()
            }
        )
        .is_ok());
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
            let a = analyze(&item(ok.clone()), &opts());
            assert!(a.is_ok(), "{}: {}", ok, a.err().unwrap());
        }
        // The relative types next to it are carried as before.
        let impls = squash(
            &leaked(quote! {
                trait T { fn f<U: ?Sized>(&self, u: &U) -> Local<U>; fn g() -> Local2 where Self: Sized; }
            })
            .impls,
        );
        assert!(
            impls.contains("impl<U:?Sized>crate::R<(*constU,),42u64,"),
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
            let e = analyze(&item(bad.clone()), &opts()).err().unwrap();
            assert_eq!(e.0[0].reason, REASON_TRAIT_PATH, "{}", bad);
        }
        // With the option, the prelude traits are accepted, `Sized` as well.
        let prelude = Options {
            allow_std_prelude: true,
            ..Options::default()
        };
        for ok in [
            quote! { struct S<T: Clone + ?Sized>(::std::boxed::Box<T>); },
            quote! { trait T { fn f(&self) where Self: Sized; } },
            quote! { trait T: Sized {} },
        ] {
            let a = analyze(&item(ok.clone()), &prelude);
            assert!(a.is_ok(), "{}: {}", ok, a.err().unwrap());
        }
    }

    #[test]
    fn value_paths_must_be_absolute() {
        for bad in [
            quote! { struct S([u8; N]); },
            quote! { enum E { A = N } },
            quote! { struct S<const M: usize = N>; },
            quote! { struct S(crate::Buf<{ N }>); },
            quote! { trait T { const M: usize = N; } },
        ] {
            assert_eq!(
                refused(bad.clone()),
                [("N".to_string(), REASON_VALUE_PATH.to_string())],
                "{}",
                bad
            );
        }
        assert_eq!(
            recorded(quote! { struct S([u8; N]); }),
            [("N".to_string(), REASON_VALUE_PATH.to_string())]
        );
        for ok in [
            quote! { struct S([u8; crate::N]); },
            quote! { struct S<const N: usize>([u8; N]); },
            quote! { enum E { A = ::core::mem::size_of::<u8>() as isize } },
        ] {
            assert!(analyze(&item(ok), &opts()).is_ok());
        }
    }

    #[test]
    fn higher_ranked_lifetime_in_a_leaked_type_is_carried() {
        // The entry's context declares `'b`, so the impl does and the consumer's
        // projection (inside the `for<'b>`) passes it.
        let impls = squash(&leaked(quote! { struct S { f: for<'b> fn(Local<'b>) } }).impls);
        assert!(
            impls.contains("impl<'b>crate::R<(&'b(),),42u64,0usize>for(){typeType=Local<'b>;}"),
            "{}",
            impls
        );
    }

    #[test]
    fn relative_type_without_repeater_is_recorded() {
        let wr = REASON_WITHOUT_REPEATER.to_string();
        assert_eq!(
            recorded(quote! { struct S { a: Local } }),
            [("Local".to_string(), wr.clone())]
        );
        // Every type that needed a table, once each.
        assert_eq!(
            recorded(quote! { struct S { a: Local, b: Other, c: Local, d: u8 } }),
            [("Local".to_string(), wr.clone()), ("Other".to_string(), wr)]
        );
        // There is no table for the item.
        match table(&item(quote! { pub struct S { a: Local } }), &opts(), 99).unwrap() {
            Some(Table::Failed(f)) => assert_eq!(f.errors.len(), 1),
            _ => panic!("a failure"),
        }
    }

    #[test]
    fn nothing_to_carry_gives_no_table_without_repeater() {
        let it = item(quote! { struct S { a: u8, b: crate::X, c: ::std::string::String } });
        assert!(table(&it, &opts(), 1).unwrap().is_none());
    }

    /// The table of `it` with the repeater `crate::R` and the id 42.
    fn leaked(it: TokenStream) -> Leaked {
        leaked_with(&item(it), &with_repeater(), 42)
    }

    fn leaked_with(it: &Item, o: &Options, random: u64) -> Leaked {
        match table(it, o, random).unwrap().unwrap() {
            Table::Leaked(l) => l,
            Table::Failed(f) => panic!("a failure: {}", f.to_definition_error()),
        }
    }

    #[test]
    fn repeater_table_has_one_impl_per_type() {
        let it = item(quote! { struct S { a: Local, b: Other, c: Local } });
        let leaked = leaked_with(&it, &with_repeater(), 42);
        let impls = squash(&leaked.impls);
        assert_eq!(impls.matches("for()").count(), 2, "{}", impls);
        // The arguments come first: `Repeater<ARGS, ID, NTH>`.
        assert!(
            impls.contains("crate::R<(),42u64,0usize>for()"),
            "{}",
            impls
        );
        assert!(
            impls.contains("crate::R<(),42u64,1usize>for()"),
            "{}",
            impls
        );
        assert!(
            impls.contains("typeType=Local;") && impls.contains("typeType=Other;"),
            "{}",
            impls
        );
        let meta = squash(&leaked.meta);
        assert!(
            meta.starts_with(
                "type_leak(repeater=crate::R,id=42u64,context=0,referrer=[\"type-leak-format-"
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
            impls.contains("impl<T:::core::fmt::Debug>crate::R<(T,),42u64,"),
            "{}",
            impls
        );
        assert!(impls.contains("typeType=Wrap<T>;"), "{}", impls);
        assert!(!impls.contains("U"), "{}", impls);
        // Lifetimes appear as `&'a ()`, consts as `[(); N]`.
        assert!(impls.contains("crate::R<(&'a(),),42u64,"), "{}", impls);
        assert!(impls.contains("crate::R<([();N],),42u64,"), "{}", impls);
        assert!(impls.contains("impl<constN:usize>"), "{}", impls);
        assert!(impls.contains("crate::R<(),42u64,"), "{}", impls);
    }

    #[test]
    fn repeater_table_encodes_const_generics_of_every_primitive_type() {
        let leaked = leaked(quote! {
            struct S<const A: u8, const B: bool, const C: char, const D: i64, const E: u64, const F: isize>(
                inner::W<A, B, C, D, E, F>
            );
        });
        let impls = squash(&leaked.impls);
        assert!(
            impls.starts_with(
                "#[doc(hidden)]impl<constA:u8,constB:bool,constC:char,constD:i64,constE:u64,constF:isize>crate::R<("
            ),
            "{}",
            impls
        );
        // Every parameter in its own slot of the repeater; `usize` is a length.
        for slot in [
            "*constdyncrate::R<(),0,0,false,'\\0',A,Type=()>",
            "*constdyncrate::R<(),0,0,B,Type=()>",
            "*constdyncrate::R<(),0,0,false,C,Type=()>",
            "*constdyncrate::R<(),E,0,Type=()>",
        ] {
            assert!(impls.contains(slot), "{}: {}", slot, impls);
        }
        assert!(impls.contains("D,Type=()>"), "{}", impls);
        assert!(impls.contains("F,Type=()>"), "{}", impls);
    }

    #[test]
    fn repeater_table_records_an_alias_as_the_type_of_a_const_generic() {
        // An absolute alias is fine for the analysis, but cannot be encoded: there is no
        // table, and the type is recorded.
        let it = quote! { struct S<const N: crate::Idx> { a: Wrap<N> } };
        assert_eq!(
            recorded_with(it, &with_repeater()),
            [(
                "crate::Idx".to_string(),
                crate::helper::NOT_PRIMITIVE.to_string()
            )]
        );
        // Not used by a carried type: nothing to encode.
        let it = item(quote! { struct S<const N: crate::Idx> { a: Local, b: [u8; 4] } });
        leaked_with(&it, &with_repeater(), 1);
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
            impls.contains("impl<X:::core::clone::Clone>crate::R<(X,),42u64,"),
            "{}",
            impls
        );
        assert!(impls.contains("impl<X>crate::R<(X,),42u64,"), "{}", impls);
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
            impls.contains("implcrate::R<(),42u64,0usize>for(){typeType=Local;}"),
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
            impls.contains("impl<T:::core::convert::Into<V>,V>crate::R<(T,V,),42u64,")
                || impls.contains("impl<T:::core::convert::Into<V>,V>crate::R<(T,V),42u64,"),
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
            impls.contains("impl<T>crate::R<(T,),42u64,0usize>for()where T:::core::clone::Clone")
                || impls.contains("impl<T:::core::clone::Clone>crate::R<(T,),42u64,0usize>for()"),
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
            impls.contains("impl<U>crate::R<(U,),42u64,0usize>for()"),
            "{}",
            impls
        );
        assert!(
            impls.contains("crate::R<(),42u64,1usize>for()"),
            "{}",
            impls
        );
        let impls = squash(
            &leaked(quote! {
                trait K { fn f<N>(&self, x: inner::LocalG<N>); fn g<const N: usize>(&self, x: inner::Arr<N>); }
            })
            .impls,
        );
        assert!(impls.contains("impl<N>crate::R<(N,),42u64,"), "{}", impls);
        assert!(
            impls.contains("impl<constN:usize>crate::R<([();N],),42u64,"),
            "{}",
            impls
        );
    }

    #[test]
    fn repeater_without_relative_types_is_an_empty_table() {
        let it = item(quote! { struct S { a: u8 } });
        let leaked = leaked_with(&it, &with_repeater(), 3);
        assert!(leaked.impls.is_empty());
        let meta = squash(&leaked.meta);
        assert!(
            meta.starts_with("type_leak(repeater=crate::R,id=3u64,context=0,referrer=["),
            "{}",
            meta
        );
        // The consumer reads it back, with nothing to replace.
        let m: crate::helper::Meta = syn::parse2(leaked.meta).unwrap();
        let read = TypeLeak::from_kv(&mut m.kv().unwrap()).unwrap();
        assert_eq!(read.types().count(), 0);
    }

    #[test]
    fn the_meta_reads_back_with_the_indices_of_the_impls() {
        let leaked = leaked(quote! {
            struct S<T> { a: Local, b: Wrap<T>, c: ::other::X<u8> }
        });
        let m: crate::helper::Meta = syn::parse2(leaked.meta).unwrap();
        let read = TypeLeak::from_kv(&mut m.kv().unwrap()).unwrap();
        let impls = squash(&leaked.impls);
        // The impls are for the required entries; `::other::X<u8>` is optional.
        assert_eq!(read.types().count(), 2);
        for (ix, ty) in read.types() {
            let nth = format!("42u64,{}usize>for()", ix);
            assert!(impls.contains(&nth), "{}: {}", nth, impls);
            assert!(
                impls.contains(&format!("typeType={};", squash(ty))),
                "{}",
                impls
            );
        }
    }

    // --- the refusals of type-leak, as recorded things ---

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
    fn every_kind_of_refusal_is_recorded_at_its_tokens() {
        for it in [
            quote! { struct S { a: inner::G<Self> } },
            quote! { struct S(inner::G<Self::A>); },
            quote! { trait Tr { type A; fn f(&self) -> inner::G<Self::A>; } },
            quote! { struct S<T: ::core::convert::From<inner::G<T>>>(T); },
            quote! { struct S<T>(T) where T: ::core::convert::From<inner::G<T>>; },
            quote! { struct S([u8; two!()]); },
            quote! { struct S<const N: inner::Ty>; },
            quote! { trait Tr { fn f<'a>(&self, a: Wrap<'a>) -> &'a u8; } },
            quote! { struct S { a: Wrap<'_> } },
            quote! { struct S { a: super::m::Wrap<'_> } },
            quote! { struct S { a: Vec<&'_ u8> } },
            quote! { struct S<F: for<'x> ::core::ops::Fn(Rel<'x>) -> &'x u8>(F); },
            quote! { type A = Self; },
            quote! { type A = inner::G<Self>; },
            quote! { type A = inner::G<Self, '_>; },
        ] {
            let errors = refused(it.clone());
            assert!(!errors.is_empty(), "{}", it);
            for (tokens, reason) in errors {
                assert!(!tokens.is_empty(), "{}", it);
                assert!(reason.starts_with("with "), "{}: {}", it, reason);
            }
            // The definition records the same.
            assert_eq!(recorded(it.clone()).len(), refused(it).len());
        }
    }

    #[test]
    fn self_has_no_meaning_in_a_type_alias() {
        for it in [
            quote! { type A = Self; },
            quote! { type A = inner::G<Self>; },
        ] {
            let errors = refused(it);
            assert_eq!(errors.len(), 1);
            assert_eq!(errors[0].0, "Self");
            assert_eq!(errors[0].1, "with `Self` in a type alias");
        }
    }

    #[test]
    fn bound_cycles_and_other_rejections_are_recorded() {
        // A relative type that names `Self` or `Self::A` cannot be carried; on
        // its own, and inside an absolute type, `Self` is fine.
        assert_eq!(
            refused(quote! { struct S(inner::G<Self::A>); }),
            [("inner::G<Self::A>".to_string(), REASON_REFUSED.to_string())]
        );
        assert_eq!(
            refused(quote! { trait Tr { fn f(&self) -> inner::G<Self>; } }),
            [("inner::G<Self>".to_string(), REASON_REFUSED.to_string())]
        );
        assert!(analyze(
            &item(quote! { trait Tr { type Out; fn f(&self) -> Self::Out; } }),
            &opts()
        )
        .is_ok());
        assert!(analyze(&item(quote! { struct S(::other::G<Self>); }), &opts()).is_ok());
        assert!(analyze(
            &item(quote! { struct S<T: ::core::ops::Add>(inner::G<T::Output>); }),
            &opts()
        )
        .is_ok());
        // A bound that names a relative type which mentions the bounded parameter.
        assert_eq!(
            refused(quote! { struct S<T: ::core::convert::From<inner::G<T>>>(T); }),
            [("inner::G<T>".to_string(), REASON_REFUSED.to_string())]
        );
        assert!(analyze(
            &item(quote! { struct S<T: ::core::convert::From<inner::G<u8>>>(T); }),
            &opts()
        )
        .is_ok());
        // A relative macro in an array length; an absolute one is fine.
        assert_eq!(
            refused(quote! { struct S([u8; two!()]); }),
            [("two".to_string(), REASON_REFUSED.to_string())]
        );
        assert!(analyze(&item(quote! { struct S([u8; crate::two!()]); }), &opts()).is_ok());
        // The type of a const parameter.
        assert_eq!(
            refused(quote! { struct S<const N: inner::Ty>; }),
            [("inner::Ty".to_string(), REASON_REFUSED.to_string())]
        );
        assert!(analyze(&item(quote! { struct S<const N: crate::Ty>; }), &opts()).is_ok());
        // A method lifetime that only a relative type keeps.
        assert_eq!(
            reasons(quote! { trait Tr { fn f<'a>(&self, a: Wrap<'a>) -> &'a u8; } }),
            [REASON_REFUSED]
        );
    }

    #[test]
    fn a_prelude_name_is_named_in_the_reason() {
        assert_eq!(
            refused(quote! { struct S { a: Vec<&'_ u8> } }),
            [(
                "Vec<&'_u8>".to_string(),
                "with the relative prelude name `Vec`".to_string()
            )]
        );
        // A relative `Option` around `Self::Out`: the reason is about the name, not about
        // what cannot be carried.
        assert_eq!(
            reasons(quote! { trait Tr { type Out; fn f(&self) -> Option<Self::Out>; } }),
            ["with the relative prelude name `Option`"]
        );
        // Both ways out work.
        let prelude = Options {
            allow_std_prelude: true,
            ..Options::default()
        };
        let it = item(quote! { trait Tr { type Out; fn f(&self) -> Option<Self::Out>; } });
        assert!(analyze(&it, &prelude).is_ok());
        let it = item(
            quote! { trait Tr { type Out; fn f(&self) -> ::core::option::Option<Self::Out>; } },
        );
        assert!(analyze(&it, &opts()).is_ok());
        assert!(analyze(&item(quote! { struct S { a: Vec<&'_ u8> } }), &prelude).is_ok());
        // Already set: the reason is about the type that cannot be carried.
        assert_eq!(
            refused_with(quote! { struct S { a: Box<Wrap<'_>> } }, &prelude)[0].1,
            REASON_REFUSED
        );
    }

    #[test]
    fn two_offending_types_are_two_records_at_their_own_tokens() {
        let wrap = REASON_REFUSED.to_string();
        let it = quote! { struct S { a: Wrap<'_>, b: Other<'_> } };
        assert_eq!(
            refused(it.clone()),
            [
                ("Wrap<'_>".to_string(), wrap.clone()),
                ("Other<'_>".to_string(), wrap.clone())
            ]
        );
        assert_eq!(recorded(it).len(), 2);
        // Three, each with its own reason.
        let three = refused(quote! { struct S { a: Wrap<'_>, b: Other<'_>, c: Vec<&'_ u8> } });
        assert_eq!(three.len(), 3);
        assert_eq!(three[2].1, "with the relative prelude name `Vec`");
    }

    #[test]
    fn a_failed_analysis_prints_what_it_found() {
        let e = analyze(
            &item(quote! { trait T: Base + inner::Other<u8> {} }),
            &opts(),
        )
        .err()
        .unwrap();
        assert_eq!(
            e.to_string(),
            "`Base` with a relative trait path in a bound; \
             `inner::Other<u8>` with a relative trait path in a bound"
        );
    }

    #[test]
    fn other_checks_stop_before_type_leak_runs() {
        // The relative trait path is the one thing recorded: the type is not looked at.
        let errors = recorded(quote! { struct S<T: Base> { a: Local, b: T } });
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].0, "Base");
        // A repeater does not carry a trait path either.
        let errors = recorded_with(
            quote! { trait T: Base { fn f(&self) -> Local; } },
            &with_repeater(),
        );
        assert_eq!(
            errors,
            [("Base".to_string(), REASON_TRAIT_PATH.to_string())]
        );
    }

    #[test]
    fn the_hard_errors_stay() {
        // A repeater on an item kind that is not analysed.
        let e = table(&item(quote! { fn f() {} }), &with_repeater(), 1)
            .err()
            .unwrap()
            .to_string();
        assert!(e.contains("`repeater` is supported on struct"), "{}", e);
        // ... and on a type alias, relative or not.
        for it in [quote! { type A = crate::X; }, quote! { type A = Local; }] {
            let e = table(&item(it), &with_repeater(), 1)
                .err()
                .unwrap()
                .to_string();
            assert!(e.contains("not supported on type aliases"), "{}", e);
        }
    }
}
