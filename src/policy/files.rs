//! Per-recording file watch: verifies the output path from two sources, then checks the file exists and keeps growing.

use super::outpath::{self, same_path, SidecarPath};
use crate::api::{ProcessInfo, RecorderId, RecorderState, Site, StreamerId};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System, UpdateKind};

/// How long the two sources may disagree before the recording counts as a conflict.
const CONFLICT_SETTLE: Duration = Duration::from_secs(5);
/// How long a recording may stay unverified before one log line is written.
const PENDING_LOG_AFTER: Duration = Duration::from_secs(120);
/// How often an unverified recording is probed for its two paths.
const PROBE_EVERY: Duration = Duration::from_secs(2);

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
            exists_grace: secs(site.exists_grace_or_default()),
            growth_grace: secs(site.growth_grace_or_default()),
            stall_window: secs(site.stall_window_or_default()),
            interval: secs(site.file_check_or_default()),
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

/// Work for a blocking thread: read ffmpeg's output path and the sidecar, and stat the verified file.
#[derive(Debug)]
pub struct Job {
    pub streamer_id: StreamerId,
    pub pids: Vec<u32>,
    pub stat_path: Option<PathBuf>,
}

#[derive(Debug)]
pub struct JobResult {
    pub ffmpeg_output: Option<PathBuf>,
    pub sidecar: Vec<SidecarPath>,
    pub stat: Option<(PathBuf, Stat)>,
}

/// Runs a job; `fallback_cwd` resolves relative paths when ffmpeg's own working directory is unknown.
pub fn run_job(job: &Job, fallback_cwd: &Path) -> JobResult {
    let (ffmpeg_output, cwd) = ffmpeg_info(&job.pids, fallback_cwd);
    JobResult {
        ffmpeg_output,
        sidecar: outpath::read_sidecar(job.streamer_id, &cwd),
        stat: job.stat_path.as_ref().map(|p| (p.clone(), Stat::of(p))),
    }
}

/// The output file of the first ffmpeg process that names one, and the directory its paths resolve against.
fn ffmpeg_info(pids: &[u32], fallback_cwd: &Path) -> (Option<PathBuf>, PathBuf) {
    if pids.is_empty() {
        return (None, fallback_cwd.to_path_buf());
    }
    let pids: Vec<Pid> = pids.iter().map(|p| Pid::from_u32(*p)).collect();
    let mut sys = System::new();
    let kind = ProcessRefreshKind::nothing().with_cmd(UpdateKind::Always).with_cwd(UpdateKind::Always);
    sys.refresh_processes_specifics(ProcessesToUpdate::Some(&pids), false, kind);
    for pid in &pids {
        let Some(p) = sys.process(*pid) else { continue };
        let cwd = p.cwd().unwrap_or(fallback_cwd);
        if let Some(out) = outpath::ffmpeg_output(p.cmd(), cwd) {
            return (Some(out), cwd.to_path_buf());
        }
    }
    (None, fallback_cwd.to_path_buf())
}

/// Work the policy layer must do after a tracker call.
#[derive(Debug, Default)]
pub struct Update {
    pub logs: Vec<String>,
    pub job: Option<Job>,
}

#[derive(Debug, Clone, PartialEq)]
enum PathCheck {
    Pending,
    Agreed(PathBuf),
    Conflict,
}

struct Entry {
    name: String,
    streamer_id: StreamerId,
    th: Thresholds,
    live: bool,
    check: PathCheck,
    created: Instant,
    unmatched_since: Option<Instant>,
    pending_logged: bool,
    agreed_at: Option<Instant>,
    last_job_at: Option<Instant>,
    in_flight: bool,
    exists: bool,
    last_size: Option<u64>,
    last_growth: Option<Instant>,
    stall: Option<String>,
}

