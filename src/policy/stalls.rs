//! Per-streamer STALLED chip state, and the per-span limit on restarting stalled recordings.

use crate::api::StreamerId;
use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

/// Most stalled recordings restarted for one streamer during one live span.
pub const MAX_RESTARTS_PER_SPAN: u32 = 20;

/// A streamer's STALLED chip; the count outlives the chip being hidden.
struct Chip {
    span: Option<u64>,
    count: u32,
    /// False once the chip is dismissed or timed out.
    shown: bool,
    /// Set once no recording of the streamer is stalled any more.
    quiet_since: Option<Instant>,
}

/// STALLED chips that stay until dismissed or timed out, with their stall counts per live span.
#[derive(Default)]
pub struct StallChips {
    chips: HashMap<StreamerId, Chip>,
    dirty: bool,
}

impl StallChips {
    /// Counts a new stall and shows the chip, starting the count over in a new live span; returns the count.
    pub fn on_stall(&mut self, id: StreamerId, span: Option<u64>) -> u32 {
        let chip = self.chips.entry(id).or_insert(Chip { span, count: 0, shown: false, quiet_since: None });
        if chip.span != span {
            chip.span = span;
            chip.count = 0;
        }
        chip.count += 1;
        chip.shown = true;
        chip.quiet_since = None;
        self.dirty = true;
        chip.count
    }

    /// Runs each chip's quiet timer only while none of the streamer's recordings is stalled.
    pub fn sync(&mut self, stalled: &HashSet<StreamerId>, now: Instant) {
        for (id, chip) in self.chips.iter_mut() {
            if stalled.contains(id) {
                chip.quiet_since = None;
            } else if chip.quiet_since.is_none() {
                chip.quiet_since = Some(now);
            }
        }
    }

    /// Hides chips that have been quiet for the streamer's auto-clear minutes, keeping their counts; 0 never clears.
    pub fn expire(&mut self, now: Instant, clear_minutes: impl Fn(StreamerId) -> u64) {
        for (id, chip) in self.chips.iter_mut().filter(|(_, c)| c.shown) {
            let minutes = clear_minutes(*id);
            let due = minutes > 0 && chip.quiet_since.is_some_and(|at| now.duration_since(at) >= Duration::from_secs(minutes * 60));
            if due {
                chip.shown = false;
                self.dirty = true;
            }
        }
    }

    /// Hides every chip, keeping the counts.
    pub fn clear(&mut self) {
        for chip in self.chips.values_mut().filter(|c| c.shown) {
            chip.shown = false;
            self.dirty = true;
        }
    }

    /// Hides the streamer's chip, keeping its count.
    pub fn clear_one(&mut self, id: StreamerId) {
        if let Some(chip) = self.chips.get_mut(&id).filter(|c| c.shown) {
            chip.shown = false;
            self.dirty = true;
        }
    }

    /// Keeps only the streamers the predicate accepts.
    pub fn retain(&mut self, mut keep: impl FnMut(StreamerId) -> bool) {
        let shown = self.chips.values().filter(|c| c.shown).count();
        self.chips.retain(|id, _| keep(*id));
        if self.chips.values().filter(|c| c.shown).count() != shown {
            self.dirty = true;
        }
    }

    /// Streamer id to its stall count; streamers without a showing chip are absent.
    pub fn counts(&self) -> HashMap<StreamerId, u32> {
        self.chips.iter().filter(|(_, c)| c.shown).map(|(id, c)| (*id, c.count)).collect()
    }

    pub fn take_dirty(&mut self) -> bool {
        std::mem::take(&mut self.dirty)
    }
}

/// Whether a stalled recording may be restarted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Restart {
    /// Restart number `n` of this live span.
    Go(u32),
    /// The span's limit is reached; `first` is true only the first time.
    Capped { first: bool },
}

#[derive(Default)]
struct SpanCount {
    span: Option<u64>,
    sent: u32,
    cap_logged: bool,
}

/// Stall restarts sent per streamer in its current live span.
#[derive(Default)]
pub struct StallRestarts {
    counts: HashMap<StreamerId, SpanCount>,
}

impl StallRestarts {
    /// Takes one restart from the streamer's live span, or reports that the limit is reached.
    pub fn fire(&mut self, id: StreamerId, span: Option<u64>) -> Restart {
        let c = self.counts.entry(id).or_default();
        if c.span != span {
            *c = SpanCount { span, ..Default::default() };
        }
        if c.sent >= MAX_RESTARTS_PER_SPAN {
            let first = !c.cap_logged;
            c.cap_logged = true;
            return Restart::Capped { first };
        }
        c.sent += 1;
        Restart::Go(c.sent)
    }

