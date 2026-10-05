//! The path under which the crate using `#[announce]` reaches the `announce`
//! facade, so that generated code works when the dependency is renamed
//! (`ann = { package = "announce", .. }`).
//!
//! The caller's `Cargo.toml` (`CARGO_MANIFEST_DIR`) is scanned for the
//! dependency whose package is `announce`, in `[dependencies]`,
//! `[dev-dependencies]`, `[build-dependencies]` and their
//! `[target.'..'.*]` forms, written inline (`ann = { package = "announce" }`),
//! as a table (`[dependencies.ann]` + `package = "announce"`), or inherited
//! from the workspace (`ann = { workspace = true }` / `ann.workspace = true`,
//! looked up in the workspace root's `[workspace.dependencies]`). A minimal
//! scanner is used instead of a TOML library to keep `announce-macros`
//! dependency-light and within its MSRV. If nothing is found, `::announce` is
//! used; `#[announce(crate = ..)]` always takes precedence.

use std::path::{Path as FsPath, PathBuf};

const PACKAGE: &str = "announce";

/// One dependency entry: its key (the name it is used under) and the
/// package it refers to, if renamed, and whether it is workspace-inherited.
struct Dep {
    key: String,
    package: Option<String>,
    workspace: bool,
}

fn strip_comment(line: &str) -> &str {
    // A `#` outside a string starts a comment.
    let mut in_str = false;
    for (i, c) in line.char_indices() {
        match c {
            '"' => in_str = !in_str,
            '#' if !in_str => return &line[..i],
            _ => {}
        }
    }
    line
}

fn unquote(s: &str) -> String {
    s.trim().trim_matches(|c| c == '"' || c == '\'').to_string()
}

/// `package = "x"` / `workspace = true` inside an inline table `{ .. }`.
fn inline_field<'a>(table: &'a str, field: &str) -> Option<&'a str> {
    for part in table.trim().trim_start_matches('{').trim_end_matches('}').split(',') {
        let mut kv = part.splitn(2, '=');
        let k = kv.next()?.trim();
        if k == field {
            return Some(kv.next()?.trim());
        }
    }
    None
}

fn is_dep_table(header: &str, workspace: bool) -> bool {
    let h = header.trim();
    if workspace {
        return h == "workspace.dependencies";
    }
    let last = h.rsplit('.').next().unwrap_or("");
    let kinds = ["dependencies", "dev-dependencies", "build-dependencies"];
    // `dependencies`, or `target.<cfg>.dependencies` (cfg may contain dots in
    // quotes; only the last component matters).
    kinds.contains(&h) || (h.starts_with("target.") && kinds.contains(&last))
}

/// `dependencies.ann` style table header: returns the dependency key.
fn dep_table_key(header: &str, workspace: bool) -> Option<String> {
    let h = header.trim();
    let kinds: &[&str] = if workspace {
        &["workspace.dependencies."]
    } else {
        &["dependencies.", "dev-dependencies.", "build-dependencies."]
    };
    for k in kinds {
        if let Some(rest) = h.strip_prefix(k) {
            return Some(unquote(rest));
        }
        if !workspace && h.starts_with("target.") {
            if let Some(pos) = h.find(&format!(".{}", k)) {
                return Some(unquote(&h[pos + 1 + k.len()..]));
            }
        }
    }
    None
}

fn scan(manifest: &str, workspace: bool) -> Vec<Dep> {
    let mut deps: Vec<Dep> = Vec::new();
    let mut in_deps = false;
    let mut table_key: Option<usize> = None;
    for raw in manifest.lines() {
        let line = strip_comment(raw).trim();
        if line.is_empty() {
            continue;
        }
        if line.starts_with('[') {
            let header = line.trim_start_matches('[').trim_end_matches(']');
            in_deps = is_dep_table(header, workspace);
            table_key = dep_table_key(header, workspace).map(|key| {
                deps.push(Dep { key, package: None, workspace: false });
                deps.len() - 1
            });
            continue;
        }
        let mut kv = line.splitn(2, '=');
        let (k, v) = match (kv.next(), kv.next()) {
            (Some(k), Some(v)) => (k.trim(), v.trim()),
            _ => continue,
        };
        if let Some(i) = table_key {
            match k {
                "package" => deps[i].package = Some(unquote(v)),
                "workspace" => deps[i].workspace = v == "true",
                _ => {}
            }
        } else if in_deps {
            if let Some(key) = k.strip_suffix(".workspace") {
                deps.push(Dep { key: unquote(key), package: None, workspace: v == "true" });
            } else if v.starts_with('{') {
                deps.push(Dep {
                    key: unquote(k),
                    package: inline_field(v, "package").map(unquote),
                    workspace: inline_field(v, "workspace") == Some("true"),
                });
            } else {
                deps.push(Dep { key: unquote(k), package: None, workspace: false });
            }
        }
    }
    deps
}

