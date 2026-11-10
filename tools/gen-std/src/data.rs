//! The data file: one record per mock, line-oriented. The format is described in the
//! header of `announce-std/build.rs`, which reads it with std only.

pub struct Item {
    pub since: u32,
    pub provided: bool,
    pub text: String,
}

pub struct Record {
    pub module: String,
    pub name: String,
    pub path: String,
    pub since: u32,
    pub until: Option<u32>,
    pub std: bool,
    pub derive: bool,
    pub unsafe_: bool,
    pub head: String,
    pub notes: Vec<String>,
    pub fallback: Option<(String, String)>,
    pub items: Vec<Item>,
}

fn yes(b: bool) -> &'static str {
    if b {
        "yes"
    } else {
        "no"
    }
}

/// The text of the data file.
pub fn write(records: &[Record], rustc_version: &str, glob_since: u32) -> String {
    let mut s = String::new();
    s.push_str("# The std traits that announce-std mocks, with their signatures.\n");
    s.push_str(&format!(
        "# Read from the rust-src of {} by tools/gen-std.\n",
        rustc_version
    ));
    s.push_str("# Do not edit by hand: run\n");
    s.push_str(
        "#   cargo run --manifest-path tools/gen-std/Cargo.toml --target-dir target/gen-std\n",
    );
    s.push_str(
        "# announce-std/build.rs keeps the records and items that the compiling rustc has,\n",
    );
    s.push_str("# and writes the mocks. The format is described in announce-std/build.rs.\n\n");
    s.push_str(&format!("glob-since 1.{}\n", glob_since));
    for r in records {
        s.push_str(&format!("\ntrait {} {}\n", r.module, r.name));
        s.push_str(&format!("path {}\n", r.path));
        s.push_str(&format!("since 1.{}\n", r.since));
        if let Some(u) = r.until {
            s.push_str(&format!("until 1.{}\n", u));
        }
        s.push_str(&format!("std {}\n", yes(r.std)));
        s.push_str(&format!("derive {}\n", yes(r.derive)));
        s.push_str(&format!("unsafe {}\n", yes(r.unsafe_)));
        s.push_str(&format!("head {}\n", r.head).replace("head \n", "head\n"));
        for n in &r.notes {
            s.push_str(&format!("note {}\n", n).replace("note \n", "note\n"));
        }
        if let Some((t, i)) = &r.fallback {
            s.push_str(&format!("fallback-trait {}\n", t));
            s.push_str(&format!("fallback-impl {}\n", i));
        }
        for it in &r.items {
            s.push_str(&format!(
                "item 1.{} {} {}\n",
                it.since,
                if it.provided { "provided" } else { "required" },
                it.text
            ));
        }
    }
    s
}