impl Entry {
    fn new(name: &str, streamer_id: StreamerId, th: Thresholds, now: Instant) -> Self {
        Self {
            name: name.to_string(),
            streamer_id,
            th,
            live: true,
            check: PathCheck::Pending,
            created: now,
            unmatched_since: None,
            pending_logged: false,
            agreed_at: None,
            last_job_at: None,
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
    fn verify(&mut self, ffmpeg: Option<&Path>, sidecar: &[SidecarPath], now: Instant, logs: &mut Vec<String>) {
        if self.check == PathCheck::Conflict {
            return;
        }
        let Some(ff) = ffmpeg else { return };
        if matches!(&self.check, PathCheck::Agreed(a) if same_path(a, ff)) {
            self.unmatched_since = None;
            return;
        }
        if sidecar.iter().any(|c| same_path(&c.path, ff)) {
            self.agree(ff.to_path_buf(), now);
            return;
        }
        if sidecar.is_empty() {
            return;
        }
        let since = *self.unmatched_since.get_or_insert(now);
        if now.duration_since(since) >= CONFLICT_SETTLE {
            self.conflict(ffmpeg, sidecar, logs);
        }
    }

    fn agree(&mut self, path: PathBuf, now: Instant) {
        self.check = PathCheck::Agreed(path);
        self.unmatched_since = None;
        self.agreed_at = Some(now);
        self.last_job_at = None;
        self.exists = false;
        self.last_size = None;
        self.last_growth = None;
        self.stall = None;
    }

    fn conflict(&mut self, ffmpeg: Option<&Path>, sidecar: &[SidecarPath], logs: &mut Vec<String>) {
        self.check = PathCheck::Conflict;
        self.exists = false;
        self.stall = None;
        let reported: Vec<_> = sidecar.iter().map(|c| (&c.tag, &c.path)).collect();
        logs.push(format!(
            "warning: {}: recording file paths disagree, file checks are off for this recording (ffmpeg: {:?}, yt-dlp: {:?})",
            self.name, ffmpeg, reported
        ));
    }

    fn note_pending(&mut self, now: Instant, logs: &mut Vec<String>) {
        if !self.pending_logged && now.duration_since(self.created) >= PENDING_LOG_AFTER {
            self.pending_logged = true;
            logs.push(format!(
                "warning: {}: recording file path not verified after {}s, file checks are off for this recording",
                self.name,
                PENDING_LOG_AFTER.as_secs()
            ));
        }
    }

    /// Returns a job when one is due: a probe while unverified, a probe plus file stat once verified.
    fn due_job(&mut self, p: &ProcessInfo, now: Instant) -> Option<Job> {
        if self.check == PathCheck::Conflict || self.in_flight {
            return None;
        }
        let stat_path = self.trusted_path().map(Path::to_path_buf);
        if stat_path.is_none() && p.ffmpeg.is_empty() {
            return None;
        }
        let every = if stat_path.is_some() { self.th.interval } else { PROBE_EVERY };
        if self.last_job_at.is_some_and(|t| now.duration_since(t) < every) {
            return None;
        }
        self.in_flight = true;
        self.last_job_at = Some(now);
        Some(Job { streamer_id: self.streamer_id, pids: p.ffmpeg.iter().map(|f| f.pid).collect(), stat_path })
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
    /// Feeds one process sample and returns the job that is due, if any.
    pub fn on_update(&mut self, p: &ProcessInfo, th: Thresholds, now: Instant) -> Update {
        let e = self.entries.entry(p.id).or_insert_with(|| Entry::new(&p.streamer_name, p.streamer_id, th, now));
        e.th = th;
        e.live = matches!(p.state, RecorderState::Starting | RecorderState::Running);
        let mut up = Update::default();
        if !e.live {
            e.stall = None;
            return up;
        }
        if e.check == PathCheck::Pending {
            e.note_pending(now, &mut up.logs);
        }
        up.job = e.due_job(p, now);
        up
    }

    /// Applies a finished job: verifies the paths, then the stat if it was taken on the still-trusted path.
    pub fn on_job(&mut self, id: RecorderId, res: JobResult, now: Instant) -> Update {
        let mut up = Update::default();
        let Some(e) = self.entries.get_mut(&id) else { return up };
        e.in_flight = false;
        if !e.live {
            return up;
        }
        e.verify(res.ffmpeg_output.as_deref(), &res.sidecar, now, &mut up.logs);
        let current = res
            .stat
            .as_ref()
            .is_some_and(|(path, _)| e.trusted_path().is_some_and(|trusted| same_path(path, trusted)));
        if let (true, Some((_, stat))) = (current, res.stat) {
            e.apply_stat(stat, now, &mut up.logs);
        }
        up
    }

    /// Forgets a recording and returns its streamer.
    pub fn remove(&mut self, id: RecorderId) -> Option<StreamerId> {
        self.entries.remove(&id).map(|e| e.streamer_id)
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
    use crate::api::ProcSample;

    fn th() -> Thresholds {
        Thresholds {
            exists_grace: Duration::from_secs(60),
            growth_grace: Duration::from_secs(60),
            stall_window: Duration::from_secs(120),
            interval: Duration::from_secs(30),
        }
    }

    fn info(state: RecorderState, pids: &[u32]) -> ProcessInfo {
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
            ffmpeg: pids
                .iter()
                .map(|pid| ProcSample { pid: *pid, name: "ffmpeg".into(), cpu_percent: 0.0, mem_bytes: 0 })
                .collect(),
            last_line: String::new(),
            cmdline: String::new(),
        }
    }

    fn running(pids: &[u32]) -> ProcessInfo {
        info(RecorderState::Running, pids)
    }

    fn sidecar(items: &[(&str, &str)]) -> Vec<SidecarPath> {
        items.iter().map(|(tag, path)| SidecarPath { tag: tag.to_string(), path: PathBuf::from(path) }).collect()
    }

    /// A probe result with no stat.
    fn probe(ffmpeg: Option<&str>, items: &[(&str, &str)]) -> JobResult {
        JobResult { ffmpeg_output: ffmpeg.map(PathBuf::from), sidecar: sidecar(items), stat: None }
    }

    /// A result whose paths agree on `/r/a.ts`, carrying a stat of `path`.
    fn stat_of(path: &str, stat: Stat) -> JobResult {
        JobResult {
            ffmpeg_output: Some(PathBuf::from("/r/a.ts")),
            sidecar: sidecar(&[("BDP", "/r/a.ts")]),
            stat: Some((PathBuf::from(path), stat)),
        }
    }

    fn at(t0: Instant, secs: u64) -> Instant {
        t0 + Duration::from_secs(secs)
    }

    /// A tracker whose recording 1 agreed on `/r/a.ts` at `t0`, with no job in flight.
    fn agreed(t0: Instant) -> FileTracker {
        let mut tr = FileTracker::default();
        assert!(tr.on_update(&running(&[10]), th(), t0).job.is_some());
        tr.on_job(1, probe(Some("/r/a.ts"), &[("BDP", "/r/a.ts")]), t0);
        assert_eq!(tr.trusted_path(1), Some(Path::new("/r/a.ts")));
        tr
    }

    #[test]
    fn probing_needs_an_ffmpeg_process_and_repeats_every_two_seconds() {
        let t0 = Instant::now();
        let mut tr = FileTracker::default();
        assert!(tr.on_update(&running(&[]), th(), t0).job.is_none());
        let job = tr.on_update(&running(&[10]), th(), at(t0, 1)).job.expect("probe due");
        assert_eq!((job.streamer_id, job.pids, job.stat_path), (2, vec![10], None));
        assert!(tr.on_update(&running(&[10]), th(), at(t0, 2)).job.is_none());
        tr.on_job(1, probe(None, &[]), at(t0, 2));
        assert!(tr.on_update(&running(&[10]), th(), at(t0, 2)).job.is_none());
        assert!(tr.on_update(&running(&[10]), th(), at(t0, 3)).job.is_some());
    }

    #[test]
    fn agrees_when_any_sidecar_form_matches_ffmpeg() {
        let t0 = Instant::now();
        let mut tr = FileTracker::default();
        tr.on_update(&running(&[10]), th(), t0);
        tr.on_job(1, probe(Some("/r/a.ts"), &[("BDP", "/r/other.ts"), ("BDN", "/r/a.ts")]), t0);
        assert_eq!(tr.trusted_path(1), Some(Path::new("/r/a.ts")));
    }

    #[test]
    fn disagreement_becomes_a_silent_conflict_after_the_settle_time() {
        let t0 = Instant::now();
        let mut tr = FileTracker::default();
        tr.on_update(&running(&[10]), th(), t0);
        let mismatch = || probe(Some("/r/a.ts"), &[("BDP", "/r/b.ts")]);
        assert!(tr.on_job(1, mismatch(), t0).logs.is_empty());
        assert!(tr.on_job(1, mismatch(), at(t0, 4)).logs.is_empty());
        let up = tr.on_job(1, mismatch(), at(t0, 5));
        assert_eq!(up.logs.len(), 1);
        assert!(up.logs[0].starts_with("warning:") && up.logs[0].contains("disagree"), "{:?}", up.logs);
        assert_eq!(tr.trusted_path(1), None);
        assert!(tr.on_job(1, mismatch(), at(t0, 6)).logs.is_empty());
        assert!(tr.on_update(&running(&[10]), th(), at(t0, 60)).job.is_none());
        assert_eq!(tr.snapshot()[&1], RecordingFile { openable: false, stalled: false });
    }

    #[test]
    fn a_late_sidecar_match_within_the_settle_time_still_agrees() {
        let t0 = Instant::now();
        let mut tr = FileTracker::default();
        tr.on_update(&running(&[10]), th(), t0);
        tr.on_job(1, probe(Some("/r/a.ts"), &[("BDP", "/r/b.ts")]), t0);
        tr.on_job(1, probe(Some("/r/a.ts"), &[("BDP", "/r/b.ts"), ("BDN", "/r/a.ts")]), at(t0, 2));
        assert_eq!(tr.trusted_path(1), Some(Path::new("/r/a.ts")));
    }

    #[test]
    fn a_changed_ffmpeg_path_after_agreement_ends_in_conflict() {
        let t0 = Instant::now();
        let mut tr = agreed(t0);
        let moved = || probe(Some("/r/b.ts"), &[("BDP", "/r/a.ts")]);
        assert!(tr.on_job(1, moved(), at(t0, 30)).logs.is_empty());
        let up = tr.on_job(1, moved(), at(t0, 60));
        assert!(up.logs[0].contains("disagree"), "{:?}", up.logs);
        assert_eq!(tr.trusted_path(1), None);
    }

    #[test]
    fn unverified_recording_logs_once() {
        let t0 = Instant::now();
        let mut tr = FileTracker::default();
        let p = running(&[]);
        assert!(tr.on_update(&p, th(), t0).logs.is_empty());
        assert_eq!(tr.on_update(&p, th(), at(t0, 120)).logs.len(), 1);
        assert!(tr.on_update(&p, th(), at(t0, 121)).logs.is_empty());
    }

    #[test]
    fn stats_are_requested_once_per_interval() {
        let t0 = Instant::now();
        let mut tr = agreed(t0);
        let p = running(&[10]);
        let job = tr.on_update(&p, th(), at(t0, 1)).job.expect("stat due");
        assert_eq!(job.stat_path, Some(PathBuf::from("/r/a.ts")));
        assert!(tr.on_update(&p, th(), at(t0, 2)).job.is_none());
        tr.on_job(1, stat_of("/r/a.ts", Stat::Size(10)), at(t0, 2));
        assert!(tr.on_update(&p, th(), at(t0, 30)).job.is_none());
        assert!(tr.on_update(&p, th(), at(t0, 31)).job.is_some());
    }

    #[test]
    fn stats_continue_after_ffmpeg_exits() {
        let t0 = Instant::now();
        let mut tr = agreed(t0);
        assert!(tr.on_update(&running(&[]), th(), at(t0, 1)).job.is_some());
    }

    #[test]
    fn missing_file_stalls_only_after_the_exists_grace() {
        let t0 = Instant::now();
        let mut tr = agreed(t0);
        assert!(tr.on_job(1, stat_of("/r/a.ts", Stat::Missing), at(t0, 59)).logs.is_empty());
        assert!(!tr.snapshot()[&1].stalled);
        let up = tr.on_job(1, stat_of("/r/a.ts", Stat::Missing), at(t0, 60));
        assert!(up.logs[0].contains("does not exist"), "{:?}", up.logs);
        assert!(tr.snapshot()[&1].stalled);
        tr.on_job(1, stat_of("/r/a.ts", Stat::Size(5)), at(t0, 70));
        assert_eq!(tr.snapshot()[&1], RecordingFile { openable: true, stalled: false });
    }

    #[test]
    fn a_file_that_never_grows_stalls_after_grace_plus_window() {
        let t0 = Instant::now();
        let mut tr = agreed(t0);
        tr.on_job(1, stat_of("/r/a.ts", Stat::Size(0)), at(t0, 1));
        tr.on_job(1, stat_of("/r/a.ts", Stat::Size(0)), at(t0, 120));
        assert!(!tr.snapshot()[&1].stalled);
        tr.on_job(1, stat_of("/r/a.ts", Stat::Size(0)), at(t0, 180));
        assert!(tr.snapshot()[&1].stalled);
    }

    #[test]
    fn growth_resets_the_stall_window_and_recovery_is_logged() {
        let t0 = Instant::now();
        let mut tr = agreed(t0);
        let size = |n| stat_of("/r/a.ts", Stat::Size(n));
        tr.on_job(1, size(100), at(t0, 30));
        tr.on_job(1, size(200), at(t0, 90));
        tr.on_job(1, size(200), at(t0, 200));
        assert!(!tr.snapshot()[&1].stalled);
        let up = tr.on_job(1, size(200), at(t0, 210));
        assert!(up.logs[0].contains("no file growth"), "{:?}", up.logs);
        assert!(tr.snapshot()[&1].stalled);
        let up = tr.on_job(1, size(300), at(t0, 240));
        assert!(up.logs[0].contains("growing again"), "{:?}", up.logs);
        assert!(!tr.snapshot()[&1].stalled);
    }

    #[test]
    fn unreadable_file_counts_as_a_failure_after_the_grace() {
        let t0 = Instant::now();
        let mut tr = agreed(t0);
        let up = tr.on_job(1, stat_of("/r/a.ts", Stat::Failed("denied".into())), at(t0, 61));
        assert!(up.logs[0].contains("cannot read file: denied"), "{:?}", up.logs);
        assert!(tr.snapshot()[&1].stalled);
    }

    #[test]
    fn a_stat_of_a_path_that_is_no_longer_trusted_is_ignored() {
        let t0 = Instant::now();
        let mut tr = agreed(t0);
        tr.on_job(1, stat_of("/r/old.ts", Stat::Size(5)), at(t0, 5));
        assert_eq!(tr.snapshot()[&1], RecordingFile { openable: false, stalled: false });
    }

    #[test]
    fn checks_pause_while_stopping_and_the_stall_clears() {
        let t0 = Instant::now();
        let mut tr = agreed(t0);
        tr.on_job(1, stat_of("/r/a.ts", Stat::Missing), at(t0, 61));
        assert!(tr.snapshot()[&1].stalled);
        let up = tr.on_update(&info(RecorderState::Stopping, &[10]), th(), at(t0, 62));
        assert!(up.job.is_none());
        assert!(!tr.snapshot()[&1].stalled);
        assert!(tr.on_job(1, stat_of("/r/a.ts", Stat::Missing), at(t0, 63)).logs.is_empty());
    }

    #[test]
    fn remove_forgets_the_recording_and_names_its_streamer() {
        let t0 = Instant::now();
        let mut tr = agreed(t0);
        assert_eq!(tr.remove(1), Some(2));
        assert_eq!(tr.trusted_path(1), None);
        assert!(tr.snapshot().is_empty());
    }
}
