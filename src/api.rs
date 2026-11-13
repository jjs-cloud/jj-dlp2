//! Engine API: commands in, events out.

use chrono::{NaiveDateTime, NaiveTime};
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc::UnboundedSender;

pub type SiteId = u64;
pub type StreamerId = u64;
pub type RecorderId = u64;

/// How a site's checker output is parsed, chosen from its checker arguments.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CheckOutput {
    /// One JSON object per stdout line (`--dump-json`).
    Json,
    /// Pipe-separated stdout lines (`--simulate --print ...`).
    Simulate,
}

impl CheckOutput {
    /// Detects the format from the arguments; `--dump-json` wins, and `None` means neither flag is present.
    pub fn detect(args: &str) -> Option<Self> {
        let has = |flags: [&str; 2]| args.split_whitespace().any(|t| flags.contains(&t));
        if has(["--dump-json", "-j"]) {
            Some(Self::Json)
        } else if has(["--simulate", "-s"]) {
            Some(Self::Simulate)
        } else {
            None
        }
    }
}

/// Checker arguments used when a site's own are blank.
pub const DEFAULT_CHECKER_ARGS: &str = "--dump-json {urls}";
/// Recorder arguments used when a site's own are blank.
pub const DEFAULT_RECORDER_ARGS: &str = r#"--fixup never --no-part --output "%(title)s.%(ext)s" {url}"#;
/// yt-dlp path used when a site's own is blank: the bundled binary on Windows/Linux, else PATH.
pub const DEFAULT_EXE: &str = if cfg!(windows) {
    r"bin\yt-dlp\windows\yt-dlp.exe"
} else if cfg!(target_os = "linux") {
    "bin/yt-dlp/linux/yt-dlp_linux"
} else {
    "yt-dlp"
};
pub const DEFAULT_INTERVAL_SECS: u64 = 60;
pub const DEFAULT_END_AFTER_MINUTES: u64 = 30;
pub const DEFAULT_FILE_EXISTS_GRACE_SECS: u64 = 60;
pub const DEFAULT_GROWTH_GRACE_SECS: u64 = 60;
pub const DEFAULT_STALL_WINDOW_SECS: u64 = 120;
pub const DEFAULT_FILE_CHECK_INTERVAL_SECS: u64 = 5;
pub const DEFAULT_WARNING_CLEAR_MINUTES: u64 = 1440;
pub const DEFAULT_RESTART_THRESHOLD: u32 = 1;
pub const DEFAULT_QUICK_CHECK_SECS: u64 = 45;
pub const DEFAULT_NOTIFY_COOLDOWN_MINUTES: u64 = 30;
pub const DEFAULT_NOTIFY_TIMEOUT_SECS: u64 = 15;

/// Subfolders recordings are placed in below the output folder.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum FolderLayout {
    Flat,
    #[default]
    Streamer,
    Site,
    StreamerSite,
    SiteStreamer,
}

impl FolderLayout {
    pub const ALL: [FolderLayout; 5] = [
        FolderLayout::Flat,
        FolderLayout::Streamer,
        FolderLayout::Site,
        FolderLayout::StreamerSite,
        FolderLayout::SiteStreamer,
    ];
}

