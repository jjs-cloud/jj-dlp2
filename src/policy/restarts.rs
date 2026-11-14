//! Per-streamer restart counts: a recorder that starts soon after an unexpected exit is a restart.

use crate::api::StreamerId;
use std::collections::HashMap;
use std::time::{Duration, Instant};

/// What a status update changed about a streamer's recording flag.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Change {
    None,
    Started,
    /// The recording ended; `unexpected` is false when it was stopped on purpose or the streamer is disabled.
    Ended { unexpected: bool },
}

#[derive(Default)]
struct Entry {
    recording: bool,
    user_stop: bool,
    exited_at: Option<Instant>,
    /// Live span of the latest counted restart.
    span: Option<u64>,
    count: u32,
    /// False once the chip is dismissed or timed out; the count is kept.
    shown: bool,
    last_restart: Option<Instant>,
}

#[derive(Default)]
pub struct RestartTracker {
    entries: HashMap<StreamerId, Entry>,
    dirty: bool,
}

impl RestartTracker {
    /// Feeds a streamer's recording flag and reports the change; `window` is how soon after an exit a new recorder counts,
    /// and a restart in a new live `span` starts the count over.
    pub fn on_status(&mut self, id: StreamerId, recording: bool, enabled: bool, window: Duration, now: Instant, span: Option<u64>) -> Change {
        let e = self.entries.entry(id).or_default();
        if recording == e.recording {
            return Change::None;
        }
        e.recording = recording;
        if recording {
            let recent = e.exited_at.take().is_some_and(|at| now.duration_since(at) < window);
            if recent {
                if e.span != span {
                    e.span = span;
                    e.count = 0;
                }
                e.count += 1;
                e.shown = true;
                e.last_restart = Some(now);
                self.dirty = true;
            }
            Change::Started
        } else {
            let unexpected = enabled && !e.user_stop;
            e.user_stop = false;
            e.exited_at = unexpected.then_some(now);
            Change::Ended { unexpected }
        }
    }

    /// Marks the streamer's running recorder as stopped on purpose, so its exit is not counted.
    pub fn stopping(&mut self, id: StreamerId) {
        if let Some(e) = self.entries.get_mut(&id).filter(|e| e.recording) {
            e.user_stop = true;
        }
    }

    /// Hides every count, keeping the counts themselves.
    pub fn clear(&mut self) {
        for e in self.entries.values_mut().filter(|e| e.shown) {
            e.shown = false;
            e.last_restart = None;
            self.dirty = true;
        }
    }

    /// Hides the streamer's restart count, keeping the count itself.
    pub fn clear_one(&mut self, id: StreamerId) {
        if let Some(e) = self.entries.get_mut(&id).filter(|e| e.shown) {
            e.shown = false;
            e.last_restart = None;
            self.dirty = true;
        }
    }

    /// Hides counts whose latest restart is older than the streamer's auto-clear minutes, keeping them; 0 never clears.
    pub fn expire(&mut self, now: Instant, clear_minutes: impl Fn(StreamerId) -> u64) {
        for (id, e) in self.entries.iter_mut().filter(|(_, e)| e.shown) {
            let minutes = clear_minutes(*id);
            let due = minutes > 0 && e.last_restart.is_some_and(|at| now.duration_since(at) >= Duration::from_secs(minutes * 60));
            if due {
                e.shown = false;
                e.last_restart = None;
                self.dirty = true;
            }
        }
    }

    /// Keeps only the streamers the predicate accepts.
    pub fn retain(&mut self, mut keep: impl FnMut(StreamerId) -> bool) {
        let shown = self.entries.values().filter(|e| e.shown).count();
        self.entries.retain(|id, _| keep(*id));
        if self.entries.values().filter(|e| e.shown).count() != shown {
            self.dirty = true;
        }
    }

    /// Streamer id to its restart count, leaving out streamers whose count is hidden.
    pub fn counts(&self) -> HashMap<StreamerId, u32> {
        self.entries.iter().filter(|(_, e)| e.shown && e.count > 0).map(|(id, e)| (*id, e.count)).collect()
    }

    /// The streamer's showing restart count; 0 while it is hidden.
    pub fn count(&self, id: StreamerId) -> u32 {
        self.entries.get(&id).filter(|e| e.shown).map_or(0, |e| e.count)
    }

