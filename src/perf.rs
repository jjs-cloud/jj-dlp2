//! Optional timing log: start with --perf to write slow-operation lines to perf.log in the config folder.

use crate::{persist, settings};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

const REPORT_EVERY: Duration = Duration::from_secs(5);

static ENABLED: AtomicBool = AtomicBool::new(false);

/// Turns timing on; called when the app is started with --perf.
pub fn enable() {
    ENABLED.store(true, Ordering::Relaxed);
}

/// True when timing was turned on with --perf.
pub fn enabled() -> bool {
    ENABLED.load(Ordering::Relaxed)
}

/// Appends one timestamped line to perf.log.
pub fn note(text: &str) {
    if enabled() {
        let line = format!("{}  {text}\n", crate::engine::logfile::stamp());
        persist::append_line(settings::config_dir().join("perf.log"), line, u64::MAX, persist::Over::Empty);
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

static BEAT: Mutex<Option<Instant>> = Mutex::new(None);

/// Called once per GUI tick; logs when the gap since the previous tick shows the UI thread was stalled.
pub fn heartbeat() {
    if !enabled() {
        return;
    }
    let now = Instant::now();
    let prev = BEAT.lock().unwrap().replace(now);
    if let Some(prev) = prev {
        let gap = now.duration_since(prev);
        if gap >= Duration::from_millis(1500) {
            note(&format!("gui stalled: {} ms between 1 s ticks", gap.as_millis()));
        }
    }
}
