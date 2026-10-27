//! Per-recording file watch: verifies the output path from two sources, then checks the file exists and keeps growing.

use crate::api::{ProcessInfo, RecorderId, RecorderState, Site};
use crate::outpath::same_path;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

/// How long the two sources may disagree before the recording counts as a conflict.
const CONFLICT_SETTLE: Duration = Duration::from_secs(5);
/// How long a recording may stay unverified before one log line is written.
const PENDING_LOG_AFTER: Duration = Duration::from_secs(120);

/// A recording's file state as shown by the GUI.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RecordingFile {
    /// The verified file exists and can be opened.
    pub openable: bool,
    /// The file is missing or has stopped growing.
    pub stalled: bool,
}

/// Per-site timing for the file checks.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Thresholds {
    pub exists_grace: Duration,
    pub growth_grace: Duration,
    pub stall_window: Duration,
    pub interval: Duration,
}

impl Thresholds {
    pub fn from_site(site: &Site) -> Self {
        let secs = |v: u64| Duration::from_secs(v.max(1));
        Self {
            exists_grace: secs(site.file_exists_grace_secs),
            growth_grace: secs(site.growth_grace_secs),
            stall_window: secs(site.stall_window_secs),
            interval: secs(site.file_check_interval_secs),
        }
    }
}

/// Result of looking at the recording file on disk.
#[derive(Debug, Clone, PartialEq)]
pub enum Stat {
    Size(u64),
    Missing,
    Failed(String),
}

impl Stat {
    pub fn of(path: &Path) -> Stat {
        match std::fs::metadata(path) {
            Ok(m) if m.is_file() => Stat::Size(m.len()),
            Ok(_) => Stat::Failed("not a regular file".into()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Stat::Missing,
            Err(e) => Stat::Failed(e.to_string()),
        }
    }
}

/// Work the policy layer must do after a tracker call.
#[derive(Debug, Default)]
pub struct Update {
    pub logs: Vec<String>,
    /// Path to stat on a blocking thread; the result goes to `FileTracker::on_stat`.
    pub stat: Option<PathBuf>,
}

#[derive(Debug, Clone, PartialEq)]
enum PathCheck {
    Pending,
    Agreed(PathBuf),
    Conflict,
}

struct Entry {
    name: String,
    th: Thresholds,
    live: bool,
    check: PathCheck,
    created: Instant,
    unmatched_since: Option<Instant>,
    pending_logged: bool,
    agreed_at: Option<Instant>,
    last_stat_at: Option<Instant>,
    in_flight: bool,
    exists: bool,
    last_size: Option<u64>,
    last_growth: Option<Instant>,
    stall: Option<String>,
}

impl Entry {
    fn new(name: &str, th: Thresholds, now: Instant) -> Self {
        Self {
            name: name.to_string(),
            th,
            live: true,
            check: PathCheck::Pending,
            created: now,
            unmatched_since: None,
            pending_logged: false,
            agreed_at: None,
            last_stat_at: None,
            in_flight: false,
            exists: false,
            last_size: None,
            last_growth: None,
            stall: None,
        }
    }

    /// The verified output path; every file check goes through this.
    fn trusted_path(&self) -> Option<&Path> {
        match &self.check {
            PathCheck::Agreed(p) => Some(p.as_path()),
            _ => None,
        }
    }

    /// Compares ffmpeg's output path with the sidecar paths and moves between pending, agreed and conflict.
    fn verify(&mut self, p: &ProcessInfo, now: Instant, logs: &mut Vec<String>) {
        if self.check == PathCheck::Conflict {
            return;
        }
        let Some(ff) = p.ffmpeg_output.as_deref() else {
            self.note_pending(now, logs);
            return;
        };
        if matches!(&self.check, PathCheck::Agreed(a) if same_path(a, ff)) {
            self.unmatched_since = None;
            return;
        }
        if p.sidecar_paths.iter().any(|c| same_path(&c.path, ff)) {
            self.agree(ff.to_path_buf(), now);
            return;
        }
        if p.sidecar_paths.is_empty() {
            self.note_pending(now, logs);
            return;
        }
        let since = *self.unmatched_since.get_or_insert(now);
        if now.duration_since(since) >= CONFLICT_SETTLE {
            self.conflict(p, logs);
        }
    }

    fn agree(&mut self, path: PathBuf, now: Instant) {
        self.check = PathCheck::Agreed(path);
        self.unmatched_since = None;
        self.agreed_at = Some(now);
        self.last_stat_at = None;
        self.exists = false;
        self.last_size = None;
        self.last_growth = None;
        self.stall = None;
    }

