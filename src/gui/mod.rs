//! The iced GUI. It only talks to the policy layer through `crate::policy`.

mod drives;
mod files;
mod files_view;
mod logs;
mod logs_view;
#[cfg(target_os = "linux")]
mod desktop;
mod posts;
mod posts_view;
pub mod prefs;
mod priority;
mod priority_view;
mod skin;
mod view;

use crate::api::*;
use chrono::{Local, NaiveDate, NaiveDateTime, NaiveTime};
use crate::policy::disks;
use crate::policy::notify::{self, Targets};
use crate::policy::uilog;
use crate::policy::update::UpdateConfig;
use crate::policy::{
    new_site, preset_for_name, NewSite, PolicyClient, PolicyCommand, PolicyEvent, Preset, RecordingFile, UpdateEvent,
    Warnings, DEFAULT_SITE_COLOR,
};
use iced::futures::{SinkExt, Stream};
use iced::{window, Size, Subscription, Task, Theme};
use skin::{Skin, ThemeKind};
use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{Duration, Instant};
use tokio::sync::mpsc::UnboundedReceiver;

/// Quick-pick colours shown under the site colour field.
pub const SITE_SWATCHES: [&str; 10] = [
    "#9146FF", "#FF0000", "#F89C1C", "#53FC18", "#25F4EE", "#FF2BD6", "#FFD24A", "#7AA2F7", "#F38BA8", "#B8FFC8",
];

/// Draft key for the unsaved blank site; real site ids start at 1.
pub const NEW_SITE_ID: SiteId = 0;

static EVENTS: Mutex<Option<UnboundedReceiver<PolicyEvent>>> = Mutex::new(None);
/// Latest window size, read when the app exits.
static LAST_SIZE: Mutex<Option<(f32, f32)>> = Mutex::new(None);

/// Hands the policy event stream to the GUI subscription.
pub fn install_event_stream(rx: UnboundedReceiver<PolicyEvent>) {
    *EVENTS.lock().unwrap() = Some(rx);
}

pub fn run(client: PolicyClient) -> iced::Result {
    let (win_w, win_h) = prefs::startup_window_size();
    #[cfg(target_os = "linux")]
    desktop::install();
    let result = iced::application(move || App::new(client.clone()), App::update, App::view)
        .title("jj-dlp")
        .subscription(App::subscription)
        .theme(App::theme)
        .window(window_settings(win_w, win_h))
        .antialiasing(true)
        .run();
    save_size_on_exit();
    result
}

/// The main window's settings, with the Linux app id that links it to its launcher entry.
fn window_settings(w: f32, h: f32) -> window::Settings {
    #[allow(unused_mut)]
    let mut settings = window::Settings { size: Size::new(w, h), position: window::Position::Centered, icon: app_icon(), ..Default::default() };
    #[cfg(target_os = "linux")]
    {
        settings.platform_specific.application_id = desktop::APP_ID.to_string();
    }
    settings
}

/// Writes the last window size to gui.json when remembering is on.
fn save_size_on_exit() {
    if !prefs::load_remember_size() {
        return;
    }
    if let Some((w, h)) = *LAST_SIZE.lock().unwrap() {
        prefs::save_window_size(w, h);
    }
}

/// Loads the embedded 128x128 RGBA window icon.
fn app_icon() -> Option<window::Icon> {
    window::icon::from_rgba(include_bytes!("../../assets/icon_128.rgba").to_vec(), 128, 128).ok()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    Livestreams,
    Posts,
    Processes,
    Files,
    Settings,
    Log,
}

/// Sub-pages of the Log tab.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogTab {
    Engine,
    Output,
}

/// Sub-pages of the Settings tab.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsTab {
    Sites,
    Posts,
    Priority,
    General,
}

/// Where the update banner is in its flow.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UpdateStage {
    Idle,
    Confirm,
    Installing,
}

/// A release the policy layer found.
#[derive(Debug, Clone)]
pub struct UpdateInfo {
    pub version: String,
    pub is_rc: bool,
}

/// Editable copy of a site's settings, kept as strings while the user types.
#[derive(Debug, Clone, PartialEq)]
pub struct SiteDraft {
    pub name: String,
    pub exe: String,
    pub interval: String,
    pub end_after: String,
    pub restart_clear: String,
    pub restart_threshold: String,
    pub forward_output: bool,
    pub interrupt_patterns: String,
    pub error_patterns: String,
    pub warn_interrupts: bool,
    pub warn_errors: bool,
    pub file_exists_grace: String,
    pub growth_grace: String,
    pub stall_window: String,
    pub file_check_interval: String,
    pub checker: String,
    pub recorder: String,
    pub url_template: String,
    pub output: String,
    pub folder_layout: FolderLayout,
    pub log_stdout: bool,
    pub log_stderr: bool,
    pub use_theme_color: bool,
    pub color: String,
    pub notify: bool,
    pub notify_desktop: bool,
    pub notify_ntfy: bool,
    pub ntfy_topic: String,
    pub notify_cooldown: String,
    pub notify_timeout: String,
    pub notify_confirmed: bool,
    pub notify_stalled: bool,
    pub notify_restarted: bool,
    pub notify_on_launch: bool,
    pub split_enabled: bool,
    pub split_minutes: String,
    pub auto_suffix: bool,
    pub paused: bool,
}

impl SiteDraft {
    fn from_site(s: &Site) -> Self {
        Self {
            name: s.name.clone(),
            exe: s.exe_path.clone(),
            interval: opt_text(s.interval_secs),
            end_after: opt_text(s.end_after_minutes),
            restart_clear: opt_text(s.restart_clear_minutes),
            restart_threshold: opt_text(s.restart_threshold),
            forward_output: s.forward_output,
            interrupt_patterns: s.interrupt_patterns.clone(),
            error_patterns: s.error_patterns.clone(),
            warn_interrupts: s.warn_interrupts,
            warn_errors: s.warn_errors,
            file_exists_grace: opt_text(s.file_exists_grace_secs),
            growth_grace: opt_text(s.growth_grace_secs),
            stall_window: opt_text(s.stall_window_secs),
            file_check_interval: opt_text(s.file_check_interval_secs),
            checker: s.checker_args.clone(),
            recorder: s.recorder_args.clone(),
            url_template: s.url_template.clone(),
            output: s.output_dir.clone(),
            folder_layout: s.folder_layout,
            log_stdout: s.log_checker_stdout,
            log_stderr: s.log_checker_stderr,
            use_theme_color: s.color.is_none(),
            color: s.color.clone().unwrap_or_else(|| DEFAULT_SITE_COLOR.to_string()),
            notify: s.notify,
            notify_desktop: s.notify_desktop,
            notify_ntfy: s.notify_ntfy,
            ntfy_topic: s.ntfy_topic.clone(),
            notify_cooldown: opt_text(s.notify_cooldown_minutes),
            notify_timeout: opt_text(s.notify_timeout_secs),
            notify_confirmed: s.notify_confirmed,
            notify_stalled: s.notify_stalled,
            notify_restarted: s.notify_restarted,
            notify_on_launch: s.notify_on_launch,
            split_enabled: s.split_enabled,
            split_minutes: opt_text(s.split_minutes),
            auto_suffix: s.auto_suffix,
            paused: s.paused,
        }
    }

