//! Recording parts: `_partN` naming and the timer that decides when a recording splits into its next part.

use crate::api::{RecorderId, StreamerId};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

/// How long a failed split waits before it is tried again.
const RETRY_AFTER: Duration = Duration::from_secs(60);
/// Time a new part gets to start, on top of two file-check intervals, before the split is given up.
const VERIFY_BASE: Duration = Duration::from_secs(30);

/// `a.%(ext)s` or `a.mp4` as `a_part2.%(ext)s` or `a_part2.mp4`; a name without an extension gets the suffix at the end.
pub fn suffix_name(name: &str, part: u32) -> String {
    let base = name.rfind(['/', '\\']).map_or(0, |i| i + 1);
    let at = name[base..].rfind('.').map_or(name.len(), |i| base + i);
    format!("{}_part{part}{}", &name[..at], &name[at..])
}

/// The path with `_partN` added to its file name.
pub fn suffix_path(path: &Path, part: u32) -> Option<PathBuf> {
    let name = path.file_name()?.to_str()?;
    Some(path.with_file_name(suffix_name(name, part)))
}

/// How long a new part has to be confirmed growing before its split is abandoned.
pub fn verify_timeout(check_interval: Duration) -> Duration {
    VERIFY_BASE + check_interval * 2
}

/// What the split timer needs to know about one streamer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Plan {
    pub minutes: u64,
    pub next_part: u32,
    pub timeout: Duration,
}

/// A split the policy layer must act on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Step {
    /// Start part `part` alongside the running recorder.
    Start { streamer: StreamerId, part: u32 },
    /// Part `part` is recording: stop `old` and move the numbering on.
    Commit { streamer: StreamerId, old: RecorderId, part: u32 },
    /// Part `part` never confirmed growing: stop `stop` (if it started) and keep the old recording.
    Abort { streamer: StreamerId, part: u32, stop: Option<RecorderId> },
}

struct Pending {
    old: RecorderId,
    new: Option<RecorderId>,
    part: u32,
    started: Instant,
    timeout: Duration,
}

#[derive(Default)]
struct Track {
    current: Option<(RecorderId, Instant)>,
    pending: Option<Pending>,
    retry_at: Option<Instant>,
}

/// Follows each streamer's recorders and the split in flight.
#[derive(Default)]
pub struct Splitter {
    tracks: HashMap<StreamerId, Track>,
}

impl Splitter {
    /// Notes a live sample of a recorder: the first one is the streamer's current recorder, or the new part during a split.
    pub fn on_running(&mut self, streamer: StreamerId, id: RecorderId, now: Instant) {
        let t = self.tracks.entry(streamer).or_default();
        if t.current.is_some_and(|(c, _)| c == id) {
            return;
        }
        match &mut t.pending {
            Some(p) if p.new.is_none() && p.old != id => p.new = Some(id),
            Some(_) => {}
            None => {
                if t.current.is_none() {
                    t.current = Some((id, now));
                }
            }
        }
    }

    /// The recorder whose file counts as the streamer's current part.
    pub fn current(&self, streamer: StreamerId) -> Option<RecorderId> {
        self.tracks.get(&streamer)?.current.map(|(id, _)| id)
    }

    /// Handles a recorder ending; a new part that ends before it is confirmed aborts its split.
    pub fn on_gone(&mut self, id: RecorderId, now: Instant) -> Option<Step> {
        for (streamer, t) in self.tracks.iter_mut() {
            if t.pending.as_ref().is_some_and(|p| p.new == Some(id)) {
                let p = t.pending.take()?;
                t.retry_at = Some(now + RETRY_AFTER);
                return Some(Step::Abort { streamer: *streamer, part: p.part, stop: None });
            }
            if t.current.is_some_and(|(c, _)| c == id) {
                t.current = None;
            }
        }
        None
    }

