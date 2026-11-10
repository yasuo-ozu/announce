//! Finds a trait in the rust-src of a toolchain and cuts it out of its file.
//!
//! std's source is not plain stable Rust (`pub const trait Add`, `[const]` and `~const`
//! bounds, unstable attributes), and `syn` cannot parse a whole file of it. So a file is
//! only tokenized (which any Rust text survives), cut into items, and just the wanted
//! trait is parsed: its head, and each of its wanted items, alone, after the unstable
//! qualifiers are removed from the tokens and a body is replaced by an empty one.

use proc_macro2::{Delimiter, Group, TokenStream, TokenTree};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;
use syn::parse::Parser;

pub type Res<T> = Result<T, String>;

/// The `library/` directory of rust-src.
pub struct Library {
    /// Every `.rs` file of `core/src`, `alloc/src` and `std/src`: (path relative to
    /// `library/`, text).
    files: Vec<(String, String)>,
}

impl Library {
    pub fn open(sysroot: &Path) -> Res<Library> {
        let root = sysroot
            .join("lib")
            .join("rustlib")
            .join("src")
            .join("rust")
            .join("library");
        if !root.join("core").join("src").is_dir() {
            return Err(format!(
                "rust-src not found at {}\n\
                 install it with: rustup component add rust-src --toolchain stable",
                root.display()
            ));
        }
        let mut files = Vec::new();
        for krate in ["core", "alloc", "std"] {
            walk(&root, &root.join(krate).join("src"), &mut files)?;
        }
        files.sort();
        Ok(Library { files })
    }

    /// The file that defines the public trait `name` and whose path contains `hint`.
    pub fn find_trait(&self, name: &str, hint: &str) -> Res<&(String, String)> {
        let found: Vec<_> = self
            .files
            .iter()
            .filter(|(rel, text)| rel.contains(hint) && defines_trait(text, name))
            .collect();
        match found.len() {
            1 => Ok(found[0]),
            0 => Err(format!(
                "no file with `{}` in its path defines `pub trait {}` (std's layout changed? fix `src` of the trait in config.rs)",
                hint, name
            )),
            _ => Err(format!(
                "`pub trait {}` is defined in several files with `{}` in the path: {}",
                name,
                hint,
                found.iter().map(|f| f.0.as_str()).collect::<Vec<_>>().join(", ")
            )),
        }
    }
}

fn walk(root: &Path, dir: &Path, out: &mut Vec<(String, String)>) -> Res<()> {
    let rd = fs::read_dir(dir).map_err(|e| format!("cannot read {}: {}", dir.display(), e))?;
    for entry in rd {
        let path = entry.map_err(|e| e.to_string())?.path();
        if path.is_dir() {
            walk(root, &path, out)?;
        } else if path.extension().is_some_and(|e| e == "rs") {
            let text = match fs::read_to_string(&path) {
                Ok(t) => t,
                Err(_) => continue,
            };
            let rel = path
                .strip_prefix(root)
                .unwrap()
                .to_string_lossy()
                .replace('\\', "/");
            out.push((rel, text));
        }
    }
    Ok(())
}

/// Does `text` have a line `pub [unsafe|const|auto ]trait NAME`?
fn defines_trait(text: &str, name: &str) -> bool {
    text.lines().any(|line| {
        let mut l = match line.strip_prefix("pub ") {
            Some(l) => l,
            None => return false,
        };
        loop {
            let mut changed = false;
            for q in ["unsafe ", "const ", "auto "] {
                if let Some(r) = l.strip_prefix(q) {
                    l = r;
                    changed = true;
                }
            }
            if !changed {
                break;
            }
        }
        match l.strip_prefix("trait ") {
            Some(r) => r
                .strip_prefix(name)
                .is_some_and(|rest| !rest.starts_with(|c: char| c.is_alphanumeric() || c == '_')),
            None => false,
        }
    })
}

