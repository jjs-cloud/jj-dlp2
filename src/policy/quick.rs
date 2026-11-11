//! Quick re-check of a streamer shortly after its recording ends.

use crate::api::StreamerId;
use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

/// Most quick checks sent for one streamer during one live span.
pub const MAX_PER_SPAN: u32 = 20;

/// Whether a due quick check may go out.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fire {
    /// Send it; carries the count of quick checks this span including this one.
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

#[derive(Default)]
pub struct QuickChecks {
    /// Streamers whose current recording has a confirmed file.
    armed: HashSet<StreamerId>,
    due: HashMap<StreamerId, Instant>,
    counts: HashMap<StreamerId, SpanCount>,
}

impl QuickChecks {
    /// Marks the streamer's recording as confirmed, so its exit can schedule a quick check.
    pub fn arm(&mut self, id: StreamerId) {
        self.armed.insert(id);
    }

    /// Handles a recording that ended unexpectedly; schedules the check only if the file was confirmed.
    pub fn on_exit(&mut self, id: StreamerId, delay: Duration, now: Instant) -> bool {
        if !self.armed.remove(&id) {
            return false;
        }
        self.due.insert(id, now + delay);
        true
    }

    /// Drops the streamer's armed state and any pending check, e.g. when a recording starts or is stopped on purpose.
    pub fn cancel(&mut self, id: StreamerId) {
        self.armed.remove(&id);
        self.due.remove(&id);
    }

    /// Removes and returns the streamers whose check is due.
    pub fn take_due(&mut self, now: Instant) -> Vec<StreamerId> {
        let ready: Vec<StreamerId> = self.due.iter().filter(|(_, at)| **at <= now).map(|(id, _)| *id).collect();
        for id in &ready {
            self.due.remove(id);
        }
        ready
    }

    /// Counts a check against the streamer's live span, or refuses it once the limit is reached.
    pub fn fire(&mut self, id: StreamerId, span: Option<u64>) -> Fire {
        let c = self.counts.entry(id).or_default();
        if c.span != span {
            *c = SpanCount { span, ..Default::default() };
        }
        if c.sent >= MAX_PER_SPAN {
            let first = !c.cap_logged;
            c.cap_logged = true;
            return Fire::Capped { first };
        }
        c.sent += 1;
        Fire::Go(c.sent)
    }

    /// Keeps only the streamers the predicate accepts.
    pub fn retain(&mut self, mut keep: impl FnMut(StreamerId) -> bool) {
        self.armed.retain(|id| keep(*id));
        self.due.retain(|id, _| keep(*id));
        self.counts.retain(|id, _| keep(*id));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DELAY: Duration = Duration::from_secs(30);

    #[test]
    fn exit_without_a_confirmed_file_schedules_nothing() {
        let (mut q, t0) = (QuickChecks::default(), Instant::now());
        assert!(!q.on_exit(1, DELAY, t0));
        assert!(q.take_due(t0 + DELAY).is_empty());
    }

    #[test]
    fn confirmed_exit_is_due_after_the_delay_and_only_once() {
        let (mut q, t0) = (QuickChecks::default(), Instant::now());
        q.arm(1);
        assert!(q.on_exit(1, DELAY, t0));
        assert!(q.take_due(t0 + Duration::from_secs(29)).is_empty());
        assert_eq!(q.take_due(t0 + DELAY), vec![1]);
        assert!(q.take_due(t0 + DELAY * 2).is_empty());
        assert!(!q.on_exit(1, DELAY, t0 + DELAY));
    }

    #[test]
    fn cancel_drops_armed_and_pending() {
        let (mut q, t0) = (QuickChecks::default(), Instant::now());
        q.arm(1);
        q.cancel(1);
        assert!(!q.on_exit(1, DELAY, t0));
        q.arm(1);
        q.on_exit(1, DELAY, t0);
        q.cancel(1);
        assert!(q.take_due(t0 + DELAY).is_empty());
    }

    #[test]
    fn span_limit_caps_and_reports_once() {
        let mut q = QuickChecks::default();
        for n in 1..=MAX_PER_SPAN {
            assert_eq!(q.fire(1, Some(100)), Fire::Go(n));
        }
        assert_eq!(q.fire(1, Some(100)), Fire::Capped { first: true });
        assert_eq!(q.fire(1, Some(100)), Fire::Capped { first: false });
    }

    #[test]
    fn a_new_span_restarts_the_count() {
        let mut q = QuickChecks::default();
        for _ in 0..MAX_PER_SPAN {
            q.fire(1, Some(100));
        }
        assert_eq!(q.fire(1, Some(200)), Fire::Go(1));
        assert_eq!(q.fire(2, Some(100)), Fire::Go(1));
    }
}
