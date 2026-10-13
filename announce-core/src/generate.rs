use crate::options::{Layout, Options};
use announce_helper::OwnedAttrs;
use proc_macro2::{Group, Ident, Span, TokenStream, TokenTree};
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

/// Escapes every `$` as `$__dollar`, recursively (DESIGN.md section 4: the
/// payload is un-escaped by a short-lived `macro_rules!` at each hop).
fn escape_dollar(ts: TokenStream, found: &mut bool) -> TokenStream {
    let d = Ident::new("__dollar", Span::call_site());
    let mut out = Vec::new();
    for tt in ts {
        match tt {
            TokenTree::Punct(p) if p.as_char() == '$' => {
                *found = true;
                out.push(TokenTree::Punct(p));
                out.push(TokenTree::Ident(d.clone()));
            }
            TokenTree::Group(g) => {
                let mut ng = Group::new(g.delimiter(), escape_dollar(g.stream(), found));
                ng.set_span(g.span());
                out.push(TokenTree::Group(ng));
            }
            t => out.push(t),
        }
    }
    out.into_iter().collect()
}

/// Rewrites path-leading `crate::` as `$crate::`, so paths in the payload
/// and META keep referring to the defining crate when the macro is expanded
/// in another crate. `crate` in `pub(crate)` and in `pub(in crate::m)` is left
/// alone: there it is the visibility's own syntax, which takes no `$crate`.
/// That is the `in` that opens the parentheses after `pub`; any other `in`
/// (`for x in crate::ARR {}`) is followed by an ordinary path, which is
/// rewritten. Run after `escape_dollar`, so the inserted `$` is not escaped.
fn crate_to_dollar_crate(ts: TokenStream) -> TokenStream {
    rewrite_crate(ts, false)
}

/// `in_pub_paren`: `ts` is the inside of the parentheses that follow `pub`.
fn rewrite_crate(ts: TokenStream, in_pub_paren: bool) -> TokenStream {
    let tts: Vec<TokenTree> = ts.into_iter().collect();
    let mut out = Vec::with_capacity(tts.len());
    for (i, tt) in tts.iter().enumerate() {
        let prev = i.checked_sub(1).and_then(|j| tts.get(j));
        match tt {
            TokenTree::Ident(id) if id == "crate" => {
                let next_is_path = matches!(
                    (tts.get(i + 1), tts.get(i + 2)),
                    (Some(TokenTree::Punct(a)), Some(TokenTree::Punct(b)))
                        if a.as_char() == ':' && a.spacing() == proc_macro2::Spacing::Joint && b.as_char() == ':'
                );
                let prev2 = i.checked_sub(2).and_then(|j| tts.get(j));
                let after_in_or_sep = match prev {
                    // `pub(in crate::m)`: the `in` is the first token inside the
                    // parentheses of `pub`. And an escaped `$crate` (`$ __dollar
                    // crate`, AUDIT #2).
                    Some(TokenTree::Ident(p)) => {
                        (p == "in" && in_pub_paren && i == 1) || p == "__dollar"
                    }
                    // `a::crate` (a path separator), but not `x: crate::T` (AUDIT #1)
                    Some(TokenTree::Punct(p)) if p.as_char() == ':' => matches!(
                        prev2,
                        Some(TokenTree::Punct(q)) if q.as_char() == ':' && q.spacing() == proc_macro2::Spacing::Joint
                    ),
                    Some(TokenTree::Punct(p)) => p.as_char() == '$',
                    _ => false,
                };
                if next_is_path && !after_in_or_sep {
                    let mut dollar = proc_macro2::Punct::new('$', proc_macro2::Spacing::Alone);
                    dollar.set_span(id.span());
                    out.push(TokenTree::Punct(dollar));
                }
                out.push(tt.clone());
            }
            TokenTree::Group(g) => {
                let after_pub = matches!(prev, Some(TokenTree::Ident(p)) if p == "pub");
                let pub_paren = after_pub && g.delimiter() == proc_macro2::Delimiter::Parenthesis;
                let mut ng = Group::new(g.delimiter(), rewrite_crate(g.stream(), pub_paren));
                ng.set_span(g.span());
                out.push(TokenTree::Group(ng));
            }
            t => out.push(t.clone()),
        }
    }
    out.into_iter().collect()
}

/// After the derive's cfg evaluation, every `#[cfg]` still present is true.
/// Remove them so the consumer does not evaluate them again with its own
/// settings. `#[cfg]` inside macro invocation bodies is left alone.
///
/// Returns the item without them (re-parsed) and its tokens. See
/// `announce_helper::remove_attributes` for how an attribute is removed.
fn strip_true_cfgs(mut item: Item) -> Result<(Item, TokenStream)> {
    announce_helper::remove_attributes(&mut item, |a| a.path().is_ident("cfg"))?;
    let tokens = item.to_token_stream();
    Ok((item, tokens))
}

