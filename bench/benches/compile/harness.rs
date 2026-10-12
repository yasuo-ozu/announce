//! The grid, the generated cargo workspace, the cargo driver and the artifact
//! size measurements.

use crate::sources::{self, bin_name, cargo_toml, Kind, Point};
use std::collections::{BTreeMap, HashSet};
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant, SystemTime};

pub const DEFAULT_SIZES: &str = "1,10,100,200";
pub const DEFAULT_COUNTS: &str = "1,2,4,8,16,32,48,64,96";
pub const DEFAULT_KINDS: &str = "trait";

fn env_list(name: &str, default: &str) -> Vec<String> {
    std::env::var(name)
        .ok()
        .filter(|v| !v.trim().is_empty())
        .unwrap_or_else(|| default.to_string())
        .split(',')
        .map(|x| x.trim().to_string())
        .filter(|x| !x.is_empty())
        .collect()
}

fn env_flag(name: &str) -> bool {
    std::env::var(name).map_or(false, |v| !v.is_empty() && v != "0")
}

/// The grid, from the `ANNOUNCE_BENCH_*` environment variables (criterion owns the CLI).
pub struct Grid {
    pub points: Vec<Point>,
    pub samples: usize,
    pub timing: bool,
}

impl Grid {
    pub fn from_env() -> Grid {
        let quick = env_flag("ANNOUNCE_BENCH_QUICK");
        let (dsizes, dcounts) = if quick {
            ("1,10", "1,4")
        } else {
            (DEFAULT_SIZES, DEFAULT_COUNTS)
        };
        let nums = |name: &str, default: &str| -> Vec<usize> {
            env_list(name, default)
                .iter()
                .map(|x| {
                    x.parse()
                        .unwrap_or_else(|_| panic!("{name}: not a number: {x:?}"))
                })
                .collect()
        };
        let kinds: Vec<Kind> = env_list("ANNOUNCE_BENCH_KINDS", DEFAULT_KINDS)
            .iter()
            .map(|x| match x.as_str() {
                "struct" => Kind::Struct,
                "trait" => Kind::Trait,
                _ => panic!("ANNOUNCE_BENCH_KINDS: expected struct or trait, got {x:?}"),
            })
            .collect();
        let sizes = nums("ANNOUNCE_BENCH_SIZES", dsizes);
        let counts = nums("ANNOUNCE_BENCH_COUNTS", dcounts);
        let mut points = Vec::new();
        for &kind in &kinds {
            for &s in &sizes {
                for &k in &counts {
                    points.push(Point { kind, s, k });
                }
            }
        }
        let samples = nums("ANNOUNCE_BENCH_SAMPLES", "10")[0].max(10);
        Grid {
            points,
            samples,
            timing: !env_flag("ANNOUNCE_BENCH_NO_TIMING"),
        }
    }
}

static GRID: OnceLock<Grid> = OnceLock::new();

/// The grid, parsed once from the environment.
pub fn grid() -> &'static Grid {
    GRID.get_or_init(Grid::from_env)
}

/// Sizes in bytes of the library artifacts of one crate.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LibSizes {
    pub rlib: u64,
    pub rmeta: u64,
}

/// Sizes of one crate; `exe` fields only for crates with a binary.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Sizes {
    pub lib: LibSizes,
    pub exe: Option<u64>,
    pub exe_strip: Option<u64>,
}

pub struct Meta {
    pub revision: String,
    pub date: String,
    pub rustc: String,
    pub machine: String,
    pub nproc: usize,
}

pub struct Harness {
    pub grid: &'static Grid,
    pub criterion_dir: PathBuf,
    pub meta: Meta,
    root: PathBuf,
    ws: PathBuf,
    tdir: PathBuf,
    cargo: OsString,
    strip: bool,
    sizes: Mutex<BTreeMap<String, Sizes>>,
    pub nondeterministic: Mutex<Vec<String>>,
    pub verified: Mutex<HashSet<String>>,
    /// Estimates written before this moment belong to an earlier run.
    pub started: SystemTime,
}

fn output_of(cmd: &mut Command) -> Option<String> {
    let o = cmd.output().ok()?;
    o.status
        .success()
        .then(|| String::from_utf8_lossy(&o.stdout).trim().to_string())
}

