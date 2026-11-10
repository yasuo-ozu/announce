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
//! - trait paths (supertraits, bounds, `impl` bounds): absolute only (a trait
//!   object is a type, carried whole),
//! - value paths in enum discriminants, const defaults and the const arguments
//!   of a bound: absolute, `Self::..` or a const parameter in scope (a
//!   repeater only carries types; the types named inside such an expression,
//!   as in `size_of::<Local>()`, are carried like any other: type-leak reads
//!   array lengths, const arguments, discriminants and the defaults of const
//!   parameters, not the defaults of associated consts). A value inside a type
//!   (`[u8; N]`, `Wrap<{ N }>`) goes with it.
//!
//! type-leak's own errors are re-worded by [`refusals`]: its help and notes talk
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
//!
//! What a table cannot carry is a failure ([`Table::Failed`]), recorded with
//! every cause of the item ([`failures`]); misused options stay errors.

use crate::helper::{
    LeakError, LeakFailure, TypeLeak, ALIAS, NOT_PRIMITIVE, REFUSED, TRAIT_PATH, VALUE_PATH,
    WITHOUT_REPEATER,
};
use proc_macro2::{Ident, Span, TokenStream, TokenTree};
use std::collections::{HashMap, HashSet};
use syn::visit::Visit;
use syn::visit_mut::VisitMut;
use syn::{Expr, Item, Path, Result, Type};
use template_quote::{quote, ToTokens};
use type_leak::{ContextId, ImplSelf, Referrer, RepeaterSlots};

pub struct Leaked {
    /// Impls emitted at the definition site.
    pub impls: TokenStream,
    /// `type_leak(repeater = .., id = .., context = .., referrer = [..])`.
    pub meta: TokenStream,
}

/// What [`table`] makes of an item that is analysed.
pub enum Table {
    /// The impls and the `type_leak(..)` META of an item with a `repeater`.
    Leaked(Leaked),
    /// Something in the item does not resolve outside its module: no table, and the
    /// META is `type_leak_error(..)`.
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

/// Trait paths (supertraits, generic and where-clause bounds, `impl` bounds)
/// must be absolute: a repeater only carries *types*, so a relative trait path
/// could never resolve at a caller (decided 2026-10-01). A trait object is a
/// type, carried whole. Bare `Sized` and `?Sized` are accepted
/// ([`is_bare_sized`]); with `allow_std_prelude = true`, bare prelude traits are
/// accepted too.
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
                self.errors.push(LeakError::new(p, TRAIT_PATH));
            }
            syn::visit::visit_trait_bound(self, b);
        }
        fn visit_type_trait_object(&mut self, _: &'ast syn::TypeTraitObject) {}
    }
    let mut v = V {
        allow_prelude,
        errors: Vec::new(),
    };
    syn::visit::Visit::visit_item(&mut v, item);
    v.errors
}

