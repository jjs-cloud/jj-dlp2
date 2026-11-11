//! Optional timing log: set JJ_PERF=1 to write slow-operation lines to perf.log in the config folder.

use crate::{persist, settings};
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

const REPORT_EVERY: Duration = Duration::from_secs(5);

/// True when the JJ_PERF environment variable is set to something other than 0.
pub fn enabled() -> bool {
    static ON: OnceLock<bool> = OnceLock::new();
    *ON.get_or_init(|| std::env::var_os("JJ_PERF").is_some_and(|v| !v.is_empty() && v != "0"))
}

/// Appends one timestamped line to perf.log.
pub fn note(text: &str) {
    if enabled() {
        let line = format!("{}  {text}\n", crate::engine::logfile::stamp());
        persist::append_line(settings::config_dir().join("perf.log"), line, u64::MAX, |_| {});
    }
}

/// Logs its label and duration when dropped, if it lived at least `limit`.
pub struct Timer {
    label: &'static str,
    start: Option<Instant>,
    limit: Duration,
}

/// Starts a timer that reports operations slower than `limit_ms`.
pub fn timer(label: &'static str, limit_ms: u64) -> Timer {
    Timer { label, start: enabled().then(Instant::now), limit: Duration::from_millis(limit_ms) }
}

impl Drop for Timer {
    fn drop(&mut self) {
        if let Some(start) = self.start {
            let took = start.elapsed();
            if took >= self.limit {
                note(&format!("slow {}: {} ms", self.label, took.as_millis()));
            }
        }
    }
}

struct Counts {
    since: Instant,
    map: HashMap<&'static str, u64>,
}

static COUNTS: Mutex<Option<Counts>> = Mutex::new(None);

/// Counts one occurrence of `label` and logs the totals every few seconds.
pub fn count(label: &'static str) {
    if !enabled() {
        return;
    }
    let mut guard = COUNTS.lock().unwrap();
    let counts = guard.get_or_insert_with(|| Counts { since: Instant::now(), map: HashMap::new() });
    *counts.map.entry(label).or_default() += 1;
    if counts.since.elapsed() >= REPORT_EVERY {
        let mut items: Vec<String> = counts.map.drain().map(|(k, v)| format!("{k}={v}")).collect();
        items.sort();
        let secs = counts.since.elapsed().as_secs();
        counts.since = Instant::now();
        drop(guard);
        note(&format!("counts over {secs}s: {}", items.join(" ")));
    }
}
