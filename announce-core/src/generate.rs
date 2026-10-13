use crate::helper::OwnedAttrs;
use crate::options::{Layout, Options};
use crate::timing::Timer;
use proc_macro2::{Delimiter, Group, Ident, Punct, Spacing, Span, TokenStream, TokenTree};
use syn::visit::Visit;
use syn::visit_mut::VisitMut;
use syn::{Item, Result, Visibility};
use template_quote::{quote, ToTokens};

/// Name, visibility and kind check of a supported item.
pub fn ident_vis(item: &Item) -> Result<(Ident, Visibility)> {
    Ok(match item {
        Item::Struct(i) => (i.ident.clone(), i.vis.clone()),
        Item::Enum(i) => (i.ident.clone(), i.vis.clone()),
        Item::Union(i) => (i.ident.clone(), i.vis.clone()),
        Item::Trait(i) => (i.ident.clone(), i.vis.clone()),
        Item::Type(i) => (i.ident.clone(), i.vis.clone()),
        Item::Mod(i) if i.content.is_some() => (i.ident.clone(), i.vis.clone()),
        Item::Mod(i) => {
            return Err(syn::Error::new_spanned(
                i,
                "#[announce] supports only inline modules (`mod m { .. }`), not `mod m;`",
            ))
        }
        other => {
            return Err(syn::Error::new_spanned(
                other,
                "#[announce] supports struct, enum, union, trait, type alias and inline mod items",
            ))
        }
    })
}

pub fn hex(random: u64) -> String {
    format!("{:016x}", random)
}

// ---------------------------------------------------------------------------
// The attributes that do not stay, and the passes over the tokens.
//
// The item is parsed once. A pass over the tree marks
// the attributes that some copy of the item loses, the tree is printed once,
// and each copy is made from the printed tokens by a pass that drops the marked
// attributes it does not keep. The pass that makes the payload also escapes `$`,
// rewrites `crate::` and counts the tokens.
// ---------------------------------------------------------------------------

/// The name that stands in for a `#[cfg]` (all copies lose it).
const CUT_CFG: &str = "__announce_cut_cfg";
/// The first segment put before the path of a string doc attribute (the payload
/// and the checks lose it, unless `docs = true`).
const CUT_DOC: &str = "__announce_cut_doc";
/// The first segment put before the path of an owned attribute (the copies that
/// rustc compiles lose it).
const CUT_OWN: &str = "__announce_cut_own";

const CFG: u8 = 1;
const DOC: u8 = 2;
const OWN: u8 = 4;

/// Which kinds of attribute the tree has marks of.
#[derive(Clone, Copy, Debug, Default)]
struct Found {
    cfg: bool,
    doc: bool,
    own: bool,
}

impl Found {
    fn any(self) -> bool {
        self.cfg || self.doc || self.own
    }
}

/// A string doc attribute: `#[doc = ".."]`, which is what `///`, `//!`, `/** */`
/// and an explicit `#[doc = ".."]` all are, also `#[doc = include_str!(..)]`.
/// The list forms (`#[doc(hidden)]`, `#[doc(alias = "x")]`, `#[doc(inline)]`,
/// `#[doc(cfg(..))]`, ..) are not: they steer rustdoc, not the reader.
fn is_doc_string(a: &syn::Attribute) -> bool {
    a.path().is_ident("doc") && matches!(a.meta, syn::Meta::NameValue(_))
}

/// Marks, at any depth that syn parses (the item, its fields, variants, generic
/// parameters, trait items, and everything inside an inline `mod`, inner
/// attributes included; not inside macro invocations and `macro_rules!` bodies,
/// which are only tokens), the attributes that some copy of the item loses:
///
/// - `#[cfg(..)]`: after the derive's cfg evaluation every one still present is
///   true; a consumer would evaluate it again with its own settings. Lost by every
///   copy (the path becomes [`CUT_CFG`]).
/// - a string doc attribute, unless `docs = true`: lost by the payload and the
///   checks (the path gets the segment [`CUT_DOC`] in front).
/// - an attribute of an owned name: lost by the copies that rustc compiles (the
///   path gets the segment [`CUT_OWN`] in front).
///
/// `syn`'s `VisitMut` can change an attribute but not take it out of its list (the
/// hook for that is recent), so an attribute is renamed here and dropped from
/// the printed tokens later (see [`crate::helper::remove_attributes`], which does
/// the same and then parses the item again; this does not).
struct Marker<'a> {
    keep_docs: bool,
    owned: &'a OwnedAttrs,
    found: Found,
}

impl Marker<'_> {
    fn prefix(a: &mut syn::Attribute, mark: &str) {
        let seg = syn::PathSegment::from(Ident::new(mark, Span::call_site()));
        match &mut a.meta {
            syn::Meta::Path(p) => p.segments.insert(0, seg),
            syn::Meta::List(l) => l.path.segments.insert(0, seg),
            syn::Meta::NameValue(nv) => nv.path.segments.insert(0, seg),
        }
    }
}

impl VisitMut for Marker<'_> {
    fn visit_attribute_mut(&mut self, a: &mut syn::Attribute) {
        if a.path().is_ident("cfg") {
            let mark: syn::Path = Ident::new(CUT_CFG, Span::call_site()).into();
            match &mut a.meta {
                syn::Meta::Path(p) => *p = mark,
                syn::Meta::List(l) => l.path = mark,
                syn::Meta::NameValue(nv) => nv.path = mark,
            }
            self.found.cfg = true;
        } else if !self.keep_docs && is_doc_string(a) {
            Self::prefix(a, CUT_DOC);
            self.found.doc = true;
        } else if self.owned.owns(a) {
            Self::prefix(a, CUT_OWN);
            self.found.own = true;
        }
    }
}

/// The cfg-evaluated item, parsed once, with its attributes marked, and the
/// tokens it prints.
pub struct Prepared {
    item: Item,
    /// The tokens of `item` (or the tokens it was parsed from, if nothing is
    /// marked), with the marks.
    printed: TokenStream,
    found: Found,
}

/// Marks the attributes of `item` and prints it. `original` are the tokens
/// that `item` was parsed from, used instead of the printed tree when nothing is
/// marked.
pub fn prepare(
    mut item: Item,
    original: Option<TokenStream>,
    opts: &Options,
    owned: &OwnedAttrs,
) -> Prepared {
    let mut marker = Marker {
        keep_docs: opts.docs,
        owned,
        found: Found::default(),
    };
    marker.visit_item_mut(&mut item);
    let found = marker.found;
    let printed = match original {
        Some(tokens) if !found.any() => tokens,
        _ => item.to_token_stream(),
    };
    Prepared {
        item,
        printed,
        found,
    }
}

/// What one pass over a token stream does (see [`run`]).
#[derive(Clone, Copy)]
struct Pass {
    /// The marks (`CFG`, `DOC`, `OWN`) whose attributes are dropped. A mark that is
    /// not dropped is taken off the attribute it stands on.
    cut: u8,
    /// Look for marked attributes at all (the item has some).
    watch: bool,
    /// Escape every `$` as `$__dollar`.
    escape: bool,
    /// Rewrite path-leading `crate::` as `$crate::`.
    krate: bool,
}

impl Pass {
    /// Only the attributes.
    fn attributes(cut: u8, found: Found) -> Pass {
        Pass {
            cut,
            watch: found.any(),
            escape: false,
            krate: false,
        }
    }

    fn is_identity(self) -> bool {
        !self.watch && !self.escape && !self.krate
    }
}

/// The result of a [`run`] besides the tokens.
#[derive(Default)]
struct Counts {
    /// A `$` was escaped.
    dollar: bool,
    /// Tokens of the result; a group counts its two delimiters and its contents.
    tokens: usize,
}

fn mark_of(id: &Ident) -> Option<u8> {
    if id == CUT_CFG {
        Some(CFG)
    } else if id == CUT_DOC {
        Some(DOC)
    } else if id == CUT_OWN {
        Some(OWN)
    } else {
        None
    }
}

/// The end of the marked attribute that starts at `tts[i]` (`#`), if it is one
/// that `p` drops. Otherwise, if it is a marked attribute that is kept, pushes it
/// to `out` without its mark and returns the index after it. `None` when the
/// tokens at `i` are not a marked attribute (the caller goes on as usual).
fn marked_attribute(
    tts: &[TokenTree],
    i: usize,
    p: &Pass,
    c: &mut Counts,
    out: &mut Vec<TokenTree>,
) -> Option<usize> {
    let mut j = i + 1;
    let bang = matches!(tts.get(j), Some(TokenTree::Punct(b)) if b.as_char() == '!');
    if bang {
        j += 1;
    }
    let g = match tts.get(j) {
        Some(TokenTree::Group(g)) if g.delimiter() == Delimiter::Bracket => g,
        _ => return None,
    };
    let inner: Vec<TokenTree> = g.stream().into_iter().collect();
    let mark = match inner.first() {
        Some(TokenTree::Ident(id)) => mark_of(id)?,
        _ => return None,
    };
    if p.cut & mark != 0 {
        return Some(j + 1);
    }
    // Kept: `#[MARK::path ..]` is `#[path ..]` again. (A `#[cfg]` is always cut.)
    let rest: TokenStream = inner.into_iter().skip(3).collect();
    out.push(tts[i].clone());
    c.tokens += 1;
    if bang {
        out.push(tts[i + 1].clone());
        c.tokens += 1;
    }
    let mut ng = Group::new(Delimiter::Bracket, run(rest, p, c, false));
    ng.set_span(g.span());
    out.push(TokenTree::Group(ng));
    c.tokens += 2;
    Some(j + 1)
}