impl std::fmt::Display for FolderLayout {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let sep = std::path::MAIN_SEPARATOR;
        match self {
            FolderLayout::Flat => write!(f, "No subfolder"),
            FolderLayout::Streamer => write!(f, "streamer{sep}"),
            FolderLayout::Site => write!(f, "site{sep}"),
            FolderLayout::StreamerSite => write!(f, "streamer{sep}site{sep}"),
            FolderLayout::SiteStreamer => write!(f, "site{sep}streamer{sep}"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Site {
    pub id: SiteId,
    pub name: String,
    /// Blank uses `DEFAULT_EXE`.
    pub exe_path: String,
    /// Blank (`None`) uses the default for this and the other timing fields below.
    pub interval_secs: Option<u64>,
    /// Blank uses `DEFAULT_CHECKER_ARGS`.
    pub checker_args: String,
    /// Blank uses `DEFAULT_RECORDER_ARGS`.
    pub recorder_args: String,
    /// Stream URL pattern; `{username}` is replaced with the name typed when adding a streamer.
    pub url_template: String,
    /// Empty means use the global output directory.
    pub output_dir: String,
    /// Subfolders recordings are placed in below the output folder.
    pub folder_layout: FolderLayout,
    /// Write the checker's stdout to this site's checker log file.
    pub log_checker_stdout: bool,
    /// Write the checker's stderr to this site's checker log file.
    pub log_checker_stderr: bool,
    /// Minutes without a live sighting after which a stream counts as ended.
    pub end_after_minutes: Option<u64>,
    /// Seconds after the recording file's path is verified before the file must exist.
    pub file_exists_grace_secs: Option<u64>,
    /// Seconds after the path is verified before file growth is checked.
    pub growth_grace_secs: Option<u64>,
    /// Seconds without file growth after which a recording counts as stalled.
    pub stall_window_secs: Option<u64>,
    /// Seconds between checks of the recording file.
    pub file_check_interval_secs: Option<u64>,
    /// Minutes after a warning's latest trigger at which it clears itself; 0 keeps warnings until cleared by hand.
    pub warning_clear_minutes: Option<u64>,
    /// Restarts needed before the RESTARTED chip shows.
    pub restart_threshold: Option<u32>,
    /// Forward recorder output to the policy layer; off saves the resources that forwarding uses.
    pub forward_output: bool,
    /// `|`-separated phrases that mark an ad interruption in recorder output; blank turns the INTERRUPT chip off.
    pub interrupt_patterns: String,
    /// `|`-separated phrases that mark an ffmpeg error in recorder output; blank turns the ERROR chip off.
    pub error_patterns: String,
    /// Count and flag interrupt matches; off ignores the interrupt patterns.
    pub warn_interrupts: bool,
    /// Count and flag error matches; off ignores the error patterns.
    pub warn_errors: bool,
    /// Site colour as `#RRGGBB`; `None` uses the theme's secondary accent.
    pub color: Option<String>,
    /// Master switch for notifications about this site's streamers.
    pub notify: bool,
    /// Show a native desktop notification.
    pub notify_desktop: bool,
    /// Send an ntfy notification to `ntfy_topic`.
    pub notify_ntfy: bool,
    /// Topic name used for ntfy notifications.
    pub ntfy_topic: String,
    /// Minutes before the same notification kind can be sent again for a streamer; blank uses the default.
    pub notify_cooldown_minutes: Option<u64>,
    /// Seconds a desktop notification stays up; blank uses the default.
    pub notify_timeout_secs: Option<u64>,
    /// Hold the live notification until the recording file is confirmed growing.
    pub notify_confirmed: bool,
    /// Notify when a recording is flagged as stalled.
    pub notify_stalled: bool,
    /// Notify when a recording is flagged as restarted.
    pub notify_restarted: bool,
    /// Also notify when a streamer is already live at their first check after launch.
    pub notify_on_launch: bool,
    /// Notify when a streamer goes live.
    #[serde(default = "default_true")]
    pub notify_live: bool,
    /// Master switch for showing this site's warning chips.
    #[serde(default = "default_true")]
    pub warnings_enabled: bool,
    /// Notify when recorder output matches an interrupt pattern.
    #[serde(default)]
    pub notify_interrupts: bool,
    /// Notify when recorder output matches an error pattern.
    #[serde(default)]
    pub notify_errors: bool,
    /// Show the STALLED chip for this site's recordings.
    #[serde(default)]
    pub warn_stalled: bool,
    /// Show the RESTARTED chip for this site's streamers.
    #[serde(default)]
    pub warn_restarts: bool,
    /// Split each recording into parts of `split_minutes`.
    pub split_enabled: bool,
    /// Minutes per part while splitting is on; blank until set.
    pub split_minutes: Option<u64>,
    /// Name a recording that restarts mid-stream `_partN` and rename the earlier file to match.
    pub auto_suffix: bool,
    /// Skip checks for this site's streamers; recordings already running continue.
    pub paused: bool,
    /// Restart a recording once per live span when the stream offers a higher quality than is being recorded.
    #[serde(default)]
    pub upgrade_quality: bool,
    /// Re-check a streamer alone shortly after its recording ends, to catch a quick restart.
    #[serde(default)]
    pub quick_check: bool,
    /// Seconds to wait after a recording ends before the quick check; blank uses the default.
    #[serde(default)]
    pub quick_check_secs: Option<u64>,
    /// Stop and restart a recording that has stalled.
    #[serde(default = "default_true")]
    pub restart_stalled: bool,
}

/// Serde default for fields that are on unless a saved file says otherwise.
fn default_true() -> bool {
    true
}

/// `text` trimmed, or `fallback` when it is blank.
fn or_default<'a>(text: &'a str, fallback: &'a str) -> &'a str {
    let t = text.trim();
    if t.is_empty() { fallback } else { t }
}

impl Site {
    pub fn exe_or_default(&self) -> &str {
        or_default(&self.exe_path, DEFAULT_EXE)
    }

    pub fn checker_or_default(&self) -> &str {
        or_default(&self.checker_args, DEFAULT_CHECKER_ARGS)
    }

    pub fn recorder_or_default(&self) -> &str {
        or_default(&self.recorder_args, DEFAULT_RECORDER_ARGS)
    }

    pub fn interval_or_default(&self) -> u64 {
        self.interval_secs.unwrap_or(DEFAULT_INTERVAL_SECS)
    }

    pub fn end_after_or_default(&self) -> u64 {
        self.end_after_minutes.unwrap_or(DEFAULT_END_AFTER_MINUTES)
    }

    pub fn exists_grace_or_default(&self) -> u64 {
        self.file_exists_grace_secs.unwrap_or(DEFAULT_FILE_EXISTS_GRACE_SECS)
    }

    pub fn growth_grace_or_default(&self) -> u64 {
        self.growth_grace_secs.unwrap_or(DEFAULT_GROWTH_GRACE_SECS)
    }

    pub fn stall_window_or_default(&self) -> u64 {
        self.stall_window_secs.unwrap_or(DEFAULT_STALL_WINDOW_SECS)
    }

    pub fn file_check_or_default(&self) -> u64 {
        self.file_check_interval_secs.unwrap_or(DEFAULT_FILE_CHECK_INTERVAL_SECS)
    }

    pub fn quick_check_or_default(&self) -> u64 {
        self.quick_check_secs.unwrap_or(DEFAULT_QUICK_CHECK_SECS)
    }

    pub fn warning_clear_or_default(&self) -> u64 {
        self.warning_clear_minutes.unwrap_or(DEFAULT_WARNING_CLEAR_MINUTES)
    }

    pub fn restart_threshold_or_default(&self) -> u32 {
        self.restart_threshold.unwrap_or(DEFAULT_RESTART_THRESHOLD)
    }

    pub fn notify_cooldown_or_default(&self) -> u64 {
        self.notify_cooldown_minutes.unwrap_or(DEFAULT_NOTIFY_COOLDOWN_MINUTES)
    }

    pub fn notify_timeout_or_default(&self) -> u64 {
        self.notify_timeout_secs.unwrap_or(DEFAULT_NOTIFY_TIMEOUT_SECS)
    }

    /// Part length streamers inherit; `None` when splitting is off or no length is set.
    pub fn inherited_split_minutes(&self) -> Option<u64> {
        if self.split_enabled { self.split_minutes.filter(|m| *m > 0) } else { None }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Streamer {
    pub id: StreamerId,
    pub site_id: SiteId,
    pub name: String,
    pub url: String,
    pub enabled: bool,
    pub options: StreamerOptions,
    /// When the schedule last switched this streamer on or off; owned by the policy layer.
    pub schedule_runs: ScheduleRuns,
}

/// Per-streamer options edited from the streamer's settings panel.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct StreamerOptions {
    /// Write the recorder's stdout to this streamer's log file.
    pub log_stdout: bool,
    /// Write the recorder's stderr to this streamer's log file.
    pub log_stderr: bool,
    /// Overrides the site's subfolder layout; `None` inherits it.
    pub folder_layout: Option<FolderLayout>,
    /// Overrides the site's notification switch; `None` inherits it.
    pub notify: Option<bool>,
    /// Wait `intro_delay_minutes` after the stream goes live before recording.
    pub intro_delay_enabled: bool,
    /// Minutes to wait when the intro delay is enabled; 0 means no delay.
    pub intro_delay_minutes: u64,
    /// Record from the start and split the intro into its own part instead of waiting; `None` is off.
    pub intro_delay_split: Option<bool>,
    /// Overrides the site's split switch; `None` inherits it.
    pub split: Option<bool>,
    /// Minutes per part when splitting is on; blank uses the site's.
    pub split_minutes: Option<u64>,
    /// Overrides the site's auto-suffix switch; `None` inherits it.
    pub auto_suffix: Option<bool>,
    /// Automatic enabling and disabling of the streamer.
    pub schedule: Schedule,
    /// Records even when the recording limit is reached; owned by the Priority tab.
    #[serde(default)]
    pub always_record: bool,
    /// Overrides the site's upgrade-quality switch; `None` inherits it.
    #[serde(default)]
    pub upgrade_quality: Option<bool>,
    /// Overrides the site's quick-check switch; `None` inherits it.
    #[serde(default)]
    pub quick_check: Option<bool>,
}

/// Whether a schedule runs once or repeats weekly.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum ScheduleMode {
    #[default]
    OneOff,
    Recurring,
}

impl ScheduleMode {
    pub const ALL: [ScheduleMode; 2] = [ScheduleMode::OneOff, ScheduleMode::Recurring];
}

impl std::fmt::Display for ScheduleMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ScheduleMode::OneOff => write!(f, "One-off"),
            ScheduleMode::Recurring => write!(f, "Recurring"),
        }
    }
}