    pub fn take_dirty(&mut self) -> bool {
        std::mem::take(&mut self.dirty)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const WINDOW: Duration = Duration::from_secs(1800);

    #[test]
    fn clear_one_resets_only_that_streamer() {
        let (mut t, t0) = (RestartTracker::default(), Instant::now());
        for id in [1, 2] {
            t.on_status(id, true, true, WINDOW, t0, None);
            t.on_status(id, false, true, WINDOW, secs(t0, 5), None);
            t.on_status(id, true, true, WINDOW, secs(t0, 65), None);
        }
        t.take_dirty();
        t.clear_one(1);
        assert_eq!((t.count(1), t.count(2)), (0, 1));
        assert!(t.take_dirty());
    }

    fn secs(t0: Instant, n: u64) -> Instant {
        t0 + Duration::from_secs(n)
    }

    #[test]
    fn a_respawn_within_the_window_counts() {
        let (mut t, t0) = (RestartTracker::default(), Instant::now());
        t.on_status(1, true, true, WINDOW, t0, None);
        t.on_status(1, false, true, WINDOW, secs(t0, 5), None);
        t.on_status(1, true, true, WINDOW, secs(t0, 65), None);
        assert_eq!(t.counts().get(&1), Some(&1));
        assert!(t.take_dirty());
    }

    #[test]
    fn a_respawn_after_the_window_does_not_count() {
        let (mut t, t0) = (RestartTracker::default(), Instant::now());
        t.on_status(1, true, true, WINDOW, t0, None);
        t.on_status(1, false, true, WINDOW, secs(t0, 5), None);
        t.on_status(1, true, true, WINDOW, secs(t0, 5 + 1860), None);
        assert!(t.counts().is_empty());
    }

    #[test]
    fn exits_without_a_respawn_do_not_count() {
        let (mut t, t0) = (RestartTracker::default(), Instant::now());
        t.on_status(1, true, true, WINDOW, t0, None);
        t.on_status(1, false, true, WINDOW, secs(t0, 5), None);
        assert!(t.counts().is_empty());
    }

    #[test]
    fn user_stops_and_disabled_streamers_do_not_count() {
        let (mut t, t0) = (RestartTracker::default(), Instant::now());
        t.on_status(1, true, true, WINDOW, t0, None);
        t.stopping(1);
        t.on_status(1, false, true, WINDOW, secs(t0, 5), None);
        t.on_status(1, true, true, WINDOW, secs(t0, 10), None);
        t.on_status(1, false, false, WINDOW, secs(t0, 15), None);
        t.on_status(1, true, true, WINDOW, secs(t0, 20), None);
        assert!(t.counts().is_empty());
    }

    #[test]
    fn hidden_counts_continue_in_the_same_span_and_start_over_in_a_new_one() {
        let (mut t, t0) = (RestartTracker::default(), Instant::now());
        let restart = |t: &mut RestartTracker, at: u64, span: Option<u64>| {
            t.on_status(1, false, true, WINDOW, secs(t0, at), span);
            t.on_status(1, true, true, WINDOW, secs(t0, at + 5), span);
        };
        t.on_status(1, true, true, WINDOW, t0, Some(100));
        restart(&mut t, 10, Some(100));
        restart(&mut t, 30, Some(100));
        assert_eq!(t.counts().get(&1), Some(&2));
        t.clear_one(1);
        assert!(t.counts().is_empty() && t.count(1) == 0);
        restart(&mut t, 50, Some(100));
        assert_eq!(t.counts().get(&1), Some(&3));
        t.clear();
        restart(&mut t, 70, Some(200));
        assert_eq!(t.counts().get(&1), Some(&1));
    }

    #[test]
    fn auto_clear_hides_the_count_without_resetting_it() {
        let (mut t, t0) = (RestartTracker::default(), Instant::now());
        t.on_status(1, true, true, WINDOW, t0, Some(100));
        t.on_status(1, false, true, WINDOW, secs(t0, 5), Some(100));
        t.on_status(1, true, true, WINDOW, secs(t0, 10), Some(100));
        t.expire(secs(t0, 10 + 600), |_| 10);
        assert!(t.counts().is_empty());
        t.on_status(1, false, true, WINDOW, secs(t0, 700), Some(100));
        t.on_status(1, true, true, WINDOW, secs(t0, 710), Some(100));
        assert_eq!(t.counts().get(&1), Some(&2));
    }

    #[test]
    fn clear_and_auto_clear_hide_counts() {
        let (mut t, t0) = (RestartTracker::default(), Instant::now());
        for i in 0..3 {
            t.on_status(1, true, true, WINDOW, secs(t0, i * 10), None);
            t.on_status(1, false, true, WINDOW, secs(t0, i * 10 + 5), None);
        }
        t.on_status(1, true, true, WINDOW, secs(t0, 40), None);
        assert_eq!(t.counts().get(&1), Some(&3));
        t.expire(secs(t0, 100), |_| 0);
        assert_eq!(t.counts().get(&1), Some(&3));
        t.expire(secs(t0, 40 + 600), |_| 10);
        assert!(t.counts().is_empty());
        t.on_status(1, false, true, WINDOW, secs(t0, 700), None);
        t.on_status(1, true, true, WINDOW, secs(t0, 710), None);
        t.clear();
        assert!(t.counts().is_empty());
    }
}
