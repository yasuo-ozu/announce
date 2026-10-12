// Version-gated mocks: traits that are newer than the MSRV (1.57), or that changed
// after it. `announce_std_rust_1_N` is set from rustc 1.N on; the sources gate on it
// with a plain `#[cfg(..)]`. The compiler version is decided by the `rustversion`
// attributes below, not by parsing `rustc --version`.
macro_rules! gates {
    ($($v:tt),* $(,)?) => {
        fn main() {
            println!("cargo:rerun-if-changed=build.rs");
            $({
                #[rustversion::since($v)]
                fn on(cfg: &str) {
                    println!("cargo:rustc-cfg={}", cfg);
                }
                #[rustversion::before($v)]
                fn on(_: &str) {}
                // `rustc-check-cfg` is known from cargo 1.80 on; older cargo warns about it.
                #[rustversion::since(1.80)]
                fn check(cfg: &str) {
                    println!("cargo:rustc-check-cfg=cfg({})", cfg);
                }
                #[rustversion::before(1.80)]
                fn check(_: &str) {}

                // `1.61` -> `announce_std_rust_1_61`
                let cfg = format!("announce_std_rust_{}", stringify!($v).replace('.', "_"));
                check(&cfg);
                on(&cfg);
            })*
        }
    };
}

// The only place that names the versions. `tools/gen_std.py` reads this list and
// refuses to emit a cfg version that is not in it (`gen_std.py --check`).
gates![1.61, 1.64, 1.80, 1.81, 1.94];
