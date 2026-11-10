//! Upgrade-quality tracking: arms against the recording's probed quality, then reports a better one from the checker.

use crate::api::{Quality, StreamerId};
use std::cmp::Ordering;
use std::collections::HashMap;
use std::time::{Duration, Instant};

/// How long after the first probe the tracker may stay unarmed before one log line is due.
const ARM_WINDOW: Duration = Duration::from_secs(60);
/// Frame rates within this many thousandths of a frame per second count as equal.
const FPS_TOLERANCE_MILLI: u32 = 1000;
/// Most distinct checker qualities remembered per live span.
const MAX_SEEN: usize = 8;

/// What the checker's quality is compared on once armed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mode {
    Resolution,
    ResolutionFps,
}

#[derive(Default)]
struct Entry {
    /// Distinct checker qualities seen this live span.
    seen: Vec<Quality>,
    /// Quality in the latest check; `None` when that check had none.
    latest: Option<Quality>,
    /// Quality ffprobe read from the current recording.
    actual: Option<Quality>,
    armed: Option<Mode>,
    probed_at: Option<Instant>,
    warned: bool,
    no_quality_logged: bool,
    probe_failure_logged: bool,
}

impl Entry {
    /// Arms when a seen checker quality has the recording's resolution; frame rate decides the mode.
    fn try_arm(&mut self) {
        if self.armed.is_some() {
            return;
        }
        let Some(actual) = self.actual else { return };
        let mut mode = None;
        for q in self.seen.iter().filter(|q| q.width == actual.width && q.height == actual.height) {
            if fps_matches(q, &actual) {
                mode = Some(Mode::ResolutionFps);
                break;
            }
            mode = Some(Mode::Resolution);
        }
        self.armed = mode;
    }
}

/// A higher quality than the recording's, found while armed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Upgrade {
    pub from: Quality,
    pub to: Quality,
}

/// A streamer still unarmed after the arming window, with the two qualities that never matched.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Unarmed {
    pub id: StreamerId,
    pub stream: Quality,
    pub recording: Quality,
}

#[derive(Default)]
pub struct QualityTracker {
    entries: HashMap<StreamerId, Entry>,
}

impl QualityTracker {
    /// Records the quality from a streamer's latest check.
    pub fn on_check(&mut self, id: StreamerId, quality: Option<Quality>) {
        let Some(q) = quality else {
            if let Some(e) = self.entries.get_mut(&id) {
                e.latest = None;
            }
            return;
        };
        let e = self.entries.entry(id).or_default();
        e.latest = Some(q);
        if !e.seen.contains(&q) {
            if e.seen.len() == MAX_SEEN {
                e.seen.remove(0);
            }
            e.seen.push(q);
        }
        e.try_arm();
    }

    /// Records the quality ffprobe read from the streamer's recording.
    pub fn on_probe(&mut self, id: StreamerId, actual: Quality, now: Instant) {
        let e = self.entries.entry(id).or_default();
        e.actual = Some(actual);
        e.probed_at.get_or_insert(now);
        e.try_arm();
    }

    /// Forgets the probed quality once the streamer has no recording.
    pub fn on_recording(&mut self, id: StreamerId, recording: bool) {
        if !recording {
            if let Some(e) = self.entries.get_mut(&id) {
                e.actual = None;
            }
        }
    }

    /// The upgrade on offer: armed, and the latest check beats the recording.
    pub fn upgrade(&self, id: StreamerId) -> Option<Upgrade> {
        let e = self.entries.get(&id)?;
        let mode = e.armed?;
        let (latest, actual) = (e.latest?, e.actual?);
        better(&latest, &actual, mode).then_some(Upgrade { from: actual, to: latest })
    }

    /// Streamers that are still unarmed past the window and have not been reported yet; each is reported once.
    pub fn unarmed(&mut self, now: Instant, enabled: impl Fn(StreamerId) -> bool) -> Vec<Unarmed> {
        let mut out = Vec::new();
        for (id, e) in self.entries.iter_mut() {
            let Some(probed) = e.probed_at else { continue };
            if e.warned || e.armed.is_some() || now.duration_since(probed) < ARM_WINDOW || !enabled(*id) {
                continue;
            }
            let (Some(stream), Some(recording)) = (e.latest.or(e.seen.last().copied()), e.actual) else { continue };
            e.warned = true;
            out.push(Unarmed { id: *id, stream, recording });
        }
        out
    }

    /// True once per live span: the checker reported no quality for a live stream.
    pub fn first_missing_quality(&mut self, id: StreamerId) -> bool {
        let e = self.entries.entry(id).or_default();
        !std::mem::replace(&mut e.no_quality_logged, true)
    }