/// What a file of the library says about the names in its module.
pub struct FileInfo {
    /// `core`, `alloc` or `std`.
    pub krate: String,
    /// The module path in the crate (`["fmt"]` for `core/src/fmt/mod.rs`).
    pub module: Vec<String>,
    /// `use` declarations of the module: the name they bring in -> the path (as written).
    pub uses: BTreeMap<String, Vec<String>>,
    /// Glob imports (`use super::*;`), as written; only for error messages.
    pub globs: Vec<Vec<String>>,
    /// The names of the type-namespace items (traits, structs, enums, unions, type aliases,
    /// modules) defined in the module.
    pub items: BTreeSet<String>,
    /// The `since` of a `#![stable(..)]` at the top of the file: the module, and so every
    /// path in it, is stable only from this version.
    pub module_since: Option<u32>,
    chunks: Vec<Vec<TokenTree>>,
}

pub fn read_file(rel: &str, text: &str, current: u32) -> Res<FileInfo> {
    let mut parts = rel.split('/');
    let krate = parts.next().unwrap().to_string();
    let mut module: Vec<String> = parts.skip(1).map(str::to_string).collect();
    if let Some(last) = module.last_mut() {
        if let Some(stem) = last.strip_suffix(".rs") {
            *last = stem.to_string();
        }
        if last == "mod" || last == "lib" {
            module.pop();
        }
    }
    let tokens: TokenStream = text
        .parse()
        .map_err(|e| format!("{}: cannot tokenize: {}", rel, e))?;
    let tts: Vec<TokenTree> = tokens.into_iter().collect();
    let chunks = split_items(&tts);
    let mut module_since = None;
    let mut i = 0;
    while i + 2 < tts.len() && is_punct(&tts[i], '#') && is_punct(&tts[i + 1], '!') {
        let attr: Vec<TokenTree> = vec![tts[i].clone(), tts[i + 2].clone()];
        if let Ok(attrs) = parse_attrs(attr) {
            if let Ok(Stability::Since(v)) = stability(&attrs, current) {
                module_since = Some(v);
            }
        }
        i += 3;
    }
    let mut info = FileInfo {
        krate,
        module,
        uses: BTreeMap::new(),
        globs: Vec::new(),
        items: BTreeSet::new(),
        module_since,
        chunks,
    };
    for chunk in &info.chunks.clone() {
        let (kind, name) = item_kind(chunk);
        match kind.as_str() {
            "use" => {
                let ts: TokenStream = chunk.iter().cloned().collect();
                if let Ok(item) = syn::parse2::<syn::ItemUse>(ts) {
                    flatten_use(&item.tree, &mut Vec::new(), &mut info);
                }
            }
            "struct" | "enum" | "union" | "trait" | "type" | "mod" => {
                if let Some(n) = name {
                    info.items.insert(n);
                }
            }
            _ => {}
        }
    }
    Ok(info)
}

fn flatten_use(tree: &syn::UseTree, prefix: &mut Vec<String>, info: &mut FileInfo) {
    match tree {
        syn::UseTree::Path(p) => {
            prefix.push(p.ident.to_string());
            flatten_use(&p.tree, prefix, info);
            prefix.pop();
        }
        syn::UseTree::Name(n) => {
            let mut path = prefix.clone();
            let name = n.ident.to_string();
            if name == "self" {
                if let Some(last) = path.last().cloned() {
                    info.uses.insert(last, path);
                }
            } else {
                path.push(name.clone());
                info.uses.insert(name, path);
            }
        }
        syn::UseTree::Rename(r) => {
            let mut path = prefix.clone();
            if r.ident != "self" {
                path.push(r.ident.to_string());
            }
            info.uses.insert(r.rename.to_string(), path);
        }
        syn::UseTree::Glob(_) => info.globs.push(prefix.clone()),
        syn::UseTree::Group(g) => {
            for t in &g.items {
                flatten_use(t, prefix, info);
            }
        }
    }
}

fn is_punct(t: &TokenTree, c: char) -> bool {
    matches!(t, TokenTree::Punct(p) if p.as_char() == c)
}

fn is_ident(t: &TokenTree, s: &str) -> bool {
    matches!(t, TokenTree::Ident(i) if i == s)
}

