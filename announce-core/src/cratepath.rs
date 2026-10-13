//! The path under which the crate using `#[announce]` reaches the `announce`
//! facade, so that generated code works when the dependency is renamed
//! (`ann = { package = "announce", .. }`).
//!
//! The caller's `Cargo.toml` (`CARGO_MANIFEST_DIR`) is scanned for the
//! dependency whose package is `announce`, in `[dependencies]`,
//! `[dev-dependencies]`, `[build-dependencies]` and their
//! `[target.'..'.*]` forms, written inline (`ann = { package = "announce" }`,
//! also over several lines), with dotted keys (`ann.package = "announce"`),
//! as a table (`[dependencies.ann]` + `package = "announce"`), or inherited
//! from the workspace (`ann = { workspace = true }` / `ann.workspace = true`,
//! looked up in the workspace root's `[workspace.dependencies]`, which may be
//! written in any of those forms). Keys may be quoted. A minimal scanner is
//! used instead of a TOML library to keep `announce-macros` dependency-light and
//! within its MSRV: it joins a value that spans lines until its brackets and
//! braces balance, drops comments and the content of strings, and reads the
//! `package` and `workspace` fields of each dependency. If nothing is found,
//! `::announce` is used; `#[announce(crate = ..)]` always takes precedence.

use std::path::{Path as FsPath, PathBuf};

const PACKAGE: &str = "announce";

/// One dependency entry: its key (the name it is used under) and the
/// package it refers to, if renamed, and whether it is workspace-inherited.
struct Dep {
    key: String,
    package: Option<String>,
    workspace: bool,
}

/// The manifest as logical lines: one `key = value` or `[header]` each, with
/// comments removed, the content of multi-line strings dropped (`""`), and a
/// value that spans lines (an inline table or an array) joined with spaces
/// until its `{` and `[` are closed. Single-line strings are kept as written.
fn logical_lines(manifest: &str) -> Vec<String> {
    fn flush(out: &mut Vec<String>, cur: &mut String) {
        let line = cur.trim();
        if !line.is_empty() {
            out.push(line.to_string());
        }
        cur.clear();
    }
    let chars: Vec<char> = manifest.chars().collect();
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut depth = 0usize;
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        match c {
            '#' => {
                while i < chars.len() && chars[i] != '\n' {
                    i += 1;
                }
                continue;
            }
            '"' | '\'' => {
                if chars[i..].starts_with(&[c, c, c]) {
                    // A multi-line string: its content is of no interest.
                    i += 3;
                    while i < chars.len() && !chars[i..].starts_with(&[c, c, c]) {
                        i += if c == '"' && chars[i] == '\\' { 2 } else { 1 };
                    }
                    // Up to two more quotes can belong to the string.
                    i = (i + 3).min(chars.len());
                    while i < chars.len() && chars[i] == c {
                        i += 1;
                    }
                    cur.push_str("\"\"");
                } else {
                    cur.push(c);
                    i += 1;
                    while i < chars.len() && chars[i] != c && chars[i] != '\n' {
                        if c == '"' && chars[i] == '\\' && i + 1 < chars.len() {
                            cur.push(chars[i]);
                            i += 1;
                        }
                        cur.push(chars[i]);
                        i += 1;
                    }
                    if i < chars.len() && chars[i] == c {
                        cur.push(c);
                        i += 1;
                    }
                }
                continue;
            }
            '[' | '{' => depth += 1,
            ']' | '}' => depth = depth.saturating_sub(1),
            '\r' => {
                i += 1;
                continue;
            }
            '\n' => {
                if depth == 0 {
                    flush(&mut out, &mut cur);
                } else {
                    cur.push(' ');
                }
                i += 1;
                continue;
            }
            _ => {}
        }
        cur.push(c);
        i += 1;
    }
    flush(&mut out, &mut cur);
    out
}

fn unquote(s: &str) -> String {
    s.trim().trim_matches(|c| c == '"' || c == '\'').to_string()
}