    fn conflict(&mut self, p: &ProcessInfo, logs: &mut Vec<String>) {
        self.check = PathCheck::Conflict;
        self.exists = false;
        self.stall = None;
        let sidecar: Vec<_> = p.sidecar_paths.iter().map(|c| (&c.tag, &c.path)).collect();
        logs.push(format!(
            "warning: {}: recording file paths disagree, file checks are off for this recording (ffmpeg: {:?}, yt-dlp: {:?})",
            self.name, p.ffmpeg_output, sidecar
        ));
    }

    fn note_pending(&mut self, now: Instant, logs: &mut Vec<String>) {
        if self.check == PathCheck::Pending && !self.pending_logged && now.duration_since(self.created) >= PENDING_LOG_AFTER {
            self.pending_logged = true;
            logs.push(format!(
                "warning: {}: recording file path not verified after {}s, file checks are off for this recording",
                self.name,
                PENDING_LOG_AFTER.as_secs()
            ));
        }
    }

    /// Returns the path to stat when a check is due; only a verified path is ever checked.
    fn due_stat(&mut self, now: Instant) -> Option<PathBuf> {
        let path = self.trusted_path()?.to_path_buf();
        if self.in_flight || self.last_stat_at.is_some_and(|t| now.duration_since(t) < self.th.interval) {
            return None;
        }
        self.in_flight = true;
        self.last_stat_at = Some(now);
        Some(path)
    }

    /// Records a stat result and raises or clears the stalled state.
    fn apply_stat(&mut self, stat: Stat, now: Instant, logs: &mut Vec<String>) {
        let Some(agreed_at) = self.agreed_at else { return };
        let th = self.th;
        let reason = match stat {
            Stat::Size(size) => {
                self.exists = true;
                if size > self.last_size.unwrap_or(0) {
                    self.last_growth = Some(now);
                }
                self.last_size = Some(size);
                let base = self.last_growth.unwrap_or(agreed_at).max(agreed_at + th.growth_grace);
                (now.duration_since(base) >= th.stall_window)
                    .then(|| format!("no file growth in {}s", th.stall_window.as_secs()))
            }
            Stat::Missing => self.unreadable(now, agreed_at, "file does not exist"),
            Stat::Failed(msg) => self.unreadable(now, agreed_at, &format!("cannot read file: {msg}")),
        };
        match (&self.stall, &reason) {
            (None, Some(r)) => logs.push(format!("warning: {}: recording file stalled: {r}", self.name)),
            (Some(_), None) => logs.push(format!("{}: recording file is growing again", self.name)),
            _ => {}
        }
        self.stall = reason;
    }

    /// Marks the file as absent; it only counts as a failure once the exists grace has passed.
    fn unreadable(&mut self, now: Instant, agreed_at: Instant, why: &str) -> Option<String> {
        self.exists = false;
        self.last_size = None;
        (now.duration_since(agreed_at) >= self.th.exists_grace).then(|| why.to_string())
    }
}

/// Tracks the file state of every active recording.
#[derive(Default)]
pub struct FileTracker {
    entries: HashMap<RecorderId, Entry>,
}

impl FileTracker {
    /// Feeds one process sample: verifies the path and decides whether a file check is due.
    pub fn on_update(&mut self, p: &ProcessInfo, th: Thresholds, now: Instant) -> Update {
        let e = self.entries.entry(p.id).or_insert_with(|| Entry::new(&p.streamer_name, th, now));
        e.th = th;
        e.live = matches!(p.state, RecorderState::Starting | RecorderState::Running);
        let mut up = Update::default();
        e.verify(p, now, &mut up.logs);
        if e.live {
            up.stat = e.due_stat(now);
        } else {
            e.stall = None;
        }
        up
    }

    /// Applies the result of a requested stat.
    pub fn on_stat(&mut self, id: RecorderId, stat: Stat, now: Instant) -> Update {
        let mut up = Update::default();
        let Some(e) = self.entries.get_mut(&id) else { return up };
        e.in_flight = false;
        if e.live && e.trusted_path().is_some() {
            e.apply_stat(stat, now, &mut up.logs);
        }
        up
    }

    pub fn remove(&mut self, id: RecorderId) {
        self.entries.remove(&id);
    }

    /// The recording's verified output path, or `None` until both sources agree.
    pub fn trusted_path(&self, id: RecorderId) -> Option<&Path> {
        self.entries.get(&id)?.trusted_path()
    }