    /// Keeps only the streamers the predicate accepts.
    pub fn retain(&mut self, mut keep: impl FnMut(StreamerId) -> bool) {
        self.counts.retain(|id, _| keep(*id));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(t0: Instant, mins: u64) -> Instant {
        t0 + Duration::from_secs(mins * 60)
    }

    #[test]
    fn stalls_count_up_within_a_span_and_start_over_in_a_new_one() {
        let mut c = StallChips::default();
        assert_eq!(c.on_stall(1, Some(100)), 1);
        assert_eq!(c.on_stall(1, Some(100)), 2);
        assert_eq!(c.on_stall(2, Some(100)), 1);
        assert_eq!(c.on_stall(1, Some(200)), 1);
        assert_eq!(c.counts().get(&1), Some(&1));
        assert!(c.take_dirty() && !c.take_dirty());
    }

    #[test]
    fn the_timer_runs_only_while_nothing_is_stalled_and_restarts_on_a_new_stall() {
        let (mut c, t0) = (StallChips::default(), Instant::now());
        c.on_stall(1, None);
        c.sync(&HashSet::from([1]), t0);
        c.expire(at(t0, 100), |_| 10);
        assert_eq!(c.counts().len(), 1, "an ongoing stall never expires");
        c.sync(&HashSet::new(), at(t0, 100));
        c.expire(at(t0, 109), |_| 10);
        assert_eq!(c.counts().len(), 1);
        c.on_stall(1, None);
        c.sync(&HashSet::new(), at(t0, 109));
        c.expire(at(t0, 118), |_| 10);
        assert_eq!(c.counts().len(), 1, "the new stall restarted the timer");
        c.expire(at(t0, 120), |_| 10);
        assert!(c.counts().is_empty());
    }

    #[test]
    fn zero_minutes_never_expires() {
        let (mut c, t0) = (StallChips::default(), Instant::now());
        c.on_stall(1, None);
        c.sync(&HashSet::new(), t0);
        c.expire(at(t0, 100_000), |_| 0);
        assert_eq!(c.counts().len(), 1);
    }

    #[test]
    fn dismissing_hides_the_chip_but_keeps_the_count_within_a_span() {
        let mut c = StallChips::default();
        c.on_stall(1, Some(5));
        c.on_stall(1, Some(5));
        c.on_stall(2, Some(5));
        c.take_dirty();
        c.clear_one(1);
        assert!(c.take_dirty() && c.counts().len() == 1);
        assert_eq!(c.on_stall(1, Some(5)), 3);
        assert_eq!(c.counts().get(&1), Some(&3));
        c.take_dirty();
        c.clear();
        assert!(c.take_dirty() && c.counts().is_empty());
        c.clear();
        assert!(!c.take_dirty());
    }

    #[test]
    fn a_hidden_count_starts_over_only_with_the_next_stall_in_a_new_span() {
        let mut c = StallChips::default();
        c.on_stall(1, Some(5));
        c.on_stall(1, Some(5));
        c.clear_one(1);
        assert!(c.counts().is_empty());
        assert_eq!(c.on_stall(1, Some(6)), 1);
    }

    #[test]
    fn timing_out_hides_the_chip_but_keeps_the_count() {
        let (mut c, t0) = (StallChips::default(), Instant::now());
        c.on_stall(1, Some(5));
        c.on_stall(1, Some(5));
        c.sync(&HashSet::new(), t0);
        c.expire(at(t0, 11), |_| 10);
        assert!(c.counts().is_empty());
        assert_eq!(c.on_stall(1, Some(5)), 3);
    }

    #[test]
    fn retain_drops_unknown_streamers() {
        let mut c = StallChips::default();
        c.on_stall(1, None);
        c.on_stall(2, None);
        c.take_dirty();
        c.retain(|id| id == 2);
        assert!(c.take_dirty());
        assert_eq!(c.counts().keys().copied().collect::<Vec<_>>(), [2]);
    }

    #[test]
    fn restarts_are_capped_per_span_and_the_cap_is_reported_once() {
        let mut r = StallRestarts::default();
        for n in 1..=MAX_RESTARTS_PER_SPAN {
            assert_eq!(r.fire(1, Some(100)), Restart::Go(n));
        }
        assert_eq!(r.fire(1, Some(100)), Restart::Capped { first: true });
        assert_eq!(r.fire(1, Some(100)), Restart::Capped { first: false });
        assert_eq!(r.fire(2, Some(100)), Restart::Go(1));
    }

    #[test]
    fn a_new_span_restarts_the_restart_count() {
        let mut r = StallRestarts::default();
        for _ in 0..MAX_RESTARTS_PER_SPAN {
            r.fire(1, Some(100));
        }
        assert_eq!(r.fire(1, Some(200)), Restart::Go(1));
    }
}
