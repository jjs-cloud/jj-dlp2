//! Engine API: commands in, events out.

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
    r"bin\windows\yt-dlp\yt-dlp.exe"
} else if cfg!(target_os = "linux") {
    "bin/linux/yt-dlp/yt-dlp"
} else {
    "yt-dlp"
};
pub const DEFAULT_INTERVAL_SECS: u64 = 60;
pub const DEFAULT_END_AFTER_MINUTES: u64 = 30;
pub const DEFAULT_FILE_EXISTS_GRACE_SECS: u64 = 60;
pub const DEFAULT_GROWTH_GRACE_SECS: u64 = 60;
pub const DEFAULT_STALL_WINDOW_SECS: u64 = 120;
pub const DEFAULT_FILE_CHECK_INTERVAL_SECS: u64 = 5;
pub const DEFAULT_RESTART_CLEAR_MINUTES: u64 = 1440;
pub const DEFAULT_RESTART_THRESHOLD: u32 = 1;
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
    /// Minutes after the latest restart at which restart warnings clear themselves; 0 keeps them until cleared by hand.
    pub restart_clear_minutes: Option<u64>,
    /// Restarts needed before the RESTARTED chip shows.
    pub restart_threshold: Option<u32>,
    /// Forward recorder output to the policy layer; off saves the resources that forwarding uses.
    pub forward_output: bool,
    /// `|`-separated phrases that mark an ad interruption in recorder output; blank turns the INTERRUPT chip off.
    pub interrupt_patterns: String,
    /// `|`-separated phrases that mark an ffmpeg error in recorder output; blank turns the ERROR chip off.
    pub error_patterns: String,
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
    /// Split each recording into parts of `split_minutes`.
    pub split_enabled: bool,
    /// Minutes per part while splitting is on; blank until set.
    pub split_minutes: Option<u64>,
    /// Name a recording that restarts mid-stream `_partN` and rename the earlier file to match.
    pub auto_suffix: bool,
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

    pub fn restart_clear_or_default(&self) -> u64 {
        self.restart_clear_minutes.unwrap_or(DEFAULT_RESTART_CLEAR_MINUTES)
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
}

/// In-memory view of the config files (settings.json, sites.json, streamers.json, state.json).
#[derive(Debug, Clone, PartialEq)]
pub struct Settings {
    pub next_id: u64,
    pub default_output_dir: String,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct StreamerStatus {
    pub live: bool,
    pub recording: bool,
    /// Unix seconds of the last check that covered this streamer.
    pub last_checked: Option<u64>,
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
    Output { id: StreamerId, line: String },
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
            restart_clear_minutes: None,
            restart_threshold: None,
            forward_output: false,
            interrupt_patterns: String::new(),
            error_patterns: String::new(),
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
            split_enabled,
            split_minutes,
            auto_suffix: true,
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
}
