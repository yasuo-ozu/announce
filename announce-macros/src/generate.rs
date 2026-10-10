use crate::options::Options;
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
/// in another crate. `crate` in `pub(crate)` and after `in` (`pub(in
/// crate::m)`) is left alone. Run after `escape_dollar`, so the inserted `$`
/// is not escaped.
fn crate_to_dollar_crate(ts: TokenStream) -> TokenStream {
    let tts: Vec<TokenTree> = ts.into_iter().collect();
    let mut out = Vec::with_capacity(tts.len());
    for (i, tt) in tts.iter().enumerate() {
        match tt {
            TokenTree::Ident(id) if id == "crate" => {
                let next_is_path = matches!(
                    (tts.get(i + 1), tts.get(i + 2)),
                    (Some(TokenTree::Punct(a)), Some(TokenTree::Punct(b)))
                        if a.as_char() == ':' && a.spacing() == proc_macro2::Spacing::Joint && b.as_char() == ':'
                );
                let prev = i.checked_sub(1).and_then(|j| tts.get(j));
                let prev2 = i.checked_sub(2).and_then(|j| tts.get(j));
                let after_in_or_sep = match prev {
                    // `pub(in crate::m)`, and an escaped `$crate` (`$ __dollar crate`, AUDIT #2)
                    Some(TokenTree::Ident(p)) => p == "in" || p == "__dollar",
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
                let mut ng = Group::new(g.delimiter(), crate_to_dollar_crate(g.stream()));
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
fn strip_true_cfgs(item: &mut Item) {
    struct V;
    impl syn::visit_mut::VisitMut for V {
        fn visit_attributes_mut(&mut self, attrs: &mut Vec<syn::Attribute>) {
            attrs.retain(|a| !a.path().is_ident("cfg"));
            for a in attrs {
                self.visit_attribute_mut(a);
            }
        }
    }
    syn::visit_mut::VisitMut::visit_item_mut(&mut V, item);
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
    // Any generic parameter, lifetimes included (AUDIT #4: `impl Marker for
    // dyn L` does not compile for `trait L<'a>`).
    if !t.generics.params.is_empty()
        || t.generics.where_clause.is_some()
        || t.auto_token.is_some()
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
                    "core::marker::Send" | "core::marker::Sync" | "std::marker::Send" | "std::marker::Sync"
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
    // `emit_item = false` (announcing from a derive) defaults to no marker:
    // the one who emits the item owns its `Marker` impl, so a second
    // announcement cannot conflict with it (AUDIT #25).
    let wanted = opts.marker.unwrap_or(opts.emit_item);
    if !wanted || opts.mock.is_some() {
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
            quote! { impl #ig #krate::Marker for #ident #tg #wc {} }
        }
        Item::Trait(t) if opts.marker == Some(true) || trait_is_simple_dyn(t) => {
            let ident = &t.ident;
            quote! { impl #krate::Marker for dyn #ident {} }
        }
        _ => TokenStream::new(),
    }
}

/// The fixed arm set of DESIGN.md section 4.1.
/// What the generated macro passes on: META and the (escaped) payload.
pub struct Variant {
    pub meta: TokenStream,
    pub payload: TokenStream,
    /// The payload contains `$`, escaped as `$__dollar`.
    pub needs_dollar: bool,
}

/// The arms that pass the definition on, for every (head shape × next
/// shape) (DESIGN.md section 4.1).
///
/// A payload containing `$` is escaped as `$__dollar`, and the arms that pass
/// it on require a leading `@__announce_dollar { $ }`: `$__dollar:tt` binds a
/// real `$`, so substituting it un-escapes the payload in place. The caller
/// does not write the prefix: a bang-head call re-invokes the trampoline
/// once through its own path (the head) with the prefix added, and
/// `announce_helper::redirect` adds it for the derive route. No helper items
/// are emitted, so the expansion is a single macro call in every position
/// (DESIGN.md section 4.6). Payloads without `$` accept the prefix optionally
/// and ignore it.
fn variant_arms(fallback: &Ident, v: &Variant) -> TokenStream {
    let Variant { meta, payload, needs_dollar } = v;
    let prefix = if *needs_dollar {
        quote! { @__announce_dollar { $__dollar:tt } }
    } else {
        quote! { $( @__announce_dollar { $__announce_ignored:tt } )? }
    };
    let opts_pat = quote! { $( ( $($__announce_o:tt)* ) )? };
    let opts_out = quote! { $( ( $($__announce_o)* ) )? };
    let heads: Vec<(TokenStream, TokenStream)> = vec![
        (
            quote! { $($hp:ident)? $(:: $hs:ident)* ! },
            quote! { $($hp)? $(:: $hs)* ! },
        ),
        (
            quote! { #[$($ha:tt)*] $($hid:ident)? },
            quote! { #[$($ha)*] $($hid)? },
        ),
    ];
    let mut out = TokenStream::new();
    for (hpat, key) in &heads {
        let bang = quote! {
            $($np)? $(:: $ns)* ! {
                #opts_out
                [ $($np)? $(:: $ns)* ! $($tail)* ]
                #key => [ #meta ] { #payload }
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
                    #key => [ #meta ] { #payload }
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
                    #key => [ #meta ] { #payload }
                    $($r)*
                }, 0).1
            }
        };
        out.extend(quote! {
            (#prefix #opts_pat [ #hpat , $($np:ident)? $(:: $ns:ident)* ! $($tail:tt)* ] $($r:tt)*) => { #bang };
            (#prefix #opts_pat [ #hpat , #[$($na:tt)*] $nid:ident $(, $($tail:tt)*)? ] $($r:tt)*) => { #named };
            (#prefix #opts_pat [ #hpat , #[$($na:tt)*] $($tail:tt)* ] $($r:tt)*) => { #anon };
        });
    }
    out
}

/// The fixed arm set of DESIGN.md section 4.1, plus error arms.
fn arms(self_name: &str, fallback: &Ident, plain: &Variant) -> TokenStream {
    let mut out = TokenStream::new();
    let any_prefix = quote! { $( @__announce_dollar { $__announce_ignored:tt } )? };
    // The derive/attribute route emits an item (the dummy enum): an error in
    // expression position (`(expr)`, section 4.6). Payload-free arms.
    let derive_in_expr = "announce: the derive/attribute route (`#[derive(..)]`/`#[attr]` in the list) \
                          emits items and cannot be used in expression position (`(expr)`)";
    out.extend(quote! {
        (#any_prefix (expr $($__announce_eo:tt)*) [ $($hp:ident)? $(:: $hs:ident)* ! , #[$($na:tt)*] $($tail:tt)* ] $($r:tt)*) => { compile_error! { #derive_in_expr } };
        (#any_prefix (expr $($__announce_eo:tt)*) [ #[$($ha:tt)*] $($hid:ident)? , #[$($na:tt)*] $($tail:tt)* ] $($r:tt)*) => { compile_error! { #derive_in_expr } };
    });
    out.extend(variant_arms(fallback, plain));
    let no_cont = format!(
        "announce: `{0}!` was invoked with no continuation; expected `{0}! {{ [{0}!, next!, ..] .. }}`",
        self_name
    );
    let malformed = format!(
        "announce: malformed trampoline input to `{0}!`; expected `{0}! {{ [{0}!, next!, ..] .. }}` \
         (optionally preceded by a global options group `(..)`), where each list element is \
         `path::to::macro!` or `#[derive(path)]`/`#[attr]` optionally followed by a name",
        self_name
    );
    out.extend(quote! {
        (#any_prefix $( ($($o:tt)*) )? [ $($hp:ident)? $(:: $hs:ident)* ! $(,)? ] $($r:tt)*) => { compile_error! { #no_cont } };
        (#any_prefix $( ($($o:tt)*) )? [ #[$($ha:tt)*] $($hid:ident)? $(,)? ] $($r:tt)*) => { compile_error! { #no_cont } };
    });
    if plain.needs_dollar {
        let no_dollar = format!(
            "announce: `{0}!` carries `$` tokens and was invoked through the derive/attribute route \
             without the `@__announce_dollar {{ $ }}` prefix; route it with `announce_helper::redirect`",
            self_name
        );
        out.extend(quote! {
            // Re-invoke through the head path, adding a real `$` (see `variant_arms`).
            ($( ($($o:tt)*) )? [ $($hp:ident)? $(:: $hs:ident)* ! $($l:tt)* ] $($r:tt)*) => {
                $($hp)? $(:: $hs)* ! { @__announce_dollar { $ } $( ($($o)*) )? [ $($hp)? $(:: $hs)* ! $($l)* ] $($r)* }
            };
            ($( ($($o:tt)*) )? [ #[$($ha:tt)*] $($l:tt)* ] $($r:tt)*) => { compile_error! { #no_dollar } };
        });
    }
    out.extend(quote! {
        ($($t:tt)*) => { compile_error! { #malformed } };
    });
    out
}

/// The generated macro and its re-export, carrying `payload` (item tokens)
/// with META = checks, `meta =` entries, `mock(..)` and `extra_meta`.
pub fn trampoline(
    ident: &Ident,
    vis: &Visibility,
    opts: &Options,
    random: u64,
    payload: TokenStream,
    extra_meta: TokenStream,
) -> TokenStream {
    let h = hex(random);
    let macro_name = opts.macro_name.clone().unwrap_or_else(|| ident.clone());
    let hidden = opts
        .export_name
        .clone()
        .unwrap_or_else(|| {
            // `r#match` is not usable inside a longer identifier (AUDIT #5).
            let bare = syn::ext::IdentExt::unraw(ident);
            Ident::new(&format!("__announce_{}_{}", bare, h), Span::call_site())
        });
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
    if let Some(m) = &opts.mock {
        meta_items.push(quote! { mock(#m) });
    }
    if !extra_meta.is_empty() {
        meta_items.push(extra_meta);
    }
    let meta = crate_to_dollar_crate(quote! { #(#meta_items),* });
    let mut needs_dollar = false;
    let payload = crate_to_dollar_crate(escape_dollar(payload, &mut needs_dollar));
    // Global options other than a leading `expr` are not interpreted by the
    // trampoline; they are forwarded unchanged (a namespace for helpers, e.g.
    // `gather`'s bookkeeping; AUDIT 3 #9).
    let plain = Variant { meta, payload, needs_dollar };
    let arms = arms(&macro_name.to_string(), &fallback, &plain);
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

/// Everything the `__Capture` derive emits for one cfg-evaluated item.
pub fn generate(mut item: Item, opts: &Options, random: u64) -> Result<TokenStream> {
    let (ident, vis) = ident_vis(&item)?;
    let krate = opts.krate();
    let h = hex(random);

    strip_true_cfgs(&mut item);
    let full_item = item.to_token_stream();

    let checks = opts.checks.iter().map(|c| {
        let p = &c.path;
        let a = c.args.clone().unwrap_or_default();
        quote! { #p! { (#a), #full_item } }
    });
    let mock_copy = opts.mock.as_ref().map(|_| {
        let m = Ident::new(&format!("__announce_mock_{}", h), Span::call_site());
        quote! {
            #[doc(hidden)]
            #[allow(dead_code, non_camel_case_types, unused_imports, unused_variables)]
            mod #m {
                #[allow(unused_imports)]
                use super::*;
                #full_item
            }
        }
    });
    let marker = marker_impl(&syn::parse2(full_item.clone())?, opts, &krate);

    // DESIGN.md §4.5: `repeater` is required when the item has types that do
    // not resolve outside this crate.
    let (leak_impls, leak_meta) = match crate::leak::table(&item, opts, random)? {
        Some(l) => (l.impls, l.meta),
        None => (TokenStream::new(), TokenStream::new()),
    };
    let tramp = trampoline(&ident, &vis, opts, random, full_item.clone(), leak_meta);

    Ok(quote! {
        #tramp
        #leak_impls
        #(#checks)*
        #mock_copy
        #marker
    })
}
