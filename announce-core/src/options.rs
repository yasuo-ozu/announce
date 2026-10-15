use crate::helper::OwnedAttrs;
use proc_macro2::{Ident, Span, TokenStream};
use syn::parse::{Parse, ParseStream};
use syn::{Error, LitBool, Path, Result, Token};
use template_quote::{quote, ToTokens};

/// One check or `meta` entry of the attribute: `path` or `path(args)`.
///
/// As a check (`#[announce(my_lib::check(args))]`) it runs at the definition,
/// as `path! { (args), ITEM }`. As a `meta` entry it is only listed in `META`.
#[derive(Clone)]
pub struct Check {
    /// The macro path.
    pub path: Path,
    /// The tokens inside the parentheses, if there were any.
    pub args: Option<TokenStream>,
}

impl ToTokens for Check {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        let p = &self.path;
        match &self.args {
            Some(a) => tokens.extend(quote! { #p(#a) }),
            None => tokens.extend(quote! { #p }),
        }
    }
}

/// How the generated trampoline holds the item (`layout = ..`, advanced).
///
/// The default, [`Layout::Auto`], is the right choice almost always.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Layout {
    /// [`Layout::Direct`] for a small item, [`Layout::Single`] for a large one
    /// (more than [`SINGLE_COPY_THRESHOLD`](crate::SINGLE_COPY_THRESHOLD)
    /// tokens with its `META`).
    Auto,
    /// The item is held once, so the macro's metadata is about the size of one
    /// item, and one call takes three expansions (two through the derive route)
    /// instead of one.
    Single,
    /// One expansion per call; the item is copied into each of the macro's
    /// three output arms.
    Direct,
}

/// The parsed arguments of `#[announce(..)]` or of the `#[announce(..)]`
/// helper attribute of `#[derive(Announce)]`.
///
/// Parse it from the attribute's tokens with [`syn::parse2`]; the grammar and
/// the meaning of every option are those of the `#[announce]` attribute
/// (`announce::announce`). Build one in code with [`Options::default`] and
/// the public fields. Turn it back into attribute arguments with
/// [`ToTokens`] (`opts.to_token_stream()`), for example to hand it to
/// [`announce()`](crate::announce).
///
/// The most useful field for a library is `krate`: the path to the `announce`
/// crate that the generated code uses (see [`Options::krate()`]). Without it the
/// path is read from the `Cargo.toml` of the crate being compiled, which only
/// works when that crate depends on `announce` itself.
#[derive(Clone)]
#[non_exhaustive]
pub struct Options {
    /// `macro_name = Name`: name of the emitted macro. Default: the item's name. All
    /// the announcements of one item must give the same name, the item's name where
    /// it is not set: one item has one trampoline.
    pub macro_name: Option<Ident>,
    /// `mock = path`: the item is a stand-in for the item at `path`.
    pub mock: Option<Path>,
    /// `repeater = path`: emit the type-leak table against this repeater trait.
    pub repeater: Option<Path>,
    /// type-leak: primitives resolve everywhere (default `true`).
    pub allow_primitive: bool,
    /// type-leak: std prelude names resolve everywhere (default `false`).
    pub allow_std_prelude: bool,
    /// `crate = path`: path to the `announce` crate in generated code. When it
    /// is `None`, [`Options::krate()`] looks the crate up in `Cargo.toml`.
    pub krate: Option<Path>,
    /// `emit_item = false`: do not re-emit the item (for derives, whose input
    /// already exists). Default `true`.
    pub emit_item: bool,
    /// `layout = auto | single | direct`: how the trampoline holds the item.
    /// Default `auto`: by the size of the item (advanced).
    pub layout: Layout,
    /// `docs = true`: keep the doc attributes (`#[doc = ".."]`, so `///` and
    /// `//!` too) in the item that callbacks and checks receive. Default
    /// `false`: they are removed there, which makes the trampoline smaller.
    /// The item that is compiled keeps its docs either way.
    pub docs: bool,
    /// The internal flag `__announce_deferred` (see [`FLAG`]): `#[announce]` writes it
    /// when it has moved itself below the other attributes of the item (see `fold`),
    /// and reads it to know that it has. Accepted when written, but not meant to be.
    pub(crate) deferred: bool,
    /// With the flag, `__announce_deferred(attrs(a, b))`: the query of the checks is
    /// answered, and these are the attribute names that they own. `None`: not asked
    /// yet (see [`Options::answer`]).
    pub(crate) owned: Option<OwnedAttrs>,
    /// Set by `#[derive(Announce)]`: the item is emitted by the user and always
    /// gets the `Announce` impl. Not an option.
    pub(crate) derived: bool,
    /// The option keys that were written (for derive-specific errors).
    pub(crate) given: Vec<String>,
    /// Checks: bare `path` / `path(args)` entries. They run at the definition.
    pub checks: Vec<Check>,
    /// `meta = path(args)`: listed in META like a check, but never invoked.
    pub metas: Vec<Check>,
}

impl Default for Options {
    fn default() -> Self {
        Options {
            macro_name: None,
            mock: None,
            repeater: None,
            allow_primitive: true,
            allow_std_prelude: false,
            krate: None,
            emit_item: true,
            layout: Layout::Auto,
            docs: false,
            deferred: false,
            owned: None,
            derived: false,
            given: Vec::new(),
            checks: Vec::new(),
            metas: Vec::new(),
        }
    }
}

/// The name of the internal flag that `#[announce]` writes in the arguments of the
/// announcement it has moved to the end of the attributes of the item, bare or with the
/// answer of the query (`__announce_deferred(attrs(a, b))`). Whether an attribute is
/// an announcement that has moved is told by this word only, never by its path.
pub(crate) const FLAG: &str = "__announce_deferred";

/// The flag as it is written: `__announce_deferred`, or, with the answer of the query
/// (`owned`), `__announce_deferred(attrs(a, b))`. An empty answer is left out when
/// there are no checks (`checks`): there was nothing to ask.
pub(crate) fn flag_tokens(owned: Option<&OwnedAttrs>, checks: bool) -> TokenStream {
    let flag = Ident::new(FLAG, Span::call_site());
    match owned {
        Some(o) if !o.is_empty() || checks => {
            let names = o.names();
            quote! { #flag(attrs(#(#names),*)) }
        }
        _ => quote! { #flag },
    }
}

impl Options {
    /// The attribute names that the checks own, if they are known: the query is
    /// answered (the flag carries the names), or there are no checks to ask.
    pub(crate) fn answer(&self) -> Option<OwnedAttrs> {
        match &self.owned {
            Some(o) => Some(o.clone()),
            None if self.checks.is_empty() => Some(OwnedAttrs::new()),
            None => None,
        }
    }

    /// The path to the `announce` crate that the generated code uses.
    ///
    /// `crate = ..` (the `krate` field) wins. Otherwise it is
    /// the name that the `Cargo.toml` of the crate being compiled
    /// (`CARGO_MANIFEST_DIR`) gives to its `announce` dependency, renames
    /// included, else `::announce`.
    pub fn krate(&self) -> Path {
        self.krate
            .clone()
            .unwrap_or_else(crate::cratepath::default_crate_path)
    }
}

/// Two announcements of one item disagree on an option that has one value only
/// (see [`Options::merge`]).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct MergeConflict {
    /// The name of the option, as written in the attribute. For `macro_name` the
    /// values are the effective trampoline names: the item's name where it is not set.
    pub option: &'static str,
    /// Its value in the announcement that is merged into.
    pub target: String,
    /// Its value in the announcement that is merged.
    pub folder: String,
}

impl MergeConflict {
    /// The message of the error, for the attribute that folds (`here`).
    pub fn message(&self) -> String {
        if self.option == "macro_name" {
            return format!(
                "announce: this item is announced twice with different trampoline names (`{}` \
                 and `{}`). One item has one trampoline, so use the same `macro_name` (or none) \
                 in every announcement",
                self.folder, self.target
            );
        }
        format!(
            "announce: this announcement sets `{} = {}`, but another announcement of the same \
             item sets `{} = {}`. The announcements of one item are merged into one trampoline, \
             so they must agree: remove one of them or make them equal",
            self.option, self.folder, self.option, self.target
        )
    }
}

fn tokens_text(t: &dyn ToTokens) -> String {
    t.to_token_stream().to_string()
}

/// The value of an option that has one value only, from two announcements: the one
/// that is set, or the common value; a conflict when both are set and differ.
fn agree<T: ToTokens + Clone>(
    option: &'static str,
    target: &Option<T>,
    folder: &Option<T>,
) -> std::result::Result<Option<T>, MergeConflict> {
    match (target, folder) {
        (Some(a), Some(b)) if tokens_text(a) != tokens_text(b) => Err(MergeConflict {
            option,
            target: tokens_text(a).split_whitespace().collect(),
            folder: tokens_text(b).split_whitespace().collect(),
        }),
        (Some(a), _) => Ok(Some(a.clone())),
        (None, b) => Ok(b.clone()),
    }
}

/// `target` followed by the entries of `folder` that are not in it yet.
fn union(target: &[Check], folder: &[Check]) -> Vec<Check> {
    let mut out = target.to_vec();
    for c in folder {
        let text = tokens_text(c);
        if !out.iter().any(|o| tokens_text(o) == text) {
            out.push(c.clone());
        }
    }
    out
}

impl Options {
    /// Whether the key was written (or, for options built in code, has a value
    /// other than the default).
    fn is_given(&self, key: &str, not_default: bool) -> bool {
        not_default || self.given.iter().any(|g| g == key)
    }

    /// `emit_item` if it is set: written, or `false`.
    fn explicit_emit_item(&self) -> Option<bool> {
        self.is_given("emit_item", !self.emit_item)
            .then(|| self.emit_item)
    }

    /// The name of the trampoline: `macro_name`, else the name of the item (`item`).
    /// A raw identifier names the same macro as the plain one.
    fn trampoline_name(&self, item: &Ident) -> String {
        use syn::ext::IdentExt;
        self.macro_name.as_ref().unwrap_or(item).unraw().to_string()
    }

    /// The options of two announcements of one item, as one announcement.
    ///
    /// `self` is the announcement that stays (the *target*), `folder` the one that
    /// is merged into it, `item` the name of the item. The result is what the target
    /// would be if it had been written with the options of both. A pure function: it
    /// does not read the item or the environment.
    ///
    /// | option | result |
    /// |---|---|
    /// | checks, `meta` | the target's, then the folder's that are not in it yet (exact duplicates dropped) |
    /// | `allow_primitive`, `allow_std_prelude`, `docs` | the OR of the two effective values (the default of `allow_primitive` is `true`, so it is `false` only when both say so) |
    /// | `crate` | the target's if set, else the folder's: two paths that both name `announce` do not conflict |
    /// | `macro_name` | the effective names must be equal (`macro_name`, else `item`): an explicit one is kept; different names are a conflict, as one item has one trampoline |
    /// | `mock`, `repeater` | the one that is set; both set and different is a conflict |
    /// | `layout` | the one that is not `auto`; both and different is a conflict |
    /// | `emit_item` | the one that is set (written, or `false`); both and different is a conflict |
    /// | `__announce_deferred` | set if either is set; the answer to the query is kept if both announcements have one (or have no checks to ask): the names are united |
    ///
    /// `derived` is not written by anyone: the target's stays. The written keys
    /// (`given`) are united. The attributes that checks own are not an option: they
    /// come from the checks' answers to the query, which runs after the merge, on the
    /// checks of both.
    pub(crate) fn merge(
        &self,
        folder: &Options,
        item: &Ident,
    ) -> std::result::Result<Options, MergeConflict> {
        let (target_name, folder_name) = (self.trampoline_name(item), folder.trampoline_name(item));
        if target_name != folder_name {
            return Err(MergeConflict {
                option: "macro_name",
                target: target_name,
                folder: folder_name,
            });
        }
        let mut out = self.clone();
        if out.macro_name.is_none() {
            out.macro_name = folder.macro_name.clone();
        }
        out.mock = agree("mock", &self.mock, &folder.mock)?;
        out.repeater = agree("repeater", &self.repeater, &folder.repeater)?;
        if out.layout == Layout::Auto {
            out.layout = folder.layout;
        } else if folder.layout != Layout::Auto && folder.layout != self.layout {
            return Err(MergeConflict {
                option: "layout",
                target: format!("{:?}", self.layout).to_lowercase(),
                folder: format!("{:?}", folder.layout).to_lowercase(),
            });
        }
        out.emit_item = match (self.explicit_emit_item(), folder.explicit_emit_item()) {
            (Some(a), Some(b)) if a != b => {
                return Err(MergeConflict {
                    option: "emit_item",
                    target: a.to_string(),
                    folder: b.to_string(),
                })
            }
            (Some(a), _) | (None, Some(a)) => a,
            (None, None) => true,
        };
        if out.krate.is_none() {
            out.krate = folder.krate.clone();
        }
        out.allow_primitive = self.allow_primitive || folder.allow_primitive;
        out.allow_std_prelude = self.allow_std_prelude || folder.allow_std_prelude;
        out.docs = self.docs || folder.docs;
        out.deferred = self.deferred || folder.deferred;
        out.owned = match (out.deferred, self.answer(), folder.answer()) {
            (true, Some(mut a), Some(b)) => {
                a.extend(b.names().iter().cloned());
                Some(a)
            }
            _ => None,
        };
        out.checks = union(&self.checks, &folder.checks);
        out.metas = union(&self.metas, &folder.metas);
        for key in &folder.given {
            if !out.given.contains(key) {
                out.given.push(key.clone());
            }
        }
        Ok(out)
    }
}

impl ToTokens for Options {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        let mut parts: Vec<TokenStream> = Vec::new();
        if let Some(k) = &self.krate {
            parts.push(quote! { crate = #k });
        }
        if let Some(n) = &self.macro_name {
            parts.push(quote! { macro_name = #n });
        }
        if let Some(m) = &self.mock {
            parts.push(quote! { mock = #m });
        }
        if let Some(r) = &self.repeater {
            parts.push(quote! { repeater = #r });
        }
        if !self.allow_primitive {
            parts.push(quote! { allow_primitive = false });
        }
        if self.allow_std_prelude {
            parts.push(quote! { allow_std_prelude = true });
        }
        if !self.emit_item {
            parts.push(quote! { emit_item = false });
        }
        match self.layout {
            Layout::Auto => {}
            Layout::Single => parts.push(quote! { layout = single }),
            Layout::Direct => parts.push(quote! { layout = direct }),
        }
        if self.docs {
            parts.push(quote! { docs = true });
        }
        if self.deferred {
            parts.push(flag_tokens(self.owned.as_ref(), !self.checks.is_empty()));
        }
        for m in &self.metas {
            parts.push(quote! { meta = #m });
        }
        for c in &self.checks {
            parts.push(quote! { #c });
        }
        tokens.extend(quote! { #(#parts),* });
    }
}

impl Parse for Options {
    fn parse(input: ParseStream) -> Result<Self> {
        let mut opts = Options::default();
        while !input.is_empty() {
            if is_flag(input) {
                input.parse::<Ident>()?;
                opts.deferred = true;
                if input.peek(syn::token::Paren) {
                    opts.owned = Some(parse_flag_answer(input)?);
                }
            } else if input.peek(Token![crate]) && input.peek2(Token![=]) {
                input.parse::<Token![crate]>()?;
                input.parse::<Token![=]>()?;
                opts.krate = Some(Path::parse_mod_style(input)?);
            } else if input.peek(syn::Ident) && input.peek2(Token![=]) && !input.peek2(Token![==]) {
                let key: Ident = input.parse()?;
                input.parse::<Token![=]>()?;
                opts.given.push(key.to_string());
                match key.to_string().as_str() {
                    "macro_name" => opts.macro_name = Some(input.parse()?),
                    "export_name" | "marker" => {
                        return Err(Error::new(
                            key.span(),
                            if key == "export_name" {
                                "`export_name` was removed: the hidden macro name is generated; \
                                 use `macro_name = ..` to choose the public name"
                            } else {
                                "`marker` was removed: the `Announce` impl follows the item \
                                 (none with `emit_item = false` or `mock`); use `#[derive(Announce)]` \
                                 to announce a struct, enum or union and implement it"
                            },
                        ))
                    }
                    "mock" => opts.mock = Some(Path::parse_mod_style(input)?),
                    "repeater" => {
                        let p = Path::parse_mod_style(input)?;
                        crate::leak::check_repeater(&p)?;
                        opts.repeater = Some(p);
                    }
                    "signature_only" | "transform" => {
                        return Err(Error::new(
                            key.span(),
                            format!(
                                "`{}` is not a definition-side option: transformers are chosen by the caller, \
                                 e.g. `F! {{ [F!, ::announce::signature_only!, callback!] .. }}`",
                                key
                            ),
                        ))
                    }
                    "emit_item" => opts.emit_item = input.parse::<LitBool>()?.value,
                    "layout" => {
                        let v: Ident = input.parse()?;
                        opts.layout = match v.to_string().as_str() {
                            "auto" => Layout::Auto,
                            "single" => Layout::Single,
                            "direct" => Layout::Direct,
                            _ => {
                                return Err(Error::new(
                                    v.span(),
                                    "`layout` is `auto`, `single` or `direct`",
                                ))
                            }
                        };
                    }
                    "docs" => opts.docs = input.parse::<LitBool>()?.value,
                    "allow_primitive" => opts.allow_primitive = input.parse::<LitBool>()?.value,
                    "allow_std_prelude" => opts.allow_std_prelude = input.parse::<LitBool>()?.value,
                    "meta" => {
                        let path = Path::parse_mod_style(input)?;
                        let args = if input.peek(syn::token::Paren) {
                            let content;
                            syn::parenthesized!(content in input);
                            Some(content.parse::<TokenStream>()?)
                        } else {
                            None
                        };
                        opts.metas.push(Check { path, args });
                    }
                    _ => {
                        return Err(Error::new(
                            key.span(),
                            "unknown announce option; expected one of `macro_name`, \
                             `mock`, `repeater`, `allow_primitive`, `allow_std_prelude`, `crate`, `emit_item`, `layout`, `docs`, `meta` (a bare path is a check)",
                        ))
                    }
                }
            } else {
                let path = Path::parse_mod_style(input)?;
                let args = if input.peek(syn::token::Paren) {
                    let content;
                    syn::parenthesized!(content in input);
                    Some(content.parse::<TokenStream>()?)
                } else {
                    None
                };
                opts.checks.push(Check { path, args });
            }
            if input.is_empty() {
                break;
            }
            input.parse::<Token![,]>()?;
        }
        Ok(opts)
    }
}

/// Whether the flag [`FLAG`] comes next: the word alone, or with its answer.
fn is_flag(input: ParseStream) -> bool {
    let fork = input.fork();
    match fork.parse::<Ident>() {
        Ok(i) if i == FLAG => {
            fork.is_empty() || fork.peek(Token![,]) || fork.peek(syn::token::Paren)
        }
        _ => false,
    }
}

/// The `(attrs(a, b))` of the flag.
fn parse_flag_answer(input: ParseStream) -> Result<OwnedAttrs> {
    const FORM: &str = "the answer of the flag is `(attrs(name, ..))`";
    let outer;
    syn::parenthesized!(outer in input);
    let key: Ident = outer.parse()?;
    if key != "attrs" || !outer.peek(syn::token::Paren) {
        return Err(Error::new(key.span(), FORM));
    }
    let names;
    syn::parenthesized!(names in outer);
    let owned = OwnedAttrs::parse_names(names.parse()?)?;
    if !outer.is_empty() {
        return Err(outer.error(FORM));
    }
    Ok(owned)
}

/// Arguments of the hidden `#[__announce(RANDOM; [OWNED;] OPTIONS)]` helper
/// attribute that `#[announce]` puts on the capture dummy. `OWNED` is a bracketed,
/// comma-separated list of the attribute names that the checks own; it is left
/// out when there are none.
pub struct CaptureArgs {
    pub random: u64,
    pub owned: crate::helper::OwnedAttrs,
    pub opts: Options,
}

impl Parse for CaptureArgs {
    fn parse(input: ParseStream) -> Result<Self> {
        let lit: syn::LitInt = input.parse()?;
        input.parse::<Token![;]>()?;
        let owned = if input.peek(syn::token::Bracket) {
            let content;
            syn::bracketed!(content in input);
            let names: syn::punctuated::Punctuated<Ident, Token![,]> =
                syn::punctuated::Punctuated::parse_terminated(&content)?;
            input.parse::<Token![;]>()?;
            crate::helper::OwnedAttrs::from_idents(names)
        } else {
            crate::helper::OwnedAttrs::new()
        };
        Ok(CaptureArgs {
            random: lit.base10_parse()?,
            owned,
            opts: input.parse()?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn opts(src: &str) -> Options {
        syn::parse2(src.parse().unwrap()).unwrap_or_else(|e| panic!("{}: {}", src, e))
    }

    fn text(o: &Options) -> String {
        o.to_token_stream().to_string().split_whitespace().collect()
    }

    fn ident(name: &str) -> Ident {
        syn::parse_str(name).unwrap()
    }

    /// The merge of two sets of options on the item `S`, printed.
    fn merged(target: &str, folder: &str) -> String {
        merged_on("S", target, folder)
    }

    fn merged_on(item: &str, target: &str, folder: &str) -> String {
        text(&opts(target).merge(&opts(folder), &ident(item)).unwrap())
    }

    fn conflict(target: &str, folder: &str) -> MergeConflict {
        conflict_on("S", target, folder)
    }

    fn conflict_on(item: &str, target: &str, folder: &str) -> MergeConflict {
        opts(target)
            .merge(&opts(folder), &ident(item))
            .err()
            .unwrap()
    }

    #[test]
    fn lists_are_a_union_that_keeps_the_order() {
        assert_eq!(
            merged("a::c1, meta = a::m", "b::c2(x), meta = b::m"),
            "meta=a::m,meta=b::m,a::c1,b::c2(x)"
        );
        // The target's entries come first, whatever the folder wrote first.
        assert_eq!(merged("meta = a::m", "meta = b::m"), "meta=a::m,meta=b::m");
        assert_eq!(merged("meta = b::m", "meta = a::m"), "meta=b::m,meta=a::m");
    }

    #[test]
    fn exact_duplicates_are_dropped() {
        assert_eq!(
            merged(
                "a::c(1), meta = m::x",
                "a::c(1), a::c(2), meta = m::x, meta = m::y"
            ),
            "meta=m::x,meta=m::y,a::c(1),a::c(2)"
        );
        // A check and a meta entry of the same path are different things.
        assert_eq!(merged("a::c", "meta = a::c"), "meta=a::c,a::c");
        // Merging with itself changes nothing.
        let o = "macro_name = N, meta = m::x, a::c(1)";
        assert_eq!(merged(o, o), text(&opts(o)));
        assert_eq!(merged("", ""), "");
    }

    #[test]
    fn booleans_are_the_or_of_the_effective_values() {
        // `allow_primitive` is true unless both say false.
        assert_eq!(merged("allow_primitive = false", ""), "");
        assert_eq!(merged("", "allow_primitive = false"), "");
        assert_eq!(
            merged("allow_primitive = false", "allow_primitive = true"),
            ""
        );
        assert_eq!(
            merged("allow_primitive = false", "allow_primitive = false"),
            "allow_primitive=false"
        );
        // The others are false unless one says true.
        assert_eq!(
            merged("allow_std_prelude = true", ""),
            "allow_std_prelude=true"
        );
        assert_eq!(
            merged("", "allow_std_prelude = true"),
            "allow_std_prelude=true"
        );
        assert_eq!(merged("allow_std_prelude = false", ""), "");
        assert_eq!(merged("docs = true", "docs = false"), "docs=true");
        assert_eq!(merged("docs = false", "docs = true"), "docs=true");
    }

    #[test]
    fn crate_is_the_targets_else_the_folders() {
        assert_eq!(
            merged("crate = ::a::announce", "crate = ::b::announce"),
            "crate=::a::announce"
        );
        assert_eq!(merged("", "crate = ::b::announce"), "crate=::b::announce");
        assert_eq!(merged("crate = ::a::announce", ""), "crate=::a::announce");
    }

    #[test]
    fn single_valued_options_agree_or_conflict() {
        for (opt, a, b) in [
            ("mock", "x::A", "x::B"),
            ("repeater", "crate::A", "crate::B"),
            ("layout", "single", "direct"),
            ("emit_item", "false", "true"),
        ] {
            let (ta, tb) = (format!("{} = {}", opt, a), format!("{} = {}", opt, b));
            let c = conflict(&ta, &tb);
            assert_eq!(c.option, opt);
            assert_eq!((c.target.as_str(), c.folder.as_str()), (a, b), "{}", opt);
            assert!(c.message().contains(opt), "{}", c.message());
            // Equal values are fine, and a value that is set beats one that is not.
            assert_eq!(merged(&ta, &ta), text(&opts(&ta)));
            assert_eq!(merged(&ta, ""), text(&opts(&ta)), "{}", opt);
            assert_eq!(merged("", &tb), text(&opts(&tb)), "{}", opt);
        }
    }

    #[test]
    fn trampoline_names_must_be_equal() {
        // Both set: equal names merge, different names are a conflict.
        assert_eq!(merged("macro_name = A", "macro_name = A"), "macro_name=A");
        let c = conflict("macro_name = A", "macro_name = B");
        assert_eq!(c.option, "macro_name");
        assert_eq!((c.target.as_str(), c.folder.as_str()), ("A", "B"));
        // Unset is the name of the item: `S` here.
        assert_eq!(merged("", ""), "");
        let c = conflict("macro_name = B", "");
        assert_eq!((c.target.as_str(), c.folder.as_str()), ("B", "S"));
        let c = conflict("", "macro_name = B");
        assert_eq!((c.target.as_str(), c.folder.as_str()), ("S", "B"));
        // Set to the name of the item and unset are the same name; the explicit one stays.
        assert_eq!(merged("macro_name = S", ""), "macro_name=S");
        assert_eq!(merged("", "macro_name = S"), "macro_name=S");
        assert_eq!(merged("macro_name = S", "macro_name = S"), "macro_name=S");
        assert_eq!(merged_on("A", "macro_name = A", ""), "macro_name=A");
        assert_eq!(merged_on("A", "", "macro_name = A"), "macro_name=A");
        // The item's name is not the name of another item.
        assert_eq!(conflict_on("A", "macro_name = S", "").option, "macro_name");
        // A raw identifier is the same macro name.
        assert_eq!(merged("macro_name = r#S", ""), "macro_name=r#S");
        assert_eq!(
            merged("macro_name = r#A", "macro_name = A"),
            "macro_name=r#A"
        );
        assert_eq!(merged_on("r#S", "macro_name = S", ""), "macro_name=S");
    }

    #[test]
    fn the_trampoline_name_conflict_says_what_to_do() {
        let m = conflict("", "macro_name = ParseAlt").message();
        assert!(
            m.contains("announced twice with different trampoline names (`ParseAlt` and `S`)"),
            "{}",
            m
        );
        assert!(m.contains("one trampoline"), "{}", m);
        assert!(
            m.contains("use the same `macro_name` (or none) in every announcement"),
            "{}",
            m
        );
        // The message names the expanding announcement's name first.
        let m = conflict("macro_name = Parse", "macro_name = ParseAlt").message();
        assert!(m.contains("(`ParseAlt` and `Parse`)"), "{}", m);
    }

    #[test]
    fn layout_auto_is_not_a_value() {
        assert_eq!(merged("layout = auto", "layout = single"), "layout=single");
        assert_eq!(merged("layout = direct", "layout = auto"), "layout=direct");
        assert_eq!(
            merged("layout = single", "layout = single"),
            "layout=single"
        );
    }

    #[test]
    fn emit_item_true_is_a_value_only_when_written() {
        // Not written: the other one decides.
        assert_eq!(merged("", "emit_item = false"), "emit_item=false");
        assert_eq!(merged("emit_item = false", ""), "emit_item=false");
        // Written as the default, and `false` from the other: a disagreement.
        assert_eq!(
            conflict("emit_item = true", "emit_item = false").option,
            "emit_item"
        );
        // An option built in code has no written keys: `false` counts as set.
        let built = Options {
            emit_item: false,
            ..Options::default()
        };
        let s = ident("S");
        assert_eq!(
            text(&opts("").merge(&built, &s).unwrap()),
            "emit_item=false"
        );
        assert_eq!(
            text(&built.merge(&opts(""), &s).unwrap()),
            "emit_item=false"
        );
        assert!(built.merge(&opts("emit_item = true"), &s).is_err());
    }

    #[test]
    fn mock_paths_are_compared_as_tokens() {
        assert_eq!(merged("mock = a::B", "mock = a::B"), "mock=a::B");
        assert!(opts("mock = a::B")
            .merge(&opts("mock = ::a::B"), &ident("S"))
            .is_err());
    }

    #[test]
    fn the_flag_is_kept_if_either_has_it() {
        let flag = "__announce_deferred";
        assert_eq!(merged(flag, ""), flag);
        assert_eq!(merged("", flag), flag);
        assert_eq!(merged("", ""), "");
        // It is an ordinary option of the parser, and prints back.
        let o = opts(&format!("macro_name = N, {}", flag));
        assert!(o.deferred && o.owned.is_none());
        assert_eq!(text(&opts(&text(&o))), text(&o));
        // The old spelling is not an option any more.
        assert!(syn::parse2::<Options>("__deferred = true".parse().unwrap()).is_err());
        // It is one entry: a path that has its name, or a value of it, is not the flag.
        assert!(!opts("meta = a::__announce_deferred").deferred);
        assert!(!opts("a::__announce_deferred").deferred);
        assert_eq!(opts("a::__announce_deferred").checks.len(), 1);
        assert!(syn::parse2::<Options>("__announce_deferred = true".parse().unwrap()).is_err());
    }

    #[test]
    fn the_answer_of_the_query_travels_in_the_flag() {
        let names = |o: &Options| -> Option<Vec<String>> {
            o.owned
                .as_ref()
                .map(|o| o.names().iter().map(|n| n.to_string()).collect())
        };
        // Bare: not asked yet (or nothing to ask).
        let o = opts("a::check, __announce_deferred");
        assert!(o.deferred && o.owned.is_none() && o.answer().is_none());
        let o = opts("__announce_deferred");
        assert!(o.owned.is_none() && o.answer().unwrap().is_empty());
        // With the answer, which can be empty, and prints back as it was.
        for src in [
            "__announce_deferred(attrs(my_lib,other)),a::check",
            "__announce_deferred(attrs()),a::check",
            "__announce_deferred(attrs(my_lib))",
            "crate=::l::announce,__announce_deferred(attrs(x)),meta=m::x,a::check",
        ] {
            let o = opts(src);
            assert_eq!(text(&o), src);
            assert!(o.answer().is_some(), "{}", src);
        }
        assert_eq!(
            names(&opts("c::check, __announce_deferred(attrs(a, b))")),
            Some(vec!["a".to_string(), "b".to_string()])
        );
        // Not a form of the answer.
        for bad in [
            "__announce_deferred(a, b)",
            "__announce_deferred(attrs)",
            "__announce_deferred(attrs(a) x)",
            "__announce_deferred(attrs(1))",
        ] {
            assert!(
                syn::parse2::<Options>(bad.parse().unwrap()).is_err(),
                "{}",
                bad
            );
        }
    }

    #[test]
    fn the_answers_are_united_when_both_are_known() {
        let a = "a::check, __announce_deferred(attrs(x, y))";
        let b = "b::check, __announce_deferred(attrs(y, z))";
        assert_eq!(
            merged(a, b),
            "__announce_deferred(attrs(x,y,z)),a::check,b::check"
        );
        // No checks: nothing to ask, the answer is empty.
        assert_eq!(
            merged(a, "__announce_deferred"),
            "__announce_deferred(attrs(x,y)),a::check"
        );
        assert_eq!(
            merged("__announce_deferred", a),
            "__announce_deferred(attrs(x,y)),a::check"
        );
        // Checks that were not asked: the merged announcement has not been asked.
        assert_eq!(
            merged(a, "b::check, __announce_deferred"),
            "__announce_deferred,a::check,b::check"
        );
        assert_eq!(
            merged(a, "b::check"),
            "__announce_deferred,a::check,b::check"
        );
        assert_eq!(
            merged("b::check", a),
            "__announce_deferred,b::check,a::check"
        );
        // Without the flag on either side there is no answer to carry.
        assert!(opts("a::check")
            .merge(&opts("b::check"), &ident("S"))
            .unwrap()
            .owned
            .is_none());
    }

    #[test]
    fn the_written_keys_are_united() {
        let m = opts("macro_name = N")
            .merge(&opts("docs = true, macro_name = N"), &ident("S"))
            .unwrap();
        assert_eq!(m.given, ["macro_name", "docs"]);
    }

    #[test]
    fn the_merge_prints_back_as_options_the_target_can_parse() {
        let m = opts(
            "crate = ::a::announce, macro_name = N, allow_std_prelude = true, meta = m::x(1), a::c",
        )
        .merge(
            &opts("docs = true, layout = single, meta = m::y, b::c(2), a::c"),
            // The folder has no `macro_name`: the item is named `N` as well.
            &ident("N"),
        )
        .unwrap();
        let printed = m.to_token_stream();
        let again: Options = syn::parse2(printed.clone()).unwrap();
        assert_eq!(text(&again), text(&m));
        assert_eq!(
            text(&m),
            "crate=::a::announce,macro_name=N,allow_std_prelude=true,layout=single,docs=true,\
             meta=m::x(1),meta=m::y,a::c,b::c(2)"
        );
    }

    #[test]
    fn the_merge_is_pure() {
        let (a, b) = (opts("meta = a::x"), opts("meta = b::x"));
        let _ = a.merge(&b, &ident("S")).unwrap();
        assert_eq!(text(&a), "meta=a::x");
        assert_eq!(text(&b), "meta=b::x");
    }
}
