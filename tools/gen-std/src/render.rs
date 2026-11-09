//! Turns the resolved parts of a trait into the one-line texts of the data file.

use crate::source::Res;
use syn::parse_quote;

/// prettyplease output of one item, as one line. prettyplease breaks long signatures
/// over several lines (and puts commas after the last element); rustc does not care,
/// and a data line must be one line.
fn squash(s: &str) -> String {
    let mut line = s
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    for (from, to) in [
        (", )", ")"),
        (", >", ">"),
        (", ]", "]"),
        ("( ", "("),
        (" )", ")"),
        ("< ", "<"),
        (" >", ">"),
        ("[ ", "["),
        (" ]", "]"),
    ] {
        line = line.replace(from, to);
    }
    line
}

fn unparse(item: syn::Item) -> String {
    prettyplease::unparse(&syn::File {
        shebang: None,
        attrs: Vec::new(),
        items: vec![item],
    })
}

fn ident(s: &str) -> syn::Ident {
    syn::Ident::new(s, proc_macro2::Span::call_site())
}

/// The part of a trait head after its name: generics (with defaults and bounds),
/// supertraits and where clause. Empty for `trait Any {}`.
pub fn render_head(head: &syn::ItemTrait) -> Res<String> {
    let mut t = head.clone();
    t.attrs.clear();
    t.vis = syn::Visibility::Inherited;
    t.unsafety = None;
    t.auto_token = None;
    t.ident = ident("__M");
    t.items.clear();
    let s = squash(&unparse(syn::Item::Trait(t)));
    let rest = s
        .strip_prefix("trait __M")
        .and_then(|r| r.strip_suffix("{}"))
        .ok_or_else(|| format!("unexpected form of a trait head: {}", s))?;
    Ok(rest.trim().trim_end_matches(',').to_string())
}

/// A trait item without attributes and body, as `fn f(..) -> R` or `type T: Bound`
/// (no `;`).
pub fn render_item(item: &syn::TraitItem) -> Res<String> {
    let t: syn::ItemTrait = parse_quote!(trait __M { #item });
    let s = unparse(syn::Item::Trait(t));
    let s = squash(&s);
    let s = s
        .strip_prefix("trait __M {")
        .and_then(|r| r.strip_suffix('}'))
        .ok_or_else(|| format!("unexpected form of a trait item: {}", s))?;
    let s = s.trim();
    let s = s
        .strip_suffix(';')
        .ok_or_else(|| format!("a trait item that does not end with `;`: {}", s))?;
    Ok(s.trim_end().trim_end_matches(',').to_string())
}

fn type_args(generics: &syn::Generics) -> proc_macro2::TokenStream {
    let args: Vec<proc_macro2::TokenStream> = generics
        .params
        .iter()
        .map(|p| match p {
            syn::GenericParam::Type(t) => {
                let i = &t.ident;
                quote::quote!(#i)
            }
            syn::GenericParam::Const(c) => {
                let i = &c.ident;
                quote::quote!(#i)
            }
            syn::GenericParam::Lifetime(l) => {
                let lt = &l.lifetime;
                quote::quote!(#lt)
            }
        })
        .collect();
    if args.is_empty() {
        quote::quote!()
    } else {
        quote::quote!(<#(#args),*>)
    }
}

/// The blanket subtrait that stands for a derive-named trait before the glob
/// re-export works: `pub trait NAME<params>: PATH<args> {}` and
/// `impl<__T: ?Sized + PATH<args>, params> NAME<args> for __T {}`. Returns the text of
/// each after `trait NAME` and after `impl`.
pub fn render_fallback(head: &syn::ItemTrait, path: &str) -> Res<(String, String)> {
    if head.generics.where_clause.is_some() {
        return Err(format!(
            "the derive-named trait {} has a where clause; its fallback subtrait needs the bounds in the generics",
            head.ident
        ));
    }
    let path: syn::Path = syn::parse_str(path).map_err(|e| e.to_string())?;
    let name = &head.ident;
    let args = type_args(&head.generics);
    let generics = &head.generics;
    let sub: syn::ItemTrait = parse_quote!(trait #name #generics: #path #args {});
    let sub = render_head(&sub)?;

    let mut impl_generics = head.generics.clone();
    for p in impl_generics.params.iter_mut() {
        if let syn::GenericParam::Type(t) = p {
            t.eq_token = None;
            t.default = None;
        }
    }
    let t: syn::TypeParam = parse_quote!(__T: ?::core::marker::Sized + #path #args);
    impl_generics.params.insert(0, syn::GenericParam::Type(t));
    // `impl<..>` needs the `<` `>` also when `head.generics` has none.
    impl_generics.lt_token.get_or_insert_with(Default::default);
    impl_generics.gt_token.get_or_insert_with(Default::default);
    let item: syn::ItemImpl = parse_quote!(impl #impl_generics #name #args for __T {});
    let text = squash(&unparse(syn::Item::Impl(item)));
    let text = text
        .strip_prefix("impl")
        .and_then(|r| r.strip_suffix("{}"))
        .ok_or_else(|| format!("unexpected form of a fallback impl: {}", text))?;
    Ok((sub, text.trim().to_string()))
}