/// One pass over `ts`, which does what `p` says at once: drops and restores the
/// marked attributes, escapes `$` and rewrites `crate::`, and counts the tokens
/// it produces, in place of three separate passes.
///
/// `$` is escaped as `$__dollar` (the payload is un-escaped by the trampoline at
/// each hop).
///
/// `crate::` as the start of a path is rewritten as `$crate::`, so paths in the
/// payload and META keep referring to the defining crate when the macro is
/// expanded in another crate. `crate` in `pub(crate)` and in `pub(in crate::m)` is
/// left alone: there it is the visibility's own syntax, which takes no `$crate`.
/// That is the `in` that opens the parentheses after `pub`; any other `in`
/// (`for x in crate::ARR {}`) is followed by an ordinary path, which is rewritten.
/// An escaped `$crate` (`$ __dollar crate`) is left alone as well: the
/// previous token of the input is read, so the decision is that of the two passes
/// (escape, then rewrite) that this replaces.
///
/// `in_pub_paren`: `ts` is the inside of the parentheses that follow `pub`.
fn run(ts: TokenStream, p: &Pass, c: &mut Counts, in_pub_paren: bool) -> TokenStream {
    let tts: Vec<TokenTree> = ts.into_iter().collect();
    let mut out: Vec<TokenTree> = Vec::with_capacity(tts.len() + 4);
    let mut i = 0;
    while i < tts.len() {
        let tt = &tts[i];
        let prev = i.checked_sub(1).and_then(|j| tts.get(j));
        match tt {
            TokenTree::Punct(pt) if p.watch && pt.as_char() == '#' => {
                if let Some(next) = marked_attribute(&tts, i, p, c, &mut out) {
                    i = next;
                    continue;
                }
                out.push(tt.clone());
                c.tokens += 1;
            }
            TokenTree::Punct(pt) if p.escape && pt.as_char() == '$' => {
                c.dollar = true;
                out.push(tt.clone());
                out.push(TokenTree::Ident(Ident::new("__dollar", Span::call_site())));
                c.tokens += 2;
            }
            TokenTree::Ident(id) if p.krate && id == "crate" => {
                let next_is_path = matches!(
                    (tts.get(i + 1), tts.get(i + 2)),
                    (Some(TokenTree::Punct(a)), Some(TokenTree::Punct(b)))
                        if a.as_char() == ':' && a.spacing() == Spacing::Joint && b.as_char() == ':'
                );
                let prev2 = i.checked_sub(2).and_then(|j| tts.get(j));
                let after_in_or_sep = match prev {
                    // `pub(in crate::m)`: the `in` is the first token inside the
                    // parentheses of `pub`. And an escaped `$crate` (`$ __dollar
                    // crate`).
                    Some(TokenTree::Ident(q)) => {
                        (q == "in" && in_pub_paren && i == 1) || q == "__dollar"
                    }
                    // `a::crate` (a path separator), but not `x: crate::T`.
                    Some(TokenTree::Punct(q)) if q.as_char() == ':' => matches!(
                        prev2,
                        Some(TokenTree::Punct(r)) if r.as_char() == ':' && r.spacing() == Spacing::Joint
                    ),
                    // `$crate`, and the `$` that `escape` puts in front of `__dollar`.
                    Some(TokenTree::Punct(q)) => q.as_char() == '$',
                    _ => false,
                };
                if next_is_path && !after_in_or_sep {
                    let mut dollar = Punct::new('$', Spacing::Alone);
                    dollar.set_span(id.span());
                    out.push(TokenTree::Punct(dollar));
                    c.tokens += 1;
                }
                out.push(tt.clone());
                c.tokens += 1;
            }
            TokenTree::Group(g) => {
                let after_pub = matches!(prev, Some(TokenTree::Ident(q)) if q == "pub");
                let pub_paren = after_pub && g.delimiter() == Delimiter::Parenthesis;
                let mut ng = Group::new(g.delimiter(), run(g.stream(), p, c, pub_paren));
                ng.set_span(g.span());
                out.push(TokenTree::Group(ng));
                c.tokens += 2;
            }
            t => {
                out.push(t.clone());
                c.tokens += 1;
            }
        }
        i += 1;
    }
    out.into_iter().collect()
}

/// `run` with the tokens counted, and the stream itself when the pass changes
/// nothing.
fn apply(ts: &TokenStream, p: &Pass) -> (TokenStream, Counts) {
    let mut c = Counts::default();
    if p.is_identity() {
        return (ts.clone(), c);
    }
    let out = run(ts.clone(), p, &mut c, false);
    (out, c)
}

/// `run` over a copy of `ts`, even if the pass changes nothing: the tokens are made
/// again. A stream the compiler handed to the macro is a handle to its own tokens, in
/// which a doc comment is still the one token the lexer made, and an old compiler
/// (1.59) prints it as `///` where it prints what `syn` writes as `#[doc = ".."]`.
/// The copies that go to the checks and to the compiler are made again, as they
/// always were, so that they look the same on every compiler.
fn rebuilt(ts: &TokenStream, p: &Pass) -> TokenStream {
    run(ts.clone(), p, &mut Counts::default(), false)
}

/// The item as rustc compiles it: without the `#[cfg]`s and the owned attributes.
fn compiled(prep: &Prepared) -> TokenStream {
    rebuilt(&prep.printed, &Pass::attributes(CFG | OWN, prep.found))
}

/// The item that callbacks and checks get: without the `#[cfg]`s and the string
/// docs (unless `docs = true`), with the owned attributes.
fn plain(prep: &Prepared) -> TokenStream {
    rebuilt(&prep.printed, &Pass::attributes(CFG | DOC, prep.found))
}

/// A mention of an identifier, in the tokens that `syn` leaves unparsed (a macro
/// invocation, the arguments of an attribute) and in the tree.
struct Mentions {
    word: &'static str,
    found: bool,
}

fn mentions_tokens(ts: TokenStream, word: &str) -> bool {
    ts.into_iter().any(|tt| match tt {
        TokenTree::Ident(i) => i == word,
        TokenTree::Group(g) => mentions_tokens(g.stream(), word),
        _ => false,
    })
}

impl<'ast> Visit<'ast> for Mentions {
    fn visit_ident(&mut self, i: &'ast Ident) {
        self.found |= i == self.word;
    }

    fn visit_macro(&mut self, m: &'ast syn::Macro) {
        self.found |= mentions_tokens(m.tokens.clone(), self.word);
        syn::visit::visit_macro(self, m);
    }

    fn visit_attribute(&mut self, a: &'ast syn::Attribute) {
        // A `#[cfg]` is not part of the item that is announced (it is marked).
        if a.path().is_ident(CUT_CFG) {
            return;
        }
        if let syn::Meta::List(l) = &a.meta {
            self.found |= mentions_tokens(l.tokens.clone(), self.word);
        }
        syn::visit::visit_attribute(self, a);
    }

    fn visit_type_impl_trait(&mut self, i: &'ast syn::TypeImplTrait) {
        self.found |= self.word == "impl";
        syn::visit::visit_type_impl_trait(self, i);
    }
}

fn mentions<'a, T>(
    word: &'static str,
    visit: impl FnOnce(&mut Mentions, &'a T),
    node: &'a T,
) -> bool {
    let mut m = Mentions { word, found: false };
    visit(&mut m, node);
    m.found
}

/// Conservative test for "`dyn Trait` is certainly a valid type", so that the
/// marker impl can never break the definer's build.
fn trait_is_simple_dyn(t: &syn::ItemTrait) -> bool {
    // Any generic parameter, lifetimes included (`impl Announce for dyn L` does
    // not compile for `trait L<'a>`).
    if !t.generics.params.is_empty() || t.generics.where_clause.is_some() || t.auto_token.is_some()
    {
        return false;
    }
    let supers_ok = t.supertraits.iter().all(|b| match b {
        syn::TypeParamBound::Lifetime(_) => true,
        // Only the std auto traits by full path: a user trait that is merely
        // *named* `Send` may have associated types.
        syn::TypeParamBound::Trait(tb) => {
            let p = tb.path.to_token_stream().to_string().replace(' ', "");
            tb.lifetimes.is_none()
                && matches!(
                    p.trim_start_matches("::"),
                    "core::marker::Send"
                        | "core::marker::Sync"
                        | "std::marker::Send"
                        | "std::marker::Sync"
                )
        }
        _ => false,
    });
    if !supers_ok {
        return false;
    }
    t.items.iter().all(|ti| match ti {
        syn::TraitItem::Fn(f) => {
            let sig = &f.sig;
            let receiver_ok = matches!(
                sig.inputs.first(),
                Some(syn::FnArg::Receiver(r)) if r.reference.is_some() && r.colon_token.is_none()
            );
            receiver_ok
                && sig.asyncness.is_none()
                && sig
                    .generics
                    .params
                    .iter()
                    .all(|p| matches!(p, syn::GenericParam::Lifetime(_)))
                && sig.generics.where_clause.is_none()
                && sig
                    .inputs
                    .iter()
                    .skip(1)
                    .all(|a| !mentions("Self", |m, a| m.visit_fn_arg(a), a))
                && !mentions("Self", |m, o| m.visit_return_type(o), &sig.output)
                && !mentions("impl", |m, s| m.visit_signature(s), sig)
        }
        _ => false,
    })
}

