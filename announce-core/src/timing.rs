//! A developer aid: the time each step of the proc macros takes.
//!
//! With the environment variable `ANNOUNCE_TIMING` set (to anything but `0` or the
//! empty string) in the environment of the compiler, every expansion of
//! `#[announce]`, of its hidden `Capture` derive, of `finish!` and of
//! `#[derive(Announce)]` prints one line to stderr:
//!
//! ```text
//! announce-timing: capture trait Big: read=0.021 parse=2.310 strip=1.873 marker=0.402 analysis(fast)=0.912 payload=2.011 quote=0.455 | total=8.013 ms
//! ```
//!
//! The steps are durations in milliseconds, in the order they ran; `total` is the
//! time from the start of the expansion to its end. The variable is read once per
//! process. Without it a [`Timer`] is a `None`: creating it is one relaxed atomic
//! load, and each step is a check of that `None`.

use std::sync::atomic::{AtomicU8, Ordering};
use std::time::{Duration, Instant};

const UNKNOWN: u8 = 0;
const OFF: u8 = 1;
const ON: u8 = 2;

static STATE: AtomicU8 = AtomicU8::new(UNKNOWN);

/// Whether `ANNOUNCE_TIMING` is set. The environment is read on the first call.
pub(crate) fn enabled() -> bool {
    match STATE.load(Ordering::Relaxed) {
        OFF => false,
        ON => true,
        _ => init(),
    }
}

#[cold]
fn init() -> bool {
    let on = std::env::var_os("ANNOUNCE_TIMING").map_or(false, |v| !v.is_empty() && v != "0");
    STATE.store(if on { ON } else { OFF }, Ordering::Relaxed);
    on
}

struct Active {
    phase: &'static str,
    subject: String,
    start: Instant,
    last: Instant,
    steps: Vec<(String, Duration)>,
}

/// Measures the steps of one expansion and prints them when dropped.
pub(crate) struct Timer(Option<Box<Active>>);

impl Timer {
    /// A timer that records nothing.
    #[cfg(test)]
    pub(crate) fn off() -> Timer {
        Timer(None)
    }

    /// Starts the clock of `phase` (`attribute`, `capture`, `finish`, `derive`).
    pub(crate) fn new(phase: &'static str) -> Timer {
        if !enabled() {
            return Timer(None);
        }
        let now = Instant::now();
        Timer(Some(Box::new(Active {
            phase,
            subject: String::new(),
            start: now,
            last: now,
            steps: Vec::new(),
        })))
    }

    /// Names what is expanded (`trait Big`). The closure runs only when timing.
    pub(crate) fn subject(&mut self, name: impl FnOnce() -> String) {
        if let Some(a) = &mut self.0 {
            a.subject = name();
        }
    }

    /// Ends a step: records the time since the previous step (or the start).
    pub(crate) fn step(&mut self, name: &'static str) {
        if let Some(a) = &mut self.0 {
            let now = Instant::now();
            a.steps.push((name.to_string(), now - a.last));
            a.last = Instant::now();
        }
    }

    /// Like [`Timer::step`], with a label that tells which way the step went
    /// (`analysis(fast)`).
    pub(crate) fn step_labelled(&mut self, name: &'static str, label: &'static str) {
        if let Some(a) = &mut self.0 {
            let now = Instant::now();
            a.steps.push((format!("{}({})", name, label), now - a.last));
            a.last = Instant::now();
        }
    }
}

/// The line a timer prints.
fn line(a: &Active, total: Duration) -> String {
    let ms = |d: Duration| d.as_secs_f64() * 1e3;
    let mut line = format!("announce-timing: {} {}:", a.phase, a.subject);
    for (name, d) in &a.steps {
        line.push_str(&format!(" {}={:.3}", name, ms(*d)));
    }
    line.push_str(&format!(" | total={:.3} ms", ms(total)));
    line
}

impl Drop for Timer {
    fn drop(&mut self) {
        if let Some(a) = self.0.take() {
            eprintln!("{}", line(&a, a.start.elapsed()));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_timer_that_is_off_records_nothing() {
        let mut t = Timer::off();
        t.subject(|| panic!("the name is made only when timing"));
        t.step("a");
        t.step_labelled("b", "fast");
        assert!(t.0.is_none());
    }

    #[test]
    fn a_line_lists_the_steps_in_order_with_milliseconds() {
        let now = Instant::now();
        let a = Active {
            phase: "capture",
            subject: "trait Big".to_string(),
            start: now,
            last: now,
            steps: vec![
                ("parse".to_string(), Duration::from_micros(2500)),
                ("analysis(fast)".to_string(), Duration::from_micros(40)),
            ],
        };
        assert_eq!(
            line(&a, Duration::from_millis(3)),
            "announce-timing: capture trait Big: parse=2.500 analysis(fast)=0.040 | total=3.000 ms"
        );
    }

    #[test]
    fn an_active_timer_records_labelled_steps() {
        // Built by hand: the environment is read once per process.
        let now = Instant::now();
        let mut t = Timer(Some(Box::new(Active {
            phase: "derive",
            subject: String::new(),
            start: now,
            last: now,
            steps: Vec::new(),
        })));
        t.subject(|| "struct S".to_string());
        t.step("parse");
        t.step_labelled("analysis", "full");
        let a = t.0.take().unwrap();
        assert_eq!(a.subject, "struct S");
        let names: Vec<&str> = a.steps.iter().map(|(n, _)| n.as_str()).collect();
        assert_eq!(names, ["parse", "analysis(full)"]);
    }
}
