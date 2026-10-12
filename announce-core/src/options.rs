use proc_macro2::{Ident, TokenStream};
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
/// The default, [`Layout::Auto`], is the right choice almost always; see
/// DESIGN.md section 4.1.
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
    /// `macro_name = Name`: name of the emitted macro. Default: the item's name.
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
    /// Default `auto`: by the size of the item (advanced, DESIGN.md section 4.1).
    pub layout: Layout,
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
            derived: false,
            given: Vec::new(),
            checks: Vec::new(),
            metas: Vec::new(),
        }
    }
}

impl Options {
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
            if input.peek(Token![crate]) && input.peek2(Token![=]) {
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
                             `mock`, `repeater`, `allow_primitive`, `allow_std_prelude`, `crate`, `emit_item`, `meta` (a bare path is a check)",
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

/// Arguments of the hidden `#[__announce(RANDOM; [OWNED;] OPTIONS)]` helper
/// attribute that `#[announce]` puts on the capture dummy. `OWNED` is a bracketed,
/// comma-separated list of the attribute names that the checks own; it is left
/// out when there are none.
pub struct CaptureArgs {
    pub random: u64,
    pub owned: announce_helper::OwnedAttrs,
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
            announce_helper::OwnedAttrs::from_idents(names)
        } else {
            announce_helper::OwnedAttrs::new()
        };
        Ok(CaptureArgs {
            random: lit.base10_parse()?,
            owned,
            opts: input.parse()?,
        })
    }
}