/// Times, in local time, between which a streamer is switched on and off.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Schedule {
    pub enabled: bool,
    pub mode: ScheduleMode,
    pub one_off_start: Option<NaiveDateTime>,
    pub one_off_end: Option<NaiveDateTime>,
    /// Weekdays a recurring schedule runs on, Monday first.
    pub days: [bool; 7],
    pub start_time: Option<NaiveTime>,
    pub end_time: Option<NaiveTime>,
}

/// The last time a schedule switched a streamer on and off.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ScheduleRuns {
    pub last_enable: Option<NaiveDateTime>,
    pub last_disable: Option<NaiveDateTime>,
}

impl StreamerOptions {
    /// Minutes per part for this streamer; `None` means its recordings are not split.
    pub fn effective_split_minutes(&self, site: &Site) -> Option<u64> {
        match self.split {
            None => site.inherited_split_minutes(),
            Some(false) => None,
            Some(true) => self.split_minutes.filter(|m| *m > 0).or_else(|| site.inherited_split_minutes()),
        }
    }

    /// Minutes the intro is recorded as its own part, when the intro delay splits instead of waiting.
    pub fn intro_split_minutes(&self) -> Option<u64> {
        let on = self.intro_delay_enabled && self.intro_delay_split == Some(true);
        (on && self.intro_delay_minutes > 0).then_some(self.intro_delay_minutes)
    }