/// Splits `s` at every `sep` that is outside strings and brackets.
fn split_top(s: &str, sep: char) -> Vec<&str> {
    let mut parts = Vec::new();
    let (mut start, mut depth, mut quote) = (0, 0usize, None);
    let mut chars = s.char_indices();
    while let Some((i, c)) = chars.next() {
        match (quote, c) {
            (Some(q), _) if c == q => quote = None,
            (Some('"'), '\\') => {
                chars.next();
            }
            (Some(_), _) => {}
            (None, '"') | (None, '\'') => quote = Some(c),
            (None, '[') | (None, '{') => depth += 1,
            (None, ']') | (None, '}') => depth = depth.saturating_sub(1),
            (None, _) if c == sep && depth == 0 => {
                parts.push(&s[start..i]);
                start = i + c.len_utf8();
            }
            _ => {}
        }
    }
    parts.push(&s[start..]);
    parts
}

/// `key = value` at the first `=` outside quotes.
fn split_kv(line: &str) -> Option<(&str, &str)> {
    let mut quote = None;
    for (i, c) in line.char_indices() {
        match (quote, c) {
            (Some(q), _) if c == q => quote = None,
            (Some(_), _) => {}
            (None, '"') | (None, '\'') => quote = Some(c),
            (None, '=') => return Some((line[..i].trim(), line[i + 1..].trim())),
            _ => {}
        }
    }
    None
}

/// The parts of a (dotted) key, unquoted: `ann.package`, `"my-ann".package`.
fn split_key(key: &str) -> Vec<String> {
    split_top(key, '.').into_iter().map(unquote).collect()
}

