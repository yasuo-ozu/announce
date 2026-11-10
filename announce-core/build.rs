use std::process::Command;

// `#[diagnostic::on_unimplemented]` needs rustc 1.78 and `#[diagnostic::do_not_recommend]`
// rustc 1.85: the check that `LeakFailure::assume_success` writes uses them. The proc macro
// that writes the check is built by the compiler that compiles the call, so these cfgs say
// what that compiler knows.
fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    let rustc = std::env::var("RUSTC").unwrap_or_else(|_| "rustc".into());
    let minor = Command::new(rustc)
        .arg("--version")
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .and_then(|v| {
            // `rustc 1.88.0 (..)`, `rustc 1.89.0-nightly (..)`.
            let mut it = v.split_whitespace().nth(1)?.split('.');
            let major: u32 = it.next()?.parse().ok()?;
            let minor: u32 = it.next()?.parse().ok()?;
            Some(if major > 1 { u32::MAX } else { minor })
        })
        // Unknown compiler version: assume a current one.
        .unwrap_or(u32::MAX);
    if minor >= 80 {
        println!("cargo:rustc-check-cfg=cfg(announce_diagnostic)");
        println!("cargo:rustc-check-cfg=cfg(announce_do_not_recommend)");
    }
    if minor >= 78 {
        println!("cargo:rustc-cfg=announce_diagnostic");
    }
    if minor >= 85 {
        println!("cargo:rustc-cfg=announce_do_not_recommend");
    }
}
