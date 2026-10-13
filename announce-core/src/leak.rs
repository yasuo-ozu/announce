//! Type context of a definition: `#[announce(repeater = path)]` and the
//! type-leak table (DESIGN.md section 4.5).
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
//! type-leak's own errors are re-worded by [`report`]: its help and notes talk
//! about its API (`LeakerConfig`, `allow_self`, ..) and an example crate name.
//!
//! If any relative type remains, `repeater = ..` is **required** (for type
//! aliases, which are not a type-leak target, it is an error instead). The
//! repeater is the trait that `define_repeater!` (type-leak's
//! `repeater_definition()`) defines in the definer's crate. Each required
//! entry `I` of the `Referrer` then gets an impl of it, written by type-leak
//! (`Referrer::repeater_impl_with`):
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

use crate::helper::TypeLeak;
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

/// The repeater must be a local trait (`crate::..` / `$crate::..`): the table
/// implements it for `()`, which the orphan rule only allows for a local
/// trait (AUDIT 3 #7). Callers resolve it through `$crate`.
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
/// `allow_std_prelude = true`, bare prelude traits are accepted too.
fn check_trait_paths(item: &Item, allow_prelude: bool) -> Result<()> {
    struct V {
        allow_prelude: bool,
        err: Option<syn::Error>,
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
            if !(is_absolute(p) || prelude || is_bare_sized(p)) && self.err.is_none() {
                self.err = Some(syn::Error::new_spanned(
                    p,
                    "announce: trait paths must be absolute (`::core::fmt::Display`, `crate::Base`; \
                     `Sized` and `?Sized` may be bare); a repeater only covers types, not traits",
                ));
            }
            syn::visit::visit_trait_bound(self, b);
        }
    }
    let mut v = V {
        allow_prelude,
        err: None,
    };
    syn::visit::Visit::visit_item(&mut v, item);
    v.err.map_or(Ok(()), Err)
}

/// Value expressions inside types and signatures (array lengths, enum
/// discriminants, const generic arguments, const defaults) are copied to
/// callers like the types around them, but a repeater cannot carry values:
/// every path in them must be absolute, `Self::..`, or a const parameter
/// (AUDIT 3 #5). A relative array length would make type-leak carry the whole
/// array type. The types named inside a value (`size_of::<Local>()`) are
/// type-leak's business, not checked here.
fn check_values(item: &Item, const_params: &[String]) -> Result<()> {
    struct V<'a> {
        consts: &'a [String],
        err: Option<syn::Error>,
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
                    if !ok && self.0.err.is_none() {
                        self.0.err = Some(syn::Error::new_spanned(
                            path,
                            "announce: value paths in array lengths, enum discriminants, const generic \
                             arguments and const defaults must be absolute (`crate::N`, \
                             `::core::mem::size_of::<T>()`), start with `Self::` or be a const parameter; \
                             a repeater only covers types, not values",
                        ));
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
        err: None,
    };
    Visit::visit_item(&mut v, item);
    v.err.map_or(Ok(()), Err)
}

/// Names of type-leak's API that its messages use for the options a user of
/// `#[announce]` does not have.
const API_NAMES: &[&str] = &[
    "LeakerConfig",
    "allow_self",
    "allowed_paths",
    "Leaker",
    "Visitor",
    "Referrer",
    "enter_type",
    "enter_impl",
    "enter_trait",
];

fn names_api(text: &str) -> bool {
    API_NAMES.iter().any(|n| text.contains(n))
}

