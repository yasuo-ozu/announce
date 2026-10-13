//! Result tables: artifact sizes (measured once per crate) and the criterion estimates.

use crate::harness::{Harness, Sizes};
use crate::sources::Point;
use std::fmt::Write;
use std::fs;
use std::path::PathBuf;

pub const G_PLAIN: &str = "definer_plain";
pub const G_ANNOUNCE: &str = "definer_announce";
pub const G_CHAIN: &str = "user_chain";
pub const G_CALLS: &str = "user_calls";
pub const G_BASELINE: &str = "baseline";
pub const BASELINE_START: &str = "empty_lib_start";
pub const BASELINE_END: &str = "empty_lib_end";

/// Median of the measured samples and its 95% confidence interval, in ms.
#[derive(Clone, Copy)]
struct Est {
    median: f64,
    lo: f64,
    hi: f64,
}

fn read_est(h: &Harness, rel: &[&str]) -> Option<Est> {
    let mut path: PathBuf = h.criterion_dir.clone();
    path.extend(rel);
    let file = path.join("new/estimates.json");
    if fs::metadata(&file).ok()?.modified().ok()? < h.started {
        return None; // from an earlier run
    }
    let v: serde_json::Value = serde_json::from_str(&fs::read_to_string(file).ok()?).ok()?;
    let ms = |v: &serde_json::Value| Some(v.as_f64()? / 1e6);
    let m = &v["median"];
    Some(Est {
        median: ms(&m["point_estimate"])?,
        lo: ms(&m["confidence_interval"]["lower_bound"])?,
        hi: ms(&m["confidence_interval"]["upper_bound"])?,
    })
}

fn est(h: &Harness, group: &str, p: Point) -> Option<Est> {
    read_est(h, &[group, p.kind.name(), &p.param()])
}

fn kib(b: Option<u64>) -> String {
    match b {
        None => "n/a".into(),
        Some(b) => {
            let x = b as f64 / 1024.0;
            if x >= 100.0 {
                format!("{x:.0}")
            } else {
                format!("{x:.1}")
            }
        }
    }
}

/// `median ±half-width of the confidence interval`
fn ms(e: Option<Est>) -> String {
    match e {
        None => "n/a".into(),
        Some(e) => {
            let half = (e.hi - e.lo) / 2.0;
            if e.median >= 100.0 {
                format!("{:.0} ±{:.0}", e.median, half)
            } else {
                format!("{:.1} ±{:.1}", e.median, half)
            }
        }
    }
}

fn table(headers: &[&str], rows: &[Vec<String>]) -> String {
    let mut out = format!("| {} |\n|", headers.join(" | "));
    for (i, _) in headers.iter().enumerate() {
        out.push_str(if i == 0 { "---|" } else { "---:|" });
    }
    out.push('\n');
    for r in rows {
        writeln!(out, "| {} |", r.join(" | ")).unwrap();
    }
    out
}