    /// Draft holding the current defaults for a new site, keeping `name`.
    fn from_defaults(name: &str, preset: Preset) -> Self {
        let n = new_site(preset);
        Self {
            name: name.to_string(),
            exe: n.exe_path,
            interval: opt_text(n.interval_secs),
            end_after: opt_text(n.end_after_minutes),
            restart_clear: opt_text(n.restart_clear_minutes),
            restart_threshold: opt_text(n.restart_threshold),
            forward_output: n.forward_output,
            interrupt_patterns: n.interrupt_patterns,
            error_patterns: n.error_patterns,
            warn_interrupts: n.warn_interrupts,
            warn_errors: n.warn_errors,
            file_exists_grace: opt_text(n.file_exists_grace_secs),
            growth_grace: opt_text(n.growth_grace_secs),
            stall_window: opt_text(n.stall_window_secs),
            file_check_interval: opt_text(n.file_check_interval_secs),
            checker: n.checker_args,
            recorder: n.recorder_args,
            url_template: n.url_template,
            output: n.output_dir,
            folder_layout: n.folder_layout,
            log_stdout: n.log_checker_stdout,
            log_stderr: n.log_checker_stderr,
            use_theme_color: n.color.is_none(),
            color: n.color.unwrap_or_else(|| DEFAULT_SITE_COLOR.to_string()),
            notify: n.notify,
            notify_desktop: n.notify_desktop,
            notify_ntfy: n.notify_ntfy,
            ntfy_topic: n.ntfy_topic,
            notify_cooldown: opt_text(n.notify_cooldown_minutes),
            notify_timeout: opt_text(n.notify_timeout_secs),
            notify_confirmed: n.notify_confirmed,
            notify_stalled: n.notify_stalled,
            notify_restarted: n.notify_restarted,
            notify_on_launch: n.notify_on_launch,
            split_enabled: n.split_enabled,
            split_minutes: opt_text(n.split_minutes),
            auto_suffix: n.auto_suffix,
            paused: n.paused,
        }
    }

    /// True when the theme colour is selected or the hex text parses.
    pub fn color_ok(&self) -> bool {
        self.use_theme_color || skin::parse_hex(&self.color).is_some()
    }

    /// The colour to store: `None` for the theme colour, else normalised `#RRGGBB`.
    pub fn site_color(&self) -> Option<String> {
        (!self.use_theme_color).then(|| format!("#{}", self.color.trim().trim_start_matches('#').to_uppercase()))
    }

    pub fn interval_secs(&self) -> Option<Option<u64>> {
        parse_opt(&self.interval, 5)
    }

    pub fn end_after_minutes(&self) -> Option<Option<u64>> {
        parse_opt(&self.end_after, 1)
    }

    /// Minutes of quiet after which restart warnings clear; 0 turns auto-clear off.
    pub fn restart_clear_minutes(&self) -> Option<Option<u64>> {
        parse_opt(&self.restart_clear, 0)
    }

    pub fn restart_threshold(&self) -> Option<Option<u32>> {
        parse_opt(&self.restart_threshold, 1)
    }

    pub fn file_exists_grace_secs(&self) -> Option<Option<u64>> {
        parse_opt(&self.file_exists_grace, 1)
    }

    pub fn growth_grace_secs(&self) -> Option<Option<u64>> {
        parse_opt(&self.growth_grace, 1)
    }

    pub fn stall_window_secs(&self) -> Option<Option<u64>> {
        parse_opt(&self.stall_window, 1)
    }

    pub fn file_check_interval_secs(&self) -> Option<Option<u64>> {
        parse_opt(&self.file_check_interval, 5)
    }

    /// Minutes before the same notification can repeat; 0 turns the cooldown off.
    pub fn notify_cooldown_minutes(&self) -> Option<Option<u64>> {
        parse_opt(&self.notify_cooldown, 0)
    }

    pub fn notify_timeout_secs(&self) -> Option<Option<u64>> {
        parse_opt(&self.notify_timeout, 1)
    }

    /// Minutes per part; blank is `Some(None)`, anything but a whole number of at least 1 is `None`.
    pub fn split_minutes(&self) -> Option<Option<u64>> {
        parse_opt(&self.split_minutes, 1)
    }

    /// True while the split length is unusable: unreadable, or blank with splitting on.
    pub fn split_invalid(&self) -> bool {
        match self.split_minutes() {
            None => true,
            Some(None) => self.split_enabled,
            Some(Some(_)) => false,
        }
    }

    /// False while ntfy is on without a usable topic.
    pub fn ntfy_ok(&self) -> bool {
        !self.notify_ntfy || notify::topic_valid(&self.ntfy_topic)
    }

    /// The channels the draft has switched on, used by the test button.
    pub fn test_targets(&self) -> Targets {
        Targets {
            desktop_timeout_secs: self
                .notify_desktop
                .then(|| self.notify_timeout_secs().flatten().unwrap_or(DEFAULT_NOTIFY_TIMEOUT_SECS)),
            ntfy_topic: (self.notify_ntfy && notify::topic_valid(&self.ntfy_topic))
                .then(|| self.ntfy_topic.trim().to_string()),
        }
    }

    /// The site this draft describes, or `None` while the name or URL template is blank or a field is invalid.
    pub fn to_new_site(&self) -> Option<NewSite> {
        if self.name.trim().is_empty()
            || self.url_template.trim().is_empty()
            || !self.color_ok()
            || !self.ntfy_ok()
            || self.split_invalid()
        {
            return None;
        }
        Some(NewSite {
            name: self.name.trim().to_string(),
            exe_path: self.exe.trim().to_string(),
            interval_secs: self.interval_secs()?,
            checker_args: self.checker.trim().to_string(),
            recorder_args: self.recorder.trim().to_string(),
            url_template: self.url_template.trim().to_string(),
            output_dir: self.output.trim().to_string(),
            folder_layout: self.folder_layout,
            log_checker_stdout: self.log_stdout,
            log_checker_stderr: self.log_stderr,
            end_after_minutes: self.end_after_minutes()?,
            file_exists_grace_secs: self.file_exists_grace_secs()?,
            growth_grace_secs: self.growth_grace_secs()?,
            stall_window_secs: self.stall_window_secs()?,
            file_check_interval_secs: self.file_check_interval_secs()?,
            restart_clear_minutes: self.restart_clear_minutes()?,
            restart_threshold: self.restart_threshold()?,
            forward_output: self.forward_output,
            interrupt_patterns: self.interrupt_patterns.trim().to_string(),
            error_patterns: self.error_patterns.trim().to_string(),
            warn_interrupts: self.warn_interrupts,
            warn_errors: self.warn_errors,
            color: self.site_color(),
            notify: self.notify,
            notify_desktop: self.notify_desktop,
            notify_ntfy: self.notify_ntfy,
            ntfy_topic: self.ntfy_topic.trim().to_string(),
            notify_cooldown_minutes: self.notify_cooldown_minutes()?,
            notify_timeout_secs: self.notify_timeout_secs()?,
            notify_confirmed: self.notify_confirmed,
            notify_stalled: self.notify_stalled,
            notify_restarted: self.notify_restarted,
            notify_on_launch: self.notify_on_launch,
            split_enabled: self.split_enabled,
            split_minutes: self.split_minutes()?,
            auto_suffix: self.auto_suffix,
            paused: self.paused,
        })
    }
}

/// A number's text, or blank for `None`.
fn opt_text<T: ToString>(v: Option<T>) -> String {
    v.map(|x| x.to_string()).unwrap_or_default()
}

/// Parses an optional whole number: blank is `Some(None)`, invalid or below `min` is `None`.
fn parse_opt<T: std::str::FromStr + PartialOrd>(text: &str, min: T) -> Option<Option<T>> {
    let t = text.trim();
    if t.is_empty() {
        return Some(None);
    }
    t.parse().ok().filter(|v| *v >= min).map(Some)
}