/// `package = "x"` / `workspace = true` inside an inline table `{ .. }`.
fn inline_field<'a>(table: &'a str, field: &str) -> Option<&'a str> {
    let table = table.trim();
    let table = table.strip_prefix('{').unwrap_or(table);
    let table = table.strip_suffix('}').unwrap_or(table);
    for part in split_top(table, ',') {
        if let Some((k, v)) = split_kv(part) {
            if split_key(k) == [field] {
                return Some(v);
            }
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
    // The dependencies of the current table start here: the lines of a dotted
    // key (`ann.package`, `ann.version`) belong to one dependency.
    let mut section = 0;
    for line in logical_lines(manifest) {
        if line.starts_with('[') {
            let header = line.trim_start_matches('[').trim_end_matches(']');
            in_deps = is_dep_table(header, workspace);
            section = deps.len();
            table_key = dep_table_key(header, workspace).map(|key| {
                deps.push(Dep {
                    key,
                    package: None,
                    workspace: false,
                });
                deps.len() - 1
            });
            continue;
        }
        let (k, v) = match split_kv(&line) {
            Some(kv) => kv,
            None => continue,
        };
        let parts = split_key(k);
        if let Some(i) = table_key {
            match parts.as_slice() {
                [f] if f == "package" => deps[i].package = Some(unquote(v)),
                [f] if f == "workspace" => deps[i].workspace = v == "true",
                _ => {}
            }
        } else if in_deps {
            match parts.as_slice() {
                // `ann = ".."` / `ann = { .. }`
                [key] => {
                    let inline = v.starts_with('{');
                    deps.push(Dep {
                        key: key.clone(),
                        package: inline
                            .then(|| inline_field(v, "package").map(unquote))
                            .flatten(),
                        workspace: inline && inline_field(v, "workspace") == Some("true"),
                    })
                }
                // `ann.package = ".."`, `ann.workspace = true`, `ann.version = ".."`
                [key, field] => {
                    let i = match deps[section..].iter().position(|d| d.key == *key) {
                        Some(i) => section + i,
                        None => {
                            deps.push(Dep {
                                key: key.clone(),
                                package: None,
                                workspace: false,
                            });
                            deps.len() - 1
                        }
                    };
                    match field.as_str() {
                        "package" => deps[i].package = Some(unquote(v)),
                        "workspace" => deps[i].workspace = v == "true",
                        _ => {}
                    }
                }
                _ => {}
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
            if logical_lines(&text).iter().any(|l| l == "[workspace]") {
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
    let name = default_crate_name();
    let ident = proc_macro2::Ident::new(&name, proc_macro2::Span::call_site());
    syn::parse_quote!(::#ident)
}

/// How long [`default_crate_name`] trusts what it read from the manifest. The
/// compiler expands every `#[announce]` of a crate in one process, and each
/// expansion asks (twice: the attribute and the `Capture` derive), so reading
/// and scanning `Cargo.toml` every time was a fixed cost of each definition. A
/// long-lived process (an IDE's macro server) sees an edit of the manifest after
/// this long.
const MANIFEST_TTL: std::time::Duration = std::time::Duration::from_secs(2);

/// The name of the `announce` dependency of the crate being compiled; see
/// [`dependency_name`]. The answer is kept for [`MANIFEST_TTL`], per manifest
/// directory and package name.
fn default_crate_name() -> String {
    let dir = std::env::var("CARGO_MANIFEST_DIR").ok();
    let pkg = std::env::var("CARGO_PKG_NAME").ok();
    cached((dir.clone(), pkg.clone()), MANIFEST_TTL, || {
        dir.filter(|_| pkg.as_deref() != Some(PACKAGE))
            .and_then(|dir| dependency_name(FsPath::new(&dir)))
            .unwrap_or_else(|| PACKAGE.to_string())
    })
}

/// The last answer of `lookup` for `key`, if it is not older than `ttl`; else
/// the answer of `lookup`, which is kept. (One entry per thread: a compiler
/// process asks for the same crate again and again.)
fn cached(
    key: (Option<String>, Option<String>),
    ttl: std::time::Duration,
    lookup: impl FnOnce() -> String,
) -> String {
    use std::cell::RefCell;
    use std::time::Instant;
    type Entry = ((Option<String>, Option<String>), Instant, String);
    thread_local! {
        static CACHE: RefCell<Option<Entry>> = RefCell::new(None);
    }
    let hit = CACHE.with(|c| match &*c.borrow() {
        Some((k, at, name)) if *k == key && at.elapsed() < ttl => Some(name.clone()),
        _ => None,
    });
    if let Some(name) = hit {
        return name;
    }
    let name = lookup();
    CACHE.with(|c| *c.borrow_mut() = Some((key, Instant::now(), name.clone())));
    name
}

#[cfg(test)]
mod tests {
    use super::*;

    // The renames, the table forms and the target tables are covered through
    // `dependency_name` in `manifest_tests`; this is the raw output of `scan`.
    #[test]
    fn scan_reads_workspace_entries_and_inheriting_members() {
        let ws = scan("[workspace]\n[workspace.dependencies]\nann = { package = \"announce\", path = \"a\" }\n", true);
        assert_eq!(ws[0].package.as_deref(), Some("announce"));
        let member = scan(
            "[dependencies]\nann.workspace = true\nb = { workspace = true }\n",
            false,
        );
        assert!(member[0].workspace && member[1].workspace);
    }
}

#[cfg(test)]
mod manifest_tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    /// A unique directory under the system temp dir, removed on drop.
    struct TempDir(PathBuf);

    impl TempDir {
        fn new(tag: &str) -> Self {
            static N: AtomicUsize = AtomicUsize::new(0);
            let dir = std::env::temp_dir().join(format!(
                "announce-cratepath-{}-{}-{}",
                std::process::id(),
                N.fetch_add(1, Ordering::Relaxed),
                tag
            ));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).unwrap();
            TempDir(dir)
        }

        /// Writes `dir/Cargo.toml` (creating `dir`) and returns `dir`.
        fn manifest(&self, dir: &str, text: &str) -> PathBuf {
            let d = if dir.is_empty() {
                self.0.clone()
            } else {
                self.0.join(dir)
            };
            std::fs::create_dir_all(&d).unwrap();
            std::fs::write(d.join("Cargo.toml"), text).unwrap();
            d
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    /// `dependency_name` of a single, standalone manifest.
    fn name_of(text: &str) -> Option<String> {
        let t = TempDir::new("single");
        dependency_name(&t.manifest("", text))
    }

    #[test]
    fn plain_dependency_and_unrelated_manifests() {
        assert_eq!(
            name_of("[dependencies]\nannounce = \"0.1\"\n").as_deref(),
            Some("announce")
        );
        assert_eq!(
            name_of("[dependencies]\nannounce = { version = \"0.1\", path = \"..\" }\n").as_deref(),
            Some("announce")
        );
        assert_eq!(
            name_of("[package]\nname = \"x\"\n[dependencies]\nserde = \"1\"\n"),
            None
        );
        assert_eq!(name_of(""), None);
        // No `Cargo.toml` at all.
        let t = TempDir::new("missing");
        assert_eq!(dependency_name(&t.0), None);
    }

    #[test]
    fn inline_table_rename_with_trailing_comment() {
        let m = "[dependencies]\n\
                 serde = \"1\"\n\
                 ann = { version = \"0.1\", package = \"announce\" } # the renamed one\n";
        assert_eq!(name_of(m).as_deref(), Some("ann"));
        // A comment line and a `#` inside a string do not confuse the scanner.
        let m = "[dependencies]\n# announce = \"0.1\"\nx = { version = \"0.1#2\", package = \"announce\" }\n";
        assert_eq!(name_of(m).as_deref(), Some("x"));
    }

    #[test]
    fn hyphens_become_underscores() {
        assert_eq!(
            name_of("[dependencies]\nmy-ann = { package = \"announce\" }\n").as_deref(),
            Some("my_ann")
        );
        assert_eq!(
            name_of("[dependencies.my-ann]\npackage = \"announce\"\n").as_deref(),
            Some("my_ann")
        );
    }

    #[test]
    fn dependency_table_form() {
        let m = "[dependencies.ann]\nversion = \"0.1\"\npackage = \"announce\"\n[dependencies.serde]\nversion = \"1\"\n";
        assert_eq!(name_of(m).as_deref(), Some("ann"));
        // The key itself is the package when `package` is absent.
        assert_eq!(
            name_of("[dependencies.announce]\nversion = \"0.1\"\n").as_deref(),
            Some("announce")
        );
        // Quoted key.
        assert_eq!(
            name_of("[dependencies.\"ann\"]\npackage = \"announce\"\n").as_deref(),
            Some("ann")
        );
        // A `package` key of another table must not leak into this one.
        let m =
            "[dependencies.ann]\nversion = \"0.1\"\n[package.metadata.x]\npackage = \"announce\"\n";
        assert_eq!(name_of(m), None);
    }

    #[test]
    fn target_specific_dependencies() {
        let m = "[target.'cfg(unix)'.dependencies]\nx = { package = \"announce\" }\n";
        assert_eq!(name_of(m).as_deref(), Some("x"));
        let m = "[target.'cfg(any(unix, windows))'.build-dependencies]\nb = { package = \"announce\" }\n";
        assert_eq!(name_of(m).as_deref(), Some("b"));
        let m =
            "[target.x86_64-unknown-linux-gnu.dev-dependencies]\nd = { package = \"announce\" }\n";
        assert_eq!(name_of(m).as_deref(), Some("d"));
        let m = "[target.'cfg(unix)'.dependencies.t]\npackage = \"announce\"\n";
        assert_eq!(name_of(m).as_deref(), Some("t"));
        // A target table that is not a dependency table.
        let m = "[target.'cfg(unix)'.lints]\nannounce = \"x\"\n";
        assert_eq!(name_of(m), None);
    }

    #[test]
    fn dev_and_build_dependency_tables() {
        assert_eq!(
            name_of("[dev-dependencies]\na = { package = \"announce\" }\n").as_deref(),
            Some("a")
        );
        assert_eq!(
            name_of("[build-dependencies]\nb = { package = \"announce\" }\n").as_deref(),
            Some("b")
        );
        assert_eq!(
            name_of("[dev-dependencies.d]\npackage = \"announce\"\n").as_deref(),
            Some("d")
        );
        assert_eq!(
            name_of("[build-dependencies.e]\npackage = \"announce\"\n").as_deref(),
            Some("e")
        );
        assert_eq!(
            name_of("[dev-dependencies]\nannounce = \"0.1\"\n").as_deref(),
            Some("announce")
        );
    }

    #[test]
    fn same_name_different_package_is_not_announce() {
        assert_eq!(
            name_of("[dependencies]\nannounce = { package = \"other\" }\n"),
            None
        );
        assert_eq!(
            name_of("[dependencies.announce]\npackage = \"other\"\n"),
            None
        );
        // Another crate renamed to a name that merely contains `announce`.
        assert_eq!(
            name_of("[dependencies]\nannounce-helper = \"0.1\"\nmy-announce = \"1\"\n"),
            None
        );
    }

    #[test]
    fn features_table_is_not_a_dependency() {
        let m = "[dependencies]\nother = \"1\"\n[features]\nannounce = []\n";
        assert_eq!(name_of(m), None);
        let m = "[features]\nannounce = [\"dep:x\"]\n";
        assert_eq!(name_of(m), None);
        // And a dependency after a non-dependency table is not picked up either.
        let m = "[features]\nx = []\n[package.metadata]\nannounce = \"1\"\n";
        assert_eq!(name_of(m), None);
    }

    #[test]
    fn first_matching_dependency_wins() {
        let m = "[dependencies]\na = { package = \"announce\" }\n[dev-dependencies]\nb = { package = \"announce\" }\n";
        assert_eq!(name_of(m).as_deref(), Some("a"));
    }

    #[test]
    fn workspace_inheritance_from_the_root_manifest() {
        let t = TempDir::new("ws");
        t.manifest(
            "",
            "[workspace]\nmembers = [\"m\"]\n\
             [workspace.dependencies]\n\
             serde = \"1\"\n\
             ann-ws = { package = \"announce\", path = \"announce\" }\n",
        );
        let m = t.manifest(
            "m",
            "[package]\nname = \"m\"\n[dependencies]\nann-ws.workspace = true\n",
        );
        assert_eq!(dependency_name(&m).as_deref(), Some("ann_ws"));
        let m = t.manifest("m2", "[dependencies]\nann-ws = { workspace = true }\n");
        assert_eq!(dependency_name(&m).as_deref(), Some("ann_ws"));
        let m = t.manifest("m3", "[dev-dependencies.ann-ws]\nworkspace = true\n");
        assert_eq!(dependency_name(&m).as_deref(), Some("ann_ws"));
        // Inherited, but not announce.
        let m = t.manifest("m4", "[dependencies]\nserde.workspace = true\n");
        assert_eq!(dependency_name(&m), None);
        // Inherited from a key the workspace does not define.
        let m = t.manifest("m5", "[dependencies]\nghost.workspace = true\n");
        assert_eq!(dependency_name(&m), None);
        // `workspace = false` is an ordinary dependency named `ann-ws`.
        let m = t.manifest(
            "m6",
            "[dependencies]\nann-ws = { workspace = false, version = \"1\" }\n",
        );
        assert_eq!(dependency_name(&m), None);
    }

    #[test]
    fn workspace_dependency_without_rename_uses_its_key() {
        let t = TempDir::new("ws-plain");
        t.manifest(
            "",
            "[workspace]\n[workspace.dependencies]\nannounce = { path = \"announce\" }\n",
        );
        let m = t.manifest(
            "crates/deep/m",
            "[dependencies]\nannounce = { workspace = true }\n",
        );
        // The root is found by walking up from the member, not only one level.
        assert_eq!(dependency_name(&m).as_deref(), Some("announce"));
    }

    #[test]
    fn workspace_dependency_table_form() {
        let t = TempDir::new("ws-table");
        t.manifest(
            "",
            "[workspace]\n[workspace.dependencies.renamed]\npackage = \"announce\"\npath = \"a\"\n",
        );
        let m = t.manifest("m", "[dependencies]\nrenamed.workspace = true\n");
        assert_eq!(dependency_name(&m).as_deref(), Some("renamed"));
    }

    #[test]
    fn workspace_and_package_in_one_manifest() {
        let t = TempDir::new("ws-self");
        let m = t.manifest(
            "",
            "[package]\nname = \"root\"\n[workspace]\n\
             [workspace.dependencies]\nann = { package = \"announce\" }\n\
             [dependencies]\nann.workspace = true\n",
        );
        assert_eq!(dependency_name(&m).as_deref(), Some("ann"));
    }

    #[test]
    fn inherited_dependency_without_a_workspace_root_is_ignored() {
        let t = TempDir::new("no-ws");
        let m = t.manifest("m", "[dependencies]\nann.workspace = true\n");
        assert_eq!(dependency_name(&m), None);
        // A parent manifest without `[workspace]` is not a root.
        t.manifest(
            "",
            "[package]\nname = \"p\"\n[workspace.dependencies]\nann = { package = \"announce\" }\n",
        );
        assert_eq!(dependency_name(&m), None);
    }

    #[test]
    fn the_manifest_answer_is_kept_for_a_while_per_key() {
        use std::time::Duration;
        let key = |d: &str| (Some(d.to_string()), Some("p".to_string()));
        let long = Duration::from_secs(3600);
        assert_eq!(cached(key("a"), long, || "one".into()), "one");
        // Within the time the lookup does not run again.
        assert_eq!(cached(key("a"), long, || panic!("looked up again")), "one");
        // Another manifest, or package, is another question.
        assert_eq!(cached(key("b"), long, || "two".into()), "two");
        assert_eq!(cached(key("a"), long, || "three".into()), "three");
        let other_package = (Some("a".to_string()), None);
        assert_eq!(cached(other_package, long, || "four".into()), "four");
        // After the time it runs again.
        assert_eq!(cached(key("a"), Duration::ZERO, || "five".into()), "five");
        assert_eq!(cached(key("a"), Duration::ZERO, || "six".into()), "six");
    }

    #[test]
    fn default_crate_path_falls_back_to_announce() {
        // Under `cargo test -p announce-macros` neither this package's manifest
        // nor its name is `announce`'s dependency.
        let p = default_crate_path();
        assert!(p.leading_colon.is_some());
        assert_eq!(p.segments.len(), 1);
        assert_eq!(p.segments[0].ident, "announce");
    }

    #[test]
    fn dotted_key_package_rename() {
        // ALGORITHM_FINDINGS #10.
        let m = "[dependencies]\nann.package = \"announce\"\nann.version = \"0.1\"\n";
        assert_eq!(name_of(m).as_deref(), Some("ann"));
        // In any order, between other dependencies, with spaces around the dot.
        let m = "[dependencies]\nserde = \"1\"\nann.version = \"0.1\"\nann . package = \"announce\"\nother.package = \"x\"\n";
        assert_eq!(name_of(m).as_deref(), Some("ann"));
        // The other tables, and target tables.
        assert_eq!(
            name_of("[dev-dependencies]\nd.package = \"announce\"\n").as_deref(),
            Some("d")
        );
        assert_eq!(
            name_of("[build-dependencies]\nb.path = \"..\"\nb.package = \"announce\"\n").as_deref(),
            Some("b")
        );
        assert_eq!(
            name_of("[target.'cfg(unix)'.dependencies]\nt.package = \"announce\"\n").as_deref(),
            Some("t")
        );
        // Single quotes, hyphens.
        assert_eq!(
            name_of("[dependencies]\nmy-ann.package = 'announce'\n").as_deref(),
            Some("my_ann")
        );
        // Another package, or a field that is not `package`.
        assert_eq!(name_of("[dependencies]\nann.package = \"other\"\n"), None);
        assert_eq!(
            name_of("[dependencies]\nann.version = \"0.1\"\nann.features = [\"announce\"]\n"),
            None
        );
        // A dotted key of one table does not reach a dependency of the next.
        let m = "[dependencies]\nann.version = \"0.1\"\n[dev-dependencies]\nann.package = \"x\"\n";
        assert_eq!(name_of(m), None);
        // Dotted keys of the workspace table, inherited by a member.
        let t = TempDir::new("ws-dotted");
        t.manifest(
            "",
            "[workspace]\n[workspace.dependencies]\nws.path = \"a\"\nws.package = \"announce\"\n",
        );
        let m = t.manifest(
            "m",
            "[dependencies]\nws.workspace = true\nws.optional = true\n",
        );
        assert_eq!(dependency_name(&m).as_deref(), Some("ws"));
    }

    #[test]
    fn quoted_keys() {
        assert_eq!(
            name_of("[dependencies]\n\"ann\" = { package = \"announce\" }\n").as_deref(),
            Some("ann")
        );
        assert_eq!(
            name_of("[dependencies]\n\"ann\".package = \"announce\"\n").as_deref(),
            Some("ann")
        );
        assert_eq!(
            name_of("[dependencies]\n'ann'.package = \"announce\"\n").as_deref(),
            Some("ann")
        );
        assert_eq!(
            name_of("[dependencies]\nann = { \"package\" = \"announce\" }\n").as_deref(),
            Some("ann")
        );
        assert_eq!(
            name_of("[dependencies]\nann = { 'package' = 'announce' }\n").as_deref(),
            Some("ann")
        );
        assert_eq!(
            name_of("[dependencies.ann]\n\"package\" = \"announce\"\n").as_deref(),
            Some("ann")
        );
        assert_eq!(
            name_of("[dependencies.'ann']\npackage = 'announce'\n").as_deref(),
            Some("ann")
        );
        // A quoted key that is not `package`.
        assert_eq!(
            name_of("[dependencies]\nann = { \"package-x\" = \"announce\" }\n"),
            None
        );
        // `=` inside a quoted key is part of the key.
        assert_eq!(
            name_of("[dependencies]\n\"a=b\" = { package = \"announce\" }\n").as_deref(),
            Some("a=b")
        );
    }

    #[test]
    fn inline_table_over_several_lines() {
        // ALGORITHM_FINDINGS #10.
        let m = "[dependencies]\nann = { version = \"0.1\",\n        package = \"announce\" }\n";
        assert_eq!(name_of(m).as_deref(), Some("ann"));
        let m = "[dependencies]\nann = {\n    version = \"0.1\",\n    package = \"announce\",\n}\nserde = \"1\"\n";
        assert_eq!(name_of(m).as_deref(), Some("ann"));
        // Comments in between, and a `}` or `#` in a string.
        let m = "[dependencies]\nann = { # the rename\n    path = \"a}#b\", # path\n    package = \"announce\" # name\n}\n";
        assert_eq!(name_of(m).as_deref(), Some("ann"));
        // A multi-line array inside the table, with nested arrays and brackets in strings.
        let m = "[dependencies]\nann = { version = \"0.1\", features = [\n  \"a]\",\n  [\"b\"],\n], package = \"announce\" }\n";
        assert_eq!(name_of(m).as_deref(), Some("ann"));
        // `package` before an array that holds commas and `=`.
        let m = "[dependencies]\nann = { package = \"announce\", features = [\"a,b\", \"package = x\"] }\n";
        assert_eq!(name_of(m).as_deref(), Some("ann"));
        // A dependency after a multi-line one is read as well, and a table that
        // follows a multi-line array is still a table.
        let m = "[dependencies]\nother = { version = \"1\",\n  features = [\"x\"] }\nann = { package = \"announce\" }\n";
        assert_eq!(name_of(m).as_deref(), Some("ann"));
        let m = "[dependencies.other]\nfeatures = [\n  \"x\",\n]\n[dependencies.ann]\npackage = \"announce\"\n";
        assert_eq!(name_of(m).as_deref(), Some("ann"));
        // Not announce, over several lines.
        let m = "[dependencies]\nann = { version = \"0.1\",\n  package = \"other\" }\n";
        assert_eq!(name_of(m), None);
        // A workspace entry over several lines, and `workspace = true` too.
        let t = TempDir::new("ws-multiline");
        t.manifest(
            "",
            "[workspace]\n[workspace.dependencies]\nws = { path = \"a\",\n  package = \"announce\" }\n",
        );
        let m = t.manifest(
            "m",
            "[dependencies]\nws = { version = \"1\",\n  workspace = true }\n",
        );
        assert_eq!(dependency_name(&m).as_deref(), Some("ws"));
    }

    #[test]
    fn multi_line_strings_and_line_endings_do_not_swallow_the_dependencies() {
        let m = "[package]\ndescription = \"\"\"\nuse { and [ and # here\n\"\"\"\n\
                 readme = '''\n{\n'''\n[dependencies]\nann = { package = \"announce\" }\n";
        assert_eq!(name_of(m).as_deref(), Some("ann"));
        // CRLF line endings.
        let m = "[dependencies]\r\nann = { version = \"0.1\",\r\n  package = \"announce\" }\r\n";
        assert_eq!(name_of(m).as_deref(), Some("ann"));
        // The `[workspace]` header is found with a comment after it and in the
        // middle of the file.
        let t = TempDir::new("ws-header");
        t.manifest(
            "",
            "[package]\nname = \"p\"\n[workspace] # root\n[workspace.dependencies]\nann = { package = \"announce\" }\n",
        );
        let m = t.manifest("m", "[dependencies]\nann.workspace = true\n");
        assert_eq!(dependency_name(&m).as_deref(), Some("ann"));
    }

    #[test]
    fn scanner_helpers() {
        assert_eq!(
            logical_lines("a = 1 # c\n# only\n\nb = 2"),
            ["a = 1", "b = 2"]
        );
        assert_eq!(logical_lines("a = \"x#y\" # c"), ["a = \"x#y\""]);
        assert_eq!(logical_lines("a = 'x#y' # c"), ["a = 'x#y'"]);
        assert_eq!(logical_lines("a = \"x\\\"#y\" # c"), ["a = \"x\\\"#y\""]);
        assert_eq!(logical_lines("[a.b]\nc = 1"), ["[a.b]", "c = 1"]);
        // Values over several lines are joined, multi-line strings are dropped.
        assert_eq!(
            logical_lines("a = { b = 1,\n c = [2,\n 3] }\nd = 4"),
            ["a = { b = 1,  c = [2,  3] }", "d = 4"]
        );
        assert_eq!(
            logical_lines("a = \"\"\"\n{ \\\"\"\"\n\"\"\"\nb = 1"),
            ["a = \"\"", "b = 1"]
        );
        assert_eq!(
            logical_lines("a = '''\n{\n'''\nb = 1"),
            ["a = \"\"", "b = 1"]
        );
        assert_eq!(
            split_top("a, [b, c], \"d,e\", {f, g}", ','),
            ["a", " [b, c]", " \"d,e\"", " {f, g}"]
        );
        assert_eq!(split_kv("a = b = c"), Some(("a", "b = c")));
        assert_eq!(split_kv("\"a=b\" = c"), Some(("\"a=b\"", "c")));
        assert_eq!(split_kv("novalue"), None);
        assert_eq!(split_key("ann.package"), ["ann", "package"]);
        assert_eq!(split_key("\"a.b\" . 'c'"), ["a.b", "c"]);
        assert_eq!(split_key("plain"), ["plain"]);
        assert_eq!(unquote(" \"ann\" "), "ann");
        assert_eq!(unquote("'cfg(unix)'"), "cfg(unix)");
        assert_eq!(
            inline_field("{ version = \"1\", package = \"announce\" }", "package"),
            Some("\"announce\"")
        );
        assert_eq!(
            inline_field("{ workspace = true }", "workspace"),
            Some("true")
        );
        assert_eq!(inline_field("{ version = \"1\" }", "package"), None);
        assert!(is_dep_table("dependencies", false));
        assert!(is_dep_table("dev-dependencies", false));
        assert!(is_dep_table("build-dependencies", false));
        assert!(is_dep_table("target.'cfg(unix)'.dependencies", false));
        assert!(is_dep_table("workspace.dependencies", true));
        assert!(!is_dep_table("features", false));
        assert!(!is_dep_table("workspace.dependencies", false));
        assert!(!is_dep_table("dependencies", true));
        assert!(!is_dep_table("target.'cfg(unix)'.lints", false));
        assert_eq!(
            dep_table_key("dependencies.ann", false).as_deref(),
            Some("ann")
        );
        assert_eq!(
            dep_table_key("dev-dependencies.\"a-b\"", false).as_deref(),
            Some("a-b")
        );
        assert_eq!(
            dep_table_key("target.'cfg(unix)'.build-dependencies.x", false).as_deref(),
            Some("x")
        );
        assert_eq!(
            dep_table_key("workspace.dependencies.ann", true).as_deref(),
            Some("ann")
        );
        assert_eq!(dep_table_key("dependencies", false), None);
        assert_eq!(dep_table_key("features", false), None);
        assert_eq!(dep_table_key("dependencies.ann", true), None);
    }
}
