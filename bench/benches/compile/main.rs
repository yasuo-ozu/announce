//! Criterion benchmark for announce: rebuild time and artifact sizes.
//!
//! `cd bench && cargo bench --offline`; the grid comes from `ANNOUNCE_BENCH_*`
//! environment variables, see bench/README.md.

mod harness;
mod report;
mod sources;

use criterion::{BenchmarkGroup, BenchmarkId, Criterion, SamplingMode};
use harness::Harness;
use sources::Point;
use std::time::Duration;

/// What is rebuilt by one criterion group: the crate `pkg_of(point)` after
/// touching its `lib.rs`. `with_bin` crates also get a tiny executable, built
/// (untimed) for the size table.
struct Metric {
    group: &'static str,
    pkg_of: fn(&Point) -> String,
    with_bin: bool,
}

const METRICS: [Metric; 4] = [
    Metric { group: report::G_PLAIN, pkg_of: |p| p.plain(), with_bin: false },
    Metric { group: report::G_ANNOUNCE, pkg_of: |p| p.definer(), with_bin: false },
    Metric { group: report::G_CHAIN, pkg_of: |p| p.chain(), with_bin: true },
    Metric { group: report::G_CALLS, pkg_of: |p| p.calls(), with_bin: true },
];

type Group<'a> = BenchmarkGroup<'a, criterion::measurement::WallTime>;

/// Ten samples of one rebuild each: flat sampling with the shortest warm-up and
/// target time, so criterion runs one warm-up rebuild and then exactly one
/// rebuild per sample.
fn configure(g: &mut Group) {
    g.sample_size(harness::grid().samples)
        .sampling_mode(SamplingMode::Flat)
        .warm_up_time(Duration::from_millis(1))
        .measurement_time(Duration::from_millis(1));
}

/// The empty crate: cargo's and rustc's fixed cost, to subtract from the other
/// rebuild times. Measured before and after the grid to show drifts of the machine's load.
fn baseline(c: &mut Criterion, id: &str) {
    let mut g = c.benchmark_group(report::G_BASELINE);
    configure(&mut g);
    g.bench_function(id, |b| {
        let h = Harness::get();
        h.ensure("baseline", Some(0));
        b.iter_custom(|iters| (0..iters).map(|_| h.rebuild("baseline")).sum::<Duration>());
    });
    g.finish();
}

fn main() {
    let mut c = Criterion::default()
        .plotting_backend(criterion::PlottingBackend::Plotters)
        .configure_from_args();
    let grid = harness::grid();
    if grid.timing {
        baseline(&mut c, report::BASELINE_START);
        // Point by point, so that the four crates of a point are measured under similar load.
        for p in &grid.points {
            for m in &METRICS {
                let mut g = c.benchmark_group(m.group);
                configure(&mut g);
                g.bench_with_input(BenchmarkId::new(p.kind.name(), p.param()), p, |b, p| {
                    // criterion calls this closure for the warm-up and for every sample:
                    // the build and the size measurement happen once, outside the timed part.
                    let h = Harness::get();
                    let pkg = (m.pkg_of)(p);
                    h.ensure(&pkg, m.with_bin.then_some(p.k));
                    b.iter_custom(|iters| (0..iters).map(|_| h.rebuild(&pkg)).sum::<Duration>());
                    h.verify(&pkg);
                });
                g.finish();
            }
        }
        baseline(&mut c, report::BASELINE_END);
        c.final_summary();
    } else {
        // Artifact sizes only: build every crate once, no timing.
        let h = Harness::get();
        h.ensure("baseline", Some(0));
        for p in &grid.points {
            for m in &METRICS {
                h.ensure(&(m.pkg_of)(p), m.with_bin.then_some(p.k));
            }
        }
    }
    if let Some(h) = Harness::try_get() {
        report::write(h);
        h.cleanup();
    }
}