    /// True once per live span: ffprobe could not read the recording.
    pub fn first_probe_failure(&mut self, id: StreamerId) -> bool {
        let e = self.entries.entry(id).or_default();
        !std::mem::replace(&mut e.probe_failure_logged, true)
    }

    /// Drops everything known about a streamer's live span.
    pub fn end_span(&mut self, id: StreamerId) {
        self.entries.remove(&id);
    }
}

fn fps_matches(a: &Quality, b: &Quality) -> bool {
    matches!((a.fps_milli, b.fps_milli), (Some(x), Some(y)) if x.abs_diff(y) <= FPS_TOLERANCE_MILLI)
}

/// True when `new` has more pixels than `cur`, or in frame-rate mode the same pixels at a clearly higher rate.
fn better(new: &Quality, cur: &Quality, mode: Mode) -> bool {
    match new.area().cmp(&cur.area()) {
        Ordering::Greater => true,
        Ordering::Less => false,
        Ordering::Equal => {
            mode == Mode::ResolutionFps
                && matches!((new.fps_milli, cur.fps_milli), (Some(n), Some(c)) if n > c.saturating_add(FPS_TOLERANCE_MILLI))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn q(w: u32, h: u32, fps: Option<f64>) -> Quality {
        Quality::new(w, h, fps)
    }

    fn up(t: &QualityTracker) -> Option<(String, String)> {
        t.upgrade(1).map(|u| (u.from.to_string(), u.to.to_string()))
    }

    #[test]
    fn matching_probe_arms_and_a_larger_check_upgrades() {
        let mut t = QualityTracker::default();
        t.on_check(1, Some(q(1280, 720, None)));
        t.on_probe(1, q(1280, 720, None), Instant::now());
        assert_eq!(up(&t), None);
        t.on_check(1, Some(q(1920, 1080, None)));
        assert_eq!(up(&t), Some(("1280x720".into(), "1920x1080".into())));
    }

    #[test]
    fn an_earlier_matching_check_arms_after_the_stream_has_moved_on() {
        let mut t = QualityTracker::default();
        t.on_check(1, Some(q(1280, 720, None)));
        t.on_check(1, Some(q(1920, 1080, None)));
        t.on_probe(1, q(1280, 720, None), Instant::now());
        assert_eq!(up(&t), Some(("1280x720".into(), "1920x1080".into())));
    }

    #[test]
    fn never_matching_checks_never_upgrade() {
        let mut t = QualityTracker::default();
        t.on_check(1, Some(q(1920, 1080, None)));
        t.on_probe(1, q(1280, 720, None), Instant::now());
        t.on_check(1, Some(q(2560, 1440, None)));
        assert_eq!(up(&t), None);
    }

    #[test]
    fn a_lower_check_is_not_an_upgrade() {
        let mut t = QualityTracker::default();
        t.on_check(1, Some(q(1920, 1080, None)));
        t.on_probe(1, q(1920, 1080, None), Instant::now());
        t.on_check(1, Some(q(1280, 720, None)));
        assert_eq!(up(&t), None);
    }

    #[test]
    fn higher_frame_rate_at_the_same_size_upgrades_in_fps_mode() {
        let mut t = QualityTracker::default();
        t.on_check(1, Some(q(1280, 720, Some(30.02))));
        t.on_probe(1, q(1280, 720, Some(30.0)), Instant::now());
        t.on_check(1, Some(q(1280, 720, Some(60.039))));
        assert_eq!(up(&t), Some(("1280x720@30".into(), "1280x720@60".into())));
    }

    #[test]
    fn small_frame_rate_differences_are_not_upgrades() {
        let mut t = QualityTracker::default();
        t.on_check(1, Some(q(1920, 1080, Some(60.0))));
        t.on_probe(1, q(1920, 1080, Some(59.94)), Instant::now());
        t.on_check(1, Some(q(1920, 1080, Some(60.0))));
        assert_eq!(up(&t), None);
    }

    #[test]
    fn a_larger_size_upgrades_even_at_a_lower_frame_rate() {
        let mut t = QualityTracker::default();
        t.on_check(1, Some(q(1280, 720, Some(60.0))));
        t.on_probe(1, q(1280, 720, Some(60.0)), Instant::now());
        t.on_check(1, Some(q(1920, 1080, Some(30.0))));
        assert_eq!(up(&t), Some(("1280x720@60".into(), "1920x1080@30".into())));
    }

    #[test]
    fn frame_rate_mismatch_falls_back_to_resolution_only() {
        let mut t = QualityTracker::default();
        t.on_check(1, Some(q(1280, 720, Some(60.0))));
        t.on_probe(1, q(1280, 720, Some(30.0)), Instant::now());
        assert_eq!(up(&t), None);
        t.on_check(1, Some(q(1280, 720, Some(90.0))));
        assert_eq!(up(&t), None);
        t.on_check(1, Some(q(1920, 1080, Some(60.0))));
        assert!(up(&t).is_some());
    }

    #[test]
    fn missing_frame_rates_fall_back_to_resolution_only() {
        let mut t = QualityTracker::default();
        t.on_check(1, Some(q(640, 1280, None)));
        t.on_probe(1, q(640, 1280, Some(25.0)), Instant::now());
        t.on_check(1, Some(q(640, 1280, None)));
        assert_eq!(up(&t), None);
        t.on_check(1, Some(q(720, 1440, None)));
        assert!(up(&t).is_some());
    }

    #[test]
    fn no_upgrade_without_a_current_recording_probe() {
        let mut t = QualityTracker::default();
        t.on_check(1, Some(q(1280, 720, None)));
        t.on_probe(1, q(1280, 720, None), Instant::now());
        t.on_recording(1, false);
        t.on_check(1, Some(q(1920, 1080, None)));
        assert_eq!(up(&t), None);
        t.on_probe(1, q(1280, 720, None), Instant::now());
        assert!(up(&t).is_some());
    }

    #[test]
    fn a_check_without_quality_clears_the_latest() {
        let mut t = QualityTracker::default();
        t.on_check(1, Some(q(1280, 720, None)));
        t.on_probe(1, q(1280, 720, None), Instant::now());
        t.on_check(1, Some(q(1920, 1080, None)));
        t.on_check(1, None);
        assert_eq!(up(&t), None);
    }

    #[test]
    fn unarmed_is_reported_once_after_the_window() {
        let mut t = QualityTracker::default();
        let t0 = Instant::now();
        t.on_check(1, Some(q(1920, 1080, None)));
        t.on_probe(1, q(1280, 720, None), t0);
        assert!(t.unarmed(t0 + Duration::from_secs(59), |_| true).is_empty());
        let due = t.unarmed(t0 + Duration::from_secs(60), |_| true);
        assert_eq!(due.len(), 1);
        assert_eq!((due[0].stream.to_string(), due[0].recording.to_string()), ("1920x1080".into(), "1280x720".into()));
        assert!(t.unarmed(t0 + Duration::from_secs(120), |_| true).is_empty());
    }

    #[test]
    fn unarmed_stays_quiet_when_disabled_armed_or_without_data() {
        let t0 = Instant::now();
        let late = t0 + Duration::from_secs(90);
        let mut off = QualityTracker::default();
        off.on_check(1, Some(q(1920, 1080, None)));
        off.on_probe(1, q(1280, 720, None), t0);
        assert!(off.unarmed(late, |_| false).is_empty());
        let mut armed = QualityTracker::default();
        armed.on_check(1, Some(q(1280, 720, None)));
        armed.on_probe(1, q(1280, 720, None), t0);
        assert!(armed.unarmed(late, |_| true).is_empty());
        let mut no_check = QualityTracker::default();
        no_check.on_probe(1, q(1280, 720, None), t0);
        assert!(no_check.unarmed(late, |_| true).is_empty());
        let mut no_probe = QualityTracker::default();
        no_probe.on_check(1, Some(q(1280, 720, None)));
        assert!(no_probe.unarmed(late, |_| true).is_empty());
    }

    #[test]
    fn inactive_notices_come_once_per_span() {
        let mut t = QualityTracker::default();
        assert!(t.first_missing_quality(1));
        assert!(!t.first_missing_quality(1));
        assert!(t.first_probe_failure(1));
        assert!(!t.first_probe_failure(1));
        t.end_span(1);
        assert!(t.first_missing_quality(1) && t.first_probe_failure(1));
    }

    #[test]
    fn inactive_notices_do_not_disturb_arming() {
        let mut t = QualityTracker::default();
        t.first_missing_quality(1);
        t.on_check(1, Some(q(1280, 720, None)));
        t.on_probe(1, q(1280, 720, None), Instant::now());
        t.on_check(1, Some(q(1920, 1080, None)));
        assert!(up(&t).is_some());
    }

    #[test]
    fn ending_the_span_resets_everything() {
        let mut t = QualityTracker::default();
        t.on_check(1, Some(q(1280, 720, None)));
        t.on_probe(1, q(1280, 720, None), Instant::now());
        t.end_span(1);
        t.on_check(1, Some(q(1920, 1080, None)));
        assert_eq!(up(&t), None);
    }
}