    /// Whether a restarted recording of this streamer gets a `_partN` suffix.
    pub fn effective_auto_suffix(&self, site: &Site) -> bool {
        self.auto_suffix.unwrap_or(site.auto_suffix)
    }

    /// Whether this streamer's recording is restarted when the stream offers a higher quality.
    pub fn effective_upgrade_quality(&self, site: &Site) -> bool {
        self.upgrade_quality.unwrap_or(site.upgrade_quality)
    }

    /// Whether this streamer is re-checked on its own shortly after its recording ends.
    pub fn effective_quick_check(&self, site: &Site) -> bool {
        self.quick_check.unwrap_or(site.quick_check)
    }
}

/// Notification and warning settings that apply to every site.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct GlobalNotify {
    /// Sends no notification of any kind.
    pub disable_notifications: bool,
    /// Shows no warning chips.
    pub disable_warnings: bool,
    /// ntfy topic used by sites whose own topic is blank.
    pub ntfy_topic: String,
}

/// In-memory view of the config files (settings.json, sites.json, streamers.json, state.json).
#[derive(Debug, Clone, PartialEq)]
pub struct Settings {
    pub next_id: u64,
    pub default_output_dir: String,
    /// Most streamers recorded at once; `None` is unlimited.
    pub max_recordings: Option<u32>,
    /// Global notification and warning switches and the fallback ntfy topic.
    pub notifications: GlobalNotify,
    pub sites: Vec<Site>,
    pub streamers: Vec<Streamer>,
}