    /// Handles a recorder's file being confirmed growing; for a new part this completes the split.
    pub fn on_confirmed(&mut self, streamer: StreamerId, id: RecorderId) -> Option<Step> {
        let t = self.tracks.get_mut(&streamer)?;
        if t.pending.as_ref().is_none_or(|p| p.new != Some(id)) {
            return None;
        }
        let p = t.pending.take()?;
        t.current = Some((id, p.started));
        Some(Step::Commit { streamer, old: p.old, part: p.part })
    }

    /// Gives up on timed-out splits and starts new ones whose part length has passed.
    pub fn due(&mut self, now: Instant, plans: &HashMap<StreamerId, Plan>) -> Vec<Step> {
        let mut steps = Vec::new();
        for (streamer, t) in self.tracks.iter_mut() {
            if let Some(p) = &t.pending {
                if now.duration_since(p.started) >= p.timeout {
                    let p = t.pending.take().expect("pending checked above");
                    t.retry_at = Some(now + RETRY_AFTER);
                    steps.push(Step::Abort { streamer: *streamer, part: p.part, stop: p.new });
                }
                continue;
            }
            let (Some((old, since)), Some(plan)) = (t.current, plans.get(streamer)) else { continue };
            let ripe = now.duration_since(since) >= Duration::from_secs(plan.minutes.saturating_mul(60));
            if ripe && t.retry_at.is_none_or(|at| now >= at) {
                t.retry_at = None;
                t.pending = Some(Pending { old, new: None, part: plan.next_part, started: now, timeout: plan.timeout });
                steps.push(Step::Start { streamer: *streamer, part: plan.next_part });
            }
        }
        steps
    }

    /// Forgets a streamer's recorders and any split in flight.
    pub fn cancel(&mut self, streamer: StreamerId) {
        self.tracks.remove(&streamer);
    }

