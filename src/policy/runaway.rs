//! Watches for a streamer holding more recorders or ffmpeg processes than expected; it only reports.

use crate::api::{RecorderId, StreamerId, MAX_FFMPEG_PER_STREAMER, MAX_RECORDERS_PER_STREAMER};
use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

/// How long too many recorders or ffmpeg processes must persist before it is reported.
const EXCESS_GRACE: Duration = Duration::from_secs(30);
/// How long more than one recorder (one Processes row each) may coexist before it is reported.
const OVERLAP_GRACE: Duration = Duration::from_secs(120);

/// One recorder's share of a streamer, as last reported.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Share {
    pub recorder: RecorderId,
    pub ffmpeg: usize,
    pub uptime_secs: u64,
}

#[derive(Debug, PartialEq)]
pub enum Finding {
    /// Too many recorders or too many ffmpeg processes in total.
    Excess { streamer: StreamerId, shares: Vec<Share>, ffmpeg: usize, for_secs: u64 },
    /// More than one recorder (so more than one Processes row) for longer than a split overlap should take.
    Overlap { streamer: StreamerId, shares: Vec<Share>, for_secs: u64 },
}

#[derive(Default)]
pub struct Runaway {
    shares: HashMap<RecorderId, (StreamerId, Share)>,
    excess_since: HashMap<StreamerId, Instant>,
    overlap_since: HashMap<StreamerId, Instant>,
    excess_logged: HashSet<StreamerId>,
    overlap_logged: HashSet<StreamerId>,
}

impl Runaway {
    pub fn on_update(&mut self, recorder: RecorderId, streamer: StreamerId, ffmpeg: usize, uptime_secs: u64) {
        self.shares.insert(recorder, (streamer, Share { recorder, ffmpeg, uptime_secs }));
    }

    pub fn remove(&mut self, recorder: RecorderId) {
        self.shares.remove(&recorder);
    }

    /// Returns each condition once when it first persists past its grace, and again only after it clears.
    pub fn check(&mut self, now: Instant) -> Vec<Finding> {
        let mut by_streamer: HashMap<StreamerId, Vec<Share>> = HashMap::new();
        for (streamer, share) in self.shares.values() {
            by_streamer.entry(*streamer).or_default().push(*share);
        }
        for shares in by_streamer.values_mut() {
            shares.sort_by_key(|s| s.recorder);
        }
        let excess = |shares: &[Share]| {
            shares.len() > MAX_RECORDERS_PER_STREAMER || shares.iter().map(|s| s.ffmpeg).sum::<usize>() > MAX_FFMPEG_PER_STREAMER
        };
        self.excess_since.retain(|s, _| by_streamer.get(s).is_some_and(|v| excess(v.as_slice())));
        self.overlap_since.retain(|s, _| by_streamer.get(s).is_some_and(|v| v.len() > 1));
        self.excess_logged.retain(|s| self.excess_since.contains_key(s));
        self.overlap_logged.retain(|s| self.overlap_since.contains_key(s));

        let mut out = Vec::new();
        let mut streamers: Vec<StreamerId> = by_streamer.keys().copied().collect();
        streamers.sort();
        for streamer in streamers {
            let shares = &by_streamer[&streamer];
            if excess(shares) {
                let since = *self.excess_since.entry(streamer).or_insert(now);
                let waited = now.saturating_duration_since(since);
                if waited >= EXCESS_GRACE && self.excess_logged.insert(streamer) {
                    let ffmpeg = shares.iter().map(|s| s.ffmpeg).sum();
                    out.push(Finding::Excess { streamer, shares: shares.clone(), ffmpeg, for_secs: waited.as_secs() });
                }
            }
            if shares.len() > 1 {
                let since = *self.overlap_since.entry(streamer).or_insert(now);
                let waited = now.saturating_duration_since(since);
                if waited >= OVERLAP_GRACE && self.overlap_logged.insert(streamer) {
                    out.push(Finding::Overlap { streamer, shares: shares.clone(), for_secs: waited.as_secs() });
                }
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_short_split_overlap_is_not_reported() {
        let mut r = Runaway::default();
        let t0 = Instant::now();
        r.on_update(1, 7, 1, 100);
        r.on_update(2, 7, 1, 5);
        assert!(r.check(t0).is_empty());
        assert!(r.check(t0 + Duration::from_secs(60)).is_empty());
        r.remove(1);
        assert!(r.check(t0 + Duration::from_secs(61)).is_empty());
    }

    #[test]
    fn a_long_overlap_is_reported_once() {
        let mut r = Runaway::default();
        let t0 = Instant::now();
        r.on_update(1, 7, 1, 100);
        r.on_update(2, 7, 1, 5);
        r.check(t0);
        let found = r.check(t0 + OVERLAP_GRACE);
        assert!(matches!(found.as_slice(), [Finding::Overlap { streamer: 7, .. }]));
        assert!(r.check(t0 + OVERLAP_GRACE + Duration::from_secs(10)).is_empty());
    }

    #[test]
    fn too_many_ffmpeg_is_reported_after_the_grace() {
        let mut r = Runaway::default();
        let t0 = Instant::now();
        r.on_update(1, 7, 8, 100);
        assert!(r.check(t0).is_empty());
        let found = r.check(t0 + EXCESS_GRACE);
        assert!(matches!(found.as_slice(), [Finding::Excess { streamer: 7, ffmpeg: 8, .. }]));
    }

    #[test]
    fn a_cleared_condition_can_be_reported_again() {
        let mut r = Runaway::default();
        let t0 = Instant::now();
        r.on_update(1, 7, 5, 100);
        r.check(t0);
        assert_eq!(r.check(t0 + EXCESS_GRACE).len(), 1);
        r.on_update(1, 7, 1, 100);
        assert!(r.check(t0 + EXCESS_GRACE + Duration::from_secs(1)).is_empty());
        r.on_update(1, 7, 5, 100);
        let t1 = t0 + Duration::from_secs(100);
        r.check(t1);
        assert_eq!(r.check(t1 + EXCESS_GRACE).len(), 1);
    }

    #[test]
    fn streamers_are_counted_separately() {
        let mut r = Runaway::default();
        let t0 = Instant::now();
        r.on_update(1, 7, 1, 100);
        r.on_update(2, 8, 1, 100);
        r.check(t0);
        assert!(r.check(t0 + OVERLAP_GRACE).is_empty());
    }
}
