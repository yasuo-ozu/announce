use std::process::Command;

// `#[diagnostic::on_unimplemented]` needs rustc 1.78.
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
        // Unknown compiler version: the texts are cosmetic, and an attribute that an old
        // compiler does not know is an error, so leave them out.
        .unwrap_or(0);
    if minor >= 80 {
        println!("cargo:rustc-check-cfg=cfg(announce_diagnostic)");
    }
    if minor >= 78 {
        println!("cargo:rustc-cfg=announce_diagnostic");
    }
}
