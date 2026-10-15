//! Proc macros used by the announce tests: an announce-aware derive, a proc-macro
//! callback, a trampoline starter and a definition-side check.

use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use template_quote::quote;

/// announce-aware derive: redirects trampolines to `::announce_test_defs::conflict::ParseAnnounce!`.
#[proc_macro_derive(Parse)]
pub fn derive_parse(input: TokenStream) -> TokenStream {
    let input: TokenStream2 = input.into();
    if let Some(out) = announce::helper::redirect(
        input,
        &syn::parse_quote!(::announce_test_defs::conflict::ParseAnnounce),
    ) {
        return out.into();
    }
    TokenStream::new()
}

/// announce-aware attribute: same redirect; consumes the dummy.
#[proc_macro_attribute]
pub fn parse_attr(_: TokenStream, input: TokenStream) -> TokenStream {
    let input: TokenStream2 = input.into();
    match announce::helper::redirect(
        input.clone(),
        &syn::parse_quote!(::announce_test_defs::conflict::ParseAnnounce),
    ) {
        Some(out) => out.into(),
        None => input.into(),
    }
}

/// Derive whose trampoline holds its item once (a large trait, `layout = single`
/// route): redirects to `::announce_test_defs::large::BigParseAnnounce!`.
#[proc_macro_derive(BigParse)]
pub fn derive_big_parse(input: TokenStream) -> TokenStream {
    let input: TokenStream2 = input.into();
    match announce::helper::redirect(
        input,
        &syn::parse_quote!(::announce_test_defs::large::BigParseAnnounce),
    ) {
        Some(out) => out.into(),
        None => TokenStream::new(),
    }
}

/// The attribute form of [`derive_big_parse`]; consumes the dummy.
#[proc_macro_attribute]
pub fn big_parse_attr(_: TokenStream, input: TokenStream) -> TokenStream {
    let input: TokenStream2 = input.into();
    match announce::helper::redirect(
        input.clone(),
        &syn::parse_quote!(::announce_test_defs::large::BigParseAnnounce),
    ) {
        Some(out) => out.into(),
        None => input.into(),
    }
}

/// Derive that redirects to a small trampoline held once
/// (`::announce_test_defs::large::SmallSingleParseAnnounce!`).
#[proc_macro_derive(SmallSingleParse)]
pub fn derive_small_single_parse(input: TokenStream) -> TokenStream {
    let input: TokenStream2 = input.into();
    match announce::helper::redirect(
        input,
        &syn::parse_quote!(::announce_test_defs::large::SmallSingleParseAnnounce),
    ) {
        Some(out) => out.into(),
        None => TokenStream::new(),
    }
}