#[derive(Debug, Clone)]
pub enum DraftField {
    Name(String),
    Exe(String),
    Interval(String),
    EndAfter(String),
    RestartClear(String),
    RestartThreshold(String),
    ForwardOutput(bool),
    InterruptPatterns(String),
    ErrorPatterns(String),
    WarnInterrupts(bool),
    WarnErrors(bool),
    FileExistsGrace(String),
    GrowthGrace(String),
    StallWindow(String),
    FileCheckInterval(String),
    Checker(String),
    Recorder(String),
    UrlTemplate(String),
    Output(String),
    Paused(bool),
    FolderLayout(FolderLayout),
    LogStdout(bool),
    LogStderr(bool),
    Color(String),
    UseThemeColor(bool),
    PickColor(String),
    Notify(bool),
    NotifyDesktop(bool),
    NotifyNtfy(bool),
    NtfyTopic(String),
    NotifyCooldown(String),
    NotifyTimeout(String),
    NotifyConfirmed(bool),
    NotifyStalled(bool),
    NotifyRestarted(bool),
    NotifyOnLaunch(bool),
    SplitEnabled(bool),
    SplitMinutes(String),
    AutoSuffix(bool),
}

/// A single field edited in a streamer's settings panel.
#[derive(Debug, Clone)]
pub enum StreamerField {
    LogStdout(bool),
    LogStderr(bool),
    FolderLayout(Option<FolderLayout>),
    Notify(Option<bool>),
    AutoSuffix(Option<bool>),
    Split(Option<bool>),
    SplitMinutes(String),
    IntroEnabled(bool),
    IntroSplit(bool),
    IntroMinutes(String),
    ScheduleEnabled(bool),
    ScheduleMode(ScheduleMode),
    ScheduleDay(usize, bool),
    OneOffStartDate(NaiveDate),
    OneOffStartTime(NaiveTime),
    OneOffEndDate(NaiveDate),
    OneOffEndTime(NaiveTime),
    RecurringStart(NaiveTime),
    RecurringEnd(NaiveTime),
}

/// A date or time picker in a streamer's schedule that can be open.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PickerTarget {
    OneOffStartDate,
    OneOffStartTime,
    OneOffEndDate,
    OneOffEndTime,
    RecurringStart,
    RecurringEnd,
}

/// Why an enabled schedule cannot be saved.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScheduleIssue {
    Incomplete,
    EndNotAfterStart,
    NoDays,
    SameTime,
}

impl ScheduleIssue {
    pub fn text(self) -> &'static str {
        match self {
            ScheduleIssue::Incomplete => "Pick a start and an end",
            ScheduleIssue::EndNotAfterStart => "End must be after start",
            ScheduleIssue::NoDays => "Select at least one day",
            ScheduleIssue::SameTime => "Start and end must differ",
        }
    }
}

/// Unsaved edits to one streamer's options.
#[derive(Debug, Clone, PartialEq)]
pub struct StreamerDraft {
    pub log_stdout: bool,
    pub log_stderr: bool,
    pub folder_layout: Option<FolderLayout>,
    pub notify: Option<bool>,
    pub auto_suffix: Option<bool>,
    pub split: Option<bool>,
    pub split_minutes: String,
    pub intro_enabled: bool,
    pub intro_split: bool,
    pub intro_minutes: String,
    pub schedule: Schedule,
}

/// `current` with its date replaced, keeping its time of day (midnight when unset).
fn with_date(current: Option<NaiveDateTime>, date: NaiveDate) -> NaiveDateTime {
    date.and_time(current.map_or(NaiveTime::MIN, |c| c.time()))
}

/// `current` with its time of day replaced, keeping its date (today when unset).
fn with_time(current: Option<NaiveDateTime>, time: NaiveTime) -> NaiveDateTime {
    current.map_or_else(|| Local::now().date_naive(), |c| c.date()).and_time(time)
}

impl StreamerDraft {
    pub fn from_options(o: &StreamerOptions) -> Self {
        Self {
            log_stdout: o.log_stdout,
            log_stderr: o.log_stderr,
            folder_layout: o.folder_layout,
            notify: o.notify,
            auto_suffix: o.auto_suffix,
            split: o.split,
            split_minutes: opt_text(o.split_minutes),
            intro_enabled: o.intro_delay_enabled,
            intro_split: o.intro_delay_split == Some(true),
            intro_minutes: if o.intro_delay_minutes == 0 { String::new() } else { o.intro_delay_minutes.to_string() },
            schedule: o.schedule.clone(),
        }
    }

    pub fn to_options(&self) -> StreamerOptions {
        StreamerOptions {
            log_stdout: self.log_stdout,
            log_stderr: self.log_stderr,
            folder_layout: self.folder_layout,
            notify: self.notify,
            auto_suffix: self.auto_suffix,
            split: self.split,
            split_minutes: self.split_minutes.trim().parse().ok().filter(|m| *m > 0),
            intro_delay_enabled: self.intro_enabled,
            intro_delay_minutes: self.intro_minutes.trim().parse().unwrap_or(0),
            intro_delay_split: self.intro_split.then_some(true),
            schedule: self.schedule.clone(),
            always_record: false,
        }
    }

    /// The first problem that stops an enabled schedule from being saved.
    pub fn schedule_issue(&self) -> Option<ScheduleIssue> {
        let s = &self.schedule;
        if !s.enabled {
            return None;
        }
        match s.mode {
            ScheduleMode::OneOff => match (s.one_off_start, s.one_off_end) {
                (Some(a), Some(b)) => (b <= a).then_some(ScheduleIssue::EndNotAfterStart),
                _ => Some(ScheduleIssue::Incomplete),
            },
            ScheduleMode::Recurring => {
                if !s.days.contains(&true) {
                    return Some(ScheduleIssue::NoDays);
                }
                match (s.start_time, s.end_time) {
                    (Some(a), Some(b)) => (a == b).then_some(ScheduleIssue::SameTime),
                    _ => Some(ScheduleIssue::Incomplete),
                }
            }
        }
    }

    /// True while the intro delay is on without a length of at least 1 minute.
    pub fn intro_invalid(&self) -> bool {
        self.intro_enabled && !matches!(parse_opt::<u64>(&self.intro_minutes, 1), Some(Some(_)))
    }

    /// True while the split length is unusable: unreadable, or blank with splitting on.
    pub fn split_invalid(&self) -> bool {
        match parse_opt::<u64>(&self.split_minutes, 1) {
            None => true,
            Some(None) => self.split == Some(true),
            Some(Some(_)) => false,
        }
    }

    pub fn invalid(&self) -> bool {
        self.intro_invalid() || self.split_invalid() || self.schedule_issue().is_some()
    }

    /// True when saving would change `saved`.
    pub fn differs_from(&self, saved: &StreamerOptions) -> bool {
        self.to_options() != *saved
    }
}