    /// Keeps only the streamers the predicate accepts.
    pub fn retain(&mut self, mut keep: impl FnMut(StreamerId) -> bool) {
        self.tracks.retain(|id, _| keep(*id));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TIMEOUT: Duration = Duration::from_secs(90);

    fn at(t0: Instant, secs: u64) -> Instant {
        t0 + Duration::from_secs(secs)
    }

    fn plan(minutes: u64, next_part: u32) -> HashMap<StreamerId, Plan> {
        HashMap::from([(1, Plan { minutes, next_part, timeout: TIMEOUT })])
    }

    #[test]
    fn suffix_goes_before_the_extension() {
        assert_eq!(suffix_name("%(title)s.%(ext)s", 2), "%(title)s_part2.%(ext)s");
        assert_eq!(suffix_name("a b [1].mp4", 1), "a b [1]_part1.mp4");
        assert_eq!(suffix_name("noext", 3), "noext_part3");
    }

    #[test]
    fn suffix_ignores_dots_in_folder_names() {
        assert_eq!(suffix_name("v1.2/clip", 2), "v1.2/clip_part2");
        assert_eq!(suffix_name("C:\\a.b\\clip.mp4", 2), "C:\\a.b\\clip_part2.mp4");
    }

    #[test]
    fn suffix_path_changes_only_the_file_name() {
        let got = suffix_path(Path::new("/rec/Emiru/live.mp4"), 1).unwrap();
        assert_eq!(got, PathBuf::from("/rec/Emiru/live_part1.mp4"));
    }

    #[test]
    fn nothing_is_due_before_the_part_length() {
        let (mut s, t0) = (Splitter::default(), Instant::now());
        s.on_running(1, 10, t0);
        assert!(s.due(at(t0, 59 * 60), &plan(60, 2)).is_empty());
        assert_eq!(s.due(at(t0, 60 * 60), &plan(60, 2)), [Step::Start { streamer: 1, part: 2 }]);
    }

    #[test]
    fn nothing_is_due_without_a_plan() {
        let (mut s, t0) = (Splitter::default(), Instant::now());
        s.on_running(1, 10, t0);
        assert!(s.due(at(t0, 10_000), &HashMap::new()).is_empty());
    }

    #[test]
    fn a_split_in_flight_is_not_started_twice() {
        let (mut s, t0) = (Splitter::default(), Instant::now());
        s.on_running(1, 10, t0);
        assert_eq!(s.due(at(t0, 60), &plan(1, 2)).len(), 1);
        assert!(s.due(at(t0, 61), &plan(1, 2)).is_empty());
    }

    #[test]
    fn confirming_the_new_part_commits_and_restarts_the_timer() {
        let (mut s, t0) = (Splitter::default(), Instant::now());
        s.on_running(1, 10, t0);
        s.due(at(t0, 60), &plan(1, 2));
        s.on_running(1, 11, at(t0, 62));
        assert_eq!(s.on_confirmed(1, 11), Some(Step::Commit { streamer: 1, old: 10, part: 2 }));
        assert_eq!(s.current(1), Some(11));
        assert!(s.due(at(t0, 100), &plan(1, 3)).is_empty());
        assert_eq!(s.due(at(t0, 120), &plan(1, 3)), [Step::Start { streamer: 1, part: 3 }]);
    }

    #[test]
    fn confirming_the_old_recorder_does_nothing() {
        let (mut s, t0) = (Splitter::default(), Instant::now());
        s.on_running(1, 10, t0);
        s.due(at(t0, 60), &plan(1, 2));
        s.on_running(1, 11, at(t0, 62));
        assert_eq!(s.on_confirmed(1, 10), None);
    }

    #[test]
    fn an_unconfirmed_part_times_out_and_waits_before_retrying() {
        let (mut s, t0) = (Splitter::default(), Instant::now());
        s.on_running(1, 10, t0);
        s.due(at(t0, 60), &plan(1, 2));
        s.on_running(1, 11, at(t0, 62));
        let abort = Step::Abort { streamer: 1, part: 2, stop: Some(11) };
        assert!(s.due(at(t0, 60 + 89), &plan(1, 2)).is_empty());
        assert_eq!(s.due(at(t0, 60 + 90), &plan(1, 2)), [abort]);
        assert!(s.due(at(t0, 60 + 90 + 59), &plan(1, 2)).is_empty());
        assert_eq!(s.due(at(t0, 60 + 90 + 60), &plan(1, 2)), [Step::Start { streamer: 1, part: 2 }]);
        assert_eq!(s.current(1), Some(10));
    }

    #[test]
    fn a_new_part_that_exits_early_aborts_the_split() {
        let (mut s, t0) = (Splitter::default(), Instant::now());
        s.on_running(1, 10, t0);
        s.due(at(t0, 60), &plan(1, 2));
        s.on_running(1, 11, at(t0, 62));
        let got = s.on_gone(11, at(t0, 70));
        assert_eq!(got, Some(Step::Abort { streamer: 1, part: 2, stop: None }));
        assert!(s.due(at(t0, 100), &plan(1, 2)).is_empty());
    }

    #[test]
    fn a_new_recorder_after_the_old_one_is_gone_becomes_current() {
        let (mut s, t0) = (Splitter::default(), Instant::now());
        s.on_running(1, 10, t0);
        assert_eq!(s.on_gone(10, at(t0, 5)), None);
        assert_eq!(s.current(1), None);
        s.on_running(1, 12, at(t0, 8));
        assert_eq!(s.current(1), Some(12));
    }

    #[test]
    fn cancel_forgets_the_streamer() {
        let (mut s, t0) = (Splitter::default(), Instant::now());
        s.on_running(1, 10, t0);
        s.due(at(t0, 60), &plan(1, 2));
        s.cancel(1);
        assert_eq!(s.current(1), None);
        assert!(s.due(at(t0, 1000), &plan(1, 2)).is_empty());
    }

    #[test]
    fn verify_timeout_grows_with_the_check_interval() {
        assert_eq!(verify_timeout(Duration::from_secs(30)), Duration::from_secs(90));
        assert_eq!(verify_timeout(Duration::from_secs(60)), Duration::from_secs(150));
    }
}