/// Callback: `summary! { [summary!] ENTRY* NAME }` emits
/// `pub const NAME: &[(&str, &str, &str)] = &[(key, meta, item-kind+ident), ..]`
/// in visiting order, and forwards if more continuations remain.
#[proc_macro]
pub fn summary(input: TokenStream) -> TokenStream {
    let input: announce::helper::Input = match syn::parse(input) {
        Ok(i) => i,
        Err(e) => return e.to_compile_error().into(),
    };
    if let Some(fwd) = input.forward() {
        return fwd.into();
    }
    let name: syn::Ident = match syn::parse2(input.rest.clone()) {
        Ok(n) => n,
        Err(e) => return e.to_compile_error().into(),
    };
    let rows = input.entries_in_order().map(|e| {
        let key = e.key.path().segments.last().unwrap().ident.to_string();
        let meta = e
            .meta
            .iter()
            .map(|m| template_quote::ToTokens::to_token_stream(m).to_string())
            .collect::<Vec<_>>()
            .join(", ");
        let item = match e.item() {
            Ok(syn::Item::Struct(s)) => format!("struct {}", s.ident),
            Ok(syn::Item::Enum(s)) => format!("enum {}", s.ident),
            Ok(syn::Item::Union(s)) => format!("union {}", s.ident),
            Ok(syn::Item::Trait(s)) => format!("trait {} ({} items)", s.ident, s.items.len()),
            Ok(syn::Item::Type(s)) => format!("type {}", s.ident),
            Ok(syn::Item::Mod(s)) => format!("mod {}", s.ident),
            Ok(_) => "other".to_string(),
            Err(e) => format!("error: {}", e),
        };
        quote! { (#key, #meta, #item) }
    });
    TokenStream::from(quote! { pub const #name: &[(&str, &str, &str)] = &[ #(#rows),* ]; })
}

/// Starts a trampoline from a proc macro: `start!([a!, #[derive(B)], ..] REST)`.
/// Attribute continuations get random dummy names.
#[proc_macro]
pub fn start(input: TokenStream) -> TokenStream {
    struct Start(announce::helper::ContList, TokenStream2);
    impl syn::parse::Parse for Start {
        fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
            let content;
            syn::bracketed!(content in input);
            Ok(Start(content.parse()?, input.parse()?))
        }
    }
    match syn::parse::<Start>(input) {
        Ok(Start(list, rest)) => list.invoke(rest).into(),
        Err(e) => e.to_compile_error().into(),
    }
}

/// Definition-side check: rejects tuple structs. `no_tuple! { (ARGS), ITEM }`.
/// It owns no attributes, and answers the attribute query all the same.
#[proc_macro]
pub fn no_tuple(input: TokenStream) -> TokenStream {
    announce::helper::check(input.into(), &[], |ci| match &ci.item {
        syn::Item::Struct(s) if matches!(s.fields, syn::Fields::Unnamed(_)) => {
            let msg = format!("this macro does not support tuple structs ({})", ci.args);
            Err(syn::Error::new_spanned(&s.fields, msg))
        }
        _ => Ok(TokenStream2::new()),
    })
    .into()
}

/// Owns the attribute `my_lib`, and rejects `#[my_lib(bad)]` anywhere in the item.
#[proc_macro]
pub fn lib_check(input: TokenStream) -> TokenStream {
    announce::helper::check(input.into(), &["my_lib"], |ci| {
        for attr in ci.owned_attributes() {
            if matches!(attr.parse_args::<syn::Ident>(), Ok(i) if i == "bad") {
                return Err(syn::Error::new_spanned(
                    &attr,
                    "`#[my_lib(bad)]` is not allowed",
                ));
            }
        }
        Ok(TokenStream2::new())
    })
    .into()
}

/// Owns the attributes `other` and `tag`, and rejects `#[other(forbidden)]`.
#[proc_macro]
pub fn other_check(input: TokenStream) -> TokenStream {
    announce::helper::check(input.into(), &["other", "tag"], |ci| {
        for attr in ci.owned_attributes() {
            if attr.path().is_ident("other")
                && attr
                    .parse_args::<syn::Ident>()
                    .map_or(false, |i| i == "forbidden")
            {
                return Err(syn::Error::new_spanned(
                    &attr,
                    "`#[other(forbidden)]` is not allowed",
                ));
            }
        }
        Ok(TokenStream2::new())
    })
    .into()
}

/// Owns `my_lib` and records what it saw: `pub const SEEN_<ITEM>: &[&str]`, one
/// string for each owned attribute (without spaces), in source order.
#[proc_macro]
pub fn record_attrs(input: TokenStream) -> TokenStream {
    announce::helper::check(input.into(), &["my_lib"], |ci| {
        let name = match &ci.item {
            syn::Item::Struct(s) => s.ident.to_string(),
            syn::Item::Enum(s) => s.ident.to_string(),
            syn::Item::Trait(s) => s.ident.to_string(),
            syn::Item::Mod(s) => s.ident.to_string(),
            other => {
                return Err(syn::Error::new_spanned(
                    other,
                    "record_attrs: unsupported item",
                ))
            }
        };
        let seen: Vec<String> = ci
            .owned_attributes()
            .iter()
            .map(|a| {
                template_quote::ToTokens::to_token_stream(&a.meta)
                    .to_string()
                    .split_whitespace()
                    .collect()
            })
            .collect();
        let id = syn::Ident::new(
            &format!("SEEN_{}", name.to_uppercase()),
            proc_macro2::Span::call_site(),
        );
        Ok(quote! { pub const #id: &[&str] = &[ #(#seen),* ]; })
    })
    .into()
}

/// Owns an attribute that rustc uses: always an error where it is used.
#[proc_macro]
pub fn doc_owner(input: TokenStream) -> TokenStream {
    announce::helper::check(input.into(), &["doc"], |_| Ok(TokenStream2::new())).into()
}

/// A check that reads the attribute query by hand and asserts that the list is
/// headed by this very macro (`head_probe!`, whatever path it is written with):
/// the head of the list is the macro being called. It owns `probed`, and answers
/// with `Query::answer`, which drops the head and calls the next element.
#[proc_macro]
pub fn head_probe(input: TokenStream) -> TokenStream {
    use announce::helper::{CheckCall, Cont, OwnedAttrs};
    match syn::parse::<CheckCall>(input) {
        Ok(CheckCall::Query(q)) => {
            let is_me = matches!(
                q.list.0.first(),
                Some(Cont::Bang(p)) if p.segments.last().map_or(false, |s| s.ident == "head_probe")
            );
            if !is_me {
                let list = template_quote::ToTokens::to_token_stream(&q.list)
                    .to_string()
                    .split_whitespace()
                    .collect::<Vec<_>>()
                    .join(" ");
                let msg = format!(
                    "head_probe: the list does not start with `head_probe!`: {}",
                    list
                );
                return syn::Error::new(proc_macro2::Span::call_site(), msg)
                    .to_compile_error()
                    .into();
            }
            q.answer(&OwnedAttrs::from_strs(&["probed"])).into()
        }
        Ok(CheckCall::Run(_)) => TokenStream::new(),
        Err(e) => e.to_compile_error().into(),
    }
}

/// Declares the attribute `rules` and does nothing else. A `#[derive(Announce)]`
/// cannot remove attributes from its item, so an item that has `#[rules(..)]` in it
/// needs a derive that declares them (the check is `announce_test_defs::rules_check`).
#[proc_macro_derive(Rules, attributes(rules))]
pub fn derive_rules(_: TokenStream) -> TokenStream {
    TokenStream::new()
}

/// A check written the way checks were before the attribute query existed: it
/// parses `CheckInput` and does not answer the query.
#[proc_macro]
pub fn old_style_check(input: TokenStream) -> TokenStream {
    match syn::parse::<announce::helper::CheckInput>(input) {
        Ok(_) => TokenStream::new(),
        Err(e) => e.to_compile_error().into(),
    }
}

/// `check_marker!(path)`: best-effort announce::Announce check for a type.
#[proc_macro]
pub fn check_marker(input: TokenStream) -> TokenStream {
    let ty: syn::Type = match syn::parse(input) {
        Ok(t) => t,
        Err(e) => return e.to_compile_error().into(),
    };
    announce::helper::check_type(&syn::parse_quote!(::announce), &ty).into()
}

/// A derive that announces its own input via `#[announce(emit_item = false)]`.
#[proc_macro_derive(Announced)]
pub fn derive_announced(input: TokenStream) -> TokenStream {
    let input: TokenStream2 = input.into();
    TokenStream::from(quote! { #[::announce::announce(emit_item = false)] #input })
}

announce::helper::transformer! {
    /// Transformer: keeps only `pub` fields in the newest entry's struct and
    /// lists their names in META as `fields(..)`.
    pub fn public_fields(entry) {
        let mut s: syn::ItemStruct = syn::parse2(entry.item.clone())?;
        let mut names = Vec::new();
        if let syn::Fields::Named(f) = &mut s.fields {
            f.named = f
                .named
                .clone()
                .into_iter()
                .filter(|f| matches!(f.vis, syn::Visibility::Public(_)))
                .collect();
            names = f.named.iter().map(|f| f.ident.clone().unwrap()).collect();
        }
        entry.item = quote! { #s };
        entry.meta.push(syn::parse_quote!(fields(#(#names),*)));
        Ok(entry)
    }
}

struct GatherRest {
    paths: Vec<syn::Path>,
    name: syn::Ident,
}
impl syn::parse::Parse for GatherRest {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        let content;
        syn::parenthesized!(content in input);
        let paths =
            syn::punctuated::Punctuated::<syn::Path, syn::Token![,]>::parse_terminated(&content)?;
        Ok(GatherRest {
            paths: paths.into_iter().collect(),
            name: input.parse()?,
        })
    }
}

announce::helper::callback! {
    /// `gathered! { [gathered!] (a::A, b::B) NAME }` fetches the listed
    /// definitions (in one go) and emits `NAME: &[&str]` with their item
    /// idents in the requested order.
    pub fn gathered(input) {
        let rest: GatherRest = input.rest_as()?;
        let needed = rest.paths.iter().cloned().map(announce::helper::Cont::bang);
        let entries = match input.gather(needed) {
            announce::helper::Gather::Ready(e) => e,
            announce::helper::Gather::Fetch(ts) => return Ok(ts),
        };
        let mut idents = Vec::new();
        for e in entries {
            let ident = match e.item()? {
                syn::Item::Struct(s) => s.ident,
                syn::Item::Enum(s) => s.ident,
                syn::Item::Trait(s) => s.ident,
                syn::Item::Type(s) => s.ident,
                other => return Err(syn::Error::new_spanned(other, "unexpected item")),
            };
            idents.push(ident.to_string());
        }
        let name = &rest.name;
        Ok(quote! { pub const #name: &[&str] = &[ #(#idents),* ]; })
    }
}

announce::helper::callback! {
    /// `fetched_raw! { [fetched_raw!] (a::A, b::B) NAME }`: like `gathered`, but
    /// with the raw `Input::fetch` for what is still missing. Emits `NAME: &[&str]`
    /// with the item idents in the requested order, and `NAME_OPTIONS: &str`, the
    /// options group the callback was last called with.
    pub fn fetched_raw(input) {
        let rest: GatherRest = input.rest_as()?;
        let needed: Vec<announce::helper::Cont> =
            rest.paths.iter().cloned().map(announce::helper::Cont::bang).collect();
        let missing: Vec<announce::helper::Cont> =
            needed.iter().filter(|c| input.entry_for(c).is_none()).cloned().collect();
        if !missing.is_empty() {
            return Ok(input.fetch(missing));
        }
        let mut idents = Vec::new();
        for c in &needed {
            let e = input.entry_for(c).expect("checked above");
            let ident = match e.item()? {
                syn::Item::Struct(s) => s.ident,
                syn::Item::Enum(s) => s.ident,
                syn::Item::Trait(s) => s.ident,
                syn::Item::Type(s) => s.ident,
                other => return Err(syn::Error::new_spanned(other, "unexpected item")),
            };
            idents.push(ident.to_string());
        }
        let name = &rest.name;
        let options = input.options.as_ref().map(|o| o.to_string()).unwrap_or_default();
        let options_name = syn::Ident::new(&format!("{}_OPTIONS", name), name.span());
        Ok(quote! {
            pub const #name: &[&str] = &[ #(#idents),* ];
            pub const #options_name: &str = #options;
        })
    }
}

announce::helper::callback! {
    /// Records, for every entry in visiting order, `pub const <REST>: &[(&str, usize, usize, String)]`
    /// is not possible in a const, so the rows are strings: `"names|in item|in stripped"`:
    /// the owned attribute names of the entry (`Entry::owned_attrs`, comma separated), the
    /// number of owned attributes in `Entry::item` and in `Entry::item_without_owned_attrs`.
    pub fn owned_report(input) {
        let name: syn::Ident = input.rest_as()?;
        let mut rows = Vec::new();
        for e in input.entries_in_order() {
            let owned = e.owned_attrs();
            let names: Vec<String> = owned.names().iter().map(|n| n.to_string()).collect();
            let item = e.item()?;
            let stripped = e.item_without_owned_attrs()?;
            rows.push(format!("{}|{}|{}", names.join(","), owned.find(&item).len(), owned.find(&stripped).len()));
        }
        Ok(quote! { pub const #name: &[&str] = &[ #(#rows),* ]; })
    }
}

announce::helper::callback! {
    /// Reports what a callback receives next to the entries, for the tests that
    /// REST is the caller's tokens and nothing else. Emits, with fixed names:
    /// `REST: &str` (`input.rest`, printed), `ENTRIES: usize`, `OWNED: &[&str]` (the
    /// names of `Entry::owned_attrs`, comma separated, one row for each entry in
    /// visiting order) and `META: &[&str]` (the META of each entry). It forwards
    /// when more continuations remain.
    pub fn rest_probe(input) {
        if let Some(fwd) = input.forward() {
            return Ok(fwd);
        }
        let rest = input.rest.to_string();
        let entries = input.entries.len();
        let mut owned = Vec::new();
        let mut metas = Vec::new();
        for e in input.entries_in_order() {
            let names: Vec<String> = e.owned_attrs().names().iter().map(|n| n.to_string()).collect();
            owned.push(names.join(","));
            metas.push(e.meta.iter().map(|m| template_quote::ToTokens::to_token_stream(m).to_string()).collect::<Vec<_>>().join(", "));
        }
        Ok(quote! {
            pub const REST: &str = #rest;
            pub const ENTRIES: usize = #entries;
            pub const OWNED: &[&str] = &[ #(#owned),* ];
            pub const META: &[&str] = &[ #(#metas),* ];
        })
    }
}

announce::helper::callback! {
    /// Re-emits every entry's item, rewritten with its type-leak information
    /// (`TypeLeak::apply`) when the definition has an repeater, and records the
    /// META as `pub const <REST>: &[&str]`.
    pub fn apply_leak(input) {
        let name: syn::Ident = input.rest_as()?;
        let mut out = proc_macro2::TokenStream::new();
        let mut metas = Vec::new();
        for e in input.entries_in_order() {
            let mut item = e.item()?;
            if let Some(tl) = e.type_leak()? {
                tl.apply(&mut item);
            }
            metas.push(e.meta.iter().map(|m| template_quote::ToTokens::to_token_stream(m).to_string()).collect::<Vec<_>>().join(", "));
            out.extend(quote! { #item });
        }
        Ok(quote! { #out pub const #name: &[&str] = &[ #(#metas),* ]; })
    }
}

announce::helper::callback! {
    /// Reads the `type_leak(..)` META through `Entry::type_leak` and records,
    /// for every entry in visiting order, `pub const <REST>: &[&str]` with
    /// `"-"` when the entry has no table, else `"[Type@Args,..]"`: each
    /// replaced type (type-leak's required entries) and the first argument of
    /// its repeater at the scope of the item (types and arguments without
    /// spaces, sorted by text, so the order of the table does not matter).
    pub fn leak_types(input) {
        let name: syn::Ident = input.rest_as()?;
        let squash = |t: &dyn template_quote::ToTokens| {
            template_quote::ToTokens::to_token_stream(t).to_string().split_whitespace().collect::<String>()
        };
        let mut rows = Vec::new();
        for e in input.entries_in_order() {
            rows.push(match e.type_leak()? {
                None => "-".to_string(),
                Some(tl) => {
                    let mut pairs = Vec::new();
                    for (ix, ty) in tl.types() {
                        let args = tl.entry_args(ix, &announce::helper::type_leak::UseSite::identity())?;
                        pairs.push(format!("{}@{}", squash(ty), squash(&args)));
                    }
                    pairs.sort();
                    format!("[{}]", pairs.join(","))
                }
            });
        }
        Ok(quote! { pub const #name: &[&str] = &[ #(#rows),* ]; })
    }
}

// ---- several announcements of one item (tests/defs/src/merge.rs) ----

/// An attribute macro that returns its input as it is. `#[announce]` cannot know
/// that, so it moves below it (see the module `merge` of the definitions).
#[proc_macro_attribute]
pub fn passthrough(_: TokenStream, item: TokenStream) -> TokenStream {
    item
}

/// `#[emit_announce(OPTIONS)]`: a macro that announces what it produces. Emits the
/// item with `#[::announce::announce(OPTIONS)]` as its outermost attribute.
#[proc_macro_attribute]
pub fn emit_announce(attr: TokenStream, item: TokenStream) -> TokenStream {
    let (attr, item): (TokenStream2, TokenStream2) = (attr.into(), item.into());
    TokenStream::from(quote! { #[::announce::announce(#attr)] #item })
}

/// `#[emit_derive_announce(OPTIONS)]`: the same with the derive form, which puts the
/// options in the helper attribute.
#[proc_macro_attribute]
pub fn emit_derive_announce(attr: TokenStream, item: TokenStream) -> TokenStream {
    let (attr, item): (TokenStream2, TokenStream2) = (attr.into(), item.into());
    TokenStream::from(quote! {
        #[derive(::announce::Announce)]
        #[announce(#attr)]
        #item
    })
}

/// `#[chain(N)]`: a macro that runs `N` times, each time announcing the item with
/// `meta = merge::step(N)` and leaving `#[chain(N-1)]` to expand next. `#[chain(0)]`
/// only returns the item. All the announcements must end up as one.
#[proc_macro_attribute]
pub fn chain(attr: TokenStream, item: TokenStream) -> TokenStream {
    let n: u32 = match attr.to_string().trim().parse() {
        Ok(n) => n,
        Err(_) => {
            return quote! { ::core::compile_error!("chain: expected a number"); }.into();
        }
    };
    let item: TokenStream2 = item.into();
    if n == 0 {
        return item.into();
    }
    let (next, step) = (
        proc_macro2::Literal::u32_unsuffixed(n - 1),
        proc_macro2::Literal::u32_unsuffixed(n),
    );
    TokenStream::from(quote! {
        #[::announce::announce(meta = merge::step(#step))]
        #[::announce_test_derive::chain(#next)]
        #item
    })
}

/// A check that leaves a mark where it runs: `tick! { (NAME), ITEM }` emits
/// `pub const TICK_<ITEM>_<NAME>: u8 = 0;`, so a check that ran twice for one item is
/// an error (the constant would be defined twice). It owns no attributes.
#[proc_macro]
pub fn tick(input: TokenStream) -> TokenStream {
    announce::helper::check(input.into(), &[], |ci| {
        let name = match &ci.item {
            syn::Item::Struct(s) => s.ident.to_string(),
            syn::Item::Enum(s) => s.ident.to_string(),
            syn::Item::Trait(s) => s.ident.to_string(),
            other => {
                return Err(syn::Error::new_spanned(other, "tick: unsupported item"));
            }
        };
        let arg: String = ci.args.to_string().split_whitespace().collect();
        let id = syn::Ident::new(
            &format!("TICK_{}_{}", name.to_uppercase(), arg.to_uppercase()),
            proc_macro2::Span::call_site(),
        );
        Ok(quote! { pub const #id: u8 = 0; })
    })
    .into()
}

// ---- announcements are told by their flag, not by their path (tests/defs/src/flag.rs) ----

/// An attribute macro of another crate that happens to be called `announce`, like
/// `#[metrics::announce(level = 2)]`. `#[announce(level = N)]` returns the item and a
/// constant `FOREIGN_LEVEL_<ITEM>: u32 = N` next to it. `#[announce]` must never take it
/// for an announcement: not by the path `announce_test_derive::announce`, nor when it
/// is imported and written as the bare `#[announce(level = 2)]`.
#[proc_macro_attribute]
pub fn announce(attr: TokenStream, item: TokenStream) -> TokenStream {
    struct Level(syn::LitInt);
    impl syn::parse::Parse for Level {
        fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
            let key: syn::Ident = input.parse()?;
            if key != "level" {
                return Err(syn::Error::new(
                    key.span(),
                    "this is not the announce attribute of the `announce` crate: expected \
                     `level = N`",
                ));
            }
            input.parse::<syn::Token![=]>()?;
            Ok(Level(input.parse()?))
        }
    }
    let level: Level = match syn::parse(attr) {
        Ok(l) => l,
        Err(e) => return e.to_compile_error().into(),
    };
    let item: TokenStream2 = item.into();
    let parsed: syn::DeriveInput = match syn::parse2(item.clone()) {
        Ok(i) => i,
        Err(e) => return e.to_compile_error().into(),
    };
    let level = level.0;
    let id = syn::Ident::new(
        &format!(
            "FOREIGN_{}_{}",
            parsed.ident.to_string().to_uppercase(),
            level.base10_digits()
        ),
        proc_macro2::Span::call_site(),
    );
    TokenStream::from(quote! {
        #item
        pub const #id: u32 = #level;
    })
}

thread_local! {
    /// How many times `count_query` answered the query for each item (by name).
    static QUERIES: std::cell::RefCell<std::collections::HashMap<String, usize>> =
        std::cell::RefCell::new(std::collections::HashMap::new());
}

/// The name of the item in the state of an attribute query
/// (`[RANDOM; MODE (OPTIONS) { ITEM }] attrs(..)*`): the identifier after the first
/// `struct`, `enum`, `union` or `trait` in it.
fn queried_item(state: &TokenStream2) -> String {
    use proc_macro2::TokenTree;
    fn find(ts: TokenStream2) -> Option<String> {
        let mut prev_kw = false;
        for t in ts {
            match t {
                TokenTree::Ident(i) if prev_kw => return Some(i.to_string()),
                TokenTree::Ident(i) => {
                    prev_kw = matches!(
                        i.to_string().as_str(),
                        "struct" | "enum" | "union" | "trait"
                    )
                }
                TokenTree::Group(g) => {
                    if let Some(found) = find(g.stream()) {
                        return Some(found);
                    }
                    prev_kw = false;
                }
                _ => prev_kw = false,
            }
        }
        None
    }
    find(state.clone()).unwrap_or_default()
}

/// A check that counts the queries it is asked. It answers the query of an item with
/// an attribute name that owns the number of the answer, `queried_0` for the first
/// query that it answers for that item, `queried_1` for the second, ..., so META lists
/// `attrs(queried_0)` if the query was sent once. A query that is sent again
/// for the same item shows in the names. (A thread-local counter: the compiler
/// expands macros on one thread.)
#[proc_macro]
pub fn count_query(input: TokenStream) -> TokenStream {
    use announce::helper::{CheckCall, OwnedAttrs};
    match syn::parse::<CheckCall>(input) {
        Ok(CheckCall::Query(q)) => {
            let item = queried_item(&q.state);
            let n = QUERIES.with(|c| {
                let mut c = c.borrow_mut();
                let n = c.entry(item).or_insert(0);
                *n += 1;
                *n - 1
            });
            q.answer(&OwnedAttrs::from_strs(&[&format!("queried_{}", n)]))
                .into()
        }
        Ok(CheckCall::Run(_)) => TokenStream::new(),
        Err(e) => e.to_compile_error().into(),
    }
}