/// A string doc attribute: `#[doc = ".."]`, which is what `///`, `//!`, `/** */`
/// and an explicit `#[doc = ".."]` all are, also `#[doc = include_str!(..)]`.
/// The list forms (`#[doc(hidden)]`, `#[doc(alias = "x")]`, `#[doc(inline)]`,
/// `#[doc(cfg(..))]`, ..) are not: they steer rustdoc, not the reader.
fn is_doc_string(a: &syn::Attribute) -> bool {
    a.path().is_ident("doc") && matches!(a.meta, syn::Meta::NameValue(_))
}

/// The item without its string doc attributes (see [`is_doc_string`]), at any
/// depth that syn parses: the item, its fields, variants, generic parameters,
/// trait items, and everything inside an inline `mod`, inner attributes
/// included. Not inside macro invocations and `macro_rules!` bodies, like
/// `strip_true_cfgs`. Run it after the cfgs are evaluated, so a
/// `#[cfg_attr(.., doc = ..)]` is a plain `#[doc = ..]` by then.
fn strip_docs(mut item: Item) -> Result<TokenStream> {
    announce_helper::remove_attributes(&mut item, is_doc_string)?;
    Ok(item.to_token_stream())
}

/// Conservative test for "`dyn Trait` is certainly a valid type", so that the
/// marker impl can never break the definer's build (DESIGN.md section 8).
fn trait_is_simple_dyn(t: &syn::ItemTrait) -> bool {
    fn mentions(ts: TokenStream, word: &str) -> bool {
        ts.into_iter().any(|tt| match tt {
            TokenTree::Ident(i) => i == word,
            TokenTree::Group(g) => mentions(g.stream(), word),
            _ => false,
        })
    }
    let mentions_self = |ts: TokenStream| mentions(ts, "Self");
    // Any generic parameter, lifetimes included (AUDIT #4: `impl Announce for
    // dyn L` does not compile for `trait L<'a>`).
    if !t.generics.params.is_empty() || t.generics.where_clause.is_some() || t.auto_token.is_some()
    {
        return false;
    }
    let supers_ok = t.supertraits.iter().all(|b| match b {
        syn::TypeParamBound::Lifetime(_) => true,
        // Only the std auto traits by full path: a user trait that is merely
        // *named* `Send` may have associated types (AUDIT #4).
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

fn marker_impl(item: &Item, opts: &Options, krate: &syn::Path) -> TokenStream {
    // `#[derive(Announce)]` always implements it. `emit_item = false` means
    // the item is emitted (and its `Announce` impl owned) by someone else, so
    // a second announcement cannot conflict with it (AUDIT #25). A mock is a
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
            let (ig, tg, wc) = generics.split_for_impl();
            quote! { impl #ig #krate::Announce for #ident #tg #wc {} }
        }
        Item::Trait(t) if trait_is_simple_dyn(t) => {
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
/// Chosen by measurement (DESIGN.md section 4.1): a direct trampoline copies
/// the item into three arms and takes one expansion; a single-copy one holds it
/// once and takes three expansions (two through the derive route). The single
/// copy has the smaller `.rmeta` from about 90 tokens; 300 weighs that against
/// the expansions, which cost time and recursion depth on the calling side.
pub const SINGLE_COPY_THRESHOLD: usize = 300;

/// Number of tokens in `ts`; a group counts its two delimiters and its contents.
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
     `announce_helper::redirect`)";

/// The derive/attribute route emits an item (the dummy enum): an error in
/// expression position (`(expr)`, section 4.6).
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
/// attribute, an anonymous attribute) (DESIGN.md section 4.1). `lead` is what
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
/// `announce_helper::redirect` adds it for the derive route. No helper items
/// are emitted, so the expansion is a single macro call in every position
/// (DESIGN.md section 4.6).
///
/// `redirect` writes `@__announce_self { path } @__announce_dollar { $ }`
/// before the list, for the layouts that call themselves again (DESIGN.md
/// section 4.1); the arms of this layout ignore the self path. A payload
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
/// carry none (DESIGN.md section 4.1).
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

/// The arms of DESIGN.md section 4.1, and the catch-all error arm.
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
/// names), `mock(..)` and `extra_meta`.
pub fn trampoline(
    ident: &Ident,
    vis: &Visibility,
    opts: &Options,
    owned: &OwnedAttrs,
    random: u64,
    payload: TokenStream,
    extra_meta: TokenStream,
) -> TokenStream {
    let h = hex(random);
    let macro_name = opts.macro_name.clone().unwrap_or_else(|| ident.clone());
    // `r#match` is not usable inside a longer identifier (AUDIT #5).
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
    let meta = crate_to_dollar_crate(quote! { #(#meta_items),* });
    let mut needs_dollar = false;
    let payload = crate_to_dollar_crate(escape_dollar(payload, &mut needs_dollar));
    let single = is_single(opts.layout, token_count(&meta) + token_count(&payload));
    // Global options other than a leading `expr` are not interpreted by the
    // trampoline; they are forwarded unchanged (a namespace for helpers, e.g.
    // `gather`'s bookkeeping; AUDIT 3 #9).
    let plain = Variant {
        meta,
        payload,
        needs_dollar,
    };
    let arms = arms(&fallback, &plain, single);
    quote! {
        #[doc(hidden)]
        // The payload holds escaped `$crate` / `crate::` tokens (false
        // positive), and `pub` items in fn bodies are fine (AUDIT #24).
        #[allow(unknown_lints, clippy::crate_in_macro_def, non_local_definitions)]
        #(if export) { #[macro_export] }
        macro_rules! #hidden { #arms }
        #[doc(hidden)]
        #[allow(unused_imports)]
        #vis use #hidden as #macro_name;
    }
}

/// The item without the owned attributes (`item` has its cfgs evaluated).
fn compiled_copy(item: &Item, owned: &OwnedAttrs) -> Result<TokenStream> {
    let mut item = item.clone();
    owned.strip(&mut item)?;
    Ok(item.to_token_stream())
}

/// Whether `generate` emits the real item itself: when something is removed from
/// it, the derive emits it instead of `#[announce]` (section 4.7 of DESIGN.md).
fn emits_real_item(opts: &Options, owned: &OwnedAttrs) -> bool {
    !owned.is_empty() && opts.emit_item && opts.mock.is_none() && !opts.derived
}

/// What stands in for `generate` when it fails: the real item, if `generate`
/// would have emitted it. Without it the error would be followed by errors about
/// the missing item.
pub fn real_item_on_error(item: Item, opts: &Options, owned: &OwnedAttrs) -> TokenStream {
    if !emits_real_item(opts, owned) {
        return TokenStream::new();
    }
    strip_true_cfgs(item)
        .and_then(|(item, _)| compiled_copy(&item, owned))
        .unwrap_or_default()
}

/// Everything the `__Capture` derive emits for one cfg-evaluated item.
///
/// `owned` are the attribute names that the checks own. The payload and the
/// checks get the item with those attributes, since callbacks and checks read
/// them. The copies that rustc compiles lose them: the real item (which this
/// emits, instead of `#[announce]`, whenever there is anything to remove) and
/// the hidden copy of a `mock`.
///
/// The payload and the checks also lose the string doc attributes
/// ([`strip_docs`]), unless `opts.docs` is set; they get the same item. The
/// copies that rustc compiles keep their docs, so rustdoc shows what the
/// author wrote.
pub fn generate(
    item: Item,
    opts: &Options,
    random: u64,
    owned: &OwnedAttrs,
) -> Result<TokenStream> {
    let (ident, vis) = ident_vis(&item)?;
    let krate = opts.krate();
    let h = hex(random);

    let (item, full_item) = strip_true_cfgs(item)?;

    // What callbacks and checks get: the same item, without its docs unless
    // `docs = true` (a smaller trampoline). Taken after the cfgs are evaluated.
    let payload = if opts.docs {
        full_item.clone()
    } else {
        strip_docs(item.clone())?
    };

    // What the compiler sees: without the owned attributes, with the docs.
    let compiled = if owned.is_empty() {
        full_item.clone()
    } else {
        compiled_copy(&item, owned)?
    };
    let real_item = emits_real_item(opts, owned).then(|| compiled.clone());

    let checks = opts.checks.iter().map(|c| {
        let p = &c.path;
        let a = c.args.clone().unwrap_or_default();
        quote! { #p! { (#a), #payload } }
    });
    let mock_copy = opts.mock.as_ref().map(|_| {
        let m = Ident::new(&format!("__announce_mock_{}", h), Span::call_site());
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
    let marker = marker_impl(&item, opts, &krate);

    // DESIGN.md §4.5: `repeater` is required when the item has types that do
    // not resolve outside this crate.
    let (leak_impls, leak_meta) = match crate::leak::table(&item, opts, random)? {
        Some(l) => (l.impls, l.meta),
        None => (TokenStream::new(), TokenStream::new()),
    };
    let tramp = trampoline(
        &ident,
        &vis,
        opts,
        owned,
        random,
        payload.clone(),
        leak_meta,
    );

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
    use proc_macro2::{Delimiter, Spacing};

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
        // ALGORITHM_FINDINGS #8: only the `in` of `pub(in ..)` is the visibility's.
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
        // adding a `$` would unescape nothing and break the pair (AUDIT #2).
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
        // `x: crate::T` is a type ascription after a single colon (AUDIT #1).
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
}
