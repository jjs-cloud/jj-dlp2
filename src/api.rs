//! Engine API: commands in, events out.

use serde::{Deserialize, Serialize};
use tokio::sync::mpsc::UnboundedSender;

pub type SiteId = u64;
pub type StreamerId = u64;
pub type RecorderId = u64;

/// How a site's checker output is interpreted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum CheckMode {
    /// One JSON object per stdout line (`--dump-json`).
    #[default]
    DumpJson,
    /// Pipe-separated stdout lines (`--simulate --print ...`).
    Simulate,
}

impl CheckMode {
    pub const ALL: [CheckMode; 2] = [CheckMode::DumpJson, CheckMode::Simulate];
}

impl std::fmt::Display for CheckMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CheckMode::DumpJson => write!(f, "--dump-json"),
            CheckMode::Simulate => write!(f, "--simulate"),
        }
    }
}

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
    pub exe_path: String,
    pub interval_secs: u64,
    pub check_mode: CheckMode,
    pub checker_args: String,
    pub recorder_args: String,
    /// Stream URL pattern; `{username}` is replaced with the name typed when adding a streamer.
    pub url_template: String,
    /// Empty means use the global output directory.
    pub output_dir: String,
    /// Subfolders recordings are placed in below the output folder.
    #[serde(default)]
    pub folder_layout: FolderLayout,
    /// Write the checker's stdout to this site's checker log file.
    #[serde(default)]
    pub log_checker_stdout: bool,
    /// Write the checker's stderr to this site's checker log file.
    #[serde(default)]
    pub log_checker_stderr: bool,
    /// Minutes without a live sighting after which a stream counts as ended.
    pub end_after_minutes: u64,
    /// Seconds after the recording file's path is verified before the file must exist.
    pub file_exists_grace_secs: u64,
    /// Seconds after the path is verified before file growth is checked.
    pub growth_grace_secs: u64,
    /// Seconds without file growth after which a recording counts as stalled.
    pub stall_window_secs: u64,
    /// Seconds between checks of the recording file.
    pub file_check_interval_secs: u64,
    /// Minutes after the latest restart at which restart warnings clear themselves; 0 keeps them until cleared by hand.
    pub restart_clear_minutes: u64,
    /// Restarts needed before the RESTARTED chip shows.
    pub restart_threshold: u32,
    /// Site colour as `#RRGGBB`; `None` uses the theme's secondary accent.
    pub color: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Streamer {
    pub id: StreamerId,
    pub site_id: SiteId,
    pub name: String,
    pub url: String,
    pub enabled: bool,
    #[serde(default)]
    pub options: StreamerOptions,
}

/// Per-streamer options edited from the streamer's settings panel.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct StreamerOptions {
    /// Write the recorder's stdout to this streamer's log file.
    pub log_stdout: bool,
    /// Write the recorder's stderr to this streamer's log file.
    pub log_stderr: bool,
    /// Overrides the site's subfolder layout; `None` inherits it.
    pub folder_layout: Option<FolderLayout>,
}

/// In-memory view of the config files (settings.json, sites.json, streamers.json, state.json).
#[derive(Debug, Clone, PartialEq)]
pub struct Settings {
    pub next_id: u64,
    pub default_output_dir: String,
    pub sites: Vec<Site>,
    pub streamers: Vec<Streamer>,
}

#[derive(Debug, Clone)]
pub enum Command {
    /// Re-reads the config files and applies them; keeps the current settings if the file is invalid.
    Reload,
    /// Starts or stops recording a streamer without changing the config files.
    SetStreamerEnabled { id: StreamerId, enabled: bool },
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