    pub fn snapshot(&self) -> HashMap<RecorderId, RecordingFile> {
        self.entries
            .iter()
            .map(|(id, e)| {
                let openable = e.trusted_path().is_some() && e.exists;
                (*id, RecordingFile { openable, stalled: e.stall.is_some() })
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::SidecarPath;

    fn th() -> Thresholds {
        Thresholds {
            exists_grace: Duration::from_secs(60),
            growth_grace: Duration::from_secs(60),
            stall_window: Duration::from_secs(120),
            interval: Duration::from_secs(30),
        }
    }

    fn info(state: RecorderState, ffmpeg: Option<&str>, sidecar: &[(&str, &str)]) -> ProcessInfo {
        ProcessInfo {
            id: 1,
            streamer_id: 2,
            streamer_name: "名前".into(),
            site_name: "Site".into(),
            state,
            uptime_secs: 0,
            root: None,
            ytdlp: None,
            has_bootloader: false,
            ffmpeg: Vec::new(),
            last_line: String::new(),
            cmdline: String::new(),
            ffmpeg_output: ffmpeg.map(PathBuf::from),
            sidecar_paths: sidecar
                .iter()
                .map(|(tag, path)| SidecarPath { tag: tag.to_string(), path: PathBuf::from(path) })
                .collect(),
        }
    }

    fn running(ffmpeg: Option<&str>, sidecar: &[(&str, &str)]) -> ProcessInfo {
        info(RecorderState::Running, ffmpeg, sidecar)
    }

    /// A tracker whose recording 1 agreed at `t0`, with its first stat already requested.
    fn agreed(t0: Instant) -> FileTracker {
        let mut tr = FileTracker::default();
        let up = tr.on_update(&running(Some("/r/a.ts"), &[("BDP", "/r/a.ts")]), th(), t0);
        assert_eq!(up.stat, Some(PathBuf::from("/r/a.ts")));
        tr
    }

    fn at(t0: Instant, secs: u64) -> Instant {
        t0 + Duration::from_secs(secs)
    }

    #[test]
    fn agrees_when_any_sidecar_form_matches_ffmpeg() {
        let t0 = Instant::now();
        let mut tr = FileTracker::default();
        let p = running(Some("/r/a.ts"), &[("BDP", "/r/other.ts"), ("BDN", "/r/a.ts")]);
        tr.on_update(&p, th(), t0);
        assert_eq!(tr.trusted_path(1), Some(Path::new("/r/a.ts")));
    }

    #[test]
    fn pending_until_both_sources_exist_and_never_checks_files() {
        let t0 = Instant::now();
        let mut tr = FileTracker::default();
        assert_eq!(tr.on_update(&running(None, &[("BDP", "/r/a.ts")]), th(), t0).stat, None);
        assert_eq!(tr.on_update(&running(Some("/r/a.ts"), &[]), th(), at(t0, 1)).stat, None);
        assert_eq!(tr.trusted_path(1), None);
    }

    #[test]
    fn disagreement_becomes_a_silent_conflict_after_the_settle_time() {
        let t0 = Instant::now();
        let mut tr = FileTracker::default();
        let p = running(Some("/r/a.ts"), &[("BDP", "/r/b.ts")]);
        assert!(tr.on_update(&p, th(), t0).logs.is_empty());
        assert!(tr.on_update(&p, th(), at(t0, 4)).logs.is_empty());
        let up = tr.on_update(&p, th(), at(t0, 5));
        assert_eq!(up.logs.len(), 1);
        assert!(up.logs[0].starts_with("warning:") && up.logs[0].contains("disagree"), "{:?}", up.logs);
        assert_eq!(up.stat, None);
        assert_eq!(tr.trusted_path(1), None);
        assert!(tr.on_update(&p, th(), at(t0, 6)).logs.is_empty());
        assert_eq!(tr.snapshot()[&1], RecordingFile { openable: false, stalled: false });
    }

    #[test]
    fn a_late_sidecar_match_within_the_settle_time_still_agrees() {
        let t0 = Instant::now();
        let mut tr = FileTracker::default();
        tr.on_update(&running(Some("/r/a.ts"), &[("BDP", "/r/b.ts")]), th(), t0);
        tr.on_update(&running(Some("/r/a.ts"), &[("BDP", "/r/b.ts"), ("BDN", "/r/a.ts")]), th(), at(t0, 2));
        assert_eq!(tr.trusted_path(1), Some(Path::new("/r/a.ts")));
    }

    #[test]
    fn unverified_recording_logs_once() {
        let t0 = Instant::now();
        let mut tr = FileTracker::default();
        let p = running(None, &[]);
        assert!(tr.on_update(&p, th(), t0).logs.is_empty());
        assert_eq!(tr.on_update(&p, th(), at(t0, 120)).logs.len(), 1);
        assert!(tr.on_update(&p, th(), at(t0, 121)).logs.is_empty());
    }

    #[test]
    fn stats_are_requested_once_per_interval() {
        let t0 = Instant::now();
        let mut tr = agreed(t0);
        let p = running(Some("/r/a.ts"), &[("BDP", "/r/a.ts")]);
        assert_eq!(tr.on_update(&p, th(), at(t0, 1)).stat, None);
        tr.on_stat(1, Stat::Size(10), at(t0, 1));
        assert_eq!(tr.on_update(&p, th(), at(t0, 29)).stat, None);
        assert!(tr.on_update(&p, th(), at(t0, 30)).stat.is_some());
    }

    #[test]
    fn missing_file_stalls_only_after_the_exists_grace() {
        let t0 = Instant::now();
        let mut tr = agreed(t0);
        assert!(tr.on_stat(1, Stat::Missing, at(t0, 59)).logs.is_empty());
        assert!(!tr.snapshot()[&1].stalled);
        let up = tr.on_stat(1, Stat::Missing, at(t0, 60));
        assert!(up.logs[0].contains("does not exist"), "{:?}", up.logs);
        assert!(tr.snapshot()[&1].stalled);
        tr.on_stat(1, Stat::Size(5), at(t0, 70));
        assert_eq!(tr.snapshot()[&1], RecordingFile { openable: true, stalled: false });
    }

    #[test]
    fn a_file_that_never_grows_stalls_after_grace_plus_window() {
        let t0 = Instant::now();
        let mut tr = agreed(t0);
        tr.on_stat(1, Stat::Size(0), at(t0, 1));
        tr.on_stat(1, Stat::Size(0), at(t0, 120));
        assert!(!tr.snapshot()[&1].stalled);
        tr.on_stat(1, Stat::Size(0), at(t0, 180));
        assert!(tr.snapshot()[&1].stalled);
    }

    #[test]
    fn growth_resets_the_stall_window_and_recovery_is_logged() {
        let t0 = Instant::now();
        let mut tr = agreed(t0);
        tr.on_stat(1, Stat::Size(100), at(t0, 30));
        tr.on_stat(1, Stat::Size(200), at(t0, 90));
        assert!(!tr.snapshot()[&1].stalled);
        tr.on_stat(1, Stat::Size(200), at(t0, 200));
        assert!(!tr.snapshot()[&1].stalled);
        let up = tr.on_stat(1, Stat::Size(200), at(t0, 210));
        assert!(up.logs[0].contains("no file growth"), "{:?}", up.logs);
        assert!(tr.snapshot()[&1].stalled);
        let up = tr.on_stat(1, Stat::Size(300), at(t0, 240));
        assert!(up.logs[0].contains("growing again"), "{:?}", up.logs);
        assert!(!tr.snapshot()[&1].stalled);
    }

    #[test]
    fn unreadable_file_counts_as_a_failure_after_the_grace() {
        let t0 = Instant::now();
        let mut tr = agreed(t0);
        let up = tr.on_stat(1, Stat::Failed("denied".into()), at(t0, 61));
        assert!(up.logs[0].contains("cannot read file: denied"), "{:?}", up.logs);
        assert!(tr.snapshot()[&1].stalled);
    }

    #[test]
    fn checks_pause_while_stopping_and_the_stall_clears() {
        let t0 = Instant::now();
        let mut tr = agreed(t0);
        tr.on_stat(1, Stat::Missing, at(t0, 61));
        assert!(tr.snapshot()[&1].stalled);
        let up = tr.on_update(&info(RecorderState::Stopping, Some("/r/a.ts"), &[("BDP", "/r/a.ts")]), th(), at(t0, 62));
        assert_eq!(up.stat, None);
        assert!(!tr.snapshot()[&1].stalled);
        assert!(tr.on_stat(1, Stat::Missing, at(t0, 63)).logs.is_empty());
    }

    #[test]
    fn remove_forgets_the_recording() {
        let t0 = Instant::now();
        let mut tr = agreed(t0);
        tr.remove(1);
        assert_eq!(tr.trusted_path(1), None);
        assert!(tr.snapshot().is_empty());
    }
}
