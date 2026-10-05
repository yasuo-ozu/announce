use proc_macro2::{Ident, TokenStream};
use syn::parse::{Parse, ParseStream};
use syn::{Error, LitBool, Path, Result, Token};

/// One `#[announce(path)]` / `#[announce(path(args))]` entry.
#[derive(Clone)]
pub struct Check {
    pub path: Path,
    pub args: Option<TokenStream>,
}

/// Parsed `#[announce(...)]` arguments. `key = value` is an option, any other
/// path is a check (DESIGN.md section 4).
#[derive(Clone)]
pub struct Options {
    pub macro_name: Option<Ident>,
    pub export_name: Option<Ident>,
    pub mock: Option<Path>,
    /// `repeater = path`: emit the type-leak table against this repeater trait.
    pub repeater: Option<Path>,
    /// type-leak: primitives resolve everywhere (default `true`).
    pub allow_primitive: bool,
    /// type-leak: std prelude names resolve everywhere (default `false`).
    pub allow_std_prelude: bool,
    pub krate: Option<Path>,
    /// `emit_item = false`: do not re-emit the item (for derives, whose input
    /// already exists).
    pub emit_item: bool,
    pub marker: Option<bool>,
    pub checks: Vec<Check>,
    /// `meta = path(args)`: listed in META like a check, but never invoked.
    pub metas: Vec<Check>,
}

impl Default for Options {
    fn default() -> Self {
        Options {
            macro_name: None,
            export_name: None,
            mock: None,
            repeater: None,
            allow_primitive: true,
            allow_std_prelude: false,
            krate: None,
            emit_item: true,
            marker: None,
            checks: Vec::new(),
            metas: Vec::new(),
        }
    }
}

impl Options {
    pub fn krate(&self) -> Path {
        // `crate = ..` wins; otherwise the name the caller's Cargo.toml gives
        // the `announce` dependency (renames included), else `::announce`.
        self.krate
            .clone()
            .unwrap_or_else(crate::cratepath::default_crate_path)
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
                match key.to_string().as_str() {
                    "macro_name" => opts.macro_name = Some(input.parse()?),
                    "export_name" => opts.export_name = Some(input.parse()?),
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
                    "allow_primitive" => opts.allow_primitive = input.parse::<LitBool>()?.value,
                    "allow_std_prelude" => opts.allow_std_prelude = input.parse::<LitBool>()?.value,
                    "marker" => opts.marker = Some(input.parse::<LitBool>()?.value),
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
                            "unknown announce option; expected one of `macro_name`, `export_name`, \
                             `mock`, `repeater`, `allow_primitive`, `allow_std_prelude`, `crate`, `emit_item`, `marker`, `meta` (a bare path is a check)",
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

/// Arguments of the hidden `#[__announce(RANDOM; OPTIONS)]` helper attribute that
/// `#[announce]` puts on the capture dummy.
pub struct CaptureArgs {
    pub random: u64,
    pub opts: Options,
}

impl Parse for CaptureArgs {
    fn parse(input: ParseStream) -> Result<Self> {
        let lit: syn::LitInt = input.parse()?;
        input.parse::<Token![;]>()?;
        Ok(CaptureArgs {
            random: lit.base10_parse()?,
            opts: input.parse()?,
        })
    }
}