pub fn write(h: &Harness) {
    let sizes = h.sizes_snapshot();
    let size_of = |name: String| sizes.get(&name).copied();
    let have = |s: &Option<Sizes>| s.is_some();
    let points: Vec<Point> = h
        .grid
        .points
        .iter()
        .copied()
        .filter(|p| {
            have(&size_of(p.plain()))
                || have(&size_of(p.definer()))
                || have(&size_of(p.chain()))
                || have(&size_of(p.calls()))
                || est(h, G_PLAIN, *p).is_some()
                || est(h, G_ANNOUNCE, *p).is_some()
                || est(h, G_CHAIN, *p).is_some()
                || est(h, G_CALLS, *p).is_some()
        })
        .collect();
    if points.is_empty() {
        return;
    }
    let lib = |s: Option<Sizes>| (kib(s.map(|s| s.lib.rlib)), kib(s.map(|s| s.lib.rmeta)));
    let exe = |s: Option<Sizes>| (kib(s.and_then(|s| s.exe)), kib(s.and_then(|s| s.exe_strip)));
    let id = |p: &Point| vec![p.kind.name().to_string(), p.s.to_string(), p.k.to_string()];

    // ---- sizes only
    let mut definer_sizes = Vec::new();
    let mut caller_sizes = Vec::new();
    // ---- sizes and times
    let mut definer_all = Vec::new();
    let mut caller_all = Vec::new();
    let mut csv = String::from(
        "kind,S,K,plain_rlib,plain_rmeta,announce_rlib,announce_rmeta,chain_rlib,chain_rmeta,chain_exe,chain_exe_stripped,\
         calls_rlib,calls_rmeta,calls_exe,calls_exe_stripped\n",
    );
    let num = |v: Option<u64>| v.map_or(String::new(), |v| v.to_string());
    for p in &points {
        let (plain, def, chain, calls) = (
            size_of(p.plain()),
            size_of(p.definer()),
            size_of(p.chain()),
            size_of(p.calls()),
        );
        let (pr, pm) = lib(plain);
        let (dr, dm) = lib(def);
        let (cr, cm) = lib(chain);
        let (kr, km) = lib(calls);
        let (ce, ces) = exe(chain);
        let (ke, kes) = exe(calls);
        let mut r = id(p);
        r.extend([pr.clone(), pm.clone(), dr.clone(), dm.clone()]);
        definer_sizes.push(r.clone());
        r.extend([ms(est(h, G_PLAIN, *p)), ms(est(h, G_ANNOUNCE, *p))]);
        definer_all.push(r);
        let mut r = id(p);
        r.extend([
            cr.clone(),
            cm.clone(),
            ce.clone(),
            ces.clone(),
            kr.clone(),
            km.clone(),
            ke.clone(),
            kes.clone(),
        ]);
        caller_sizes.push(r);
        let mut r = id(p);
        r.extend([
            cr,
            cm,
            ce,
            ces,
            ms(est(h, G_CHAIN, *p)),
            kr,
            km,
            ke,
            kes,
            ms(est(h, G_CALLS, *p)),
        ]);
        caller_all.push(r);
        let b = |s: Option<Sizes>| {
            (
                num(s.map(|s| s.lib.rlib)),
                num(s.map(|s| s.lib.rmeta)),
                num(s.and_then(|s| s.exe)),
                num(s.and_then(|s| s.exe_strip)),
            )
        };
        let (c, k) = (b(chain), b(calls));
        writeln!(
            csv,
            "{},{},{},{},{},{},{},{},{},{},{},{},{},{},{}",
            p.kind.name(),
            p.s,
            p.k,
            b(plain).0,
            b(plain).1,
            b(def).0,
            b(def).1,
            c.0,
            c.1,
            c.2,
            c.3,
            k.0,
            k.1,
            k.2,
            k.3
        )
        .unwrap();
    }

    let id_h = ["kind", "S", "K"];
    let cat = |a: &[&str], b: &[&str]| -> Vec<String> {
        a.iter().chain(b).map(|s| s.to_string()).collect()
    };
    let dh_sizes = cat(
        &id_h,
        &[
            "plain rlib KiB",
            "plain rmeta KiB",
            "announce rlib KiB",
            "announce rmeta KiB",
        ],
    );
    let dh_all = cat(
        &id_h,
        &[
            "plain rlib KiB",
            "plain rmeta KiB",
            "announce rlib KiB",
            "announce rmeta KiB",
            "plain rebuild ms",
            "announce rebuild ms",
        ],
    );
    let ch_sizes = cat(
        &id_h,
        &[
            "chain rlib KiB",
            "chain rmeta KiB",
            "chain exe KiB",
            "chain exe stripped KiB",
            "K calls rlib KiB",
            "K calls rmeta KiB",
            "K calls exe KiB",
            "K calls exe stripped KiB",
        ],
    );
    let ch_all = cat(
        &id_h,
        &[
            "chain rlib KiB",
            "chain rmeta KiB",
            "chain exe KiB",
            "chain exe stripped KiB",
            "chain rebuild ms",
            "K calls rlib KiB",
            "K calls rmeta KiB",
            "K calls exe KiB",
            "K calls exe stripped KiB",
            "K calls rebuild ms",
        ],
    );
    let t = |hs: &[String], rows: &[Vec<String>]| {
        table(&hs.iter().map(String::as_str).collect::<Vec<_>>(), rows)
    };

    // ---- header
    let m = &h.meta;
    let base = size_of("baseline".into());
    let (br, bm) = lib(base);
    let (be, bes) = exe(base);
    let mut head = format!(
        "Date {}; {}; {}, nproc={}; announce at {}; dev profile{}; sizes in KiB (1024 bytes), one measurement per crate.\n\
         Empty crate: rlib {br} KiB, rmeta {bm} KiB, trivial executable {be} KiB (stripped {bes} KiB).\n",
        m.date,
        m.rustc,
        m.machine,
        m.nproc,
        m.revision,
        if m.opt_build {
            " with [profile.dev.build-override] opt-level = 3"
        } else {
            ""
        }
    );
    let bad = h.nondeterministic.lock().unwrap();
    let verified = h.verified.lock().unwrap().len();
    if verified == 0 {
        head.push_str("Size determinism: not checked (no timed rebuilds in this run).\n");
    } else if bad.is_empty() {
        write!(head, "Size determinism: rlib and rmeta sizes of {verified} crates were identical after all timed rebuilds.\n").unwrap();
    } else {
        write!(
            head,
            "Size determinism: DIFFERENCES after rebuilds: {}\n",
            bad.join("; ")
        )
        .unwrap();
    }
    drop(bad);
    let base_start = read_est(h, &[G_BASELINE, BASELINE_START]);
    let base_end = read_est(h, &[G_BASELINE, BASELINE_END]);
    let timing_note = if h.grid.timing {
        format!(
            "Rebuild times: criterion median +- half-width of its 95% confidence interval, {} samples per benchmark; empty-crate rebuild {} ms before and {} ms after the grid.\n",
            h.grid.samples,
            ms(base_start),
            ms(base_end)
        )
    } else {
        String::new()
    };

    let sizes_md = format!(
        "{head}\n### Definer side (sizes)\n\n{}\n### Calling side (sizes)\n\n{}",
        t(&dh_sizes, &definer_sizes),
        t(&ch_sizes, &caller_sizes)
    );
    let report_md = format!(
        "{head}{timing_note}\n### Definer side\n\n{}\n### Calling side\n\n{}",
        t(&dh_all, &definer_all),
        t(&ch_all, &caller_all)
    );
    let dir = &h.criterion_dir;
    fs::create_dir_all(dir).unwrap();
    fs::write(dir.join("announce-sizes.csv"), csv).unwrap();
    fs::write(dir.join("announce-sizes.md"), &sizes_md).unwrap();
    fs::write(dir.join("announce-report.md"), &report_md).unwrap();
    println!("\n{report_md}");
    println!(
        "announce-bench: wrote {}/announce-{{sizes.csv,sizes.md,report.md}}",
        dir.display()
    );
}