/// Cuts a token list into items: a chunk ends at a `;`, or at a `{..}` group (and the
/// `;` that follows it). A `use` chunk ends only at its `;` (it has `{..}` groups).
fn split_items(tts: &[TokenTree]) -> Vec<Vec<TokenTree>> {
    let mut out = Vec::new();
    let mut cur: Vec<TokenTree> = Vec::new();
    let mut i = 0;
    while i < tts.len() {
        let t = &tts[i];
        // An inner attribute (`#![..]`) belongs to no item.
        if cur.is_empty()
            && is_punct(t, '#')
            && tts.get(i + 1).is_some_and(|n| is_punct(n, '!'))
            && matches!(tts.get(i + 2), Some(TokenTree::Group(g)) if g.delimiter() == Delimiter::Bracket)
        {
            i += 3;
            continue;
        }
        cur.push(t.clone());
        i += 1;
        if is_punct(t, ';') {
            out.push(std::mem::take(&mut cur));
        } else if matches!(t, TokenTree::Group(g) if g.delimiter() == Delimiter::Brace)
            && item_kind(&cur).0 != "use"
        {
            if i < tts.len() && is_punct(&tts[i], ';') {
                cur.push(tts[i].clone());
                i += 1;
            }
            out.push(std::mem::take(&mut cur));
        }
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

/// The keyword (`struct`, `trait`, `fn`, `use`, ..) of an item chunk and the name it
/// defines, skipping attributes, visibility and qualifiers.
fn item_kind(chunk: &[TokenTree]) -> (String, Option<String>) {
    let mut i = 0;
    while i < chunk.len() {
        match &chunk[i] {
            t if is_punct(t, '#') => {
                i += 1;
                if i < chunk.len() && is_punct(&chunk[i], '!') {
                    i += 1;
                }
                i += 1; // the [..] group
            }
            TokenTree::Ident(id) => {
                let s = id.to_string();
                match s.as_str() {
                    "pub" => {
                        i += 1;
                        if matches!(chunk.get(i), Some(TokenTree::Group(g)) if g.delimiter() == Delimiter::Parenthesis)
                        {
                            i += 1;
                        }
                    }
                    "unsafe" | "async" | "default" | "auto" | "extern" => {
                        i += 1;
                        if matches!(chunk.get(i), Some(TokenTree::Literal(_))) {
                            i += 1;
                        }
                    }
                    "const"
                        if matches!(chunk.get(i + 1), Some(TokenTree::Ident(n))
                            if matches!(n.to_string().as_str(), "fn" | "unsafe" | "trait" | "async" | "extern")) =>
                    {
                        i += 1;
                    }
                    "struct" | "enum" | "union" | "trait" | "type" | "mod" | "fn" | "const"
                    | "static" | "use" | "impl" | "macro_rules" => {
                        let name = match chunk.get(i + 1) {
                            Some(TokenTree::Ident(n)) => Some(n.to_string()),
                            Some(t) if is_punct(t, '!') => match chunk.get(i + 2) {
                                Some(TokenTree::Ident(n)) => Some(n.to_string()),
                                _ => None,
                            },
                            _ => None,
                        };
                        return (s, name);
                    }
                    _ => return (s, None),
                }
            }
            _ => return (String::new(), None),
        }
    }
    (String::new(), None)
}

/// Removes what stable `syn` cannot parse: `~const` and `[const]` in bounds, and the
/// `const` of `const trait`.
fn sanitize(ts: TokenStream) -> TokenStream {
    let v: Vec<TokenTree> = ts.into_iter().collect();
    let mut out: Vec<TokenTree> = Vec::new();
    let mut i = 0;
    while i < v.len() {
        let t = &v[i];
        if is_punct(t, '~') && v.get(i + 1).is_some_and(|n| is_ident(n, "const")) {
            i += 2;
            continue;
        }
        if is_ident(t, "const") && v.get(i + 1).is_some_and(|n| is_ident(n, "trait")) {
            i += 1;
            continue;
        }
        if let TokenTree::Group(g) = t {
            if g.delimiter() == Delimiter::Bracket {
                let inner: Vec<TokenTree> = g.stream().into_iter().collect();
                if inner.len() == 1 && is_ident(&inner[0], "const") {
                    i += 1;
                    continue;
                }
            }
            let mut ng = Group::new(g.delimiter(), sanitize(g.stream()));
            ng.set_span(g.span());
            out.push(TokenTree::Group(ng));
        } else {
            out.push(t.clone());
        }
        i += 1;
    }
    out.into_iter().collect()
}

/// What std's source says about an item's stability.
#[derive(Debug, Clone, PartialEq)]
pub enum Stability {
    /// Stable since the minor version 1.N (`CURRENT_RUSTC_VERSION` is the toolchain's).
    Since(u32),
    Unstable,
    Unmarked,
}

fn stability(attrs: &[syn::Attribute], current: u32) -> Res<Stability> {
    let mut result = Stability::Unmarked;
    for a in attrs {
        let is_stable = a.path().is_ident("stable");
        if !is_stable && !a.path().is_ident("unstable") {
            continue;
        }
        if !is_stable {
            result = Stability::Unstable;
            continue;
        }
        let args = a
            .parse_args_with(
                syn::punctuated::Punctuated::<syn::Meta, syn::Token![,]>::parse_terminated,
            )
            .map_err(|e| format!("cannot read a #[stable(..)] attribute: {}", e))?;
        let mut since = None;
        for m in &args {
            if let syn::Meta::NameValue(nv) = m {
                if nv.path.is_ident("since") {
                    if let syn::Expr::Lit(syn::ExprLit {
                        lit: syn::Lit::Str(s),
                        ..
                    }) = &nv.value
                    {
                        since = Some(s.value());
                    }
                }
            }
        }
        let since = since.ok_or("a #[stable(..)] attribute has no `since`")?;
        result = Stability::Since(parse_since(&since, current)?);
    }
    Ok(result)
}

fn parse_since(s: &str, current: u32) -> Res<u32> {
    if s == "CURRENT_RUSTC_VERSION" {
        return Ok(current);
    }
    let mut it = s.split('.');
    match (it.next(), it.next().map(str::parse::<u32>)) {
        (Some("1"), Some(Ok(n))) => Ok(n),
        _ => Err(format!("cannot read the stable version `{}`", s)),
    }
}

/// The method names of `#[rustc_must_implement_one_of(a, b)]`.
fn must_implement_one_of(attrs: &[syn::Attribute]) -> Vec<String> {
    let mut out = Vec::new();
    for a in attrs {
        if a.path().is_ident("rustc_must_implement_one_of") {
            if let Ok(names) = a.parse_args_with(
                syn::punctuated::Punctuated::<syn::Ident, syn::Token![,]>::parse_terminated,
            ) {
                out.extend(names.iter().map(|n| n.to_string()));
            }
        }
    }
    out
}

/// One item of a trait.
pub struct ItemSrc {
    pub item: syn::TraitItem,
    pub provided: bool,
    pub since: u32,
}

/// A trait cut out of its file.
pub struct TraitSrc {
    /// The trait as written, without items, attributes and visibility.
    pub head: syn::ItemTrait,
    pub since: u32,
    /// The wanted items, in the order asked for.
    pub items: Vec<ItemSrc>,
}

fn empty_body() -> TokenTree {
    TokenTree::Group(Group::new(Delimiter::Brace, TokenStream::new()))
}

fn leading_attrs(chunk: &[TokenTree]) -> (usize, Vec<TokenTree>) {
    let mut i = 0;
    while i + 1 < chunk.len()
        && is_punct(&chunk[i], '#')
        && matches!(&chunk[i + 1], TokenTree::Group(g) if g.delimiter() == Delimiter::Bracket)
    {
        i += 2;
    }
    (i, chunk[..i].to_vec())
}

fn parse_attrs(tokens: Vec<TokenTree>) -> Res<Vec<syn::Attribute>> {
    syn::Attribute::parse_outer
        .parse2(tokens.into_iter().collect())
        .map_err(|e| format!("cannot read attributes: {}", e))
}

fn name_of_item(it: &syn::TraitItem) -> Option<String> {
    match it {
        syn::TraitItem::Fn(f) => Some(f.sig.ident.to_string()),
        syn::TraitItem::Type(t) => Some(t.ident.to_string()),
        syn::TraitItem::Const(c) => Some(c.ident.to_string()),
        _ => None,
    }
}

/// Cuts the trait `name` and its `wanted` items out of the file. `current` is the minor
/// version of the toolchain (`since = "CURRENT_RUSTC_VERSION"`).
pub fn cut_trait(info: &FileInfo, name: &str, wanted: &[&str], current: u32) -> Res<TraitSrc> {
    let chunk = info
        .chunks
        .iter()
        .find(|c| {
            let (k, n) = item_kind(c);
            k == "trait" && n.as_deref() == Some(name)
        })
        .ok_or_else(|| format!("trait {} not found in its file", name))?;
    let (n_attrs, attr_tokens) = leading_attrs(chunk);
    let attrs = parse_attrs(attr_tokens)?;
    let since = match stability(&attrs, current)? {
        Stability::Since(v) => v,
        Stability::Unstable => return Err(format!("trait {} is unstable", name)),
        Stability::Unmarked => return Err(format!("trait {} has no #[stable] attribute", name)),
    };
    // The trait is as new as its module (`core::error` is stable since 1.81, though
    // `Error` is as old as 1.0).
    let since = since.max(info.module_since.unwrap_or(0));
    let one_of = must_implement_one_of(&attrs);
    let body = match chunk.last() {
        Some(TokenTree::Group(g)) if g.delimiter() == Delimiter::Brace => g.stream(),
        _ => return Err(format!("trait {} has no body", name)),
    };
    // The head: everything before the body, with an empty body.
    let mut head_tokens: Vec<TokenTree> = chunk[n_attrs..chunk.len() - 1].to_vec();
    head_tokens.push(empty_body());
    let head_ts = sanitize(head_tokens.into_iter().collect());
    let head: syn::ItemTrait = syn::parse2(head_ts)
        .map_err(|e| format!("cannot parse the head of trait {}: {}", name, e))?;

    let body_tts: Vec<TokenTree> = body.into_iter().collect();
    let chunks = split_items(&body_tts);
    let mut items = Vec::new();
    for want in wanted {
        let mut found: Option<&Vec<TokenTree>> = None;
        for c in &chunks {
            let (kind, n) = item_kind(c);
            if matches!(kind.as_str(), "fn" | "type" | "const") && n.as_deref() == Some(*want) {
                found = Some(c);
                break;
            }
        }
        let c = found.ok_or_else(|| format!("{}::{} not found in std's source", name, want))?;
        let (n_attrs, attr_tokens) = leading_attrs(c);
        let attrs = parse_attrs(attr_tokens)?;
        let since =
            match stability(&attrs, current).map_err(|e| format!("{}::{}: {}", name, want, e))? {
                Stability::Since(v) => v,
                Stability::Unstable => {
                    return Err(format!("{}::{} is unstable in std's source", name, want))
                }
                Stability::Unmarked => {
                    return Err(format!("{}::{} has no #[stable] attribute", name, want))
                }
            };
        let mut tokens: Vec<TokenTree> = c[n_attrs..].to_vec();
        // A body is replaced by an empty one: only whether it exists matters.
        // ..but `#[rustc_must_implement_one_of(read_buf, read)]` makes the items it names
        // required in the mock: a std method that is only a default for another
        // (here unstable) one is still what an impl must give.
        let mut provided = false;
        if let Some(TokenTree::Group(g)) = tokens.last() {
            if g.delimiter() == Delimiter::Brace {
                provided = !one_of.iter().any(|n| n == want);
                tokens.pop();
                tokens.push(empty_body());
            }
        }
        let ts = sanitize(tokens.into_iter().collect());
        let item: syn::TraitItem =
            syn::parse2(ts).map_err(|e| format!("cannot parse {}::{}: {}", name, want, e))?;
        if name_of_item(&item).as_deref() != Some(*want) {
            return Err(format!("{}::{}: parsed as another item", name, want));
        }
        if matches!(item, syn::TraitItem::Const(_)) {
            return Err(format!(
                "{}::{} is an associated const, which the data format does not have",
                name, want
            ));
        }
        items.push(ItemSrc {
            item,
            provided,
            since,
        });
    }
    Ok(TraitSrc { head, since, items })
}