/// Token streams print a space between every token (`inner :: G < Self :: A >`,
/// `T : :: core :: convert :: From < G < T > >`); text with such types in it
/// printed as one writes them.
fn tidy(text: &str) -> String {
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

/// The help of type-leak without the options that name its API: "allow it
/// with `LeakerConfig::allowed_paths`", "analyze the item with
/// `allow_self = true`" (`Self` is never internable for `#[announce]`) and
/// "enter a scope that declares `Self`". They are clauses of a ", or"-list, so
/// the separator goes with them.
fn without_api_options(help: &str) -> String {
    let mut help = help.to_string();
    for clause in [
        "allow it with `LeakerConfig::allowed_paths`",
        "analyze the item with `allow_self = true`",
        "enter a scope that declares `Self` around it \
         (`Leaker::enter_type`, `enter_impl` or `enter_trait`)",
    ] {
        for pattern in [
            format!(", or {}", clause),
            format!(", {}", clause),
            format!("{}, or ", clause),
            format!("{}, ", clause),
            clause.to_string(),
        ] {
            help = help.replace(&pattern, "");
        }
    }
    help
}

/// type-leak shows the absolute form of a relative path as an example in an
/// invented crate: "(e.g. `::my_crate::inner::G`)". Inside the defining crate
/// it is `crate::inner::G`; `::other_crate::..` is for another crate.
fn crate_example(help: &str) -> String {
    const EXAMPLE: &str = "(e.g. `::my_crate::";
    let mut help = help.to_string();
    if let Some(i) = help.find(EXAMPLE) {
        let after = &help[i + EXAMPLE.len()..];
        if let Some(j) = after.find("`)") {
            let rest = &after[..j];
            help = format!(
                "{}(e.g. `crate::{rest}`, or `::other_crate::{rest}` for an item of another \
                 crate){}",
                &help[..i],
                &after[j + 2..],
                rest = rest
            );
        }
    }
    help.replace("::my_crate::", "crate::")
}

/// A note without type-leak's API in it.
fn note_text(note: &str) -> Option<String> {
    let note = note
        .replace(" (`Visitor::with_self()`)", "")
        .replace("the Leaker", "announce")
        .replace("the Referrer", "the table");
    if names_api(&note) {
        None
    } else {
        Some(tidy(&note))
    }
}

/// The help of one error, in terms of `#[announce]`.
fn help_text(item: &type_leak::ErrorItem, opts: &crate::options::Options) -> String {
    let mut help = without_api_options(item.help_text().unwrap_or_default());
    if item.message.contains("has no meaning here") {
        // `Self` in a type alias; type-leak: "or enter a scope that declares `Self`".
        help = "write the type instead of `Self`".to_string();
    } else if help.starts_with("use `") {
        // A prelude name: type-leak says which absolute path it is.
        let name = item
            .tokens
            .clone()
            .into_iter()
            .next()
            .map(|t| t.to_string());
        if name.map_or(false, |n| PRELUDE.contains(&n.as_str())) && !opts.allow_std_prelude {
            help.push_str(" (or set `allow_std_prelude = true` to accept prelude names)");
        }
    }
    let help = tidy(&crate_example(&help));
    if help.trim().is_empty() || names_api(&help) {
        format!(
            "write the paths in `{}` as absolute paths",
            tidy(&item.tokens.to_string())
        )
    } else {
        help
    }
}

/// The errors of type-leak, one `syn::Error` per offending path or type: the
/// headline, the notes that say why, and what to write instead. Their wording
/// is type-leak's, minus its API and its example crate name (see
/// [`help_text`], [`note_text`] and [`tidy`]).
fn report(e: type_leak::NotInternableError, opts: &crate::options::Options) -> syn::Error {
    let mut out: Option<syn::Error> = None;
    for item in e.items() {
        let mut message = format!("announce: {}", tidy(&item.message));
        let mut seen: Vec<String> = Vec::new();
        for note in item.notes.iter().filter_map(|(_, n)| note_text(n)) {
            if !seen.contains(&note) {
                message.push_str(&format!("\n  = note: {}", note));
                seen.push(note);
            }
        }
        message.push_str(&format!("\n  = help: {}", help_text(&item, opts)));
        let err = syn::Error::new(item.span, message);
        match &mut out {
            Some(o) => o.combine(err),
            None => out = Some(err),
        }
    }
    out.expect("a NotInternableError has at least one item")
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
/// (inline modules: the documented wrapper pattern).
///
/// In a test build every answer is checked against the fast path
/// ([`crate::fastpath`]), which must never accept more than this does.
pub fn analyze(item: &Item, opts: &crate::options::Options) -> Result<Option<Analysis>> {
    let result = analyze_full(item, opts);
    #[cfg(test)]
    crate::fastpath::check_analysis(item, opts, &result);
    result
}

fn analyze_full(item: &Item, opts: &crate::options::Options) -> Result<Option<Analysis>> {
    if !matches!(
        item,
        Item::Struct(_) | Item::Enum(_) | Item::Union(_) | Item::Trait(_) | Item::Type(_)
    ) {
        return Ok(None);
    }
    check_trait_paths(item, opts.allow_std_prelude)?;
    check_values(item, &const_param_names(item))?;
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
    let context = analysed.map_err(|e| report(e, opts))?;
    // Only the required entries need a repeater; absolute paths of other
    // crates (`::other::Wrap<u8>`) are optional entries, left as written by a
    // consumer's Referrer. The order is type-leak's own, which is
    // deterministic. The same type text is one entry per context (method
    // generics with different bounds, a generic named like a type): each has
    // its own impl and its own arguments.
    let referrer = leaker.finish();
    Ok(Some(Analysis { referrer, context }))
}

/// An error of type-leak's encoders, in announce's words.
fn encode_error(e: type_leak::EncodeError) -> syn::Error {
    match &e {
        type_leak::EncodeError::UnsupportedConstType(ty) => syn::Error::new_spanned(
            ty,
            "announce: a const generic that a carried (`repeater`) type uses must have a \
             primitive type (`bool`, `char`, `u8`..`u128`, `i8`..`i128`, `usize`, `isize`); \
             a type alias or another type cannot be encoded",
        ),
        _ => syn::Error::new(e.span(), format!("announce: {}", e)),
    }
}

/// Enforces `repeater` when it is needed, and builds the table.
pub fn table(item: &Item, opts: &crate::options::Options, random: u64) -> Result<Option<Leaked>> {
    let repeater = opts.repeater.as_ref();
    // Type aliases are not a type-leak target: their right-hand side must be
    // written with paths that resolve everywhere.
    if let Item::Type(alias) = item {
        if let Some(a) = repeater {
            return Err(syn::Error::new_spanned(
                a,
                "`repeater` is not supported on type aliases; write the right-hand side with absolute paths",
            ));
        }
        if let Some(analysis) = analyze(item, opts)? {
            if let Some((_, ty)) = analysis.required().next() {
                return Err(syn::Error::new_spanned(
                    ty,
                    format!(
                        "announce: the right-hand side of type alias `{}` must not contain relative paths; \
                         write this type as an absolute path (`crate::..`, `$crate::..` or `::other_crate::..`)",
                        alias.ident
                    ),
                ));
            }
        }
        return Ok(None);
    }
    let analysis = analyze(item, opts)?;
    let (repeater, analysis) = match (repeater, analysis) {
        (Some(a), None) => {
            return Err(syn::Error::new_spanned(
                a,
                "`repeater` is supported on struct, enum, union and trait items",
            ))
        }
        (None, None) => return Ok(None),
        (None, Some(analysis)) => {
            let n = analysis.required().count();
            let ty = match analysis.required().next() {
                None => return Ok(None),
                Some((_, t)) => t,
            };
            return Err(syn::Error::new_spanned(
                ty,
                format!(
                    "announce: this type does not resolve outside the defining crate{}; \
                     add `#[announce(repeater = crate::path::to::Repeater)]` (define the trait \
                     with `announce::define_repeater!(pub Repeater);`), \
                     or write it as an absolute path (`crate::..`, `::std::vec::Vec<..>`, `::other_crate::..`; \
                     std prelude names count as relative)",
                    if n > 1 { format!(" ({} such types)", n) } else { String::new() }
                ),
            ));
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
    for (ix, _) in analysis.required() {
        let r = referrer
            .repeater_impl_with(ix, ImplSelf::Marker(&marker), |value, param| {
                slots.encode_const(value, &param.ty)
            })
            .map_err(encode_error)?;
        let (impl_generics, _, where_clause) = r.generics.split_for_impl();
        let (args, self_ty, ty) = (&r.args, &r.self_ty, &r.ty);
        impls.extend(quote! {
            #[doc(hidden)]
            impl #impl_generics #repeater<#args, #id, #ix> for #self_ty #where_clause {
                type Type = #ty;
            }
        });
    }
    let table = TypeLeak::new(repeater.clone(), id, referrer.clone(), analysis.context);
    // The consumer reads the META back and uses the same indices as the impls
    // above, so the table must survive its own serialization unchanged.
    let meta = table.meta();
    check_round_trip(&meta, referrer)?;
    Ok(Some(Leaked { impls, meta }))
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
    fn type_alias_with_a_relative_type_is_an_error() {
        let e = table(&item(quote! { type A = Local; }), &opts(), 1)
            .err()
            .unwrap()
            .to_string();
        assert!(
            e.contains("right-hand side of type alias `A` must not contain relative paths"),
            "{}",
            e
        );
        let e = table(&item(quote! { type B<T> = Wrap<T>; }), &opts(), 1)
            .err()
            .unwrap()
            .to_string();
        assert!(e.contains("type alias `B`"), "{}", e);
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
        // ALGORITHM_FINDINGS #14, still open: type-leak does not read the
        // default of an associated const, so nothing is carried.
        let it = quote! { trait Tr { const N: usize = ::core::mem::size_of::<inner::Local>(); } };
        assert!(analysed(it, &opts()).is_empty());
    }

    #[test]
    fn trait_paths_must_be_absolute() {
        let e = analyze(&item(quote! { trait T: Base {} }), &opts())
            .err()
            .unwrap()
            .to_string();
        assert!(e.contains("trait paths must be absolute"), "{}", e);
        let e = analyze(&item(quote! { struct S<T: Base>(T); }), &opts())
            .err()
            .unwrap()
            .to_string();
        assert!(e.contains("trait paths must be absolute"), "{}", e);
        assert!(analyze(&item(quote! { trait T: crate::Base {} }), &opts()).is_ok());
        assert!(analyze(
            &item(quote! { struct S<T: ::core::fmt::Debug>(T); }),
            &opts()
        )
        .is_ok());
        // Bare prelude traits only with `allow_std_prelude`, and then the
        // trait-path error is the one that goes away.
        let it = item(quote! { struct S<T: Clone>(T); });
        let e = analyze(&it, &opts()).err().unwrap().to_string();
        assert!(e.contains("trait paths must be absolute"), "{}", e);
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
        // ALGORITHM_FINDINGS #12: `Sized` cannot be meaningfully shadowed.
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
            let e = analyze(&item(bad.clone()), &opts())
                .err()
                .unwrap()
                .to_string();
            assert!(e.contains("trait paths must be absolute"), "{}: {}", bad, e);
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
            let e = analyze(&item(bad.clone()), &opts())
                .err()
                .unwrap()
                .to_string();
            assert!(
                e.contains(
                    "value paths in array lengths, enum discriminants, const generic arguments"
                ),
                "{}: {}",
                bad,
                e
            );
            assert!(!e.contains("trait paths"), "{}", e);
        }
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
        // projection (inside the `for<'b>`) passes it (ALGORITHM_FINDINGS #7).
        let impls = squash(&leaked(quote! { struct S { f: for<'b> fn(Local<'b>) } }).impls);
        assert!(
            impls.contains("impl<'b>crate::R<(&'b(),),42u64,0usize>for(){typeType=Local<'b>;}"),
            "{}",
            impls
        );
    }

    #[test]
    fn relative_type_without_repeater_is_an_error() {
        let e = table(&item(quote! { struct S { a: Local } }), &opts(), 1)
            .err()
            .unwrap()
            .to_string();
        assert!(
            e.contains("does not resolve outside the defining crate"),
            "{}",
            e
        );
        assert!(e.contains("repeater = crate::path::to::Repeater"), "{}", e);
        assert!(
            e.contains("announce::define_repeater!(pub Repeater);"),
            "{}",
            e
        );
        assert!(!e.contains("such types"), "{}", e);
        let e = table(
            &item(quote! { struct S { a: Local, b: Other } }),
            &opts(),
            1,
        )
        .err()
        .unwrap()
        .to_string();
        assert!(e.contains("(2 such types)"), "{}", e);
    }

    #[test]
    fn nothing_to_carry_gives_no_table_without_repeater() {
        let it = item(quote! { struct S { a: u8, b: crate::X, c: ::std::string::String } });
        assert!(table(&it, &opts(), 1).unwrap().is_none());
    }

    /// The table of `it` with the repeater `crate::R` and the id 42.
    fn leaked(it: TokenStream) -> Leaked {
        table(&item(it), &with_repeater(), 42).unwrap().unwrap()
    }

    #[test]
    fn repeater_table_has_one_impl_per_type() {
        let it = item(quote! { struct S { a: Local, b: Other, c: Local } });
        let leaked = table(&it, &with_repeater(), 42).unwrap().unwrap();
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
    fn repeater_table_rejects_an_alias_as_the_type_of_a_const_generic() {
        // An absolute alias is fine for the analysis, but cannot be encoded.
        let it = item(quote! { struct S<const N: crate::Idx> { a: Wrap<N> } });
        let e = table(&it, &with_repeater(), 1).err().map(|e| e.to_string());
        assert!(
            e.map_or(false, |e| e.contains("must have a primitive type")),
            "an error"
        );
        // Not used by a carried type: nothing to encode.
        let it = item(quote! { struct S<const N: crate::Idx> { a: Local, b: [u8; 4] } });
        assert!(table(&it, &with_repeater(), 1).is_ok());
    }

    #[test]
    fn same_text_in_two_contexts_is_two_entries_with_their_own_generics() {
        // ALGORITHM_FINDINGS A: the bound of `X` in `f` must not reach `g`.
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
        // ALGORITHM_FINDINGS A: `Local` is a generic only inside `f`.
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
        // ALGORITHM_FINDINGS #1: `T: Into<V>` needs `V`, which comes with it.
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
        // ALGORITHM_FINDINGS #2.
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
        // ALGORITHM_FINDINGS #3 and #4.
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
        let leaked = table(&it, &with_repeater(), 3).unwrap().unwrap();
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

    // --- report(): type-leak's errors in announce's words ---

    fn errors_of(it: TokenStream, o: &Options) -> Vec<String> {
        analyze(&item(it), o)
            .expect_err("an error")
            .into_iter()
            .map(|e| e.to_string())
            .collect()
    }

    fn first_error(it: TokenStream) -> String {
        errors_of(it, &opts()).remove(0)
    }

    /// Nothing of type-leak's API or example crate name, and no token spacing.
    fn assert_announce_wording(e: &str) {
        assert!(!names_api(e), "type-leak API in: {}", e);
        assert!(!e.contains("my_crate"), "example crate in: {}", e);
        for spaced in [" :: ", " < ", " > ", "< '", ":: ", "& '"] {
            assert!(!e.contains(spaced), "spaced tokens `{}` in: {}", spaced, e);
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
    fn api_options_are_removed_from_the_help() {
        let allow = "allow it with `LeakerConfig::allowed_paths`";
        let self_ = "analyze the item with `allow_self = true`";
        let scope = "enter a scope that declares `Self` around it \
                     (`Leaker::enter_type`, `enter_impl` or `enter_trait`)";
        for (given, want) in [
            // Last, middle, and the only other option.
            (format!("write `X`, or {}", allow), "write `X`"),
            (format!("write `X`, {}, or {}", allow, self_), "write `X`"),
            (format!("write `X`, or {}", self_), "write `X`"),
            (
                format!("write `X`, {}, or name it", self_),
                "write `X`, or name it",
            ),
            (format!("write the type, or {}", scope), "write the type"),
            ("write `X`".to_string(), "write `X`"),
        ] {
            let got = without_api_options(&given);
            assert_eq!(got, want, "{}", given);
            assert!(!names_api(&got), "{}", got);
        }
    }

    #[test]
    fn the_example_crate_becomes_the_defining_crate() {
        assert_eq!(
            crate_example("write `inner::G` with an absolute path (e.g. `::my_crate::inner::G`)"),
            "write `inner::G` with an absolute path (e.g. `crate::inner::G`, or \
             `::other_crate::inner::G` for an item of another crate)"
        );
        // The rest of the help stays, other mentions change.
        assert_eq!(
            crate_example("write `Tr` (e.g. `::my_crate::Tr`), or name `<Self as ::my_crate::Tr>`"),
            "write `Tr` (e.g. `crate::Tr`, or `::other_crate::Tr` for an item of another \
             crate), or name `<Self as crate::Tr>`"
        );
        assert_eq!(
            crate_example("use `::std::vec::Vec` instead of `Vec`"),
            "use `::std::vec::Vec` instead of `Vec`"
        );
    }

    #[test]
    fn the_help_does_not_name_type_leak() {
        // The help of type-leak for this item names `LeakerConfig`, `allow_self`
        // and `::my_crate`: that is what `report` has to re-word.
        let it: syn::ItemStruct = syn::parse_quote!(
            struct S {
                a: inner::G<Self>,
            }
        );
        let raw = type_leak::Leaker::new()
            .intern_struct(&it, false)
            .unwrap_err()
            .help()
            .unwrap();
        assert!(raw.contains("allow_self"), "{}", raw);
        assert!(raw.contains("LeakerConfig"), "{}", raw);
        assert!(raw.contains("::my_crate::inner::G"), "{}", raw);
        let e = first_error(quote! { struct S { a: inner::G<Self> } });
        assert_announce_wording(&e);
        assert!(
            e.contains("= help: write `inner::G` with an absolute path (e.g. `crate::inner::G`, or `::other_crate::inner::G` for an item of another crate)"),
            "{}",
            e
        );
        assert!(e.contains("use absolute path: 'inner::G<Self>'"), "{}", e);
    }

    #[test]
    fn every_kind_of_error_is_in_announce_wording() {
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
            for e in errors_of(it.clone(), &opts()) {
                assert_announce_wording(&e);
                assert!(e.starts_with("announce: "), "{}", e);
                assert!(e.contains("\n  = help: "), "{}: {}", it, e);
            }
        }
    }

    #[test]
    fn self_has_no_meaning_in_a_type_alias() {
        for it in [
            quote! { type A = Self; },
            quote! { type A = inner::G<Self>; },
        ] {
            let e = first_error(it);
            assert!(e.contains("announce: `Self` has no meaning here"), "{}", e);
            assert!(
                e.ends_with("= help: write the type instead of `Self`"),
                "{}",
                e
            );
        }
    }

    #[test]
    fn bound_cycles_and_other_rejections_are_definer_errors() {
        // A relative type that names `Self` or `Self::A` cannot be carried; on
        // its own, and inside an absolute type, `Self` is fine.
        assert!(
            first_error(quote! { struct S(inner::G<Self::A>); }).contains(
                "write `<Self as SomeTrait>::A` instead of `Self::A` in `inner::G<Self::A>`"
            )
        );
        assert!(
            first_error(quote! { trait Tr { fn f(&self) -> inner::G<Self>; } })
                .contains("use absolute path: 'inner::G<Self>'")
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
        assert!(
            first_error(quote! { struct S<T: ::core::convert::From<inner::G<T>>>(T); }).contains(
                "is in a bound or where predicate, and a bound of its generics names a leaked type"
            )
        );
        assert!(analyze(
            &item(quote! { struct S<T: ::core::convert::From<inner::G<u8>>>(T); }),
            &opts()
        )
        .is_ok());
        // A relative macro in an array length; an absolute one is fine.
        assert!(
            first_error(quote! { struct S([u8; two!()]); }).contains("use absolute path: 'two'")
        );
        assert!(analyze(&item(quote! { struct S([u8; crate::two!()]); }), &opts()).is_ok());
        // The type of a const parameter.
        assert!(first_error(quote! { struct S<const N: inner::Ty>; })
            .contains("use absolute path: 'inner::Ty'"));
        assert!(analyze(&item(quote! { struct S<const N: crate::Ty>; }), &opts()).is_ok());
        // A method lifetime that only a relative type keeps.
        assert!(
            first_error(quote! { trait Tr { fn f<'a>(&self, a: Wrap<'a>) -> &'a u8; } }).contains(
                "leaves the lifetime `'a` of the method unconstrained in the copy (E0195)"
            )
        );
    }

    #[test]
    fn a_prelude_name_gets_the_prelude_option() {
        let e = first_error(quote! { struct S { a: Vec<&'_ u8> } });
        assert!(
            e.ends_with("= help: use `::std::vec::Vec` instead of `Vec` (or set `allow_std_prelude = true` to accept prelude names)"),
            "{}",
            e
        );
        // The option does what the help says.
        let prelude = Options {
            allow_std_prelude: true,
            ..Options::default()
        };
        assert!(analyze(&item(quote! { struct S { a: Vec<&'_ u8> } }), &prelude).is_ok());
        // Already set: type-leak's own help stays.
        let e = errors_of(quote! { struct S { a: Box<Wrap<'_>> } }, &prelude).remove(0);
        assert!(e.contains("use absolute path: 'Wrap<'_>'"), "{}", e);
    }

    #[test]
    fn two_offending_types_are_two_errors_at_their_own_spans() {
        let it = item(quote! { struct S { a: Wrap<'_>, b: Other<'_> } });
        let err = analyze(&it, &opts()).err().unwrap();
        let all: Vec<syn::Error> = err.into_iter().collect();
        assert_eq!(all.len(), 2);
        let (a, b) = (all[0].to_string(), all[1].to_string());
        assert!(a.contains("use absolute path: 'Wrap<'_>'"), "{}", a);
        assert!(a.contains("`Wrap` with an absolute path"), "{}", a);
        assert!(b.contains("use absolute path: 'Other<'_>'"), "{}", b);
        assert!(b.contains("`Other` with an absolute path"), "{}", b);
        assert_announce_wording(&a);
        assert_announce_wording(&b);
        // `table` reports them the same way.
        let n = table(&it, &opts(), 1).err().unwrap().into_iter().count();
        assert_eq!(n, 2);
        // Three, each with its own help.
        let n = errors_of(
            quote! { struct S { a: Wrap<'_>, b: Other<'_>, c: Vec<&'_ u8> } },
            &opts(),
        );
        assert_eq!(n.len(), 3);
        assert!(n[2].contains("`::std::vec::Vec`"), "{}", n[2]);
    }

    #[test]
    fn notes_repeat_once() {
        let e = first_error(quote! { struct S(inner::G<Self::A>); });
        let note = "= note: it cannot be leaked as a whole because of `Self::A`";
        assert_eq!(e.matches(note).count(), 1, "{}", e);
    }
}