#[derive(Debug, Clone)]
pub enum Msg {
    Policy(PolicyEvent),
    Tick(#[allow(dead_code)] Instant),
    Anim(#[allow(dead_code)] Instant),
    SetTab(Tab),
    Files(files::FilesMsg),
    Posts(posts::PostsMsg),
    Priority(priority::PriorityMsg),
    SetSettingsTab(SettingsTab),
    SetLogTab(LogTab),
    Logs(logs::LogsMsg),
    ThemeChosen(ThemeKind),
    OpenConfigDir,
    OpenLogsDir,
    OpenRecording(RecorderId),
    NewStreamerName(String),
    NewStreamerUsername(String),
    PickStreamerSite(SiteTarget),
    AddStreamer,
    RemoveStreamer(StreamerId),
    ToggleStreamer(StreamerId, bool),
    ToggleStreamerSettings(StreamerId),
    StreamerEdit(StreamerId, StreamerField),
    OpenSchedulePicker(StreamerId, PickerTarget),
    CloseSchedulePicker,
    SchedulePicked(StreamerId, StreamerField),
    SaveStreamer(StreamerId),
    RevertStreamer(StreamerId),
    DefaultsStreamer(StreamerId),
    AddSite(Preset),
    RemoveSite(SiteId),
    ConfirmRemoveSite(SiteId),
    BrowseSiteOutput(SiteId),
    SiteOutputChosen(SiteId, Option<std::path::PathBuf>),
    OpenSiteOutput(SiteId),
    CancelRemoveSite,
    Draft(SiteId, DraftField),
    SaveSite(SiteId),
    RevertSite(SiteId),
    DefaultsSite(SiteId),
    TestNotify(SiteId),
    GlobalOutEdit(String),
    GlobalOutSave,
    BrowseGlobalOut,
    GlobalOutChosen(Option<PathBuf>),
    OpenGlobalOut,
    Recheck,
    ClearWarnings,
    CopyText(String),
    ToggleProc(RecorderId),
    SetWrapOutput(bool),
    CopyOutput(RecorderId),
    SetCompact(bool),
    SetRowMin(f32),
    SetRowMax(f32),
    SaveRowRange,
    SetTwoColWidth(f32),
    SaveTwoColWidth,
    SetThreeColWidth(f32),
    SaveThreeColWidth,
    SetNameClip(f32),
    SaveNameClip,
    SetLogLines(f32),
    SaveLogLines,
    SetRememberSize(bool),
    ResetWindowSize,
    ResetRowRange,
    Resized(Size),
    UpdateClicked,
    UpdateCancel,
    UpdateConfirm,
    CheckUpdateNow,
    SetUpdateChecks(bool),
    SetIncludeRc(bool),
    StartFresh,
    AddDrive(PathBuf),
    RemoveDrive(PathBuf),
    MoveDrive(PathBuf, isize),
    DiskShowPercent(bool),
    DiskShowFree(bool),
    DiskLowGb(String),
    DiskLowPercent(String),
    DisksRefreshed(disks::Snapshot),
}

/// Most output lines kept per recording.
const OUTPUT_MAX_LINES: usize = 500;

/// A recording's newest output lines, newline-separated.
#[derive(Debug, Default)]
pub struct OutputLog {
    pub text: String,
    lines: usize,
}

impl OutputLog {
    /// Appends a line, dropping the oldest once over `OUTPUT_MAX_LINES`.
    fn push(&mut self, line: &str) {
        if self.lines > 0 {
            self.text.push('\n');
        }
        self.text.push_str(line);
        self.lines += 1;
        if self.lines > OUTPUT_MAX_LINES {
            let cut = self.text.find('\n').map_or(self.text.len(), |i| i + 1);
            self.text.drain(..cut);
            self.lines -= 1;
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct CheckInfo {
    pub at: u64,
    pub ok: bool,
    pub problem: Option<String>,
}

/// Widget id of the "Streamer username" input.
pub(crate) fn username_input_id() -> iced::widget::Id {
    iced::widget::Id::new("streamer-username")
}

/// What the add-streamer dropdown can point at: an existing site, or a preset that has no site yet.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SiteTarget {
    Site(SiteId),
    Template(Preset),
}

/// One entry of the add-streamer dropdown.
#[derive(Debug, Clone, PartialEq)]
pub struct SiteOption {
    pub target: SiteTarget,
    pub name: String,
}

impl std::fmt::Display for SiteOption {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.name)
    }
}

/// Scrollable id of the Posts settings site list.
pub(crate) fn posts_scroll_id() -> iced::widget::Id {
    iced::widget::Id::new("posts-sites-scroll")
}

/// Scrollable id of the Sites settings list.
pub(crate) fn sites_scroll_id() -> iced::widget::Id {
    iced::widget::Id::new("sites-scroll")
}

pub struct App {
    pending_jump: Option<String>,
    client: PolicyClient,
    settings: Settings,
    kind: ThemeKind,
    tab: Tab,
    statuses: HashMap<StreamerId, StreamerStatus>,
    restarts: HashMap<StreamerId, u32>,
    warnings: HashMap<StreamerId, Warnings>,
    live_since: HashMap<StreamerId, u64>,
    last_live: HashMap<StreamerId, u64>,
    resolutions: HashMap<StreamerId, (u32, u32)>,
    checks: HashMap<SiteId, CheckInfo>,
    issues: HashMap<StreamerId, String>,
    notices: Vec<Notice>,
    processes: BTreeMap<RecorderId, ProcessInfo>,
    recording_files: HashMap<RecorderId, RecordingFile>,
    log: VecDeque<String>,
    drafts: HashMap<SiteId, SiteDraft>,
    new_streamer: (String, String),
    add_site: Option<SiteTarget>,
    open_streamer_settings: HashSet<StreamerId>,
    confirm_remove_site: Option<SiteId>,
    schedule_picker: Option<(StreamerId, PickerTarget)>,
    streamer_drafts: HashMap<StreamerId, StreamerDraft>,
    settings_tab: SettingsTab,
    log_tab: LogTab,
    logs: logs::LogsState,
    site_added: Option<(String, u64)>,
    global_out: String,
    phase: f32,
    now: u64,
    expanded_procs: HashSet<RecorderId>,
    output: HashMap<RecorderId, OutputLog>,
    wrap_output: bool,
    compact: bool,
    row_min: f32,
    row_max: f32,
    two_col_w: f32,
    three_col_w: f32,
    name_clip: f32,
    log_lines: f32,
    remember_size: bool,
    win_w: f32,
    win_h: f32,
    update: Option<UpdateInfo>,
    update_stage: UpdateStage,
    update_error: Option<String>,
    update_cfg: UpdateConfig,
    files: files::FilesState,
    posts: posts::PostsState,
    priority: priority::PriorityState,
    disks: disks::Snapshot,
    extra_drives: Vec<PathBuf>,
    drive_order: Vec<PathBuf>,
    disk_limits: disks::Limits,
    low_gb_text: String,
    low_percent_text: String,
    default_out: String,
    disks_dirty: bool,
    disks_busy: bool,
    disks_at: u64,
}

impl App {
    /// The folder the global output field points at; the default when blank.
    fn global_out_dir(&self) -> &str {
        let dir = self.global_out.trim();
        if dir.is_empty() { &self.default_out } else { dir }
    }

    fn new(client: PolicyClient) -> (App, Task<Msg>) {
        client.send(PolicyCommand::RequestSnapshot);
        let (row_min, row_max) = prefs::load_row_range();
        let (win_w, win_h) = prefs::startup_window_size();
        let mut app = App {
            client,
            settings: crate::settings::default_settings(),
            kind: prefs::load_theme(),
            tab: Tab::Livestreams,
            statuses: HashMap::new(),
            restarts: HashMap::new(),
            warnings: HashMap::new(),
            live_since: HashMap::new(),
            last_live: HashMap::new(),
            resolutions: HashMap::new(),
            checks: HashMap::new(),
            issues: HashMap::new(),
            notices: Vec::new(),
            processes: BTreeMap::new(),
            recording_files: HashMap::new(),
            log: VecDeque::new(),
            drafts: HashMap::new(),
            new_streamer: (String::new(), String::new()),
            add_site: None,
            pending_jump: None,
            open_streamer_settings: HashSet::new(),
            confirm_remove_site: None,
            schedule_picker: None,
            streamer_drafts: HashMap::new(),
            settings_tab: SettingsTab::Sites,
            log_tab: LogTab::Engine,
            logs: logs::LogsState::new(),
            site_added: None,
            global_out: String::new(),
            phase: 0.0,
            now: now_secs(),
            expanded_procs: HashSet::new(),
            output: HashMap::new(),
            wrap_output: prefs::load_wrap_output(),
            compact: prefs::load_compact(),
            row_min,
            row_max,
            two_col_w: prefs::load_two_col_width(),
            three_col_w: prefs::load_three_col_width(),
            name_clip: prefs::load_name_clip(),
            log_lines: prefs::load_log_lines(),
            remember_size: prefs::load_remember_size(),
            win_w,
            win_h,
            update: None,
            update_stage: UpdateStage::Idle,
            update_error: None,
            update_cfg: UpdateConfig::default(),
            files: files::FilesState::new(),
            posts: posts::PostsState::new(),
            priority: priority::PriorityState::default(),
            disks: disks::Snapshot::default(),
            extra_drives: prefs::load_extra_drives().into_iter().map(PathBuf::from).collect(),
            drive_order: prefs::load_drive_order().into_iter().map(PathBuf::from).collect(),
            disk_limits: prefs::load_disk_limits(),
            low_gb_text: String::new(),
            low_percent_text: String::new(),
            default_out: crate::settings::default_output_dir(),
            disks_dirty: true,
            disks_busy: false,
            disks_at: 0,
        };
        disks::set_limits(app.disk_limits);
        if let Some(e) = crate::policy::update::take_last_error() {
            app.push_log(e);
        }
        (app, Task::none())
    }

    fn skin(&self) -> Skin {
        Skin::new(self.kind)
    }

    fn theme(&self) -> Theme {
        self.skin().theme()
    }

    fn subscription(&self) -> Subscription<Msg> {
        Subscription::batch([
            Subscription::run(engine_stream),
            iced::time::every(Duration::from_secs(1)).map(Msg::Tick),
            if self.animating() { iced::time::every(Duration::from_millis(100)).map(Msg::Anim) } else { Subscription::none() },
            window::resize_events().map(on_resize),
            if self.tab == Tab::Files { iced::keyboard::listen().map(files::key_msg) } else { Subscription::none() },
        ])
    }

    /// Jumps to a newly added site's editor on a later tick, once the editor is laid out.
    fn try_jump(&mut self) -> Option<Task<Msg>> {
        let name = self.pending_jump.clone()?;
        let task = self.jump_to_site(&name)?;
        self.pending_jump = None;
        self.now = now_secs();
        self.site_added = Some((name, self.now));
        Some(task)
    }

    /// The length a streamer inherits from its site, if splitting is on there.
    pub(crate) fn inherited_split(&self, st: &Streamer) -> Option<u64> {
        self.settings.sites.iter().find(|s| s.id == st.site_id).and_then(Site::inherited_split_minutes)
    }

    /// Applies one edit to a streamer's draft.
    fn edit_streamer(&mut self, id: StreamerId, field: StreamerField) {
        let Some(st) = self.settings.streamers.iter().find(|s| s.id == id) else { return };
        let d = self.streamer_drafts.entry(id).or_insert_with(|| StreamerDraft::from_options(&st.options));
        let digits = |v: &str| v.len() <= 6 && v.chars().all(|c| c.is_ascii_digit());
        match field {
            StreamerField::LogStdout(v) => d.log_stdout = v,
            StreamerField::LogStderr(v) => d.log_stderr = v,
            StreamerField::FolderLayout(v) => d.folder_layout = v,
            StreamerField::Notify(v) => d.notify = v,
            StreamerField::AutoSuffix(v) => d.auto_suffix = v,
            StreamerField::Split(v) => {
                d.split = v;
                if v != Some(true) {
                    d.split_minutes.clear();
                }
            }
            StreamerField::SplitMinutes(v) if digits(&v) => {
                d.split = if v.is_empty() { None } else { Some(true) };
                d.split_minutes = v;
            }
            StreamerField::IntroEnabled(v) => d.intro_enabled = v,
            StreamerField::IntroSplit(v) => d.intro_split = v,
            StreamerField::IntroMinutes(v) if digits(&v) => {
                if !v.is_empty() {
                    d.intro_enabled = true;
                }
                d.intro_minutes = v;
            }
            StreamerField::SplitMinutes(_) | StreamerField::IntroMinutes(_) => {}
            StreamerField::ScheduleEnabled(v) => d.schedule.enabled = v,
            StreamerField::ScheduleMode(v) => d.schedule.mode = v,
            StreamerField::ScheduleDay(i, v) => {
                if let Some(day) = d.schedule.days.get_mut(i) {
                    *day = v;
                }
            }
            StreamerField::OneOffStartDate(v) => d.schedule.one_off_start = Some(with_date(d.schedule.one_off_start, v)),
            StreamerField::OneOffStartTime(v) => d.schedule.one_off_start = Some(with_time(d.schedule.one_off_start, v)),
            StreamerField::OneOffEndDate(v) => d.schedule.one_off_end = Some(with_date(d.schedule.one_off_end, v)),
            StreamerField::OneOffEndTime(v) => d.schedule.one_off_end = Some(with_time(d.schedule.one_off_end, v)),
            StreamerField::RecurringStart(v) => d.schedule.start_time = Some(v),
            StreamerField::RecurringEnd(v) => d.schedule.end_time = Some(v),
        }
    }

    /// Sends a streamer's draft to the policy layer unless it is invalid.
    fn save_streamer(&self, id: StreamerId) {
        let Some(d) = self.streamer_drafts.get(&id) else { return };
        if d.invalid() {
            return;
        }
        self.client.send(PolicyCommand::SetStreamerOptions { id, options: d.to_options() });
    }

    /// The add-streamer dropdown entries: each named preset (its site once created), then custom sites.
    pub(crate) fn site_options(&self) -> Vec<SiteOption> {
        let mut out: Vec<SiteOption> = Vec::new();
        for preset in Preset::NAMED {
            let name = new_site(preset).name;
            let found = self.settings.sites.iter().find(|s| s.name.eq_ignore_ascii_case(&name));
            out.push(match found {
                Some(site) => SiteOption { target: SiteTarget::Site(site.id), name: site.name.clone() },
                None => SiteOption { target: SiteTarget::Template(preset), name },
            });
        }
        for site in &self.settings.sites {
            if !out.iter().any(|o| o.target == SiteTarget::Site(site.id)) {
                out.push(SiteOption { target: SiteTarget::Site(site.id), name: site.name.clone() });
            }
        }
        out
    }

    /// The dropdown entry streamers are added to: the picked one, else the first.
    pub(crate) fn add_choice(&self) -> Option<SiteOption> {
        let opts = self.site_options();
        let picked = self.add_site.and_then(|t| match t {
            SiteTarget::Site(_) => opts.iter().find(|o| o.target == t),
            SiteTarget::Template(p) => {
                let name = new_site(p).name;
                opts.iter().find(|o| o.target == t || o.name.eq_ignore_ascii_case(&name))
            }
        });
        let choice = picked.or_else(|| opts.first()).cloned();
        choice
    }

    /// Scrolls the Sites list to the named site, if it exists.
    fn jump_to_site(&self, name: &str) -> Option<Task<Msg>> {
        let sites = &self.settings.sites;
        let idx = sites.iter().position(|s| s.name.eq_ignore_ascii_case(name))?;
        let y = if sites.len() > 1 { idx as f32 / (sites.len() - 1) as f32 } else { 0.0 };
        Some(iced::widget::operation::snap_to(
            sites_scroll_id(),
            iced::widget::scrollable::RelativeOffset { x: 0.0, y },
        ))
    }

    fn push_log(&mut self, line: String) {
        self.log.push_front(format!("{}  {line}", uilog::stamp()));
        self.log.truncate(self.log_lines as usize);
    }

    fn update(&mut self, msg: Msg) -> Task<Msg> {
        match msg {
            Msg::Policy(ev) => {
                if matches!(ev, PolicyEvent::Update(UpdateEvent::Restarting)) {
                    return iced::exit();
                }
                self.on_event(ev)
            }
            Msg::Anim(_) => {
                self.phase = (self.phase + 0.04) % 1.0;
                if let Some(task) = self.try_jump() {
                    return task;
                }
            }
            Msg::Tick(_) => {
                self.now = now_secs();
                if !self.animating() {
                    self.phase = 0.25;
                }
                if let Some(task) = self.try_jump() {
                    return task;
                }
                let posts = Task::batch([self.posts_tick(), self.disks_tick()]);
                if self.tab == Tab::Files {
                    return Task::batch([posts, self.files_tick()]);
                }
                if self.tab == Tab::Log && self.log_tab == LogTab::Output {
                    return Task::batch([posts, self.logs_tick()]);
                }
                return posts;
            }
            Msg::SetTab(t) => {
                // Collapse all processes when leaving the Processes tab.
                if self.tab == Tab::Processes && t != Tab::Processes {
                    self.expanded_procs.clear();
                }
                self.tab = t;
                if t == Tab::Livestreams {
                    // Collapse any open streamer settings on entering Livestreams.
                    self.open_streamer_settings.clear();
                    self.schedule_picker = None;
                }
                if t == Tab::Files {
                    self.files.on_enter();
                }
                if t == Tab::Log && self.log_tab == LogTab::Output {
                    return self.logs_tick();
                }
            }
            Msg::Files(m) => return self.files_update(m),
            Msg::Logs(m) => return self.logs_update(m),
            Msg::Posts(m) => return self.posts_update(m),
            Msg::Priority(m) => self.priority_update(m),
            Msg::SetSettingsTab(w) => self.settings_tab = w,
            Msg::SetLogTab(w) => {
                self.log_tab = w;
                if w == LogTab::Output {
                    return self.logs_tick();
                }
            }
            Msg::ThemeChosen(k) => {
                self.kind = k;
                prefs::save_theme(k);
            }
            Msg::AddDrive(mount) => self.add_drive(mount),
            Msg::RemoveDrive(mount) => self.remove_drive(&mount),
            Msg::MoveDrive(mount, delta) => self.move_drive(&mount, delta),
            Msg::DiskShowPercent(on) => self.set_disk_limits(disks::Limits { show_percent: on, ..self.disk_limits }),
            Msg::DiskShowFree(on) => self.set_disk_limits(disks::Limits { show_free: on, ..self.disk_limits }),
            Msg::DiskLowGb(v) => self.edit_low_gb(v),
            Msg::DiskLowPercent(v) => self.edit_low_percent(v),
            Msg::DisksRefreshed(snap) => self.on_disks(snap),
            Msg::OpenConfigDir => self.client.send(PolicyCommand::OpenConfigDir),
            Msg::OpenLogsDir => self.client.send(PolicyCommand::OpenLogsDir),
            Msg::OpenRecording(id) => self.client.send(PolicyCommand::OpenRecording(id)),
            Msg::NewStreamerName(v) => self.new_streamer.0 = v,
            Msg::NewStreamerUsername(v) => self.new_streamer.1 = v,
            Msg::PickStreamerSite(target) => self.add_site = Some(target),
            Msg::AddStreamer => {
                if let Some(choice) = self.add_choice() {
                    let (name, username) = std::mem::take(&mut self.new_streamer);
                    if username.trim().is_empty() {
                        self.new_streamer = (name, username);
                    } else {
                        self.add_site = Some(choice.target);
                        self.client.send(match choice.target {
                            SiteTarget::Site(site_id) => PolicyCommand::AddStreamer { site_id, name, username },
                            SiteTarget::Template(preset) => PolicyCommand::AddStreamerToPreset { preset, name, username },
                        });
                    }
                }
                return iced::widget::operation::focus(username_input_id());
            }
            Msg::RemoveStreamer(id) => self.client.send(PolicyCommand::RemoveStreamer(id)),
            Msg::ToggleStreamer(id, enabled) => self.client.send(PolicyCommand::SetStreamerEnabled { id, enabled }),
            Msg::ToggleStreamerSettings(id) => {
                self.schedule_picker = None;
                if !self.open_streamer_settings.remove(&id) {
                    self.open_streamer_settings.insert(id);
                }
            }
            Msg::StreamerEdit(id, field) => self.edit_streamer(id, field),
            Msg::OpenSchedulePicker(id, target) => self.schedule_picker = Some((id, target)),
            Msg::CloseSchedulePicker => self.schedule_picker = None,
            Msg::SchedulePicked(id, field) => {
                self.schedule_picker = None;
                self.edit_streamer(id, field);
            }
            Msg::SaveStreamer(id) => self.save_streamer(id),
            Msg::RevertStreamer(id) => {
                if let Some(st) = self.settings.streamers.iter().find(|s| s.id == id) {
                    self.streamer_drafts.insert(id, StreamerDraft::from_options(&st.options));
                }
            }
            Msg::DefaultsStreamer(id) => {
                self.streamer_drafts.insert(id, StreamerDraft::from_options(&StreamerOptions::default()));
            }
            Msg::AddSite(preset) => {
                if matches!(preset, Preset::Blank) {
                    self.drafts.entry(NEW_SITE_ID).or_insert_with(|| SiteDraft::from_defaults("", Preset::Blank));
                    return iced::widget::operation::snap_to(
                        sites_scroll_id(),
                        iced::widget::scrollable::RelativeOffset { x: 0.0, y: 0.0 },
                    );
                }
                let site = new_site(preset);
                // Named presets are added once.
                if self.settings.sites.iter().any(|s| s.name.eq_ignore_ascii_case(&site.name)) {
                    if let Some(task) = self.jump_to_site(&site.name) {
                        return task;
                    }
                } else {
                    self.pending_jump = Some(site.name.clone());
                    self.client.send(PolicyCommand::AddSite(site));
                }
            }
            Msg::RemoveSite(id) => {
                if id == NEW_SITE_ID {
                    self.drafts.remove(&id);
                } else {
                    self.confirm_remove_site = Some(id);
                }
            }
            Msg::ConfirmRemoveSite(id) => {
                self.confirm_remove_site = None;
                self.client.send(PolicyCommand::RemoveSite(id));
            }
            Msg::CancelRemoveSite => self.confirm_remove_site = None,
            Msg::BrowseSiteOutput(id) => {
                let global = &self.settings.default_output_dir;
                let start = std::path::PathBuf::from(self.drafts.get(&id).map_or(global.trim(), |d| crate::policy::layout::base_dir(&d.output, global)));
                return Task::perform(
                    async move {
                        let mut dlg = rfd::AsyncFileDialog::new().set_title("Output folder");
                        if start.is_dir() {
                            dlg = dlg.set_directory(start);
                        }
                        dlg.pick_folder().await.map(|h| h.path().to_path_buf())
                    },
                    move |p| Msg::SiteOutputChosen(id, p),
                );
            }
            Msg::SiteOutputChosen(id, Some(dir)) => {
                if let Some(d) = self.drafts.get_mut(&id) {
                    d.output = dir.to_string_lossy().into_owned();
                }
            }
            Msg::SiteOutputChosen(_, None) => {}
            Msg::OpenSiteOutput(id) => {
                let global = &self.settings.default_output_dir;
                let dir = std::path::PathBuf::from(self.drafts.get(&id).map_or(global.trim(), |d| crate::policy::layout::base_dir(&d.output, global)));
                let _ = std::fs::create_dir_all(&dir);
                let _ = open::that_detached(dir);
            }
            Msg::Draft(id, field) => {
                if let Some(d) = self.drafts.get_mut(&id) {
                    match field {
                        DraftField::Name(v) => d.name = v,
                        DraftField::Exe(v) => d.exe = v,
                        DraftField::Interval(v) => d.interval = v,
                        DraftField::EndAfter(v) => d.end_after = v,
                        DraftField::RestartClear(v) => d.restart_clear = v,
                        DraftField::RestartThreshold(v) => d.restart_threshold = v,
                        DraftField::ForwardOutput(v) => d.forward_output = v,
                        DraftField::InterruptPatterns(v) => d.interrupt_patterns = v,
                        DraftField::ErrorPatterns(v) => d.error_patterns = v,
                        DraftField::WarnInterrupts(v) => d.warn_interrupts = v,
                        DraftField::WarnErrors(v) => d.warn_errors = v,
                        DraftField::FileExistsGrace(v) => d.file_exists_grace = v,
                        DraftField::GrowthGrace(v) => d.growth_grace = v,
                        DraftField::StallWindow(v) => d.stall_window = v,
                        DraftField::FileCheckInterval(v) => d.file_check_interval = v,
                        DraftField::Checker(v) => d.checker = v,
                        DraftField::Recorder(v) => d.recorder = v,
                        DraftField::UrlTemplate(v) => d.url_template = v,
                        DraftField::Output(v) => d.output = v,
                        DraftField::Paused(v) => d.paused = v,
                        DraftField::FolderLayout(v) => d.folder_layout = v,
                        DraftField::LogStdout(v) => d.log_stdout = v,
                        DraftField::LogStderr(v) => d.log_stderr = v,
                        DraftField::Color(v) => d.color = v,
                        DraftField::UseThemeColor(v) => d.use_theme_color = v,
                        DraftField::PickColor(v) => {
                            d.color = v;
                            d.use_theme_color = false;
                        }
                        DraftField::Notify(v) => d.notify = v,
                        DraftField::NotifyDesktop(v) => d.notify_desktop = v,
                        DraftField::NotifyNtfy(v) => d.notify_ntfy = v,
                        DraftField::NtfyTopic(v) => d.ntfy_topic = v,
                        DraftField::NotifyCooldown(v) => d.notify_cooldown = v,
                        DraftField::NotifyTimeout(v) => d.notify_timeout = v,
                        DraftField::NotifyConfirmed(v) => d.notify_confirmed = v,
                        DraftField::NotifyStalled(v) => d.notify_stalled = v,
                        DraftField::NotifyRestarted(v) => d.notify_restarted = v,
                        DraftField::NotifyOnLaunch(v) => d.notify_on_launch = v,
                        DraftField::SplitEnabled(v) => {
                            d.split_enabled = v;
                            if !v {
                                d.split_minutes.clear();
                            }
                        }
                        DraftField::SplitMinutes(v) => {
                            if !v.trim().is_empty() {
                                d.split_enabled = true;
                            }
                            d.split_minutes = v;
                        }
                        DraftField::AutoSuffix(v) => d.auto_suffix = v,
                    }
                }
            }
            Msg::SaveSite(id) => {
                if let Some(site) = self.drafts.get(&id).and_then(SiteDraft::to_new_site) {
                    if id == NEW_SITE_ID {
                        self.drafts.remove(&NEW_SITE_ID);
                        self.pending_jump = Some(site.name.clone());
                        self.client.send(PolicyCommand::AddSite(site));
                    } else {
                        self.client.send(PolicyCommand::UpdateSite(site.into_site(id)));
                    }
                }
            }
            Msg::RevertSite(id) => {
                if let Some(site) = self.settings.sites.iter().find(|s| s.id == id) {
                    self.drafts.insert(id, SiteDraft::from_site(site));
                }
            }
            Msg::TestNotify(id) => {
                if let Some(d) = self.drafts.get(&id) {
                    let targets = d.test_targets();
                    if d.ntfy_ok() && !targets.is_empty() {
                        let site_name = if d.name.trim().is_empty() { "Site".to_string() } else { d.name.trim().to_string() };
                        let folder = PathBuf::from(crate::policy::layout::base_dir(&d.output, &self.settings.default_output_dir));
                        self.client.send(PolicyCommand::TestNotification { site_name, targets, folder });
                    }
                }
            }
            Msg::DefaultsSite(id) => {
                if let Some(site) = self.settings.sites.iter().find(|s| s.id == id) {
                    let draft = SiteDraft::from_defaults(&site.name, preset_for_name(&site.name));
                    self.drafts.insert(id, draft);
                }
            }
            Msg::GlobalOutEdit(v) => self.global_out = v,
            Msg::GlobalOutSave => self.client.send(PolicyCommand::SetGlobalOutputDir(self.global_out.trim().to_string())),
            Msg::BrowseGlobalOut => {
                let start = PathBuf::from(self.global_out_dir());
                return Task::perform(
                    async move {
                        let mut dlg = rfd::AsyncFileDialog::new().set_title("Global output folder");
                        if start.is_dir() {
                            dlg = dlg.set_directory(start);
                        }
                        dlg.pick_folder().await.map(|h| h.path().to_path_buf())
                    },
                    Msg::GlobalOutChosen,
                );
            }
            Msg::GlobalOutChosen(Some(dir)) => self.global_out = dir.to_string_lossy().into_owned(),
            Msg::GlobalOutChosen(None) => {}
            Msg::OpenGlobalOut => {
                let dir = PathBuf::from(self.global_out_dir());
                let _ = std::fs::create_dir_all(&dir);
                let _ = open::that_detached(dir);
            }
            Msg::Recheck => self.client.send(PolicyCommand::Recheck),
            Msg::ClearWarnings => self.client.send(PolicyCommand::ClearWarnings),
            Msg::CopyText(text) => return iced::clipboard::write(text),
            Msg::SetCompact(on) => {
                self.compact = on;
                prefs::save_compact(on);
            }
            Msg::SetRowMin(v) => {
                self.row_min = v.round().clamp(prefs::ROW_MIN_LIMIT, prefs::ROW_MAX_LIMIT);
                self.row_max = self.row_max.max(self.row_min);
            }
            Msg::SetRowMax(v) => {
                self.row_max = v.round().clamp(prefs::ROW_MIN_LIMIT, prefs::ROW_MAX_LIMIT);
                self.row_min = self.row_min.min(self.row_max);
            }
            Msg::SaveRowRange => prefs::save_row_range(self.row_min, self.row_max),
            Msg::SetTwoColWidth(v) => {
                self.two_col_w = ((v / 10.0).round() * 10.0).clamp(prefs::TWO_COL_MIN_LIMIT, prefs::TWO_COL_MAX_LIMIT);
            }
            Msg::SaveTwoColWidth => prefs::save_two_col_width(self.two_col_w),
            Msg::SetThreeColWidth(v) => {
                self.three_col_w = ((v / 10.0).round() * 10.0).clamp(prefs::TWO_COL_MIN_LIMIT, prefs::TWO_COL_MAX_LIMIT);
            }
            Msg::SaveThreeColWidth => prefs::save_three_col_width(self.three_col_w),
            Msg::SetNameClip(v) => {
                self.name_clip = v.round().clamp(prefs::NAME_CLIP_MIN_LIMIT, prefs::NAME_CLIP_MAX_LIMIT);
            }
            Msg::SaveNameClip => prefs::save_name_clip(self.name_clip),
            Msg::SetLogLines(v) => {
                self.log_lines = ((v / 100.0).round() * 100.0).clamp(prefs::LOG_LINES_MIN_LIMIT, prefs::LOG_LINES_MAX_LIMIT);
            }
            Msg::SaveLogLines => {
                prefs::save_log_lines(self.log_lines);
                self.log.truncate(self.log_lines as usize);
            }
            Msg::SetRememberSize(on) => {
                self.remember_size = on;
                prefs::save_remember_size(on);
                if on {
                    prefs::save_window_size(self.win_w, self.win_h);
                }
            }
            Msg::ResetWindowSize => {
                let (w, h) = (prefs::WIN_DEFAULT_W, prefs::WIN_DEFAULT_H);
                self.win_w = w;
                self.win_h = h;
                *LAST_SIZE.lock().unwrap() = Some((w, h));
                if self.remember_size {
                    prefs::save_window_size(w, h);
                }
                return window::latest().and_then(move |id| window::resize(id, Size::new(w, h)));
            }
            Msg::ResetRowRange => {
                self.row_min = prefs::ROW_MIN_DEFAULT;
                self.row_max = prefs::ROW_MAX_DEFAULT;
                self.two_col_w = prefs::TWO_COL_DEFAULT;
                self.three_col_w = prefs::THREE_COL_DEFAULT;
                self.name_clip = prefs::NAME_CLIP_DEFAULT;
                prefs::save_row_range(self.row_min, self.row_max);
                prefs::save_two_col_width(self.two_col_w);
                prefs::save_three_col_width(self.three_col_w);
                prefs::save_name_clip(self.name_clip);
            }
            Msg::Resized(size) => {
                self.win_w = size.width;
                self.win_h = size.height;
                *LAST_SIZE.lock().unwrap() = Some((size.width, size.height));
            }
            Msg::SetWrapOutput(on) => {
                self.wrap_output = on;
                prefs::save_wrap_output(on);
            }
            Msg::CopyOutput(id) => {
                if let Some(log) = self.output.get(&id) {
                    return iced::clipboard::write(log.text.clone());
                }
            }
            Msg::ToggleProc(id) => {
                if !self.expanded_procs.remove(&id) {
                    self.expanded_procs.insert(id);
                }
            }
            Msg::UpdateClicked => {
                if self.update.is_some() {
                    self.update_error = None;
                    self.update_stage = UpdateStage::Confirm;
                }
            }
            Msg::UpdateCancel => self.update_stage = UpdateStage::Idle,
            Msg::UpdateConfirm => {
                if self.update.is_some() {
                    self.update_stage = UpdateStage::Installing;
                    self.client.send(PolicyCommand::InstallUpdate);
                }
            }
            Msg::CheckUpdateNow => self.client.send(PolicyCommand::CheckForUpdate),
            Msg::SetUpdateChecks(on) => {
                self.update_cfg.enabled = on;
                self.client.send(PolicyCommand::SetUpdateConfig(self.update_cfg));
            }
            Msg::SetIncludeRc(on) => {
                self.update_cfg.include_rc = on;
                self.client.send(PolicyCommand::SetUpdateConfig(self.update_cfg));
            }
            Msg::StartFresh => {
                if crate::settings::load_problem().is_some() || crate::policy::posts::load_problem().is_some() {
                    self.client.send(PolicyCommand::DiscardBrokenConfig);
                }
            }
        }
        Task::none()
    }

    fn on_event(&mut self, ev: PolicyEvent) {
        match ev {
            PolicyEvent::Settings(s) => self.on_settings(s),
            PolicyEvent::Posts(e) => self.posts_on_event(e),
            PolicyEvent::Log(line) => self.push_log(line),
            PolicyEvent::LogHistory(lines) => {
                self.log.extend(lines.into_iter().rev());
                self.log.truncate(self.log_lines as usize);
            }
            PolicyEvent::Update(ev) => self.on_update_event(ev),
            PolicyEvent::LiveSince(map) => self.live_since = map,
            PolicyEvent::LastLive(map) => self.last_live = map,
            PolicyEvent::RecordingFiles(map) => self.recording_files = map,
            PolicyEvent::Resolution(id, size) => match size {
                Some(v) => {
                    self.resolutions.insert(id, v);
                }
                None => {
                    self.resolutions.remove(&id);
                }
            },
            PolicyEvent::Restarts(map) => self.restarts = map,
            PolicyEvent::Warnings(map) => self.warnings = map,
            PolicyEvent::Output(batch) => {
                for (id, lines) in batch {
                    let log = self.output.entry(id).or_default();
                    for line in &lines {
                        log.push(line);
                    }
                }
            }
            PolicyEvent::Engine(ev) => match ev {
                Event::StreamerStatus { id, status } => {
                    self.statuses.insert(id, status);
                }
                Event::SiteChecked { id, at, ok, problem } => {
                    self.checks.insert(id, CheckInfo { at, ok, problem });
                }
                Event::StreamerIssue { id, issue } => match issue {
                    Some(text) => {
                        self.issues.insert(id, text);
                    }
                    None => {
                        self.issues.remove(&id);
                    }
                },
                Event::Preflight(notices) => self.notices = notices,
                Event::ProcessUpdate(p) => {
                    self.processes.insert(p.id, p);
                }
                Event::ProcessRemoved(id) => {
                    self.processes.remove(&id);
                    self.recording_files.remove(&id);
                    self.expanded_procs.remove(&id);
                    self.output.remove(&id);
                }
                Event::Output { .. } => {}
                Event::Log(line) => self.push_log(line),
            },
        }
    }

    fn on_update_event(&mut self, ev: UpdateEvent) {
        match ev {
            UpdateEvent::Config(cfg) => self.update_cfg = cfg,
            UpdateEvent::Available { version, is_rc } => self.update = Some(UpdateInfo { version, is_rc }),
            UpdateEvent::NoUpdate => {
                if self.update_stage != UpdateStage::Installing {
                    self.update = None;
                    self.update_stage = UpdateStage::Idle;
                }
            }
            UpdateEvent::Downloading => self.update_stage = UpdateStage::Installing,
            UpdateEvent::Failed(e) => {
                self.update_error = Some(e);
                self.update_stage = UpdateStage::Idle;
            }
            UpdateEvent::Restarting => {}
        }
    }

    fn on_settings(&mut self, s: Settings) {
        self.disks_dirty = true;
        if self.global_out.is_empty() || self.global_out == self.settings.default_output_dir {
            self.global_out = s.default_output_dir.clone();
        }
        self.drafts.retain(|id, _| *id == NEW_SITE_ID || s.sites.iter().any(|x| x.id == *id));
        for site in &s.sites {
            let old = self.settings.sites.iter().find(|o| o.id == site.id);
            let editing = match (self.drafts.get(&site.id), old) {
                (Some(d), Some(old)) => *d != SiteDraft::from_site(old),
                _ => false,
            };
            if !editing {
                self.drafts.insert(site.id, SiteDraft::from_site(site));
            }
        }
        self.streamer_drafts.retain(|id, _| s.streamers.iter().any(|x| x.id == *id));
        for st in &s.streamers {
            let old = self.settings.streamers.iter().find(|o| o.id == st.id);
            let editing = match (self.streamer_drafts.get(&st.id), old) {
                (Some(d), Some(old)) => d.differs_from(&old.options),
                _ => false,
            };
            if !editing {
                self.streamer_drafts.insert(st.id, StreamerDraft::from_options(&st.options));
            }
        }
        self.statuses.retain(|id, _| s.streamers.iter().any(|x| x.id == *id));
        self.open_streamer_settings.retain(|id| s.streamers.iter().any(|x| x.id == *id));
        self.checks.retain(|id, _| s.sites.iter().any(|x| x.id == *id));
        if self.confirm_remove_site.is_some_and(|id| !s.sites.iter().any(|x| x.id == id)) {
            self.confirm_remove_site = None;
        }
        self.issues.retain(|id, _| s.streamers.iter().any(|x| x.id == *id));
        self.priority_on_settings(&s);
        self.settings = s;
    }
}

fn on_resize((_, size): (window::Id, Size)) -> Msg {
    Msg::Resized(size)
}

fn engine_stream() -> impl Stream<Item = Msg> {
    iced::stream::channel(64, async |mut out: iced::futures::channel::mpsc::Sender<Msg>| {
        let rx = EVENTS.lock().unwrap().take();
        if let Some(mut rx) = rx {
            while let Some(ev) = rx.recv().await {
                if out.send(Msg::Policy(ev)).await.is_err() {
                    return;
                }
            }
        }
        std::future::pending::<()>().await;
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn output_log_keeps_newest_lines() {
        let mut log = OutputLog::default();
        for i in 0..OUTPUT_MAX_LINES + 3 {
            log.push(&format!("line {i}"));
        }
        let lines: Vec<&str> = log.text.lines().collect();
        assert_eq!(lines.len(), OUTPUT_MAX_LINES);
        assert_eq!(lines[0], "line 3");
        assert_eq!(lines[OUTPUT_MAX_LINES - 1], format!("line {}", OUTPUT_MAX_LINES + 2));
    }
}