/// Who decides when the engine starts recording.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Control {
    /// The engine starts a recorder whenever an enabled streamer is live.
    Autonomous,
    /// The engine starts a recorder only when sent `StartRecording`.
    Managed,
}

#[derive(Debug, Clone)]
pub enum Command {
    /// Re-reads the config files and applies them; keeps the current settings if the file is invalid.
    Reload,
    /// Starts recording a live streamer that has no recorder running.
    StartRecording { id: StreamerId },
    /// Starts an extra recorder for a streamer that is already recording, for a split.
    StartSegment { id: StreamerId },
    /// Stops one segment's recorder, leaving the streamer's other recorders running.
    StopSegment { id: RecorderId },
    /// Stops a streamer's recorders; an autonomous engine starts them again at the next live check.
    StopRecording { id: StreamerId },
    /// Runs the site's checker for this streamer alone and applies the result to its status.
    QuickCheck { id: StreamerId },
    /// Replaces a site's checker_args until cleared with `None`.
    #[allow(dead_code)]
    SetCheckerOverride { site_id: SiteId, args: Option<String> },
    /// Replaces a streamer's recorder_args for new recordings until cleared with `None`.
    #[allow(dead_code)]
    SetRecorderOverride { streamer_id: StreamerId, args: Option<String> },
    /// Re-runs the setup checks and clears recording problems shown for streamers.
    Recheck,
    /// Stops all recorders gracefully, then ends the engine.
    Shutdown,
}

/// A video's resolution and frame rate; the frame rate is in thousandths of a frame per second.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Quality {
    pub width: u32,
    pub height: u32,
    pub fps_milli: Option<u32>,
}

impl Quality {
    /// A quality from a frame rate in frames per second; an unusable rate becomes `None`.
    pub fn new(width: u32, height: u32, fps: Option<f64>) -> Self {
        let fps_milli = fps.filter(|f| f.is_finite() && *f > 0.0 && *f < 1000.0).map(|f| (f * 1000.0).round() as u32);
        Self { width, height, fps_milli }
    }

    pub fn area(&self) -> u64 {
        u64::from(self.width) * u64::from(self.height)
    }

    /// The frame rate rounded to whole frames per second.
    pub fn fps_rounded(&self) -> Option<u32> {
        self.fps_milli.map(|m| m.saturating_add(500) / 1000)
    }
}