/// Value expressions outside types (enum discriminants, const defaults, const
/// arguments of a bound) are copied to callers like the types around them, but a
/// repeater cannot carry values: every path in them must be absolute, `Self::..`,
/// or a const parameter in scope: one of the item, or one of the method or the
/// generic associated type that the value is in. A value inside a type (`[u8; N]`)
/// is carried with it. The types named inside a value (`size_of::<Local>()`) are
/// type-leak's business, not checked here.
fn check_values(item: &Item) -> Vec<LeakError> {
    struct V {
        consts: Vec<String>,
        errors: Vec<LeakError>,
    }
    impl V {
        fn check(&mut self, e: &syn::Expr) {
            struct P<'b>(&'b mut V);
            impl<'ast> Visit<'ast> for P<'_> {
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
                        self.0.errors.push(LeakError::new(path, VALUE_PATH));
                    }
                    syn::visit::visit_expr_path(self, p);
                }
                fn visit_type(&mut self, _: &'ast Type) {}
            }
            Visit::visit_expr(&mut P(self), e);
        }

        /// Runs `f` with the const parameters of `g` in scope.
        fn scoped(&mut self, g: &syn::Generics, f: impl FnOnce(&mut Self)) {
            let outer = self.consts.len();
            self.consts
                .extend(g.const_params().map(|c| c.ident.to_string()));
            f(self);
            self.consts.truncate(outer);
        }
    }
    impl<'ast> Visit<'ast> for V {
        fn visit_type(&mut self, _: &'ast Type) {}
        fn visit_variant(&mut self, v: &'ast syn::Variant) {
            if let Some((_, e)) = &v.discriminant {
                self.check(e);
            }
            syn::visit::visit_variant(self, v);
        }
        fn visit_trait_item_const(&mut self, c: &'ast syn::TraitItemConst) {
            self.scoped(&c.generics, |s| {
                if let Some((_, e)) = &c.default {
                    s.check(e);
                }
                syn::visit::visit_trait_item_const(s, c);
            });
        }
        fn visit_trait_item_fn(&mut self, f: &'ast syn::TraitItemFn) {
            self.scoped(&f.sig.generics, |s| syn::visit::visit_trait_item_fn(s, f));
        }
        fn visit_trait_item_type(&mut self, t: &'ast syn::TraitItemType) {
            self.scoped(&t.generics, |s| syn::visit::visit_trait_item_type(s, t));
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
    let generics = match item {
        Item::Struct(i) => &i.generics,
        Item::Enum(i) => &i.generics,
        Item::Union(i) => &i.generics,
        Item::Trait(i) => &i.generics,
        Item::Type(i) => &i.generics,
        _ => return Vec::new(),
    };
    let mut v = V {
        consts: Vec::new(),
        errors: Vec::new(),
    };
    v.scoped(generics, |v| Visit::visit_item(v, item));
    v.errors
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

/// The std prelude name that heads the offending type, when it is relative here
/// (`allow_std_prelude` is off). type-leak then names the whole type, says why it
/// cannot be leaked (`Self::Out`, `'_`, ..) and shows the absolute path of the
/// prelude name: "use `::core::option::Option` instead of `Option`". The reason
/// to act on is the relative name, which this finds.
fn prelude_name(item: &type_leak::ErrorItem, opts: &crate::options::Options) -> Option<String> {
    if opts.allow_std_prelude || !item.help_text()?.starts_with("use `") {
        return None;
    }
    let name = crate::helper::cause_path(&item.tokens)?.path;
    if PRELUDE.contains(&name.as_str()) {
        Some(name)
    } else {
        None
    }
}

/// The start of the note of type-leak that names what stops a relative type from being
/// leaked: "it cannot be leaked as a whole because of `Self::Out`".
const WHOLE_NOTE: &str = "it cannot be leaked as a whole because of ";

/// The help of one error, in terms of `#[announce]`.
fn help_text(item: &type_leak::ErrorItem, opts: &crate::options::Options) -> String {
    let mut help = without_api_options(item.help_text().unwrap_or_default());
    if item.message.contains("has no meaning here") {
        // `Self` in a type alias; type-leak: "or enter a scope that declares `Self`".
        help = "write the type instead of `Self`".to_string();
    } else if let Some(name) = prelude_name(item, opts) {
        // type-leak: "use `::core::option::Option` instead of `Option`".
        if let Some(absolute) = help.split('`').nth(1) {
            let args = match crate::helper::cause_path(&item.tokens) {
                Some(c) if c.generic => "<..>",
                _ => "",
            };
            return format!(
                "write `{}{}` instead of `{}{}`, or set `allow_std_prelude = true` to accept \
                 prelude names",
                absolute, args, name, args
            );
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

/// The errors of type-leak, one [`LeakError`] per offending path or type: the headline,
/// the notes that say why, and what to write instead. Their wording is type-leak's, minus
/// its API and its example crate name (see [`help_text`], [`note_text`] and [`tidy`]). A
/// relative prelude name is recorded as that ([`prelude_name`]).
fn refusals(e: type_leak::NotInternableError, opts: &crate::options::Options) -> Vec<LeakError> {
    let mut out = Vec::new();
    for item in e.items() {
        let mut seen: Vec<String> = Vec::new();
        let prelude = prelude_name(&item, opts);
        for note in item.notes.iter().filter_map(|(_, n)| note_text(n)) {
            // The headline says that the name is relative: say what that costs, not
            // that the type "cannot be leaked".
            let note = match note.strip_prefix(WHOLE_NOTE) {
                Some(cause) if prelude.is_some() => {
                    format!("a relative type cannot be carried with {} in it", cause)
                }
                _ => note,
            };
            if !seen.contains(&note) {
                seen.push(note);
            }
        }
        let reason = match prelude {
            Some(name) => format!("with the relative prelude name `{}`", name),
            None if item.message.contains("has no meaning here") => {
                "with `Self` in a type alias".to_string()
            }
            None => REFUSED.to_string(),
        };
        out.push(
            LeakError::new(&item.tokens, reason)
                .with_message(tidy(&item.message))
                .with_notes(seen)
                .with_help(help_text(&item, opts)),
        );
    }
    out
}

/// The analysis of an item, or every cause that no table can carry.
pub type Analysed = std::result::Result<Option<Analysis>, Vec<LeakError>>;

/// Analyses `item` with type-leak. `None` for items that are not analysed
/// (inline modules: the documented wrapper pattern).
///
/// In a test build every answer is checked against the fast path
/// ([`crate::fastpath`]), which must never accept more than this does.
pub fn analyze(item: &Item, opts: &crate::options::Options) -> Analysed {
    let result = analyze_full(item, opts);
    #[cfg(test)]
    crate::fastpath::check_analysis(item, opts, &result);
    result
}

fn analyze_full(item: &Item, opts: &crate::options::Options) -> Analysed {
    if !matches!(
        item,
        Item::Struct(_) | Item::Enum(_) | Item::Union(_) | Item::Trait(_) | Item::Type(_)
    ) {
        return Ok(None);
    }
    let mut own = check_trait_paths(item, opts.allow_std_prelude);
    own.extend(check_values(item));
    if own.is_empty() {
        if let Ok(analysis) = intern(item, opts) {
            return Ok(Some(analysis));
        }
    }
    Err(failures(item, opts, own))
}

/// Runs type-leak over `item`, which `analyze_full` has found to be analysed.
fn intern(
    item: &Item,
    opts: &crate::options::Options,
) -> std::result::Result<Analysis, type_leak::NotInternableError> {
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
    let context = match item {
        Item::Struct(s) => leaker.intern_struct(s, false),
        Item::Enum(e) => leaker.intern_enum(e, false),
        Item::Union(u) => leaker.intern_union(u, false),
        Item::Trait(t) => leaker.intern_trait(t, false),
        Item::Type(a) => leaker
            .enter_generics(&a.generics, |view| syn::visit::visit_item_type(view, a))
            .map(|(context, ())| context),
        _ => unreachable!("`analyze_full` passes the analysed items only"),
    }?;
    // Only the required entries need a repeater; absolute paths of other
    // crates (`::other::Wrap<u8>`) are optional entries, left as written by a
    // consumer's Referrer. The order is type-leak's own, which is
    // deterministic. The same type text is one entry per context (method
    // generics with different bounds, a generic named like a type): each has
    // its own impl and its own arguments.
    let referrer = leaker.finish();
    Ok(Analysis { referrer, context })
}

/// The tokens as one string, to match nodes by what they say.
fn text(t: &impl ToTokens) -> String {
    t.to_token_stream().to_string()
}

/// Where a token is written, as text. A token with the span of the call has none: it is not
/// written in the item, but made (type-leak says `Self` with it for the tokens of a macro), and
/// is matched by what it says. `Span` has no equality, but its `Debug` is the position in a proc
/// macro. Where the spans are all alike (a stream that `quote!` made, a test build) no token has
/// a position, and the text alone tells nodes apart.
fn spot(t: &TokenTree, call: &str) -> String {
    let at = format!("{:?}", t.span());
    if at == call {
        String::new()
    } else {
        at
    }
}

/// `text`, led by where the first and the last token are written, to match a node by what it
/// says *and* where: the same type in two places is two nodes.
fn place(t: &impl ToTokens) -> String {
    let tokens = t.to_token_stream();
    let call = format!("{:?}", Span::call_site());
    let end = |t: Option<TokenTree>| t.map_or_else(String::new, |t| spot(&t, &call));
    let (first, last) = (
        tokens.clone().into_iter().next(),
        tokens.clone().into_iter().last(),
    );
    format!("{} {} {}", end(first), end(last), tokens)
}

/// The tokens of `ts` one by one, groups opened.
fn leaves(ts: TokenStream) -> Vec<TokenTree> {
    ts.into_iter()
        .flat_map(|t| match t {
            TokenTree::Group(g) => leaves(g.stream()),
            t => vec![t],
        })
        .collect()
}

/// Where the tokens of `needle` are in a row in `hay`, by text and place ([`spot`]).
fn find_tokens(hay: &[TokenTree], needle: &[TokenTree]) -> Option<usize> {
    let call = format!("{:?}", Span::call_site());
    let same = |w: &[TokenTree]| {
        w.iter().zip(needle).all(|(a, b)| {
            let at = spot(b, &call);
            a.to_string() == b.to_string() && (at.is_empty() || at == spot(a, &call))
        })
    };
    match needle.len() {
        0 => None,
        n => hay.windows(n).position(same),
    }
}

/// Rewrites what is an error already into what type-leak accepts, so that it goes on to
/// find the rest: the trait and value paths that announce's checks found, and what
/// type-leak refused. Nodes are matched by their text and place ([`place`]). A type or a
/// value is replaced as a whole; where only a path in it is refused (the bound of a trait
/// object, the trait of `<Self as Tr>::X`, a macro) the smallest type or value around it is.
/// `opaque` also looks for the tokens of a refusal in those of a macro.
#[derive(Default, Clone)]
struct Neutralise {
    /// The relative trait and value paths that announce's checks found, by [`place`].
    own: HashSet<String>,
    /// What type-leak refused, by [`place`].
    types: HashSet<String>,
    opaque: Vec<TokenStream>,
    replaced: usize,
    /// Refused paths seen and not yet covered by what holds them.
    paths: usize,
    in_dyn: usize,
    in_type: usize,
}

impl Neutralise {
    fn refused(&self, node: &impl ToTokens) -> bool {
        !self.types.is_empty() && self.types.contains(&place(node))
    }

    /// Replaces `node` by `with()` when it is `whole` refused or holds a refused path;
    /// else visits it.
    fn cover<T>(
        &mut self,
        node: &mut T,
        whole: bool,
        visit: fn(&mut Self, &mut T),
        with: fn() -> T,
    ) {
        let paths = self.paths;
        if !whole {
            visit(self, node);
        }
        if whole || self.paths > paths {
            self.paths = paths;
            self.replaced += 1;
            *node = with();
        }
    }
}

impl VisitMut for Neutralise {
    fn visit_trait_bound_mut(&mut self, b: &mut syn::TraitBound) {
        if self.in_dyn == 0 && self.own.contains(&place(&b.path)) {
            // Absolute; the arguments stay, type-leak reads the types in them.
            let last = b.path.segments.last().cloned();
            b.path = Path {
                leading_colon: Some(Default::default()),
                segments: ["core", "marker"]
                    .iter()
                    .map(|n| Ident::new(n, Span::call_site()).into())
                    .chain(last)
                    .collect(),
            };
            self.replaced += 1;
        }
        syn::visit_mut::visit_trait_bound_mut(self, b);
    }

    fn visit_path_mut(&mut self, p: &mut Path) {
        self.paths += usize::from(self.refused(p));
        syn::visit_mut::visit_path_mut(self, p);
    }

    fn visit_macro_mut(&mut self, m: &mut syn::Macro) {
        let inside = leaves(m.tokens.clone());
        let held = |n: &TokenStream| find_tokens(&inside, &leaves(n.clone())).is_some();
        self.paths += usize::from(self.opaque.iter().any(held));
        syn::visit_mut::visit_macro_mut(self, m);
    }

    fn visit_expr_mut(&mut self, e: &mut Expr) {
        let whole = self.refused(e)
            || matches!(&*e, Expr::Path(p) if self.in_type == 0
                && p.qself.is_none()
                && self.own.contains(&place(&p.path)));
        self.cover(e, whole, syn::visit_mut::visit_expr_mut, || {
            syn::parse_quote!(0)
        });
    }

    fn visit_type_mut(&mut self, ty: &mut Type) {
        let whole = self.refused(ty);
        self.in_type += 1;
        self.cover(ty, whole, syn::visit_mut::visit_type_mut, || {
            syn::parse_quote!(::core::primitive::u8)
        });
        self.in_type -= 1;
    }

    fn visit_type_trait_object_mut(&mut self, t: &mut syn::TypeTraitObject) {
        self.in_dyn += 1;
        syn::visit_mut::visit_type_trait_object_mut(self, t);
        self.in_dyn -= 1;
    }

    // Neither type-leak nor `check_values` reads the bodies of provided methods.
    fn visit_trait_item_fn_mut(&mut self, f: &mut syn::TraitItemFn) {
        f.attrs.iter_mut().for_each(|a| self.visit_attribute_mut(a));
        self.visit_signature_mut(&mut f.sig);
    }

    // type-leak reads the values in an attribute (`#[name = Value]`); the checks do not.
    fn visit_attribute_mut(&mut self, a: &mut syn::Attribute) {
        self.in_dyn += 1;
        self.in_type += 1;
        syn::visit_mut::visit_attribute_mut(self, a);
        self.in_dyn -= 1;
        self.in_type -= 1;
    }
}

/// The types and paths of an item by their text and place after `fixes` (what type-leak saw
/// of them), to report an error found in the rewritten item at the tokens as written. The
/// tokens of an error that holds nothing replaced are their own place already.
struct Originals {
    fixes: Vec<Neutralise>,
    found: HashMap<String, TokenStream>,
}

impl Originals {
    fn note<T: Clone + ToTokens>(&mut self, node: &T, rewrite: fn(&mut Neutralise, &mut T)) {
        for fix in &mut self.fixes {
            let mut seen = node.clone();
            rewrite(fix, &mut seen);
            self.found
                .entry(place(&seen))
                .or_insert_with(|| node.to_token_stream());
        }
    }
}

impl<'ast> Visit<'ast> for Originals {
    fn visit_type(&mut self, ty: &'ast Type) {
        // Not the node itself, which an error may be about.
        self.note(ty, syn::visit_mut::visit_type_mut);
        syn::visit::visit_type(self, ty);
    }

    fn visit_path(&mut self, p: &'ast Path) {
        self.note(p, syn::visit_mut::visit_path_mut);
        syn::visit::visit_path(self, p);
    }

    // A refusal looked for in the tokens of a macro is those tokens.
    fn visit_macro(&mut self, m: &'ast syn::Macro) {
        let inside = leaves(m.tokens.clone());
        for needle in self.fixes.iter().flat_map(|fix| &fix.opaque) {
            let tokens = leaves(needle.clone());
            if let Some(at) = find_tokens(&inside, &tokens) {
                self.found
                    .entry(place(needle))
                    .or_insert_with(|| inside[at..at + tokens.len()].iter().cloned().collect());
            }
        }
        syn::visit::visit_macro(self, m);
    }

    fn visit_trait_item_fn(&mut self, f: &'ast syn::TraitItemFn) {
        self.visit_signature(&f.sig);
    }

    fn visit_attribute(&mut self, _: &'ast syn::Attribute) {}
}

/// `error` at the tokens `at`, which its texts quote.
fn moved_to(mut error: LeakError, at: TokenStream) -> LeakError {
    let (from, to) = (tidy(&text(&error.tokens)), tidy(&text(&at)));
    if from != to {
        let swap = |s: &str| s.replace(&from, &to);
        error.message = error.message.as_deref().map(swap);
        error.notes = error.notes.iter().map(|n| swap(n)).collect();
        error.help = error.help.as_deref().map(swap);
    }
    error.tokens = at;
    error
}

/// The most passes of [`failures`]; the bound is for a rewrite that does not hold.
const PASSES: usize = 32;

/// Every cause in an item that cannot be carried, at once. `own` is what announce's checks
/// found.
///
/// type-leak reports every refusal of a pass, but a relative type only shows in a pass that
/// succeeds, and so does what the encoders say of the entries ([`repeater_impls`]). So what
/// is an error is replaced by what type-leak accepts ([`Neutralise`]) in a copy of the item,
/// the paths of `own` first and the refused tokens after each pass, until a pass succeeds. A
/// pass that rewrites nothing looks for the refusals in the tokens of macros; one that still
/// rewrites nothing is the end. Errors are at the tokens of `item`.
fn failures(item: &Item, opts: &crate::options::Options, own: Vec<LeakError>) -> Vec<LeakError> {
    let mut fix = Neutralise::default();
    fix.own.extend(own.iter().map(|e| place(&e.tokens)));
    let mut work = item.clone();
    fix.visit_item_mut(&mut work);
    let mut fixes = vec![fix.clone()];
    let mut refused = Vec::new();
    let mut required = Vec::new();
    let mut encoded = Vec::new();
    for _ in 0..PASSES {
        let found = match intern(&work, opts) {
            Ok(analysis) => {
                required = analysis.required().map(|(_, t)| t.clone()).collect();
                // The table of a repeater is written for the entries: what stops that is a
                // cause too.
                if let (Some(repeater), false) = (&opts.repeater, matches!(item, Item::Type(_))) {
                    encoded = repeater_impls(&analysis, repeater).1;
                }
                break;
            }
            Err(e) => refusals(e, opts),
        };
        let opaque: Vec<_> = found.iter().map(|r| r.tokens.clone()).collect();
        for r in found {
            if fix.types.insert(place(&r.tokens)) {
                refused.push(r);
            }
        }
        fix.replaced = 0;
        fix.visit_item_mut(&mut work);
        if fix.replaced == 0 {
            fix.opaque = opaque;
            fix.visit_item_mut(&mut work);
            fixes.push(fix.clone());
            fix.opaque.clear();
            if fix.replaced == 0 {
                break;
            }
        }
    }
    fixes.push(fix);
    let mut originals = Originals {
        fixes,
        found: HashMap::new(),
    };
    originals.visit_item(item);
    let at = |e: LeakError| match originals.found.get(&place(&e.tokens)).cloned() {
        Some(tokens) => moved_to(e, tokens),
        None => e,
    };
    let mut errors = own;
    errors.extend(refused.into_iter().map(&at));
    if opts.repeater.is_none() || matches!(item, Item::Type(_)) {
        let reason = unrepeated(item);
        errors.extend(required.iter().map(|ty| at(LeakError::new(ty, reason))));
    }
    errors.extend(encoded.into_iter().map(&at));
    errors
}

/// Why a relative type is an error where no repeater carries it.
fn unrepeated(item: &Item) -> &'static str {
    if matches!(item, Item::Type(_)) {
        ALIAS
    } else {
        WITHOUT_REPEATER
    }
}

/// An error of type-leak's encoders, as a recorded thing.
fn encode_error(e: type_leak::EncodeError) -> LeakError {
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
        EncodeError::NeedsSelf(ty) => LeakError::new(ty, "with `Self` in a carried type"),
        _ => LeakError::new(
            TokenStream::new(),
            format!("with a type that cannot be encoded ({})", e),
        ),
    }
}

/// One impl of `repeater` per required entry of `analysis`, written by type-leak from the
/// context of the entry: its own generics with their bounds and where predicates, and the
/// trait arguments that encode the ones the type uses (const generics through the slots of
/// the repeater). The impl is for `()`: no entry names `Self` (`allow_self = false`), so any
/// marker will do, and `()` is what a consumer's projection names. And the causes that keep
/// an entry from having one, each once: a const generic that several entries use stops them
/// all.
fn repeater_impls(
    analysis: &Analysis,
    repeater: &Path,
) -> (Vec<(usize, type_leak::RepeaterImpl)>, Vec<LeakError>) {
    let slots = RepeaterSlots::new(repeater.clone());
    let marker: Type = syn::parse_quote!(());
    let (mut impls, mut errors) = (Vec::new(), Vec::<LeakError>::new());
    for (ix, _) in analysis.required() {
        match analysis
            .referrer
            .repeater_impl_with(ix, ImplSelf::Marker(&marker), |value, param| {
                slots.encode_const(value, &param.ty)
            }) {
            Ok(r) => impls.push((ix, r)),
            Err(e) => {
                let e = encode_error(e);
                let same =
                    |x: &LeakError| x.reason == e.reason && place(&x.tokens) == place(&e.tokens);
                if !errors.iter().any(same) {
                    errors.push(e);
                }
            }
        }
    }
    (impls, errors)
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
    let analysis = match analyze(item, opts) {
        Ok(analysis) => analysis,
        Err(errors) => return failed(item, random, errors),
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
            let reason = unrepeated(item);
            let errors = analysis
                .required()
                .map(|(_, ty)| LeakError::new(ty, reason));
            return failed(item, random, errors.collect());
        }
        (Some(a), Some(analysis)) => (a, analysis),
    };
    let id = syn::LitInt::new(&format!("{}u64", random), Span::call_site());
    let referrer = &analysis.referrer;
    let (written, errors) = repeater_impls(&analysis, repeater);
    if !errors.is_empty() {
        return failed(item, random, errors);
    }
    let mut impls = TokenStream::new();
    for (ix, r) in written {
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
    Ok(Some(Table::Leaked(Leaked { impls, meta })))
}

/// The `Referrer` that a consumer parses from `meta` is the one the impls were
/// written from (same entries, same indices, same contexts).
fn check_round_trip(meta: &TokenStream, referrer: &Referrer) -> Result<()> {
    let read: TypeLeak = syn::parse2(meta.clone())?;
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

    /// A reason in a word, to write the causes of an item in a line.
    fn tag(reason: &str) -> &str {
        match reason {
            TRAIT_PATH => "trait",
            VALUE_PATH => "value",
            WITHOUT_REPEATER => "relative",
            ALIAS => "alias",
            REFUSED => "refused",
            NOT_PRIMITIVE => "const",
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

    #[derive(Default)]
    struct Paths(Vec<Path>);

    impl<'ast> Visit<'ast> for Paths {
        fn visit_path(&mut self, p: &'ast Path) {
            self.0.push(p.clone());
            syn::visit::visit_path(self, p);
        }
    }

    /// The tokens of `path` as `it` has them, which is where a refusal is made of in use: a
    /// path of it, or with `run` a run of its tokens (a macro's). Else `path` as parsed, which
    /// is in no place of `it`.
    fn written(it: &Item, path: &Path, run: bool) -> TokenStream {
        let found = if run {
            let (hay, needle) = (leaves(it.to_token_stream()), leaves(path.to_token_stream()));
            let same = |w: &&[TokenTree]| {
                w.iter()
                    .zip(&needle)
                    .all(|(a, b)| a.to_string() == b.to_string())
            };
            hay.windows(needle.len())
                .find(same)
                .map(|w| w.iter().cloned().collect())
        } else {
            let mut all = Paths::default();
            all.visit_item(it);
            all.0
                .iter()
                .find(|p| text(*p) == text(path))
                .map(|p| p.to_token_stream())
        };
        found.unwrap_or_else(|| path.to_token_stream())
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
        // A prelude name is relative too; a type is recorded whole.
        assert_eq!(
            causes(quote! { type C = (A, ::core::option::Option<B>, Vec<u8>); }),
            ["(A,::core::option::Option<B>,Vec<u8>): alias"]
        );
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
        for bad in [
            quote! { trait T: Base {} },
            quote! { struct S<T: Base>(T); },
            quote! { trait T { fn f(&self, x: impl Base); } },
            quote! { trait T { fn f<U: Base>(&self, u: U); } },
        ] {
            assert_eq!(causes(bad), ["Base: trait"]);
        }
        // The `Fn(..)` sugar of a bound is a bound.
        assert_eq!(
            causes(quote! { struct S<F: Call(u8) -> u8>(F); }),
            ["Call(u8)->u8: trait"]
        );
        assert!(analyze(&item(quote! { trait T: crate::Base {} }), &opts()).is_ok());
        assert!(analyze(
            &item(quote! { struct S<T: ::core::fmt::Debug>(T); }),
            &opts()
        )
        .is_ok());
        // Bare prelude traits only with `allow_std_prelude`, and then the
        // trait-path error is the one that goes away.
        let it = quote! { struct S<T: Clone>(T); };
        assert_eq!(causes(it.clone()), ["Clone: trait"]);
        assert!(analyze(
            &item(it),
            &Options {
                allow_std_prelude: true,
                ..Options::default()
            }
        )
        .is_ok());
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
            assert!(analyze(&item(it.clone()), &opts()).is_ok(), "{}", it);
            carried(it, ty);
        }
        // A bound in a trait object is no more looked at than the object.
        let it = quote! { struct S<T: Base> { a: ::std::boxed::Box<dyn Base>, t: T } };
        assert_eq!(causes_with(it, &with_repeater()), ["Base: trait"]);
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
            let a = analyze(&item(ok.clone()), &opts());
            assert!(a.is_ok(), "{}: {:?}", ok, a.err().unwrap());
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
            assert!(causes(bad.clone())[0].ends_with(": trait"), "{}", bad);
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
            assert!(a.is_ok(), "{}: {:?}", ok, a.err().unwrap());
        }
    }

    #[test]
    fn value_paths_must_be_absolute() {
        for bad in [
            quote! { enum E { A = N } },
            quote! { struct S<const M: usize = N>; },
            quote! { trait T { const M: usize = N; } },
            quote! { struct S<T: ::x::Tr<{ N }>>(T); },
            quote! { struct S<T>(T) where T: ::x::Tr<{ N }>; },
        ] {
            assert_eq!(causes(bad.clone()), ["N: value"], "{}", bad);
            // A repeater does not carry a value.
            assert_eq!(causes_with(bad.clone(), &with_repeater()), ["N: value"]);
        }
        for ok in [
            quote! { struct S([u8; crate::N]); },
            quote! { struct S<const N: usize>([u8; N]); },
            quote! { enum E { A = ::core::mem::size_of::<u8>() as isize } },
            quote! { enum E { A = crate::N, B = 2 } },
            quote! { struct S<const M: usize = { crate::N }>; },
            quote! { struct S<T: ::x::Tr<{ crate::N }>>(T); },
        ] {
            assert!(analyze(&item(ok), &opts()).is_ok());
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
            assert_eq!(causes(bad.clone()), ["N: value"], "{}", bad);
            assert_eq!(causes_with(bad.clone(), &with_repeater()), ["N: value"]);
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
            let a = analyze(&item(ok.clone()), &opts());
            assert!(a.is_ok(), "{}: {:?}", ok, a.err().unwrap());
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
        // The values outside of types are checked all the same.
        let it = quote! { enum E { A = M, B([u8; N]) } };
        assert_eq!(causes(it.clone()), ["M: value", "[u8;N]: relative"]);
        assert_eq!(causes_with(it, &with_repeater()), ["M: value"]);
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
            impls.contains("impl<'b>crate::R<(&'b(),),42u64,0usize>for(){typeType=Local<'b>;}"),
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
        // A relative trait path.
        let f = record(quote! { pub trait T: Base {} }, &opts());
        assert_eq!(
            f.errors[0].help.as_deref(),
            Some("write `Base` with an absolute path (e.g. `crate::Base`)")
        );
        // A refusal of type-leak: its message and why, and what to write instead.
        let f = record(quote! { trait T { fn f(&self, a: Wrap<'_>); } }, &opts());
        let e = &f.errors[0];
        assert_eq!(e.message.as_deref(), Some("use absolute path: 'Wrap<'_>'"));
        assert_eq!(e.notes, ["it cannot be leaked as a whole because of `'_`"]);
        let help = e.help.as_deref().unwrap();
        assert!(help.starts_with("write `Wrap` with an absolute path (e.g. `crate::Wrap`"));
        // A prelude name: the absolute path to write, and what the name costs.
        let f = record(quote! { trait T { fn f(&self, a: Vec<&'_ u8>); } }, &opts());
        let e = &f.errors[0];
        assert_eq!(
            e.notes,
            ["a relative type cannot be carried with `'_` in it"]
        );
        assert_eq!(
            e.help.as_deref(),
            Some(
                "write `::std::vec::Vec<..>` instead of `Vec<..>`, or set `allow_std_prelude = true` \
                 to accept prelude names"
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

    /// The table of `it` with the repeater `crate::R` and the id 42.
    fn leaked(it: TokenStream) -> Leaked {
        match table(&item(it), &with_repeater(), 42).unwrap() {
            Some(Table::Leaked(l)) => l,
            _ => panic!("a table"),
        }
    }

    #[test]
    fn repeater_table_has_one_impl_per_type() {
        let leaked = leaked(quote! { struct S { a: Local, b: Other, c: Local } });
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
        // An absolute alias is fine for the analysis, but cannot be encoded.
        let it = quote! { struct S<const N: crate::Idx> { a: Wrap<N> } };
        assert_eq!(causes_with(it, &with_repeater()), ["crate::Idx: const"]);
        // Not used by a carried type: nothing to encode.
        leaked(quote! { struct S<const N: crate::Idx> { a: Local, b: [u8; 4] } });
    }

    #[test]
    fn a_const_that_cannot_be_encoded_is_reported_with_the_other_causes() {
        // The table is written for the entries that are left when the rest is replaced.
        check("
            struct S<T: Base, const N: crate::Idx> { a: Local<N>, t: T } => Base: trait | crate::Idx: const
            struct S<T: Base, const N: crate::Idx> { a: Local<N>, b: Wrap<'_>, t: T } => Base: trait | Wrap<'_>: refused | crate::Idx: const
            enum E<const N: crate::Idx> { A = M, B(Local<N>) } => M: value | crate::Idx: const
            struct S<const N: crate::Idx, const M: crate::Idx2> { a: Local<N>, b: Other<M> } => crate::Idx: const | crate::Idx2: const
            struct S<T: Base, const N: crate::Idx> { a: Local, t: T } => Base: trait
        ", &with_repeater());
        // One cause makes several entries fail, and is one error.
        check("
            struct S<T: Base, const N: crate::Idx> { a: Local<N>, b: Other<N>, t: T } => Base: trait | crate::Idx: const
        ", &with_repeater());
        // Without a repeater there is no table to write.
        check("
            struct S<T: Base, const N: crate::Idx> { a: Local<N>, t: T } => Base: trait | Local<N>: relative
        ", &opts());
        let record = record(
            quote! { struct S<T: Base, const N: crate::Idx> { a: Local<N>, t: T } },
            &with_repeater(),
        );
        assert_eq!(record.errors.len(), 2);
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
        let leaked = leaked(quote! { struct S { a: u8 } });
        assert!(leaked.impls.is_empty());
        let meta = squash(&leaked.meta);
        assert!(
            meta.starts_with("type_leak(repeater=crate::R,id=42u64,context=0,referrer=["),
            "{}",
            meta
        );
        // The consumer reads it back, with nothing to replace.
        let read: TypeLeak = syn::parse2(leaked.meta).unwrap();
        assert_eq!(read.types().count(), 0);
    }

    #[test]
    fn the_meta_reads_back_with_the_indices_of_the_impls() {
        let leaked = leaked(quote! {
            struct S<T> { a: Local, b: Wrap<T>, c: ::other::X<u8> }
        });
        let read: TypeLeak = syn::parse2(leaked.meta).unwrap();
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

    // --- refusals(): type-leak's errors in announce's words ---

    /// The texts of each cause of `it`: the message, then the notes and the help.
    fn errors_of(it: TokenStream, o: &Options) -> Vec<String> {
        let text = |e: LeakError| {
            let notes: String = e
                .notes
                .iter()
                .map(|n| format!("\n  = note: {}", n))
                .collect();
            format!(
                "{}{}\n  = help: {}",
                e.message.unwrap(),
                notes,
                e.help.unwrap()
            )
        };
        record(it, o).errors.into_iter().map(text).collect()
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
                assert!(e.contains("\n  = help: "), "{}: {}", it, e);
            }
            for e in record(it.clone(), &opts()).errors {
                assert!(
                    !e.tokens.is_empty() && e.reason.starts_with("with "),
                    "{}",
                    it
                );
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
            assert!(e.contains("`Self` has no meaning here"), "{}", e);
            assert!(
                e.ends_with("= help: write the type instead of `Self`"),
                "{}",
                e
            );
        }
        let reason = "`Self` in a type alias";
        assert_eq!(
            causes(quote! { type A = Self; }),
            [format!("Self: {}", reason)]
        );
        // `inner::G` is relative too, which is no less an error in an alias.
        assert_eq!(
            causes(quote! { type A = inner::G<Self>; }),
            [
                format!("Self: {}", reason),
                "inner::G<Self>: alias".to_string()
            ]
        );
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
        let it = quote! { struct S { a: Vec<&'_ u8> } };
        assert_eq!(
            causes(it.clone()),
            ["Vec<&'_u8>: the relative prelude name `Vec`"]
        );
        let e = first_error(it);
        assert!(
            e.ends_with("= help: write `::std::vec::Vec<..>` instead of `Vec<..>`, or set `allow_std_prelude = true` to accept prelude names"),
            "{}",
            e
        );
        // A relative `Option` around `Self::Out`: the error is about the name, not
        // about "leaking" the whole type.
        let it = quote! { trait Tr { type Out; fn f(&self) -> Option<Self::Out>; } };
        assert_eq!(
            causes(it.clone()),
            ["Option<Self::Out>: the relative prelude name `Option`"]
        );
        let e = first_error(it);
        assert_eq!(
            e,
            "use absolute path: 'Option<Self::Out>'\n  \
             = note: a relative type cannot be carried with `Self::Out` in it\n  \
             = help: write `::core::option::Option<..>` instead of `Option<..>`, or set \
             `allow_std_prelude = true` to accept prelude names"
        );
        assert!(!e.contains("leaked"), "{}", e);
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
        // The option does what the help says.
        let prelude = Options {
            allow_std_prelude: true,
            ..Options::default()
        };
        assert!(analyze(&item(quote! { struct S { a: Vec<&'_ u8> } }), &prelude).is_ok());
        // Already set: type-leak's own help stays.
        let it = quote! { struct S { a: Box<Wrap<'_>> } };
        let e = errors_of(it.clone(), &prelude).remove(0);
        assert!(e.contains("use absolute path: 'Wrap<'_>'"), "{}", e);
        assert_eq!(causes_with(it, &prelude), ["Wrap<'_>: refused"]);
    }

    #[test]
    fn notes_repeat_once() {
        let e = first_error(quote! { struct S(inner::G<Self::A>); });
        let note = "= note: it cannot be leaked as a whole because of `Self::A`";
        assert_eq!(e.matches(note).count(), 1, "{}", e);
    }

    #[test]
    fn refusals_are_recorded_at_their_tokens() {
        for (it, cause) in rows(
            "
            struct S(inner::G<Self::A>); => inner::G<Self::A>
            trait Tr { fn f(&self) -> inner::G<Self>; } => inner::G<Self>
            struct S<T: ::core::convert::From<inner::G<T>>>(T); => inner::G<T>
            struct S([u8; two!()]); => two
            struct S<const N: inner::Ty>; => inner::Ty
            trait Tr { fn f<'a>(&self, a: Wrap<'a>) -> &'a u8; } => Wrap<'a>
        ",
        ) {
            assert_eq!(causes(it), [format!("{}: refused", cause)]);
        }
    }

    #[test]
    fn every_cause_of_an_item_is_reported_at_once() {
        // A relative trait path hides no relative type; a type alias has its own reason.
        check("
            struct S<T: Base> { a: Local, t: T } => Base: trait | Local: relative
            trait Sub: Base { const M: usize = N; fn f(&self, a: Wrap<'_>, b: Local, c: [u8; N]); } => Base: trait | N: value | Wrap<'_>: refused | Local: relative | [u8;N]: relative
            type A<T: Base> = Option<Local>; => Base: trait | Option<Local>: alias
            trait T: Base + inner::Other<u8> where Self: Third { fn f(&self) -> Local; } => Third: trait | Base: trait | inner::Other<u8>: trait | Local: relative
            struct S { a: Local, b: Wrap<'_>, c: Other, d: Vec<&'_ u8>, e: Local } => Wrap<'_>: refused | Vec<&'_u8>: the relative prelude name `Vec` | Local: relative | Other: relative
        ", &opts());
        // The same refused type in two places is a cause in each, or one where the places are
        // alike (a test build has no positions).
        let mut twice = causes(
            "struct S { a: Wrap<'_>, b: Wrap<'_>, z: Local }"
                .parse()
                .unwrap(),
        );
        twice.dedup();
        assert_eq!(twice, ["Wrap<'_>: refused", "Local: relative"]);
        // A repeater carries the relative type: it is no error, the rest still are.
        check("
            struct S<T: Base> { a: Local, t: T } => Base: trait
            trait Sub: Base { const M: usize = N; fn f(&self, a: Wrap<'_>, b: Local, c: [u8; N]); } => Base: trait | N: value | Wrap<'_>: refused
        ", &with_repeater());
    }

    #[test]
    fn every_kind_of_cause_is_reported_with_the_others() {
        check("
            enum E { A = N, B(Local) } => N: value | Local: relative
            struct S<const M: usize = N>(Local); => N: value | Local: relative
            trait T { const M: usize = N; fn f(&self) -> Local; } => N: value | Local: relative
            struct S<U: ::x::Tr<{ N }>>(U, Local); => N: value | Local: relative
            struct S<T>(T, Local) where T: Base; => Base: trait | Local: relative
            trait T { fn f(&self, x: impl Base) -> Local; } => Base: trait | Local: relative
            struct S<T: Base>(::std::boxed::Box<[u8; 2]>, T, Local); => Base: trait | Local: relative
            struct S<T: Base>(::std::boxed::Box<dyn Base>, T); => Base: trait | dynBase: relative
        ", &opts());
        // What is not looked at is not replaced: the types around a cause are as written.
        check("
            enum E { A = N, B(Local<[u8; N]>) } => N: value | Local<[u8;N]>: relative
            struct S<T: Base> { a: ::std::boxed::Box<dyn Base>, b: Wrap<dyn Base>, t: T } => Base: trait | Wrap<dynBase>: relative | dynBase: relative
            struct S { a: Wrap<Foo<'_>>, b: Local } => Wrap<Foo<'_>>: refused | Local: relative
            struct S { a: [u8; two!()], b: Local } => two: refused | Local: relative
        ", &opts());
        // The arguments of a relative trait path are still read.
        check("
            struct S<T: Base<Local>>(T); => Base<Local>: trait | Local: relative
            struct S<F: for<'a> MyFn(&'a Local) -> u8>(F); => MyFn(&'aLocal)->u8: trait | Local: relative
        ", &opts());
    }

    #[test]
    fn a_refused_path_is_reported_with_the_relative_types() {
        // The bound of a trait object that cannot be carried whole, and the trait of a
        // qualified path, are paths, not types. `Self::X` passes the checks of announce,
        // which type-leak does not accept. type-leak reads the value of `#[name = ..]`,
        // where the checks do not look, and says `Self` for the tokens of a macro.
        for (it, cause) in rows(
            "
            struct S { a: *const dyn Tr<'_>, z: Local } => Tr<'_>
            union U { a: *mut dyn Tr<'_>, z: Local } => Tr<'_>
            enum E { A(&dyn Tr), B(Local) } => Tr
            trait T { fn f(&self, a: &dyn Tr); fn z(&self) -> Local; } => Tr
            struct S<T: ::core::convert::Into<*const dyn Tr<'_>>>(T, Local); => Tr<'_>
            struct S { a: <Self as Other>::X, z: Local } => Other::X
            trait T { fn f(&self) -> <Self as T>::A; type A; fn z(&self) -> Local; } => T::A
            struct S(Local) where <Self as Other>::X: Sized; => Other::X
            enum E { A = Self::X, B(Local) } => Self::X
            enum E { A = <Self as Tr>::X, B(Local) } => <SelfasTr>::X
            struct S<const N: usize = { Self::X }>(Local); => Self::X
            trait T<const N: usize = { 1 + Self::X }> { fn z(&self) -> Local; } => Self::X
            struct S<T: ::x::Tr<{ Self::X }>>(T, Local); => Self::X
            struct S<T>(T, Local) where T: ::x::Tr<{ Self::X }>; => Self::X
            trait T { fn f<const N: usize = { Self::X }>(&self); fn z(&self) -> Local; } => Self::X
            #[name = Wrap::X] struct S(Local); => Wrap::X
            struct S { #[name = Wrap] a: u8, z: Local } => Wrap
            enum E { #[name = Wrap] A, B(Local) } => Wrap
            trait T { #[name = Wrap] fn f(&self); fn z(&self) -> Local; } => Wrap
            trait T { fn f(&self, #[name = Wrap] a: u8); fn z(&self) -> Local; } => Wrap
            struct S<#[name = Wrap] T>(T, Local); => Wrap
            struct S<T = m!(Self)>(T, Local); => Self
            trait T<X = m!(Self)> { fn z(&self) -> Local; } => Self
            struct S<T = m!(Self::A)>(Local, T); => Self
        ",
        ) {
            let found = format!("{}: refused", cause);
            let relative = "Local: relative".to_string();
            assert_eq!(causes(it.clone()), [found.clone(), relative], "{}", it);
            // A repeater carries the relative type; the refusal stays.
            assert_eq!(causes_with(it.clone(), &with_repeater()), [found], "{}", it);
        }
        check("
            struct S { a: &dyn Fn(&Wrap<'_>) -> &u8, z: Local } => Fn(&Wrap<'_>)->&u8: the relative prelude name `Fn` | Local: relative
            type A = (*const dyn Tr<'_>, Local); => Tr<'_>: refused | Local: alias
            enum E { A = N, B = Self::X, C(Local) } => N: value | Self::X: refused | Local: relative
            struct S<T = m!(Self)>(T, m!(u8), Local); => Self: refused | Local: relative | m!(u8): relative
            struct S { a: (Other, *const dyn Tr<'_>), f: fn(&dyn Tr4) -> Other2, z: Local } => Tr<'_>: refused | Tr4: refused | Local: relative | Other: relative | Other2: relative
        ", &opts());
    }

    #[test]
    fn every_kind_of_refused_token_is_reported_at_once() {
        let it = quote! {
            trait T<const N: usize = { Self::X }> {
                #[name = Wrap]
                fn f(&self, a: *const dyn Tr<'_>) -> <Self as Other>::X;
                fn g<'x>(&self, b: Wrap<'x>) -> &'x u8;
                fn h(&self) -> [u8; two!()];
                fn z(&self) -> Local;
            }
        };
        let refused = ["Self::X", "Wrap", "Tr<'_>", "Other::X", "Wrap<'x>", "two"];
        let found = causes(it.clone());
        assert_eq!(found.len(), 7, "{:?}", found);
        for cause in refused.iter().map(|c| format!("{}: refused", c)) {
            assert!(found.contains(&cause), "{:?}: {:?}", cause, found);
        }
        assert!(
            found.contains(&"Local: relative".to_string()),
            "{:?}",
            found
        );
        // With a repeater the relative type is carried.
        assert_eq!(causes_with(it, &with_repeater()).len(), 6);
    }

    #[test]
    fn a_refused_path_is_covered_by_the_smallest_thing_that_holds_it() {
        // `refused | item => rewritten`, with the refused tokens as a type or in a macro.
        let cover = |opaque: bool, table: &str| {
            for (left, want) in pairs(table) {
                let (refused, it) = left.split_once(" | ").unwrap();
                let refused: Path = syn::parse_str(refused).unwrap();
                let mut it = item(it.parse().unwrap());
                let refused = written(&it, &refused, opaque);
                let mut fix = Neutralise::default();
                if opaque {
                    fix.opaque.push(refused);
                } else {
                    fix.types.insert(place(&refused));
                }
                fix.visit_item_mut(&mut it);
                assert_eq!(squash(it), squash(item(want.parse().unwrap())), "{}", left);
            }
        };
        // A type, a value.
        cover(false, "
            Tr<'_> | struct S { a: Box<*const dyn Tr<'_>> } => struct S { a: Box<*const ::core::primitive::u8> }
            Other::X | struct S { a: <Self as Other>::X } => struct S { a: ::core::primitive::u8 }
            Tr | enum E { A = size_of::<dyn Tr>() } => enum E { A = size_of::<::core::primitive::u8>() }
            m | enum E { A = 1 + m!(a, b), B } => enum E { A = 1 + 0, B }
        ");
        // The tokens of a macro, when they are looked for.
        cover(true, "
            Wrap | struct S<T = m!(a, Wrap)>(T, n!(a)); => struct S<T = ::core::primitive::u8>(T, n!(a));
            Wrap | struct S { a: m!(Wrap2) } => struct S { a: m!(Wrap2) }
        ");
    }

    #[test]
    fn the_texts_of_a_cause_in_a_rewritten_item_are_of_the_item_as_written() {
        let it = quote! { struct S<T: Base> { a: Local<[u8; N]>, t: T } };
        let local = &record(it, &opts()).errors[1];
        let message = "use absolute path: 'Local<[u8 ; N]>'";
        assert_eq!(local.message.as_deref(), Some(message));
        assert!(local.help.as_deref().unwrap().contains("`Local`"));
        assert_eq!(squash(&local.tokens), "Local<[u8;N]>");
    }
}
