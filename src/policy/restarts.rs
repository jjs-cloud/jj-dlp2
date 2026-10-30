//! Per-streamer restart counts: a recorder that starts soon after an unexpected exit is a restart.

use crate::api::StreamerId;
use std::collections::HashMap;
use std::time::{Duration, Instant};

#[derive(Default)]
struct Entry {
    recording: bool,
    user_stop: bool,
    exited_at: Option<Instant>,
    count: u32,
    last_restart: Option<Instant>,
}

#[derive(Default)]
pub struct RestartTracker {
    entries: HashMap<StreamerId, Entry>,
    dirty: bool,
}

impl RestartTracker {
    /// Feeds a streamer's recording flag; `window` is how soon after an exit a new recorder counts.
    pub fn on_status(&mut self, id: StreamerId, recording: bool, enabled: bool, window: Duration, now: Instant) {
        let e = self.entries.entry(id).or_default();
        if recording == e.recording {
            return;
        }
        e.recording = recording;
        if recording {
            let recent = e.exited_at.take().is_some_and(|at| now.duration_since(at) < window);
            if recent {
                e.count += 1;
                e.last_restart = Some(now);
                self.dirty = true;
            }
        } else {
            let unexpected = enabled && !e.user_stop;
            e.user_stop = false;
            e.exited_at = unexpected.then_some(now);
        }
    }

    /// Marks the streamer's running recorder as stopped on purpose, so its exit is not counted.
    pub fn stopping(&mut self, id: StreamerId) {
        if let Some(e) = self.entries.get_mut(&id).filter(|e| e.recording) {
            e.user_stop = true;
        }
    }

    /// Resets every count.
    pub fn clear(&mut self) {
        for e in self.entries.values_mut() {
            if e.count > 0 {
                self.dirty = true;
            }
            e.count = 0;
            e.last_restart = None;
        }
    }

    /// Resets counts whose latest restart is older than the streamer's auto-clear minutes; 0 never clears.
    pub fn expire(&mut self, now: Instant, clear_minutes: impl Fn(StreamerId) -> u64) {
        for (id, e) in self.entries.iter_mut() {
            let minutes = clear_minutes(*id);
            let due = minutes > 0
                && e.count > 0
                && e.last_restart.is_some_and(|at| now.duration_since(at) >= Duration::from_secs(minutes * 60));
            if due {
                e.count = 0;
                e.last_restart = None;
                self.dirty = true;
            }
        }
    }

    /// Keeps only the streamers the predicate accepts.
    pub fn retain(&mut self, mut keep: impl FnMut(StreamerId) -> bool) {
        let before = self.entries.len();
        let had_counts = self.entries.values().any(|e| e.count > 0);
        self.entries.retain(|id, _| keep(*id));
        if had_counts && self.entries.len() != before {
            self.dirty = true;
        }
    }

    /// Streamer id to its restart count, leaving out streamers with none.
    pub fn counts(&self) -> HashMap<StreamerId, u32> {
        self.entries.iter().filter(|(_, e)| e.count > 0).map(|(id, e)| (*id, e.count)).collect()
    }

    pub fn take_dirty(&mut self) -> bool {
        std::mem::take(&mut self.dirty)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const WINDOW: Duration = Duration::from_secs(1800);

    fn secs(t0: Instant, n: u64) -> Instant {
        t0 + Duration::from_secs(n)
    }

    #[test]
    fn a_respawn_within_the_window_counts() {
        let (mut t, t0) = (RestartTracker::default(), Instant::now());
        t.on_status(1, true, true, WINDOW, t0);
        t.on_status(1, false, true, WINDOW, secs(t0, 5));
        t.on_status(1, true, true, WINDOW, secs(t0, 65));
        assert_eq!(t.counts().get(&1), Some(&1));
        assert!(t.take_dirty());
    }

    #[test]
    fn a_respawn_after_the_window_does_not_count() {
        let (mut t, t0) = (RestartTracker::default(), Instant::now());
        t.on_status(1, true, true, WINDOW, t0);
        t.on_status(1, false, true, WINDOW, secs(t0, 5));
        t.on_status(1, true, true, WINDOW, secs(t0, 5 + 1860));
        assert!(t.counts().is_empty());
    }

    #[test]
    fn exits_without_a_respawn_do_not_count() {
        let (mut t, t0) = (RestartTracker::default(), Instant::now());
        t.on_status(1, true, true, WINDOW, t0);
        t.on_status(1, false, true, WINDOW, secs(t0, 5));
        assert!(t.counts().is_empty());
    }

    #[test]
    fn user_stops_and_disabled_streamers_do_not_count() {
        let (mut t, t0) = (RestartTracker::default(), Instant::now());
        t.on_status(1, true, true, WINDOW, t0);
        t.stopping(1);
        t.on_status(1, false, true, WINDOW, secs(t0, 5));
        t.on_status(1, true, true, WINDOW, secs(t0, 10));
        t.on_status(1, false, false, WINDOW, secs(t0, 15));
        t.on_status(1, true, true, WINDOW, secs(t0, 20));
        assert!(t.counts().is_empty());
    }

    #[test]
    fn clear_and_auto_clear_reset_counts() {
        let (mut t, t0) = (RestartTracker::default(), Instant::now());
        for i in 0..3 {
            t.on_status(1, true, true, WINDOW, secs(t0, i * 10));
            t.on_status(1, false, true, WINDOW, secs(t0, i * 10 + 5));
        }
        t.on_status(1, true, true, WINDOW, secs(t0, 40));
        assert_eq!(t.counts().get(&1), Some(&3));
        t.expire(secs(t0, 100), |_| 0);
        assert_eq!(t.counts().get(&1), Some(&3));
        t.expire(secs(t0, 40 + 600), |_| 10);
        assert!(t.counts().is_empty());
        t.on_status(1, false, true, WINDOW, secs(t0, 700));
        t.on_status(1, true, true, WINDOW, secs(t0, 710));
        t.clear();
        assert!(t.counts().is_empty());
    }
}