impl std::fmt::Display for Quality {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}x{}", self.width, self.height)?;
        match self.fps_rounded() {
            Some(fps) => write!(f, "@{fps}"),
            None => Ok(()),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct StreamerStatus {
    pub live: bool,
    pub recording: bool,
    /// Unix seconds of the last check that covered this streamer.
    pub last_checked: Option<u64>,
    /// The latest check could not be trusted for this streamer, so it can't prove the stream ended.
    pub check_failed: bool,
    /// Quality of the stream's best format in the latest check; `None` when not live or not reported.
    pub quality: Option<Quality>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecorderState {
    Starting,
    Running,
    Stopping,
    /// Root process exited; waiting for every tracked process to be gone.
    Exited(Option<i32>),
}

#[derive(Debug, Clone, PartialEq)]
pub struct ProcSample {
    pub pid: u32,
    pub name: String,
    pub cpu_percent: f32,
    pub mem_bytes: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ProcessInfo {
    pub id: RecorderId,
    pub streamer_id: StreamerId,
    pub streamer_name: String,
    pub site_name: String,
    pub state: RecorderState,
    pub uptime_secs: u64,
    /// The process we spawned (the PyInstaller bootloader when one is present).
    pub root: Option<ProcSample>,
    /// The real yt-dlp process; equals `root` when there is no bootloader.
    pub ytdlp: Option<ProcSample>,
    pub has_bootloader: bool,
    pub ffmpeg: Vec<ProcSample>,
    pub last_line: String,
    pub cmdline: String,
}

/// A setup problem and how to fix it; `command` is shown with a copy button.
#[derive(Debug, Clone, PartialEq)]
pub struct Notice {
    pub title: String,
    pub steps: String,
    pub command: Option<String>,
}

#[derive(Debug, Clone)]
pub enum Event {
    StreamerStatus { id: StreamerId, status: StreamerStatus },
    SiteChecked { id: SiteId, at: u64, ok: bool, problem: Option<String> },
    /// A setup problem blocking a streamer's recording; `None` clears it.
    StreamerIssue { id: StreamerId, issue: Option<String> },
    /// Setup problems found before anything runs; empty means all clear.
    Preflight(Vec<Notice>),
    ProcessUpdate(ProcessInfo),
    ProcessRemoved(RecorderId),
    /// One line of a recorder's output; sent only for sites with output forwarding on.
    Output { id: StreamerId, recorder: RecorderId, line: String },
    Log(String),
}

/// Most log lines the UI can keep; the saved UI log holds at least this many.
pub const LOG_HISTORY_MAX: usize = 5000;

/// Cheap handle for sending commands to the engine.
#[derive(Clone)]
pub struct EngineClient {
    tx: UnboundedSender<Command>,
}

impl EngineClient {
    pub fn new(tx: UnboundedSender<Command>) -> Self {
        Self { tx }
    }

    pub fn send(&self, cmd: Command) {
        let _ = self.tx.send(cmd);
    }
}

pub fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn site_with(split_enabled: bool, split_minutes: Option<u64>) -> Site {
        Site {
            id: 1,
            name: "s".into(),
            exe_path: String::new(),
            interval_secs: None,
            checker_args: String::new(),
            recorder_args: String::new(),
            url_template: String::new(),
            output_dir: String::new(),
            folder_layout: FolderLayout::default(),
            log_checker_stdout: false,
            log_checker_stderr: false,
            end_after_minutes: None,
            file_exists_grace_secs: None,
            growth_grace_secs: None,
            stall_window_secs: None,
            file_check_interval_secs: None,
            warning_clear_minutes: None,
            restart_threshold: None,
            forward_output: false,
            interrupt_patterns: String::new(),
            error_patterns: String::new(),
            warn_interrupts: true,
            warn_errors: true,
            color: None,
            notify: false,
            notify_desktop: false,
            notify_ntfy: false,
            ntfy_topic: String::new(),
            notify_cooldown_minutes: None,
            notify_timeout_secs: None,
            notify_confirmed: false,
            notify_stalled: false,
            notify_restarted: false,
            notify_on_launch: false,
            notify_live: true,
            warnings_enabled: true,
            notify_interrupts: false,
            notify_errors: false,
            warn_stalled: true,
            warn_restarts: true,
            split_enabled,
            split_minutes,
            auto_suffix: true,
            paused: false,
            upgrade_quality: false,
            quick_check: false,
            quick_check_secs: None,
            restart_stalled: true,
        }
    }

    #[test]
    fn streamer_split_inherits_overrides_or_turns_off() {
        let site = site_with(true, Some(60));
        let opts = |split, minutes| StreamerOptions { split, split_minutes: minutes, ..Default::default() };
        assert_eq!(opts(None, Some(10)).effective_split_minutes(&site), Some(60));
        assert_eq!(opts(Some(false), Some(10)).effective_split_minutes(&site), None);
        assert_eq!(opts(Some(true), Some(10)).effective_split_minutes(&site), Some(10));
        assert_eq!(opts(Some(true), None).effective_split_minutes(&site), Some(60));
    }

    #[test]
    fn split_needs_a_length_from_somewhere() {
        let off = site_with(false, Some(60));
        let blank = site_with(true, None);
        let on = StreamerOptions { split: Some(true), ..Default::default() };
        assert_eq!(StreamerOptions::default().effective_split_minutes(&off), None);
        assert_eq!(StreamerOptions::default().effective_split_minutes(&blank), None);
        assert_eq!(on.effective_split_minutes(&off), None);
    }

    #[test]
    fn intro_split_needs_the_delay_on_a_length_and_the_switch() {
        let opts = |enabled, minutes, split| StreamerOptions {
            intro_delay_enabled: enabled,
            intro_delay_minutes: minutes,
            intro_delay_split: split,
            ..Default::default()
        };
        assert_eq!(opts(true, 5, Some(true)).intro_split_minutes(), Some(5));
        assert_eq!(opts(false, 5, Some(true)).intro_split_minutes(), None);
        assert_eq!(opts(true, 0, Some(true)).intro_split_minutes(), None);
        assert_eq!(opts(true, 5, None).intro_split_minutes(), None);
    }

    #[test]
    fn auto_suffix_follows_site_unless_overridden() {
        let site = site_with(false, None);
        assert!(StreamerOptions::default().effective_auto_suffix(&site));
        let off = StreamerOptions { auto_suffix: Some(false), ..Default::default() };
        assert!(!off.effective_auto_suffix(&site));
    }

    #[test]
    fn output_kind_is_detected_from_flags() {
        assert_eq!(CheckOutput::detect("--dump-json {urls}"), Some(CheckOutput::Json));
        assert_eq!(CheckOutput::detect("-s --print x {urls}"), Some(CheckOutput::Simulate));
        assert_eq!(CheckOutput::detect("--simulate --dump-json {urls}"), Some(CheckOutput::Json));
        assert_eq!(CheckOutput::detect("--cookies-from-browser firefox {urls}"), None);
    }

    #[test]
    fn site_deserializes_without_the_newer_notification_fields() {
        let mut value = serde_json::to_value(site_with(false, None)).unwrap();
        let map = value.as_object_mut().unwrap();
        for key in ["notify_interrupts", "notify_errors", "warn_stalled", "warn_restarts", "upgrade_quality"] {
            map.remove(key);
        }
        let site: Site = serde_json::from_value(value).unwrap();
        assert!(!site.notify_interrupts && !site.notify_errors && !site.warn_stalled && !site.warn_restarts && !site.upgrade_quality);
    }

    #[test]
    fn site_without_restart_stalled_keeps_it_on() {
        let mut value = serde_json::to_value(site_with(false, None)).unwrap();
        value.as_object_mut().unwrap().remove("restart_stalled");
        let site: Site = serde_json::from_value(value).unwrap();
        assert!(site.restart_stalled);
    }

    #[test]
    fn site_without_live_and_warning_switches_keeps_them_on() {
        let mut value = serde_json::to_value(site_with(false, None)).unwrap();
        for key in ["notify_live", "warnings_enabled"] {
            value.as_object_mut().unwrap().remove(key);
        }
        let site: Site = serde_json::from_value(value).unwrap();
        assert!(site.notify_live && site.warnings_enabled);
    }

    #[test]
    fn global_notify_fills_missing_fields_with_defaults() {
        let g: GlobalNotify = serde_json::from_str("{}").unwrap();
        assert_eq!(g, GlobalNotify::default());
    }

    #[test]
    fn upgrade_quality_follows_site_unless_overridden() {
        let mut site = site_with(false, None);
        assert!(!StreamerOptions::default().effective_upgrade_quality(&site));
        site.upgrade_quality = true;
        assert!(StreamerOptions::default().effective_upgrade_quality(&site));
        let off = StreamerOptions { upgrade_quality: Some(false), ..Default::default() };
        assert!(!off.effective_upgrade_quality(&site));
        site.upgrade_quality = false;
        let on = StreamerOptions { upgrade_quality: Some(true), ..Default::default() };
        assert!(on.effective_upgrade_quality(&site));
    }

    #[test]
    fn quick_check_follows_site_unless_overridden() {
        let mut site = site_with(false, None);
        assert!(!StreamerOptions::default().effective_quick_check(&site));
        site.quick_check = true;
        assert!(StreamerOptions::default().effective_quick_check(&site));
        let off = StreamerOptions { quick_check: Some(false), ..Default::default() };
        assert!(!off.effective_quick_check(&site));
        site.quick_check = false;
        let on = StreamerOptions { quick_check: Some(true), ..Default::default() };
        assert!(on.effective_quick_check(&site));
    }

    #[test]
    fn quick_check_delay_defaults_when_blank() {
        let mut site = site_with(false, None);
        assert_eq!(site.quick_check_or_default(), DEFAULT_QUICK_CHECK_SECS);
        site.quick_check_secs = Some(12);
        assert_eq!(site.quick_check_or_default(), 12);
    }

    #[test]
    fn old_settings_deserialize_without_the_quick_check_fields() {
        let mut value = serde_json::to_value(site_with(false, None)).unwrap();
        for key in ["quick_check", "quick_check_secs"] {
            value.as_object_mut().unwrap().remove(key);
        }
        let site: Site = serde_json::from_value(value).unwrap();
        assert!(!site.quick_check && site.quick_check_secs.is_none());
        let mut opts = serde_json::to_value(StreamerOptions::default()).unwrap();
        opts.as_object_mut().unwrap().remove("quick_check");
        let opts: StreamerOptions = serde_json::from_value(opts).unwrap();
        assert_eq!(opts.quick_check, None);
    }

    #[test]
    fn quality_rounds_and_prints_its_frame_rate() {
        assert_eq!(Quality::new(1920, 1080, Some(60.039)).to_string(), "1920x1080@60");
        assert_eq!(Quality::new(1280, 720, Some(29.97)).fps_rounded(), Some(30));
        assert_eq!(Quality::new(640, 1280, None).to_string(), "640x1280");
        assert_eq!(Quality::new(640, 1280, Some(0.0)).fps_milli, None);
        assert_eq!(Quality::new(1920, 1080, None).area(), 2_073_600);
    }

    #[test]
    fn streamer_options_deserializes_without_upgrade_quality() {
        let mut value = serde_json::to_value(StreamerOptions::default()).unwrap();
        value.as_object_mut().unwrap().remove("upgrade_quality");
        let opts: StreamerOptions = serde_json::from_value(value).unwrap();
        assert_eq!(opts.upgrade_quality, None);
    }

    #[test]
    fn streamer_options_deserializes_without_always_record() {
        let mut value = serde_json::to_value(StreamerOptions::default()).unwrap();
        value.as_object_mut().unwrap().remove("always_record");
        let opts: StreamerOptions = serde_json::from_value(value).unwrap();
        assert!(!opts.always_record);
    }
}
