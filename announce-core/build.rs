use std::process::Command;

// `#[diagnostic::on_unimplemented]` needs rustc 1.78 and `#[diagnostic::do_not_recommend]`
// rustc 1.85: the check of `LeakFailure::assume_success` uses them. The proc macro that writes
// it is built by the compiler that compiles the call, so these cfgs say what that one knows.
fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    let rustc = std::env::var("RUSTC").unwrap_or_else(|_| "rustc".into());
    let version = Command::new(rustc).arg("--version").output().ok();
    // `rustc 1.88.0 (..)`; a compiler that does not tell is assumed to be a current one.
    let minor = version
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .and_then(|v| v.split('.').nth(1)?.parse::<u32>().ok())
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