fn write_if_changed(path: &Path, text: &str) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    if fs::read_to_string(path).map_or(true, |old| old != text) {
        fs::write(path, text).unwrap();
    }
}

/// Where cargo puts `target/` for this package (criterion's rule).
fn cargo_target_directory(manifest: &Path, cargo: &OsString) -> PathBuf {
    if let Some(d) = std::env::var_os("CARGO_TARGET_DIR") {
        return PathBuf::from(d);
    }
    output_of(
        Command::new(cargo)
            .args([
                "metadata",
                "--format-version",
                "1",
                "--no-deps",
                "--offline",
            ])
            .current_dir(manifest),
    )
    .and_then(|json| {
        let v: serde_json::Value = serde_json::from_str(&json).ok()?;
        Some(PathBuf::from(v["target_directory"].as_str()?))
    })
    .unwrap_or_else(|| manifest.join("target"))
}

/// The commit of the benchmarked checkout (`ANNOUNCE_BENCH_REV` overrides, for a copy without `.git`).
fn revision(repo: &Path) -> String {
    if let Ok(r) = std::env::var("ANNOUNCE_BENCH_REV") {
        return r;
    }
    let git = |args: &[&str]| output_of(Command::new("git").arg("-C").arg(repo).args(args));
    match git(&["rev-parse", "--short", "HEAD"]) {
        Some(rev) => {
            let dirty = git(&[
                "status",
                "--porcelain",
                "--",
                "announce",
                "announce-core",
                "announce-macros",
                "announce-helper",
                "Cargo.toml",
                "Cargo.lock",
            ])
            .is_some_and(|s| !s.is_empty());
            format!("{rev}{}", if dirty { " + uncommitted changes" } else { "" })
        }
        None => "unknown".into(),
    }
}

static HARNESS: OnceLock<Harness> = OnceLock::new();

