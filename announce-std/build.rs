use std::process::Command;

// Version-gated mocks (traits that changed after the 1.71 MSRV).
const VERSIONS: &[u32] = &[80, 81, 94];

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    let rustc = std::env::var("RUSTC").unwrap_or_else(|_| "rustc".into());
    let minor = Command::new(rustc)
        .arg("--version")
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .and_then(|v| {
            // `rustc 1.83.0 (..)`, `rustc 1.84.0-nightly (..)`.
            let mut it = v.split_whitespace().nth(1)?.split('.');
            let major: u32 = it.next()?.parse().ok()?;
            let minor: u32 = it.next()?.parse().ok()?;
            Some(if major > 1 { u32::MAX } else { minor })
        })
        // Unknown compiler version: assume a current one (AUDIT #17).
        .unwrap_or(u32::MAX);
    for v in VERSIONS {
        if minor >= 80 {
            println!("cargo:rustc-check-cfg=cfg(announce_std_rust_1_{})", v);
        }
        if minor >= *v {
            println!("cargo:rustc-cfg=announce_std_rust_1_{}", v);
        }
    }
}