fn workspace_root(start: &FsPath) -> Option<PathBuf> {
    let mut dir = start.parent();
    while let Some(d) = dir {
        let candidate = d.join("Cargo.toml");
        if let Ok(text) = std::fs::read_to_string(&candidate) {
            if text.lines().any(|l| strip_comment(l).trim() == "[workspace]") {
                return Some(candidate);
            }
        }
        dir = d.parent();
    }
    None
}

/// The name (as an identifier) under which `manifest_dir`'s crate depends
/// on `announce`, if it does.
pub fn dependency_name(manifest_dir: &FsPath) -> Option<String> {
    let manifest_path = manifest_dir.join("Cargo.toml");
    let manifest = std::fs::read_to_string(&manifest_path).ok()?;
    let deps = scan(&manifest, false);
    let mut ws_deps: Option<Vec<Dep>> = None;
    for d in &deps {
        let package = if d.workspace {
            let ws = ws_deps.get_or_insert_with(|| {
                workspace_root(&manifest_path)
                    .and_then(|p| std::fs::read_to_string(p).ok())
                    .map(|t| scan(&t, true))
                    .unwrap_or_default()
            });
            ws.iter()
                .find(|w| w.key == d.key)
                .map(|w| w.package.clone().unwrap_or_else(|| w.key.clone()))
        } else {
            Some(d.package.clone().unwrap_or_else(|| d.key.clone()))
        };
        if package.as_deref() == Some(PACKAGE) {
            return Some(d.key.replace('-', "_"));
        }
    }
    None
}

/// `::<name>` for the caller's `announce` dependency, defaulting to
/// `::announce` (also when the caller *is* the announce package, i.e. its own
/// tests and doctests, which reach it as `::announce`).
pub fn default_crate_path() -> syn::Path {
    let name = std::env::var("CARGO_MANIFEST_DIR")
        .ok()
        .filter(|_| std::env::var("CARGO_PKG_NAME").ok().as_deref() != Some(PACKAGE))
        .and_then(|dir| dependency_name(FsPath::new(&dir)))
        .unwrap_or_else(|| PACKAGE.to_string());
    let ident = proc_macro2::Ident::new(&name, proc_macro2::Span::call_site());
    syn::parse_quote!(::#ident)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn name_in(manifest: &str) -> Option<String> {
        scan(manifest, false).into_iter().find_map(|d| {
            let p = d.package.clone().unwrap_or_else(|| d.key.clone());
            (p == PACKAGE && !d.workspace).then(|| d.key.replace('-', "_"))
        })
    }

    #[test]
    fn finds_renames() {
        assert_eq!(name_in("[dependencies]\nannounce = \"0.1\"\n").as_deref(), Some("announce"));
        assert_eq!(
            name_in("[dependencies]\nmy-ann = { version = \"0.1\", package = \"announce\" } # c\n").as_deref(),
            Some("my_ann")
        );
        assert_eq!(
            name_in("[dependencies.ann]\nversion = \"0.1\"\npackage = \"announce\"\n").as_deref(),
            Some("ann")
        );
        assert_eq!(
            name_in("[target.'cfg(unix)'.dependencies]\nx = { package = \"announce\" }\n").as_deref(),
            Some("x")
        );
        assert_eq!(name_in("[dependencies]\nother = \"1\"\n[features]\nannounce = []\n"), None);
        let ws = scan("[workspace]\n[workspace.dependencies]\nann = { package = \"announce\", path = \"a\" }\n", true);
        assert_eq!(ws[0].package.as_deref(), Some("announce"));
        let member = scan("[dependencies]\nann.workspace = true\nb = { workspace = true }\n", false);
        assert!(member[0].workspace && member[1].workspace);
    }
}