impl Harness {
    /// Generates the workspace and builds the shared crates on first use.
    pub fn get() -> &'static Harness {
        HARNESS.get_or_init(Harness::new)
    }

    pub fn try_get() -> Option<&'static Harness> {
        HARNESS.get()
    }

    fn new() -> Harness {
        let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let repo = std::env::var_os("ANNOUNCE_BENCH_REPO")
            .map(PathBuf::from)
            .unwrap_or_else(|| manifest.parent().unwrap().to_path_buf());
        let cargo = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
        let target = cargo_target_directory(&manifest, &cargo);
        let root = target.join("announce-bench");
        let rustc = std::env::var_os("RUSTC").unwrap_or_else(|| "rustc".into());
        let rustc_v = output_of(Command::new(&rustc).arg("-vV")).unwrap_or_default();
        let h = Harness {
            grid: grid(),
            criterion_dir: target.join("criterion"),
            meta: Meta {
                revision: revision(&repo),
                date: output_of(Command::new("date").arg("+%F")).unwrap_or_default(),
                rustc: rustc_v
                    .lines()
                    .find(|l| l.starts_with("rustc "))
                    .unwrap_or("rustc ?")
                    .to_string(),
                machine: output_of(Command::new("uname").arg("-m"))
                    .unwrap_or_else(|| std::env::consts::ARCH.into()),
                nproc: std::thread::available_parallelism().map_or(0, |n| n.get()),
            },
            ws: root.join("ws"),
            tdir: root.join("target"),
            root,
            cargo,
            strip: Command::new("strip")
                .arg("--version")
                .output()
                .is_ok_and(|o| o.status.success()),
            sizes: Mutex::new(BTreeMap::new()),
            nondeterministic: Mutex::new(Vec::new()),
            verified: Mutex::new(HashSet::new()),
            started: SystemTime::now(),
        };
        h.generate(&repo);
        eprintln!("announce-bench: building bench_cb and the empty crate");
        h.build(&["-p", "bench_cb", "-p", "baseline"]);
        h
    }

    /// Writes the generated workspace (regenerated from scratch each run;
    /// the build directory is kept so that shared dependencies stay built).
    fn generate(&self, repo: &Path) {
        let _ = fs::remove_dir_all(&self.ws);
        let announce = repo.join("announce");
        let helper = repo.join("announce-helper");
        let mut members = vec!["bench_cb".to_string(), "baseline".to_string()];
        let ws = &self.ws;
        let put =
            |name: &str, rel: &str, text: &str| write_if_changed(&ws.join(name).join(rel), text);

        put(
            "bench_cb",
            "Cargo.toml",
            &cargo_toml(
                "bench_cb",
                &format!(
                    "announce-helper = {{ path = \"{}\" }}\nsyn = \"2\"\nproc-macro2 = \"1\"\n",
                    helper.display()
                ),
                "\n[lib]\nproc-macro = true\n",
            ),
        );
        put("bench_cb", "src/lib.rs", sources::CALLBACK_LIB);
        put("baseline", "Cargo.toml", &cargo_toml("baseline", "", ""));
        put("baseline", "src/lib.rs", "pub const ENTRIES: usize = 0;\n");
        put(
            "baseline",
            &format!("src/bin/{}.rs", bin_name("baseline")),
            "fn main() { println!(\"{}\", baseline::ENTRIES); }\n",
        );

        for &p in &self.grid.points {
            let def = p.definer();
            for (name, announced) in [(def.clone(), true), (p.plain(), false)] {
                let deps = if announced {
                    format!("announce = {{ path = \"{}\" }}\n", announce.display())
                } else {
                    String::new()
                };
                put(&name, "Cargo.toml", &cargo_toml(&name, &deps, ""));
                put(&name, "src/lib.rs", &sources::definer(p, announced));
                members.push(name);
            }
            for (name, separate) in [(p.chain(), false), (p.calls(), true)] {
                let deps = format!(
                    "{def} = {{ path = \"../{def}\" }}\nbench_cb = {{ path = \"../bench_cb\" }}\n"
                );
                put(&name, "Cargo.toml", &cargo_toml(&name, &deps, ""));
                put(&name, "src/lib.rs", &sources::user(&def, p.k, separate));
                put(
                    &name,
                    &format!("src/bin/{}.rs", bin_name(&name)),
                    &sources::user_bin(p.kind, &name, &def),
                );
                members.push(name);
            }
        }
        let list: String = members.iter().map(|m| format!("  \"{m}\",\n")).collect();
        write_if_changed(
            &ws.join("Cargo.toml"),
            &format!("[workspace]\nresolver = \"2\"\nmembers = [\n{list}]\n"),
        );
        let lock = ws.join("Cargo.lock");
        if !lock.exists() && repo.join("Cargo.lock").exists() {
            // Pin to versions known to be in the offline cache.
            fs::copy(repo.join("Cargo.lock"), lock).unwrap();
        }
    }

    fn cargo_cmd(&self) -> Command {
        let mut cmd = Command::new(&self.cargo);
        cmd.arg("--offline")
            .current_dir(&self.ws)
            .env("CARGO_TARGET_DIR", &self.tdir)
            .env("CARGO_INCREMENTAL", "0")
            .env_remove("RUSTFLAGS")
            .env_remove("CARGO_ENCODED_RUSTFLAGS")
            .env_remove("CARGO_BUILD_RUSTFLAGS");
        cmd
    }

    /// `cargo build <args>` in the generated workspace (dev profile); returns the wall time.
    fn build(&self, args: &[&str]) -> Duration {
        let mut cmd = self.cargo_cmd();
        cmd.arg("build")
            .args(args)
            .stdout(Stdio::null())
            .stderr(Stdio::piped());
        let start = Instant::now();
        let out = cmd.output().expect("cannot run cargo");
        let elapsed = start.elapsed();
        if !out.status.success() {
            let err = String::from_utf8_lossy(&out.stderr);
            let tail: String = err
                .chars()
                .rev()
                .take(6000)
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
                .collect();
            panic!("cargo build {args:?} failed:\n{tail}");
        }
        elapsed
    }

    /// Newest `lib<pkg>-<hash>.<ext>` among cargo's intermediate directories
    /// (`debug/deps` or, with the newer layout, `debug/build/<pkg>/<hash>/out`).
    /// Package names are unique per grid point and `-` ends the name, so only
    /// this point's file matches.
    fn newest(&self, pkg: &str, ext: &str) -> PathBuf {
        let prefix = format!("lib{pkg}-");
        let suffix = format!(".{ext}");
        let mut dirs = vec![self.tdir.join("debug").join("deps")];
        if let Ok(rd) = fs::read_dir(self.tdir.join("debug").join("build").join(pkg)) {
            dirs.extend(rd.flatten().map(|e| e.path().join("out")));
        }
        let mut best: Option<(SystemTime, PathBuf)> = None;
        for dir in dirs {
            for e in fs::read_dir(dir).into_iter().flatten().flatten() {
                let name = e.file_name().to_string_lossy().into_owned();
                if name.starts_with(&prefix) && name.ends_with(&suffix) {
                    let t = e
                        .metadata()
                        .and_then(|m| m.modified())
                        .unwrap_or(SystemTime::UNIX_EPOCH);
                    if best.as_ref().map_or(true, |(bt, _)| t >= *bt) {
                        best = Some((t, e.path()));
                    }
                }
            }
        }
        best.unwrap_or_else(|| panic!("no .{ext} found for {pkg} under {}", self.tdir.display()))
            .1
    }

    fn lib_sizes(&self, pkg: &str) -> LibSizes {
        let len = |ext| fs::metadata(self.newest(pkg, ext)).unwrap().len();
        LibSizes {
            rlib: len("rlib"),
            rmeta: len("rmeta"),
        }
    }

    fn stripped_size(&self, exe: &Path) -> Option<u64> {
        if !self.strip {
            return None;
        }
        let tmp = self.root.join("stripped.tmp");
        let ok = Command::new("strip")
            .arg("-o")
            .arg(&tmp)
            .arg(exe)
            .status()
            .is_ok_and(|s| s.success());
        let size = ok
            .then(|| fs::metadata(&tmp).map(|m| m.len()).ok())
            .flatten();
        let _ = fs::remove_file(&tmp);
        size
    }

    /// Builds `pkg` (library, and its binary when `entries` is `Some`) and
    /// records its artifact sizes once (cached). The binary must print `entries`
    /// as its first word. Not timed; called from benchmark setup.
    pub fn ensure(&self, pkg: &str, entries: Option<usize>) -> Sizes {
        let mut map = self.sizes.lock().unwrap();
        if let Some(s) = map.get(pkg) {
            return *s;
        }
        self.build(&["-p", pkg]);
        let lib = self.lib_sizes(pkg);
        let (mut exe, mut exe_strip) = (None, None);
        if let Some(k) = entries {
            let path = self.tdir.join("debug").join(bin_name(pkg));
            let out = Command::new(&path)
                .output()
                .expect("cannot run the generated executable");
            let text = String::from_utf8_lossy(&out.stdout).into_owned();
            assert_eq!(
                text.split_whitespace().next(),
                Some(k.to_string().as_str()),
                "{pkg} printed {text:?}"
            );
            exe = Some(fs::metadata(&path).unwrap().len());
            exe_strip = self.stripped_size(&path);
        }
        let s = Sizes {
            lib,
            exe,
            exe_strip,
        };
        map.insert(pkg.to_string(), s);
        s
    }

    /// Touches `pkg`'s `lib.rs` and times `cargo build -p pkg --lib` (its dependencies are built).
    pub fn rebuild(&self, pkg: &str) -> Duration {
        let lib = self.ws.join(pkg).join("src").join("lib.rs");
        fs::File::options()
            .write(true)
            .open(&lib)
            .unwrap()
            .set_modified(SystemTime::now())
            .unwrap();
        self.build(&["-p", pkg, "--lib"])
    }

    /// Checks that rebuilding left the library sizes unchanged (they are deterministic).
    pub fn verify(&self, pkg: &str) {
        let before = self.sizes.lock().unwrap().get(pkg).map(|s| s.lib);
        let after = self.lib_sizes(pkg);
        self.verified.lock().unwrap().insert(pkg.to_string());
        if let Some(before) = before {
            if before != after {
                let msg = format!("{pkg}: {before:?} -> {after:?}");
                let mut bad = self.nondeterministic.lock().unwrap();
                if !bad.contains(&msg) {
                    eprintln!("announce-bench: WARNING: sizes changed between builds: {msg}");
                    bad.push(msg);
                }
            }
        }
    }

    pub fn sizes_snapshot(&self) -> BTreeMap<String, Sizes> {
        self.sizes.lock().unwrap().clone()
    }

    /// Deletes the generated workspace and the build directory (unless `ANNOUNCE_BENCH_KEEP=1`).
    pub fn cleanup(&self) {
        if !env_flag("ANNOUNCE_BENCH_KEEP") {
            let _ = fs::remove_dir_all(&self.root);
        }
    }
}