/// The `Announce` impl, if `generate` writes one. Its generics are those of the
/// item, without the attributes of the parameters.
fn marker_impl(item: &Item, opts: &Options) -> TokenStream {
    // `#[derive(Announce)]` always implements it. `emit_item = false` means
    // the item is emitted (and its `Announce` impl owned) by someone else, so
    // a second announcement cannot conflict with it. A mock is a
    // stand-in, not the real item.
    if opts.mock.is_some() || !(opts.derived || opts.emit_item) {
        return TokenStream::new();
    }
    match item {
        Item::Struct(syn::ItemStruct {
            ident, generics, ..
        })
        | Item::Enum(syn::ItemEnum {
            ident, generics, ..
        })
        | Item::Union(syn::ItemUnion {
            ident, generics, ..
        }) => {
            let krate = opts.krate();
            let mut generics = generics.clone();
            for p in generics.params.iter_mut() {
                match p {
                    syn::GenericParam::Type(t) => t.attrs.clear(),
                    syn::GenericParam::Lifetime(l) => l.attrs.clear(),
                    syn::GenericParam::Const(c) => c.attrs.clear(),
                }
            }
            let (ig, tg, wc) = generics.split_for_impl();
            quote! { impl #ig #krate::Announce for #ident #tg #wc {} }
        }
        Item::Trait(t) if trait_is_simple_dyn(t) => {
            let krate = opts.krate();
            let ident = &t.ident;
            quote! { impl #krate::Announce for dyn #ident {} }
        }
        _ => TokenStream::new(),
    }
}

/// What the generated macro passes on: META and the (escaped) payload.
pub struct Variant {
    pub meta: TokenStream,
    pub payload: TokenStream,
    /// The payload contains `$`, escaped as `$__dollar`.
    pub needs_dollar: bool,
}

/// Tokens (delimiters of a group counted) from which `layout = auto` holds the
/// item once (`Layout::Single`) instead of copying it into each arm
/// (`Layout::Direct`), counted on the escaped payload plus META.
///
/// Chosen by measurement: a direct trampoline copies
/// the item into three arms and takes one expansion; a single-copy one holds it
/// once and takes three expansions (two through the derive route). The single
/// copy has the smaller `.rmeta` from about 90 tokens; 300 weighs that against
/// the expansions, which cost time and recursion depth on the calling side.
pub const SINGLE_COPY_THRESHOLD: usize = 300;

/// Number of tokens in `ts`; a group counts its two delimiters and its contents.
/// (The passes of [`run`] count the tokens they produce; this is the reference
/// of the tests.)
#[cfg(test)]
pub(crate) fn token_count(ts: &TokenStream) -> usize {
    ts.clone()
        .into_iter()
        .map(|tt| match tt {
            TokenTree::Group(g) => 2 + token_count(&g.stream()),
            _ => 1,
        })
        .sum()
}

/// Whether the trampoline of an item of `size` tokens (payload and META) holds
/// the item once.
pub(crate) fn is_single(layout: Layout, size: usize) -> bool {
    match layout {
        Layout::Single => true,
        Layout::Direct => false,
        Layout::Auto => size > SINGLE_COPY_THRESHOLD,
    }
}

/// One text for every malformed call, so that the macros of all definitions
/// share one string. Rustc's note names the macro.
const BAD_INPUT: &str =
    "announce: invalid trampoline input; expected `Item! { [Item!, next!, ..] .. }` \
     (a head and at least one continuation; a derive or attribute head goes through \
     `announce::helper::redirect`)";

/// The derive/attribute route emits an item (the dummy enum): an error in
/// expression position (`(expr)`).
const DERIVE_IN_EXPR: &str =
    "announce: a derive/attribute continuation emits items, so it cannot be used with `(expr)`";

/// `$( ( $($__announce_o:tt)* ) )?`: the global options group.
fn opts_pat() -> TokenStream {
    quote! { $( ( $($__announce_o:tt)* ) )? }
}

fn opts_out() -> TokenStream {
    quote! { $( ( $($__announce_o)* ) )? }
}

/// The head of the list: `path::to::macro!`, or `#[attr] Name?`. One pattern
/// for both, with an optional group for each, instead of an arm for each
/// (rare) head shape.
fn head_pat() -> TokenStream {
    quote! { $( $($hp:ident)? $(:: $hs:ident)* ! )? $( #[$($ha:tt)*] $($hid:ident)? )? }
}

/// The head as the `KEY` of an entry.
fn head_key() -> TokenStream {
    quote! { $( $($hp)? $(:: $hs)* ! )? $( #[$($ha)*] $($hid)? )? }
}

/// The error for `(expr)` followed by a derive or attribute continuation.
/// `lead` is what the pattern starts with.
fn expr_error_arm(lead: &TokenStream) -> TokenStream {
    let head = head_pat();
    quote! {
        (#lead (expr $($__announce_eo:tt)*) [ #head , #[$($na:tt)*] $($tail:tt)* ] $($r:tt)*) => {
            compile_error! { #DERIVE_IN_EXPR }
        };
    }
}

/// The arms that pass an entry on, one for every next shape (a bang, a named
/// attribute, an anonymous attribute). `lead` is what
/// each pattern starts with, and `entry` is `KEY => [META] { ITEM }` for the
/// head `key`.
fn shape_arms(
    lead: &TokenStream,
    entry: &dyn Fn(&TokenStream) -> TokenStream,
    fallback: &Ident,
) -> TokenStream {
    let (opts_pat, opts_out) = (opts_pat(), opts_out());
    let (head, key) = (head_pat(), head_key());
    let entry = entry(&key);
    let bang = quote! {
        $($np)? $(:: $ns)* ! {
            #opts_out
            [ $($np)? $(:: $ns)* ! $($tail)* ]
            #entry
            $($r)*
        }
    };
    let named = quote! {
        #[$($na)*]
        #[allow(dead_code, non_camel_case_types)]
        enum $nid {
            __AnnouncePayload = (stringify! {
                #opts_out
                [ #[$($na)*] $nid $(, $($tail)*)? ]
                #entry
                $($r)*
            }, 0).1
        }
    };
    let anon = quote! {
        #[$($na)*]
        #[allow(dead_code, non_camel_case_types)]
        enum #fallback {
            __AnnouncePayload = (stringify! {
                #opts_out
                [ #[$($na)*] #fallback $($tail)* ]
                #entry
                $($r)*
            }, 0).1
        }
    };
    quote! {
        (#lead #opts_pat [ #head , $($np:ident)? $(:: $ns:ident)* ! $($tail:tt)* ] $($r:tt)*) => { #bang };
        (#lead #opts_pat [ #head , #[$($na:tt)*] $nid:ident $(, $($tail:tt)*)? ] $($r:tt)*) => { #named };
        (#lead #opts_pat [ #head , #[$($na:tt)*] $($tail:tt)* ] $($r:tt)*) => { #anon };
    }
}

/// `Layout::Direct`: every arm holds its own copy of the item.
///
/// A payload containing `$` is escaped as `$__dollar`, and the arms that pass
/// it on require a leading `@__announce_dollar { $ }`: `$__dollar:tt` binds a
/// real `$`, so substituting it un-escapes the payload in place. The caller
/// does not write the prefix: a bang-head call re-invokes the trampoline
/// once through its own path (the head) with the prefix added, and
/// `announce::helper::redirect` adds it for the derive route. No helper items
/// are emitted, so the expansion is a single macro call in every position.
///
/// `redirect` writes `@__announce_self { path } @__announce_dollar { $ }`
/// before the list, for the layouts that call themselves again; the arms of
/// this layout ignore the self path. A payload
/// without `$` ignores the dollar prefix too.
fn direct_arms(fallback: &Ident, v: &Variant) -> TokenStream {
    let Variant {
        meta,
        payload,
        needs_dollar,
    } = v;
    let prefix = if *needs_dollar {
        quote! { $( @__announce_self { $($__announce_s:tt)* } )? @__announce_dollar { $__dollar:tt } }
    } else {
        quote! { $( @__announce_self { $($__announce_s:tt)* } @__announce_dollar { $__announce_d:tt } )? }
    };
    let mut out = expr_error_arm(&TokenStream::new());
    out.extend(shape_arms(
        &prefix,
        &|key| quote! { #key => [ #meta ] { #payload } },
        fallback,
    ));
    if *needs_dollar {
        out.extend(quote! {
            // Re-invoke through the head path, adding a real `$`.
            ($( ($($o:tt)*) )? [ $($hp:ident)? $(:: $hs:ident)* ! $($l:tt)* ] $($r:tt)*) => {
                $($hp)? $(:: $hs)* ! { @__announce_dollar { $ } $( ($($o)*) )? [ $($hp)? $(:: $hs)* ! $($l)* ] $($r)* }
            };
        });
    }
    out
}

/// `Layout::Single`: the item is in one arm, and the routing and output arms
/// carry none.
///
/// 1. A call `($(opts)? [head! ..] REST)` has its head's path re-invoked with
///    `@__announce_self { head path } @__announce_dollar { $ }` before it
///    (the derive route's `redirect` writes the same prefix).
/// 2. The only arm that holds the item binds that prefix and calls the
///    trampoline again through the path it names, with
///    `@__announce_emit [META] { ITEM }` in front of the rest. `$__dollar:tt`
///    is a real `$`, so substituting it un-escapes the item.
/// 3. The output arms match `[META]` and `{ ITEM }` as one `tt` each, so the
///    item is not read again, and write the entry for every next shape.
///
/// Every call after the first starts with `@`, which routing arms cannot
/// match: the three levels never loop. Anything else ends at the catch-all.
fn single_arms(fallback: &Ident, v: &Variant) -> TokenStream {
    let Variant {
        meta,
        payload,
        needs_dollar,
    } = v;
    let dollar = if *needs_dollar {
        quote! { $__dollar:tt }
    } else {
        quote! { $__announce_ignored:tt }
    };
    let mut out = quote! {
        (@__announce_self { $($__announce_s:tt)* } @__announce_dollar { #dollar } $($__announce_a:tt)*) => {
            $($__announce_s)* ! { @__announce_emit [ #meta ] { #payload } $($__announce_a)* }
        };
    };
    let emit = quote! { @__announce_emit $__announce_m:tt $__announce_i:tt };
    out.extend(expr_error_arm(&emit));
    out.extend(shape_arms(
        &emit,
        &|key| quote! { #key => $__announce_m $__announce_i },
        fallback,
    ));
    out.extend(quote! {
        ($( ($($o:tt)*) )? [ $($hp:ident)? $(:: $hs:ident)* ! $($l:tt)* ] $($r:tt)*) => {
            $($hp)? $(:: $hs)* ! {
                @__announce_self { $($hp)? $(:: $hs)* } @__announce_dollar { $ }
                $( ($($o)*) )? [ $($hp)? $(:: $hs)* ! $($l)* ] $($r)*
            }
        };
    });
    out
}

/// The arms of the chosen layout, and the catch-all error arm.
fn arms(fallback: &Ident, plain: &Variant, single: bool) -> TokenStream {
    let mut out = if single {
        single_arms(fallback, plain)
    } else {
        direct_arms(fallback, plain)
    };
    out.extend(quote! {
        ($($t:tt)*) => { compile_error! { #BAD_INPUT } };
    });
    out
}
/// The generated macro and its re-export, carrying `payload` (item tokens)
/// with META = checks, `meta =` entries, `attrs(..)` (the owned attribute
/// names), `mock(..)` and `extra_meta`. (The code goes through
/// [`trampoline_with`]; this is its form for tokens that need no more than the
/// escaping, which the tests use.)
#[cfg(test)]
pub fn trampoline(
    ident: &Ident,
    vis: &Visibility,
    opts: &Options,
    owned: &OwnedAttrs,
    random: u64,
    payload: TokenStream,
    extra_meta: TokenStream,
) -> TokenStream {
    let pass = Pass {
        cut: 0,
        watch: false,
        escape: true,
        krate: true,
    };
    trampoline_with(
        ident,
        vis,
        opts,
        owned,
        random,
        (&payload, pass),
        extra_meta,
        &mut Timer::off(),
    )
}

/// [`trampoline`] with the pass that makes the payload from `source`: the
/// escaping of `$`, the rewriting of `crate::` and (for the tokens of a
/// [`Prepared`]) the dropping of the marked attributes are one pass over it.
#[allow(clippy::too_many_arguments)]
fn trampoline_with(
    ident: &Ident,
    vis: &Visibility,
    opts: &Options,
    owned: &OwnedAttrs,
    random: u64,
    (source, pass): (&TokenStream, Pass),
    extra_meta: TokenStream,
    t: &mut Timer,
) -> TokenStream {
    let h = hex(random);
    let macro_name = opts.macro_name.clone().unwrap_or_else(|| ident.clone());
    // `r#match` is not usable inside a longer identifier.
    let hidden = Ident::new(
        &format!("__announce_{}_{}", syn::ext::IdentExt::unraw(ident), h),
        Span::call_site(),
    );
    let fallback = Ident::new(&format!("__AnnounceTrampoline_{}", h), Span::call_site());
    let export = matches!(vis, Visibility::Public(_));

    let mut meta_items: Vec<TokenStream> = opts
        .checks
        .iter()
        .chain(&opts.metas)
        .map(|c| {
            let p = &c.path;
            match &c.args {
                Some(a) => quote! { #p(#a) },
                None => quote! { #p },
            }
        })
        .collect();
    if !owned.is_empty() {
        meta_items.push(quote! { #owned });
    }
    if let Some(m) = &opts.mock {
        meta_items.push(quote! { mock(#m) });
    }
    if !extra_meta.is_empty() {
        meta_items.push(extra_meta);
    }
    let (meta, meta_count) = apply(
        &quote! { #(#meta_items),* },
        &Pass {
            cut: 0,
            watch: false,
            escape: false,
            krate: true,
        },
    );
    // The payload is made once, here, and shared by every arm (a stream is a
    // handle: quoting it again does not copy it).
    let (payload, counts) = apply(source, &pass);
    t.step("payload");
    let single = is_single(opts.layout, meta_count.tokens + counts.tokens);
    // Global options other than a leading `expr` are not interpreted by the
    // trampoline; they are forwarded unchanged (a namespace for helpers, e.g.
    // `gather`'s bookkeeping).
    let plain = Variant {
        meta,
        payload,
        needs_dollar: counts.dollar,
    };
    let arms = arms(&fallback, &plain, single);
    let out = quote! {
        #[doc(hidden)]
        // The payload holds escaped `$crate` / `crate::` tokens (false
        // positive), and `pub` items in fn bodies are fine.
        #[allow(unknown_lints, clippy::crate_in_macro_def, non_local_definitions)]
        #(if export) { #[macro_export] }
        macro_rules! #hidden { #arms }
        #[doc(hidden)]
        #[allow(unused_imports)]
        #vis use #hidden as #macro_name;
    };
    t.step("quote");
    out
}

/// Whether `generate` emits the real item itself: when something is removed from
/// it, the derive emits it instead of `#[announce]`.
fn emits_real_item(opts: &Options, owned: &OwnedAttrs) -> bool {
    !owned.is_empty() && opts.emit_item && opts.mock.is_none() && !opts.derived
}

/// What stands in for `generate` when it fails: the real item, if `generate`
/// would have emitted it. Without it the error would be followed by errors about
/// the missing item.
#[cfg(test)]
pub fn real_item_on_error(item: Item, opts: &Options, owned: &OwnedAttrs) -> TokenStream {
    if !emits_real_item(opts, owned) {
        return TokenStream::new();
    }
    real_item(&prepare(item, None, opts, owned), opts, owned)
}

/// The real item, if `generate` emits it ([`emits_real_item`]).
pub fn real_item(prep: &Prepared, opts: &Options, owned: &OwnedAttrs) -> TokenStream {
    if emits_real_item(opts, owned) {
        compiled(prep)
    } else {
        TokenStream::new()
    }
}

/// Everything the `__Capture` derive emits for one cfg-evaluated item.
///
/// `owned` are the attribute names that the checks own. The payload and the
/// checks get the item with those attributes, since callbacks and checks read
/// them. The copies that rustc compiles lose them: the real item (which this
/// emits, instead of `#[announce]`, whenever there is anything to remove) and
/// the hidden copy of a `mock`.
///
/// The payload and the checks also lose the string doc attributes, unless
/// `opts.docs` is set; they get the same item. The copies that rustc compiles
/// keep their docs, so rustdoc shows what the author wrote.
#[cfg(test)]
pub fn generate(
    item: Item,
    opts: &Options,
    random: u64,
    owned: &OwnedAttrs,
) -> Result<TokenStream> {
    generate_timed(item, opts, random, owned, &mut Timer::off())
}

/// [`generate`], with the steps timed by `t`.
pub(crate) fn generate_timed(
    item: Item,
    opts: &Options,
    random: u64,
    owned: &OwnedAttrs,
    t: &mut Timer,
) -> Result<TokenStream> {
    let prep = prepare(item, None, opts, owned);
    t.step("strip");
    emit(&prep, opts, random, owned, t)
}

/// [`generate`] for a [`Prepared`] item. The steps are timed by `t` (the caller
/// has ended the step of `prepare`).
pub(crate) fn emit(
    prep: &Prepared,
    opts: &Options,
    random: u64,
    owned: &OwnedAttrs,
    t: &mut Timer,
) -> Result<TokenStream> {
    let item = &prep.item;
    let (ident, vis) = ident_vis(item)?;
    let h = hex(random);

    // What the compiler sees: without the owned attributes, with the docs.
    let real_item = real_item(prep, opts, owned);
    // What callbacks and checks get: the same item, without its docs unless
    // `docs = true` (a smaller trampoline). Made only for the checks; the
    // trampoline makes its payload in its own pass.
    let checks_item = (!opts.checks.is_empty()).then(|| plain(prep));
    let checks = opts.checks.iter().map(|c| {
        let p = &c.path;
        let a = c.args.clone().unwrap_or_default();
        quote! { #p! { (#a), #checks_item } }
    });
    let mock_copy = opts.mock.as_ref().map(|_| {
        let m = Ident::new(&format!("__announce_mock_{}", h), Span::call_site());
        let compiled = compiled(prep);
        quote! {
            #[doc(hidden)]
            #[allow(dead_code, non_camel_case_types, unused_imports, unused_variables)]
            mod #m {
                #[allow(unused_imports)]
                use super::*;
                #compiled
            }
        }
    });
    t.step("views");
    let marker = marker_impl(item, opts);
    t.step("marker");

    // `repeater` is required when the item has types that do not resolve
    // outside this crate. When every path in the item certainly resolves
    // everywhere there is nothing to find, and type-leak is not run.
    let fast = opts.repeater.is_none() && crate::fastpath::accepts(item, opts);
    let (leak_impls, leak_meta) = if fast {
        #[cfg(test)]
        crate::fastpath::assert_agrees(item, opts);
        (TokenStream::new(), TokenStream::new())
    } else {
        match crate::leak::table(item, opts, random)? {
            Some(l) => (l.impls, l.meta),
            None => (TokenStream::new(), TokenStream::new()),
        }
    };
    t.step_labelled("analysis", if fast { "fast" } else { "full" });
    let payload = match &checks_item {
        // The item of the checks has lost what the payload loses already.
        Some(plain) => (
            plain,
            Pass {
                cut: 0,
                watch: false,
                escape: true,
                krate: true,
            },
        ),
        None => (
            &prep.printed,
            Pass {
                cut: CFG | DOC,
                watch: prep.found.any(),
                escape: true,
                krate: true,
            },
        ),
    };
    let tramp = trampoline_with(&ident, &vis, opts, owned, random, payload, leak_meta, t);

    Ok(quote! {
        #real_item
        #tramp
        #leak_impls
        #(#checks)*
        #mock_copy
        #marker
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `$` escaped as `$__dollar`, on its own (the passes of the code are one).
    fn escape_dollar(ts: TokenStream, found: &mut bool) -> TokenStream {
        let pass = Pass {
            cut: 0,
            watch: false,
            escape: true,
            krate: false,
        };
        let (out, counts) = apply(&ts, &pass);
        *found |= counts.dollar;
        out
    }

    /// `crate::` rewritten as `$crate::`, on its own.
    fn crate_to_dollar_crate(ts: TokenStream) -> TokenStream {
        let pass = Pass {
            cut: 0,
            watch: false,
            escape: false,
            krate: true,
        };
        apply(&ts, &pass).0
    }

    /// The item without its string docs, as the payload has it.
    fn strip_docs(item: Item) -> Result<TokenStream> {
        Ok(plain(&prepare(
            item,
            None,
            &Options::default(),
            &OwnedAttrs::new(),
        )))
    }

    /// The item without its true `#[cfg]`s, as rustc compiles it (no owned names).
    fn strip_true_cfgs(item: Item) -> Result<(Item, TokenStream)> {
        let tokens = compiled(&prepare(
            item,
            None,
            &Options::default(),
            &OwnedAttrs::new(),
        ));
        Ok((syn::parse2(tokens.clone())?, tokens))
    }

    fn squash(ts: impl ToTokens) -> String {
        ts.to_token_stream()
            .to_string()
            .split_whitespace()
            .collect()
    }

    fn dc(ts: TokenStream) -> String {
        squash(crate_to_dollar_crate(ts))
    }

    #[test]
    fn hex_is_sixteen_lowercase_digits() {
        assert_eq!(hex(0), "0000000000000000");
        assert_eq!(hex(255), "00000000000000ff");
        assert_eq!(hex(u64::MAX), "ffffffffffffffff");
    }

    #[test]
    fn escape_dollar_sets_the_flag_and_escapes_every_dollar() {
        let mut found = false;
        let out = escape_dollar(quote! { a $ b $ c }, &mut found);
        assert!(found);
        assert_eq!(squash(out), "a$__dollarb$__dollarc");
    }

    #[test]
    fn escape_dollar_recurses_into_nested_groups() {
        let mut found = false;
        let out = escape_dollar(quote! { f($x, [{ $y }]) }, &mut found);
        assert!(found);
        assert_eq!(squash(out), "f($__dollarx,[{$__dollary}])");
    }

    #[test]
    fn escape_dollar_keeps_delimiters_and_leaves_dollar_free_input_alone() {
        let mut found = false;
        let input = quote! { fn f(a: u8) -> [u8; 2] { (1, 2) } };
        let out = escape_dollar(input.clone(), &mut found);
        assert!(!found);
        assert_eq!(squash(&out), squash(&input));
        let delims: Vec<Delimiter> = out
            .into_iter()
            .filter_map(|t| {
                if let TokenTree::Group(g) = t {
                    Some(g.delimiter())
                } else {
                    None
                }
            })
            .collect();
        assert_eq!(
            delims,
            [Delimiter::Parenthesis, Delimiter::Bracket, Delimiter::Brace]
        );
    }

    #[test]
    fn escape_dollar_never_clears_the_flag() {
        let mut found = true;
        escape_dollar(quote! { a b }, &mut found);
        assert!(found);
        let mut found = false;
        escape_dollar(TokenStream::new(), &mut found);
        assert!(!found);
    }

    #[test]
    fn rewrites_path_leading_crate() {
        assert_eq!(dc(quote! { crate::x }), "$crate::x");
        assert_eq!(dc(quote! { crate::a::b::c() }), "$crate::a::b::c()");
        assert_eq!(dc(quote! { struct S(crate::T); }), "structS($crate::T);");
        assert_eq!(
            dc(quote! { <crate::T as crate::Tr>::X }),
            "<$crate::Tas$crate::Tr>::X"
        );
        assert_eq!(dc(quote! { &crate::T }), "&$crate::T");
    }

    #[test]
    fn inserted_dollar_is_a_lone_punct_with_the_span_of_crate() {
        let out = crate_to_dollar_crate(quote! { crate::x });
        let tts: Vec<TokenTree> = out.into_iter().collect();
        match (&tts[0], &tts[1]) {
            (TokenTree::Punct(d), TokenTree::Ident(c)) => {
                assert_eq!(d.as_char(), '$');
                assert_eq!(d.spacing(), Spacing::Alone);
                assert_eq!(c.to_string(), "crate");
            }
            other => panic!("{:?}", other),
        }
    }

    #[test]
    fn rewrites_inside_nested_groups() {
        assert_eq!(
            dc(quote! { f(crate::a, [crate::b], { crate::c }) }),
            "f($crate::a,[$crate::b],{$crate::c})"
        );
    }

    #[test]
    fn keeps_pub_crate_and_pub_in_crate() {
        assert_eq!(dc(quote! { pub(crate) struct S; }), "pub(crate)structS;");
        assert_eq!(
            dc(quote! { pub(in crate::m) struct S; }),
            "pub(incrate::m)structS;"
        );
        assert_eq!(
            dc(quote! { pub(in crate) fn f() {} }),
            "pub(incrate)fnf(){}"
        );
        // A bare `crate` that is not followed by `::`.
        assert_eq!(dc(quote! { use crate; }), "usecrate;");
        assert_eq!(dc(quote! { a.crate }), "a.crate");
        // Also inside other groups: a field, a module body.
        assert_eq!(
            dc(quote! { struct S(pub(in crate::m) u8, crate::T); }),
            "structS(pub(incrate::m)u8,$crate::T);"
        );
        assert_eq!(
            dc(quote! { mod m { pub(in crate::a::b) fn f(x: crate::T) {} } }),
            "modm{pub(incrate::a::b)fnf(x:$crate::T){}}"
        );
    }

    #[test]
    fn crate_after_an_ordinary_in_is_rewritten() {
        // Only the `in` of `pub(in ..)` is the visibility's.
        assert_eq!(dc(quote! { for x in crate::ARR {} }), "forxin$crate::ARR{}");
        assert_eq!(
            dc(quote! { const C: u8 = { let mut s = 0; for x in crate::ARR { s += x; } s }; }),
            "constC:u8={letmuts=0;forxin$crate::ARR{s+=x;}s};"
        );
        // `in` inside parentheses that do not follow `pub`.
        assert_eq!(dc(quote! { f(in crate::x) }), "f(in$crate::x)");
        assert_eq!(dc(quote! { not_pub(in crate::x) }), "not_pub(in$crate::x)");
        // `pub in crate::x` without the parentheses is not a visibility.
        assert_eq!(dc(quote! { pub in crate::x }), "pubin$crate::x");
        // Next to a visibility.
        assert_eq!(
            dc(quote! { pub(in crate::m) fn f() { for x in crate::ARR {} } }),
            "pub(incrate::m)fnf(){forxin$crate::ARR{}}"
        );
        assert_eq!(dc(quote! { pub(in a in crate::m) }), "pub(inain$crate::m)");
    }

    #[test]
    fn keeps_an_escaped_dollar_crate() {
        // `$crate` written by the user is escaped first (`$ __dollar crate`):
        // adding a `$` would unescape nothing and break the pair.
        let mut found = false;
        let escaped = escape_dollar(quote! { $crate::x }, &mut found);
        assert!(found);
        assert_eq!(squash(crate_to_dollar_crate(escaped)), "$__dollarcrate::x");
        // An unescaped `$ crate` is left alone as well.
        assert_eq!(dc(quote! { $crate::x }), "$crate::x");
        // Both side by side.
        let mut found = false;
        let both =
            crate_to_dollar_crate(escape_dollar(quote! { $crate::x + crate::y }, &mut found));
        assert_eq!(squash(both), "$__dollarcrate::x+$crate::y");
    }

    #[test]
    fn keeps_crate_after_a_path_separator_but_not_after_a_single_colon() {
        // `a::crate::b`: `crate` is a path segment after `::`.
        assert_eq!(dc(quote! { a::crate::b }), "a::crate::b");
        assert_eq!(dc(quote! { ::crate::b }), "::crate::b");
        // `x: crate::T` is a type ascription after a single colon.
        assert_eq!(dc(quote! { fn f(x: crate::T) {} }), "fnf(x:$crate::T){}");
        assert_eq!(
            dc(quote! { struct S { a: crate::T, b: Vec<crate::U> } }),
            "structS{a:$crate::T,b:Vec<$crate::U>}"
        );
        assert_eq!(dc(quote! { x: crate::T }), "x:$crate::T");
        // A first colon that is not joined to the second is not a `::`.
        let colon = |spacing| TokenTree::Punct(proc_macro2::Punct::new(':', spacing));
        let split: TokenStream = [
            TokenTree::Ident(Ident::new("crate", Span::call_site())),
            colon(Spacing::Alone),
            colon(Spacing::Alone),
            TokenTree::Ident(Ident::new("x", Span::call_site())),
        ]
        .into_iter()
        .collect();
        assert_eq!(squash(crate_to_dollar_crate(split)), "crate::x");
    }

    #[test]
    fn crate_to_dollar_crate_is_not_applied_twice_to_the_same_path() {
        let once = crate_to_dollar_crate(quote! { crate::x });
        assert_eq!(squash(crate_to_dollar_crate(once)), "$crate::x");
    }

    #[test]
    fn token_count_counts_leaves_and_both_delimiters() {
        assert_eq!(token_count(&TokenStream::new()), 0);
        assert_eq!(token_count(&quote! { a b c }), 3);
        // `a`, `(`..`)`, `b`: 1 + 2 + 1 + 1 for the group's contents.
        assert_eq!(token_count(&quote! { a (b) }), 4);
        assert_eq!(
            token_count(&quote! { f(a, [b], {}) }),
            1 + 2 + 1 + 1 + (2 + 1) + 1 + 2
        );
        // Punctuation counts one token each.
        assert_eq!(token_count(&quote! { a :: b }), 4);
    }

    #[test]
    fn auto_layout_switches_above_the_threshold() {
        assert!(!is_single(Layout::Auto, 0));
        assert!(!is_single(Layout::Auto, SINGLE_COPY_THRESHOLD));
        assert!(is_single(Layout::Auto, SINGLE_COPY_THRESHOLD + 1));
        assert!(is_single(Layout::Auto, usize::MAX));
        // The option wins over the size.
        for size in [0, SINGLE_COPY_THRESHOLD, SINGLE_COPY_THRESHOLD + 1] {
            assert!(is_single(Layout::Single, size));
            assert!(!is_single(Layout::Direct, size));
        }
    }

    /// The generated macro for `payload` (`pub struct S { .. }`), as a string.
    fn emitted(layout: Layout, payload: TokenStream) -> String {
        let opts = Options {
            layout,
            ..Options::default()
        };
        let ident = Ident::new("S", Span::call_site());
        let vis: Visibility = syn::parse_quote!(pub);
        squash(trampoline(
            &ident,
            &vis,
            &opts,
            &OwnedAttrs::new(),
            1,
            payload,
            TokenStream::new(),
        ))
    }

    /// `pub struct S { f0: u8, .. }` with `n` fields, the first named `marker_zzz`.
    fn struct_of(n: usize) -> TokenStream {
        let fields = (0..n).map(|i| {
            let f = if i == 0 {
                Ident::new("marker_zzz", Span::call_site())
            } else {
                Ident::new(&format!("f{}", i), Span::call_site())
            };
            quote! { pub #f: u8, }
        });
        quote! { pub struct S { #(#fields)* } }
    }

    #[test]
    fn direct_layout_copies_the_item_into_three_arms() {
        let out = emitted(Layout::Direct, struct_of(3));
        assert_eq!(out.matches("marker_zzz").count(), 3, "{}", out);
        assert!(!out.contains("__announce_emit"), "{}", out);
        // The derive route's prefix is optional in each arm and ignored;
        // without `$` in the item there is no re-invoking arm.
        assert_eq!(out.matches("@__announce_self{").count(), 3, "{}", out);
        assert!(!out.contains("__announce_dollar{$}"), "{}", out);
    }

    #[test]
    fn single_layout_holds_the_item_once() {
        let out = emitted(Layout::Single, struct_of(3));
        assert_eq!(out.matches("marker_zzz").count(), 1, "{}", out);
        // The item arm, the call it makes, and the output arms (three next
        // shapes, and the `(expr)` error) matching the two groups as one `tt`
        // each.
        assert_eq!(out.matches("@__announce_emit[").count(), 1, "{}", out);
        assert_eq!(
            out.matches("@__announce_emit$__announce_m:tt$__announce_i:tt")
                .count(),
            4,
            "{}",
            out
        );
        // One routing arm, which adds the self path and a real `$`.
        assert_eq!(
            out.matches("@__announce_self{$($hp)?$(::$hs)*}@__announce_dollar{$}")
                .count(),
            1,
            "{}",
            out
        );
    }

    #[test]
    fn single_layout_unescapes_dollar_only_in_the_item_arm() {
        let payload = quote! { pub mod m { macro_rules! k { ($($x:tt)*) => {}; } } };
        let out = emitted(Layout::Single, payload.clone());
        // The binder of the item arm, and the two `$` of the item.
        assert_eq!(out.matches("$__dollar").count(), 1 + 2, "{}", out);
        assert!(out.contains("@__announce_dollar{$__dollar:tt}"), "{}", out);
        // The direct layout needs the prefix in each of its arms.
        let out = emitted(Layout::Direct, payload);
        assert_eq!(
            out.matches("@__announce_dollar{$__dollar:tt}").count(),
            3,
            "{}",
            out
        );
    }

    #[test]
    fn auto_layout_follows_the_size_of_the_item() {
        let small = emitted(Layout::Auto, struct_of(10));
        assert_eq!(small.matches("marker_zzz").count(), 3);
        // 5 tokens per field: `pub` `name` `:` `u8` `,`.
        let n = SINGLE_COPY_THRESHOLD / 5 + 10;
        let large = emitted(Layout::Auto, struct_of(n));
        assert_eq!(large.matches("marker_zzz").count(), 1);
        // Just below the threshold the item is still copied.
        let below = (SINGLE_COPY_THRESHOLD - 10) / 5;
        assert!(token_count(&struct_of(below)) <= SINGLE_COPY_THRESHOLD);
        let edge = emitted(Layout::Auto, struct_of(below));
        assert_eq!(edge.matches("marker_zzz").count(), 3);
    }

    #[test]
    fn meta_counts_towards_the_size() {
        // A small item with a large META is single too: META is held once.
        let opts = Options {
            layout: Layout::Auto,
            ..Options::default()
        };
        let ident = Ident::new("S", Span::call_site());
        let vis: Visibility = syn::parse_quote!(pub);
        let extra: TokenStream = std::iter::repeat(quote! { k(1, 2), })
            .take(SINGLE_COPY_THRESHOLD / 5)
            .flatten()
            .collect();
        let out = squash(trampoline(
            &ident,
            &vis,
            &opts,
            &OwnedAttrs::new(),
            1,
            quote! { pub struct S { pub marker_zzz: u8 } },
            extra,
        ));
        assert_eq!(out.matches("marker_zzz").count(), 1, "{}", out);
    }

    #[test]
    fn both_layouts_end_with_one_shared_catch_all() {
        for layout in [Layout::Direct, Layout::Single] {
            let out = emitted(layout, struct_of(2));
            assert_eq!(
                out.matches(squash(BAD_INPUT).as_str()).count(),
                1,
                "{}",
                out
            );
            assert!(out.contains(squash(DERIVE_IN_EXPR).as_str()), "{}", out);
        }
    }

    fn generate_for(opts: &str, owned: &[&str], item: TokenStream) -> String {
        let opts: Options = syn::parse2(opts.parse().unwrap()).unwrap();
        let item: Item = syn::parse2(item).unwrap();
        squash(generate(item, &opts, 3, &OwnedAttrs::from_strs(owned)).unwrap())
    }

    #[test]
    fn owned_attributes_stay_in_the_payload_and_the_checks_only() {
        let item = quote! {
            #[my_lib(item)]
            pub struct S { #[my_lib(skip)] #[doc = "kept"] pub a: u8, #[other] pub b: u8 }
        };
        let out = generate_for(
            "crate = ::l::announce, c::check(x)",
            &["my_lib"],
            item.clone(),
        );
        // The compiler sees the item without them (the dummy is not emitted by `announce`).
        assert!(
            out.starts_with("pubstructS{#[doc=\"kept\"]puba:u8,#[other]pubb:u8}"),
            "{}",
            out
        );
        // The payload and the check have them (once each: the macro holds one copy per
        // arm of the layout, so count the check's), but not the doc: both lose it.
        assert!(
            out.contains("c::check!{(x),#[my_lib(item)]pubstructS{#[my_lib(skip)]puba:u8,#[other]"),
            "{}",
            out
        );
        assert!(
            out.contains(
                "=>[c::check(x),attrs(my_lib)]{#[my_lib(item)]pubstructS{#[my_lib(skip)]puba:u8"
            ),
            "{}",
            out
        );
        assert_eq!(out.matches("#[doc=\"kept\"]").count(), 1, "{}", out);
        // `docs = true` keeps it in the payload and the check as well.
        let out = generate_for(
            "crate = ::l::announce, docs = true, c::check(x)",
            &["my_lib"],
            item.clone(),
        );
        assert!(
            out.contains(
                "c::check!{(x),#[my_lib(item)]pubstructS{#[my_lib(skip)]#[doc=\"kept\"]puba:u8"
            ),
            "{}",
            out
        );
        assert!(
            out.contains("=>[c::check(x),attrs(my_lib)]{#[my_lib(item)]pubstructS{#[my_lib(skip)]#[doc=\"kept\"]"),
            "{}",
            out
        );
        // No owned name: the item is the caller's to emit, and nothing is stripped.
        let out = generate_for("crate = ::l::announce", &[], item);
        assert!(out.starts_with("#[doc(hidden)]"), "{}", out);
        assert!(out.contains("=>[]{#[my_lib(item)]pubstructS"), "{}", out);
    }

    #[test]
    fn owned_attributes_are_not_in_the_copies_that_rustc_compiles() {
        let item = quote! { pub trait T { #[my_lib(m)] fn f(&self); } };
        // A mock: its hidden copy is compiled, so it has none; the payload does.
        let out = generate_for(
            "crate = ::l::announce, mock = a::T",
            &["my_lib"],
            item.clone(),
        );
        let hidden = out.split("modsuper").next().unwrap().to_string();
        assert!(
            out.contains("mod__announce_mock_0000000000000003{"),
            "{}",
            out
        );
        assert!(!out.starts_with("pubtrait"), "{}", out);
        let copy = out.split("usesuper::*;").nth(1).unwrap();
        assert!(copy.starts_with("pubtraitT{fnf(&self);}}"), "{}", copy);
        assert!(out.contains("mock(a::T)"), "{}", hidden);
        assert!(
            out.contains("=>[attrs(my_lib),mock(a::T)]{pubtraitT{#[my_lib(m)]fnf(&self);}}"),
            "{}",
            out
        );
        // `emit_item = false`: nothing is emitted for rustc at all.
        let out = generate_for("emit_item = false", &["my_lib"], item.clone());
        assert!(out.starts_with("#[doc(hidden)]"), "{}", out);
        // A derive does not emit its input either.
        let opts = Options {
            derived: true,
            emit_item: false,
            ..Options::default()
        };
        let out = squash(
            generate(
                syn::parse2(item).unwrap(),
                &opts,
                3,
                &OwnedAttrs::from_strs(&["my_lib"]),
            )
            .unwrap(),
        );
        assert!(out.starts_with("#[doc(hidden)]"), "{}", out);
    }

    #[test]
    fn a_failed_generate_still_gives_the_real_item() {
        let owned = OwnedAttrs::from_strs(&["my_lib"]);
        let opts = Options::default();
        // With owned names the derive would have emitted the item.
        let item2: Item =
            syn::parse2(quote! { pub struct S { #[my_lib] #[cfg(a)] pub a: u8 } }).unwrap();
        let out = squash(real_item_on_error(item2.clone(), &opts, &owned));
        assert_eq!(out, "pubstructS{puba:u8}");
        // Otherwise `#[announce]` emitted it already, or nobody does.
        assert!(real_item_on_error(item2.clone(), &opts, &OwnedAttrs::new()).is_empty());
        let no_emit = Options {
            emit_item: false,
            ..Options::default()
        };
        assert!(real_item_on_error(item2, &no_emit, &owned).is_empty());
    }

    #[test]
    fn string_docs_are_stripped_everywhere_but_in_macro_bodies() {
        let item: Item = syn::parse2(quote! {
            /// DOCMARK item
            #[doc = "DOCMARK explicit"]
            #[doc(hidden)]
            #[doc(alias = "other")]
            pub mod m {
                //! DOCMARK inner
                #![doc = "DOCMARK inner explicit"]
                #![doc(html_no_source)]
                /** DOCMARK block */
                pub struct S<#[doc = "DOCMARK generic"] T> {
                    /// DOCMARK field
                    pub x: T,
                }
                pub enum E {
                    /// DOCMARK variant
                    A(#[doc = "DOCMARK tuple field"] u8),
                }
                pub trait Tr {
                    /// DOCMARK method
                    #[doc(inline)]
                    fn f(&self) {
                        /// DOCMARK statement
                        let _ = 0;
                    }
                    /// DOCMARK assoc
                    type A;
                }
                macro_rules! k { () => { #[doc = "in a macro"] }; }
                k!(#[doc = "in a call"] x);
            }
        })
        .unwrap();
        let out = squash(strip_docs(item).unwrap());
        assert!(
            !out.contains("on a") && !out.contains("DOCMARK explicit"),
            "{}",
            out
        );
        assert!(
            !out.contains("inner doc") && !out.contains("block doc"),
            "{}",
            out
        );
        assert!(!out.contains("__announce_removed_attribute"), "{}", out);
        // Non-string `doc(..)` attributes stay, also the inner one.
        assert!(
            out.contains("#[doc(hidden)]#[doc(alias=\"other\")]pubmodm{#![doc(html_no_source)]"),
            "{}",
            out
        );
        assert!(out.contains("#[doc(inline)]fnf(&self)"), "{}", out);
        // Macro bodies are opaque.
        assert!(
            out.contains("macro_rules!k{()=>{#[doc=\"inamacro\"]};}k!(#[doc=\"inacall\"]x);"),
            "{}",
            out
        );
        // Nothing to strip: unchanged.
        let item: Item = syn::parse2(quote! { #[doc(hidden)] pub struct S; }).unwrap();
        assert_eq!(
            squash(strip_docs(item).unwrap()),
            "#[doc(hidden)]pubstructS;"
        );
    }

    #[test]
    fn docs_are_stripped_from_the_payload_and_the_checks_not_from_the_compiled_copy() {
        let item = quote! {
            /// the item
            #[my_lib(item)]
            pub struct S {
                /// a field
                pub a: u8,
            }
        };
        // A mock: the hidden copy that rustc compiles keeps its docs, the
        // payload and the check do not.
        let out = generate_for(
            "crate = ::l::announce, mock = a::S, c::check",
            &[],
            item.clone(),
        );
        let copy = out.split("usesuper::*;").nth(1).unwrap();
        assert!(
            copy.starts_with(
                "#[doc=\"theitem\"]#[my_lib(item)]pubstructS{#[doc=\"afield\"]puba:u8,}}"
            ),
            "{}",
            copy
        );
        assert!(
            out.contains("c::check!{(),#[my_lib(item)]pubstructS{puba:u8,}}"),
            "{}",
            out
        );
        assert!(
            out.contains("=>[c::check,mock(a::S)]{#[my_lib(item)]pubstructS{puba:u8,}}"),
            "{}",
            out
        );
        assert_eq!(out.matches("theitem").count(), 1, "{}", out);
        // `docs = true`: the payload has them too.
        let out = generate_for("crate = ::l::announce, mock = a::S, docs = true", &[], item);
        assert!(
            out.contains("=>[mock(a::S)]{#[doc=\"theitem\"]#[my_lib(item)]pubstructS{#[doc=\"afield\"]puba:u8,}}"),
            "{}",
            out
        );
    }

    #[test]
    fn true_cfgs_are_stripped_everywhere_but_in_macro_bodies() {
        let item: Item = syn::parse2(quote! {
            #[cfg(a)]
            #[derive(Clone)]
            pub struct S<#[cfg(b)] T> {
                #[cfg(c)]
                pub x: u8,
                pub y: [u8; { #[cfg(d)] 1 }],
            }
        })
        .unwrap();
        let (item, tokens) = strip_true_cfgs(item).unwrap();
        let out = squash(&tokens);
        assert!(
            !out.contains("cfg(a)") && !out.contains("cfg(b)"),
            "{}",
            out
        );
        assert!(
            !out.contains("cfg(c)") && !out.contains("cfg(d)"),
            "{}",
            out
        );
        assert!(
            out.contains("#[derive(Clone)]") && out.contains("pubx:u8"),
            "{}",
            out
        );
        assert!(!out.contains("__announce_removed_attribute"), "{}", out);
        assert_eq!(squash(&item), out);

        // Macro bodies are opaque; an item without `cfg` is returned as it is.
        let item: Item = syn::parse2(quote! {
            #[cfg(a)]
            mod m { #![cfg(w)] m!(#[cfg(z)] x); }
        })
        .unwrap();
        let (_, tokens) = strip_true_cfgs(item).unwrap();
        let out = squash(&tokens);
        assert!(
            out.contains("m!(#[cfg(z)]x)") && !out.contains("cfg(a)") && !out.contains("cfg(w)"),
            "{}",
            out
        );
    }

    // ---- the passes over the tokens -----------------------------------------

    /// Both rewrites of the payload in one pass.
    fn fused(ts: TokenStream) -> (TokenStream, Counts) {
        let pass = Pass {
            cut: 0,
            watch: false,
            escape: true,
            krate: true,
        };
        apply(&ts, &pass)
    }

    const PAYLOADS: &[&str] = &[
        "pub struct S { pub a: crate::T, pub b: Vec<crate::U> }",
        "pub(crate) struct S(pub(in crate::m) u8, crate::T);",
        "pub fn f(x: crate::T) { for y in crate::ARR { let _ = $x; } }",
        "mod m { macro_rules! k { ($($x:expr),*) => { 0 $(+ $x)* }; } pub fn f() -> u8 { k!(crate::A) } }",
        "const C: u8 = $crate::N + crate::M + a::crate::b + ::crate::c;",
        "struct S { x: $crate::T, y: $ crate::U, z: crate::V }",
        "fn f(a: crate::A) -> crate::B where crate::C: crate::D { crate::g() }",
        "use crate; struct S(pub in crate::x);",
        "trait Tr { fn f(&self) -> crate::T; const N: usize = $crate::K; }",
        "a $ b $ crate :: c",
        "",
    ];

    #[test]
    fn the_fused_pass_equals_the_two_it_replaces() {
        for src in PAYLOADS {
            let ts: TokenStream = src.parse().unwrap();
            let mut found = false;
            let chained = crate_to_dollar_crate(escape_dollar(ts.clone(), &mut found));
            let (both, counts) = fused(ts);
            assert_eq!(squash(&both), squash(&chained), "{}", src);
            assert_eq!(counts.dollar, found, "{}", src);
        }
    }

    #[test]
    fn the_pass_counts_the_tokens_it_produces() {
        for src in PAYLOADS {
            let (out, counts) = fused(src.parse().unwrap());
            assert_eq!(counts.tokens, token_count(&out), "{}", src);
        }
        // The meta stream: only the rewrite of `crate::`.
        let pass = Pass {
            cut: 0,
            watch: false,
            escape: false,
            krate: true,
        };
        let (out, counts) = apply(&quote! { a::check(crate::x), b }, &pass);
        assert_eq!(squash(&out), "a::check($crate::x),b");
        assert_eq!(counts.tokens, token_count(&out));
        assert!(!counts.dollar);
    }

    #[test]
    fn a_pass_that_changes_nothing_is_the_stream_itself() {
        let pass = Pass::attributes(CFG | DOC, Found::default());
        assert!(pass.is_identity());
        let ts = quote! { struct S; };
        let (out, counts) = apply(&ts, &pass);
        assert_eq!(squash(&out), squash(&ts));
        assert_eq!(counts.tokens, 0);
    }

    fn prepared(src: TokenStream, opts: &str, owned: &[&str]) -> (Prepared, Options, OwnedAttrs) {
        let opts: Options = syn::parse2(opts.parse().unwrap()).unwrap();
        let owned = OwnedAttrs::from_strs(owned);
        let item: Item = syn::parse2(src).unwrap();
        (prepare(item, None, &opts, &owned), opts, owned)
    }

    #[test]
    fn marks_decide_what_each_copy_loses() {
        let item = quote! {
            /// docs
            #[cfg(a)]
            #[my_lib(item)]
            pub struct S {
                #[cfg(b)] #[doc = "field"] #[my_lib(skip)] #[other] pub a: u8,
            }
        };
        let (prep, _, _) = prepared(item.clone(), "", &["my_lib"]);
        assert!(prep.found.cfg && prep.found.doc && prep.found.own);
        // The payload: no cfg, no docs; the owned attributes and the others stay.
        let payload = squash(plain(&prep));
        assert_eq!(
            payload,
            "#[my_lib(item)]pubstructS{#[my_lib(skip)]#[other]puba:u8,}"
        );
        // What rustc compiles: no cfg, no owned; the docs stay, as written.
        let compiled = squash(compiled(&prep));
        assert_eq!(
            compiled,
            "#[doc=\"docs\"]pubstructS{#[doc=\"field\"]#[other]puba:u8,}"
        );
        // A mark never reaches a copy.
        for out in [&payload, &compiled] {
            assert!(!out.contains("__announce_cut"), "{}", out);
        }
        // `docs = true`: the payload keeps them.
        let (prep, _, _) = prepared(item, "docs = true", &["my_lib"]);
        assert!(!prep.found.doc);
        assert_eq!(
            squash(plain(&prep)),
            "#[doc=\"docs\"]#[my_lib(item)]pubstructS{#[doc=\"field\"]#[my_lib(skip)]#[other]puba:u8,}"
        );
    }

    #[test]
    fn marks_are_not_made_inside_macros_or_for_other_attributes() {
        let (prep, _, _) = prepared(
            quote! {
                #[doc(hidden)]
                pub mod m {
                    #![doc(html_no_source)]
                    k!(#[cfg(z)] #[doc = "in a call"] #[my_lib] x);
                    macro_rules! k { () => { #[doc = "in a macro"] }; }
                    #[cfg_attr(a, derive(Clone))]
                    pub struct T;
                }
            },
            "",
            &["my_lib"],
        );
        assert!(!prep.found.any());
        let out = squash(plain(&prep));
        assert!(
            out.contains("k!(#[cfg(z)]#[doc=\"inacall\"]#[my_lib]x);"),
            "{}",
            out
        );
        assert!(out.contains("#[doc=\"inamacro\"]"), "{}", out);
        assert!(
            out.contains("#[doc(hidden)]") && out.contains("#![doc(html_no_source)]"),
            "{}",
            out
        );
        assert!(out.contains("#[cfg_attr(a,derive(Clone))]"), "{}", out);
    }

    #[test]
    fn the_tokens_the_item_was_parsed_from_are_kept_when_nothing_is_marked() {
        let tokens = quote! { pub struct S { pub a: u8 } };
        let item: Item = syn::parse2(tokens.clone()).unwrap();
        let (opts, owned) = (Options::default(), OwnedAttrs::new());
        let prep = prepare(item.clone(), Some(tokens.clone()), &opts, &owned);
        assert!(!prep.found.any());
        assert_eq!(squash(&prep.printed), squash(&tokens));
        // With marks the tree is printed (the original has the attributes).
        let documented = quote! { /** x */ pub struct S; };
        let item: Item = syn::parse2(documented.clone()).unwrap();
        let prep = prepare(item, Some(documented), &opts, &owned);
        assert!(prep.found.doc);
        assert!(squash(&prep.printed).contains("__announce_cut_doc"));
        assert_eq!(squash(plain(&prep)), "pubstructS;");
    }

    #[test]
    fn the_payload_is_made_in_one_pass_from_the_printed_tree() {
        // docs and cfgs dropped, `$` escaped, `crate::` rewritten, all at once;
        // with the same result as the passes one after the other.
        let (prep, opts, owned) = prepared(
            quote! {
                /// doc
                #[cfg(a)]
                #[my_lib(crate::x)]
                pub struct S { #[cfg(b)] #[doc = "f"] pub a: crate::T, pub b: m!($x) }
            },
            "",
            &["my_lib"],
        );
        let ident = Ident::new("S", Span::call_site());
        let vis: Visibility = syn::parse_quote!(pub);
        let pass = Pass {
            cut: CFG | DOC,
            watch: true,
            escape: true,
            krate: true,
        };
        let one = trampoline_with(
            &ident,
            &vis,
            &opts,
            &owned,
            1,
            (&prep.printed, pass),
            TokenStream::new(),
            &mut Timer::off(),
        );
        let separately = trampoline(
            &ident,
            &vis,
            &opts,
            &owned,
            1,
            plain(&prep),
            TokenStream::new(),
        );
        assert_eq!(squash(&one), squash(&separately));
        let out = squash(&one);
        assert!(
            out.contains("=>[attrs(my_lib)]{#[my_lib($crate::x)]pubstructS{puba:$crate::T,pubb:m!($__dollarx)}}"),
            "{}",
            out
        );
    }

    #[test]
    fn the_announce_impl_leaves_out_the_attributes_of_the_parameters() {
        let opts = Options::default();
        let (prep, _, _) = prepared(
            quote! { pub struct S<#[cfg(a)] #[doc = "t"] #[my_lib] T, 'a, #[cfg(b)] const N: usize>(T); },
            "",
            &["my_lib"],
        );
        let out = squash(marker_impl(&prep.item, &opts));
        assert!(out.ends_with("forS<'a,T,N>{}"), "{}", out);
        assert!(!out.contains('#'), "{}", out);
    }

    /// The test of the version before the item was parsed once: tokens of the
    /// pieces of the signature (`mentions`), which `trait_is_simple_dyn` replaces
    /// by a walk of the tree.
    fn reference_simple_dyn(t: &syn::ItemTrait) -> bool {
        fn mentions(ts: TokenStream, word: &str) -> bool {
            ts.into_iter().any(|tt| match tt {
                TokenTree::Ident(i) => i == word,
                TokenTree::Group(g) => mentions(g.stream(), word),
                _ => false,
            })
        }
        let mentions_self = |ts: TokenStream| mentions(ts, "Self");
        if !t.generics.params.is_empty()
            || t.generics.where_clause.is_some()
            || t.auto_token.is_some()
        {
            return false;
        }
        let supers_ok = t.supertraits.iter().all(|b| match b {
            syn::TypeParamBound::Lifetime(_) => true,
            syn::TypeParamBound::Trait(tb) => {
                let p = tb.path.to_token_stream().to_string().replace(' ', "");
                tb.lifetimes.is_none()
                    && matches!(
                        p.trim_start_matches("::"),
                        "core::marker::Send"
                            | "core::marker::Sync"
                            | "std::marker::Send"
                            | "std::marker::Sync"
                    )
            }
            _ => false,
        });
        if !supers_ok {
            return false;
        }
        t.items.iter().all(|ti| match ti {
            syn::TraitItem::Fn(f) => {
                let sig = &f.sig;
                let receiver_ok = matches!(
                    sig.inputs.first(),
                    Some(syn::FnArg::Receiver(r)) if r.reference.is_some() && r.colon_token.is_none()
                );
                receiver_ok
                    && sig.asyncness.is_none()
                    && sig
                        .generics
                        .params
                        .iter()
                        .all(|p| matches!(p, syn::GenericParam::Lifetime(_)))
                    && sig.generics.where_clause.is_none()
                    && sig
                        .inputs
                        .iter()
                        .skip(1)
                        .all(|a| !mentions_self(a.to_token_stream()))
                    && !mentions_self(sig.output.to_token_stream())
                    && !mentions(sig.to_token_stream(), "impl")
            }
            _ => false,
        })
    }

    #[test]
    fn simple_dyn_is_what_it_was() {
        let traits = [
            "pub trait T { fn f(&self); }",
            "pub trait T { fn f(&self) -> u8; fn g(&mut self, x: &u8) -> &u8; }",
            "/// doc\n pub trait T { /// m\n #[inline] fn f(&self, #[a] x: u8); }",
            "trait T { fn f(&'static self); fn g<'a>(&'a self, x: &'a u8) -> &'a u8; }",
            "trait T: ::core::marker::Send + ::std::marker::Sync + 'static { fn f(&self); }",
            "trait T: Send { fn f(&self); }",
            "trait T: Base { fn f(&self); }",
            "trait T<'a> { fn f(&self); }",
            "trait T<X> { fn f(&self); }",
            "trait T where u8: Copy { fn f(&self); }",
            "unsafe trait T { fn f(&self); }",
            "trait T { fn f(&self, x: Self); }",
            "trait T { fn f(&self) -> Self; }",
            "trait T { fn f(&self) -> Option<Self>; }",
            "trait T { fn f(&self, x: m!(Self)); }",
            "trait T { fn f(&self, x: m!(y)); }",
            "trait T { fn f(&self, #[a(Self)] x: u8); }",
            "trait T { fn f(&self, #[a(impl)] x: u8); }",
            "trait T { fn f(&self, #[cfg(Self)] x: u8); }",
            "trait T { fn f(&self, #[cfg(impl)] x: u8); }",
            "trait T { fn f(&self, x: impl Debug); }",
            "trait T { fn f(&self) -> impl Debug; }",
            "trait T { fn f(&self, x: Box<impl Debug>); }",
            "trait T { fn f(&self, x: fn(Self)); }",
            "trait T { fn f(self); }",
            "trait T { fn f(self: Box<Self>); }",
            "trait T { fn f(); }",
            "trait T { fn f(&self) { let _: Self = x(); } }",
            "trait T { async fn f(&self); }",
            "trait T { fn f<U>(&self, u: U); }",
            "trait T { fn f(&self) where u8: Copy; }",
            "trait T { type A; fn f(&self); }",
            "trait T { const C: u8; }",
            "trait T { m!(); }",
            "trait T {}",
            "trait T { fn f(&self, x: [u8; 3], y: (u8, bool), z: &[u8]); }",
            "trait T { fn f(&self, Self_: u8); }",
        ];
        for src in traits {
            let item: Item = syn::parse_str(src).unwrap_or_else(|e| panic!("{}: {}", e, src));
            // The reference saw the item without its `#[cfg]`s; the code sees
            // the marked tree.
            let (stripped, _) = strip_true_cfgs(item.clone()).unwrap();
            let (stripped, marked) = match (stripped, item) {
                (Item::Trait(a), Item::Trait(b)) => (a, b),
                _ => unreachable!(),
            };
            let marked = match prepare(
                Item::Trait(marked),
                None,
                &Options::default(),
                &OwnedAttrs::new(),
            )
            .item
            {
                Item::Trait(t) => t,
                _ => unreachable!(),
            };
            assert_eq!(
                trait_is_simple_dyn(&marked),
                reference_simple_dyn(&stripped),
                "{}",
                src
            );
        }
    }
}
