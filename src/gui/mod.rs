//! The iced GUI. It only talks to the policy layer through `crate::policy`.

mod drives;
mod files;
mod files_view;
mod logs;
mod logs_view;
mod notify_view;
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
use crate::policy::update::UpdateConfig;
use crate::policy::{
    new_site, preset_for_name, NewSite, PolicyClient, PolicyCommand, PolicyEvent, Preset, RecordingFile, UpdateEvent,
    WarningChip, Warnings, DEFAULT_SITE_COLOR,
};
use iced::futures::{SinkExt, Stream};
use iced::{window, Point, Size, Subscription, Task, Theme};
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
    Notifications,
    General,
}

/// Sub-pages of the Settings > Notifications tab.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotifyTab {
    Livestreams,
    Posts,
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
    pub warning_clear: String,
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
    pub notify_live: bool,
    pub warnings_enabled: bool,
    pub flash_warnings: bool,
    pub notify_interrupts: bool,
    pub notify_errors: bool,
    pub warn_stalled: bool,
    pub warn_restarts: bool,
    pub split_enabled: bool,
    pub split_minutes: String,
    pub auto_suffix: bool,
    pub upgrade_quality: bool,
    pub quick_check: bool,
    pub quick_check_secs: String,
    pub restart_stalled: bool,
    pub paused: bool,
}

impl SiteDraft {
    fn from_site(s: &Site) -> Self {
        Self {
            name: s.name.clone(),
            exe: s.exe_path.clone(),
            interval: opt_text(s.interval_secs),
            end_after: opt_text(s.end_after_minutes),
            warning_clear: opt_text(s.warning_clear_minutes),
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
            notify_live: s.notify_live,
            warnings_enabled: s.warnings_enabled,
            flash_warnings: s.flash_warnings,
            notify_interrupts: s.notify_interrupts,
            notify_errors: s.notify_errors,
            warn_stalled: s.warn_stalled,
            warn_restarts: s.warn_restarts,
            split_enabled: s.split_enabled,
            split_minutes: opt_text(s.split_minutes),
            auto_suffix: s.auto_suffix,
            upgrade_quality: s.upgrade_quality,
            quick_check: s.quick_check,
            quick_check_secs: opt_text(s.quick_check_secs),
            restart_stalled: s.restart_stalled,
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
            warning_clear: opt_text(n.warning_clear_minutes),
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
            notify_live: n.notify_live,
            warnings_enabled: n.warnings_enabled,
            flash_warnings: n.flash_warnings,
            notify_interrupts: n.notify_interrupts,
            notify_errors: n.notify_errors,
            warn_stalled: n.warn_stalled,
            warn_restarts: n.warn_restarts,
            split_enabled: n.split_enabled,
            split_minutes: opt_text(n.split_minutes),
            auto_suffix: n.auto_suffix,
            upgrade_quality: n.upgrade_quality,
            quick_check: n.quick_check,
            quick_check_secs: opt_text(n.quick_check_secs),
            restart_stalled: n.restart_stalled,
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

    /// Minutes after which warnings clear themselves; 0 turns auto-clear off.
    pub fn warning_clear_minutes(&self) -> Option<Option<u64>> {
        parse_opt(&self.warning_clear, 0)
    }

    pub fn restart_threshold(&self) -> Option<Option<u32>> {
        parse_opt(&self.restart_threshold, 1)
    }

    pub fn quick_check_secs(&self) -> Option<Option<u64>> {
        parse_opt(&self.quick_check_secs, 1)
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

    /// False while ntfy is on without a usable topic, own or `global`.
    pub fn ntfy_ok(&self, global: &str) -> bool {
        !self.notify_ntfy || notify::topic_ok(&self.ntfy_topic, global)
    }

    /// True while command output parsing is on (`parsing_off` overrides it) and an Interrupts toggle is on.
    pub fn interrupts_active(&self, parsing_off: bool) -> bool {
        self.forward_output && !parsing_off && (self.notify_interrupts || self.warn_interrupts)
    }

    /// True while command output parsing is on (`parsing_off` overrides it) and an Errors toggle is on.
    pub fn errors_active(&self, parsing_off: bool) -> bool {
        self.forward_output && !parsing_off && (self.notify_errors || self.warn_errors)
    }

    /// True while an active pattern field is blank.
    pub fn patterns_missing(&self, parsing_off: bool) -> bool {
        (self.interrupts_active(parsing_off) && self.interrupt_patterns.trim().is_empty())
            || (self.errors_active(parsing_off) && self.error_patterns.trim().is_empty())
    }

    /// Takes every field shown on the Notifications tab from `d`, leaving the rest.
    fn copy_notify_from(&mut self, d: &SiteDraft) {
        self.notify = d.notify;
        self.warnings_enabled = d.warnings_enabled;
        self.flash_warnings = d.flash_warnings;
        self.notify_desktop = d.notify_desktop;
        self.notify_ntfy = d.notify_ntfy;
        self.ntfy_topic = d.ntfy_topic.clone();
        self.notify_cooldown = d.notify_cooldown.clone();
        self.notify_timeout = d.notify_timeout.clone();
        self.notify_confirmed = d.notify_confirmed;
        self.notify_on_launch = d.notify_on_launch;
        self.notify_live = d.notify_live;
        self.notify_stalled = d.notify_stalled;
        self.notify_restarted = d.notify_restarted;
        self.notify_interrupts = d.notify_interrupts;
        self.notify_errors = d.notify_errors;
        self.warn_stalled = d.warn_stalled;
        self.warn_restarts = d.warn_restarts;
        self.warn_interrupts = d.warn_interrupts;
        self.warn_errors = d.warn_errors;
        self.warning_clear = d.warning_clear.clone();
        self.restart_threshold = d.restart_threshold.clone();
        self.file_exists_grace = d.file_exists_grace.clone();
        self.growth_grace = d.growth_grace.clone();
        self.stall_window = d.stall_window.clone();
        self.file_check_interval = d.file_check_interval.clone();
        self.forward_output = d.forward_output;
        self.interrupt_patterns = d.interrupt_patterns.clone();
        self.error_patterns = d.error_patterns.clone();
    }

    /// The channels the draft has switched on, used by the test button.
    pub fn test_targets(&self, global: &str) -> Targets {
        Targets {
            desktop_timeout_secs: self
                .notify_desktop
                .then(|| self.notify_timeout_secs().flatten().unwrap_or(DEFAULT_NOTIFY_TIMEOUT_SECS)),
            ntfy_topic: if self.notify_ntfy { notify::effective_topic(&self.ntfy_topic, global) } else { None },
        }
    }

    /// The site this draft describes, or `None` while the name or URL template is blank or a field is invalid.
    pub fn to_new_site(&self, global_topic: &str, parsing_off: bool) -> Option<NewSite> {
        if self.name.trim().is_empty()
            || self.url_template.trim().is_empty()
            || !self.color_ok()
            || !self.ntfy_ok(global_topic)
            || self.split_invalid()
            || self.patterns_missing(parsing_off)
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
            warning_clear_minutes: self.warning_clear_minutes()?,
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
            notify_live: self.notify_live,
            warnings_enabled: self.warnings_enabled,
            flash_warnings: self.flash_warnings,
            notify_interrupts: self.notify_interrupts,
            notify_errors: self.notify_errors,
            warn_stalled: self.warn_stalled,
            warn_restarts: self.warn_restarts,
            split_enabled: self.split_enabled,
            split_minutes: self.split_minutes()?,
            auto_suffix: self.auto_suffix,
            upgrade_quality: self.upgrade_quality,
            quick_check: self.quick_check,
            quick_check_secs: self.quick_check_secs()?,
            restart_stalled: self.restart_stalled,
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
    NotifyLive(bool),
    WarningsEnabled(bool),
    FlashWarnings(bool),
    Name(String),
    Exe(String),
    Interval(String),
    EndAfter(String),
    WarningClear(String),
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
    NotifyInterrupts(bool),
    NotifyErrors(bool),
    WarnStalled(bool),
    WarnRestarts(bool),
    SplitEnabled(bool),
    SplitMinutes(String),
    AutoSuffix(bool),
    UpgradeQuality(bool),
    RestartStalled(bool),
    QuickCheck(bool),
    QuickCheckSecs(String),
}

/// A single field edited in a streamer's settings panel.
#[derive(Debug, Clone)]
pub enum StreamerField {
    Name(String),
    FolderLayout(Option<FolderLayout>),
    Notify(Option<bool>),
    AutoSuffix(Option<bool>),
    UpgradeQuality(Option<bool>),
    QuickCheck(Option<bool>),
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
    pub name: String,
    pub log_stdout: bool,
    pub log_stderr: bool,
    pub folder_layout: Option<FolderLayout>,
    pub notify: Option<bool>,
    pub auto_suffix: Option<bool>,
    pub upgrade_quality: Option<bool>,
    pub quick_check: Option<bool>,
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
    /// A draft of a streamer's saved name and options.
    pub fn from_streamer(st: &Streamer) -> Self {
        Self { name: st.name.clone(), ..Self::from_options(&st.options) }
    }

    pub fn from_options(o: &StreamerOptions) -> Self {
        Self {
            name: String::new(),
            log_stdout: o.log_stdout,
            log_stderr: o.log_stderr,
            folder_layout: o.folder_layout,
            notify: o.notify,
            auto_suffix: o.auto_suffix,
            upgrade_quality: o.upgrade_quality,
            quick_check: o.quick_check,
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
            upgrade_quality: self.upgrade_quality,
            quick_check: self.quick_check,
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

    /// True when saving would change `saved`; always-record is owned by the Priority tab and ignored.
    pub fn differs_from(&self, saved: &Streamer) -> bool {
        let options = StreamerOptions { always_record: saved.options.always_record, ..self.to_options() };
        options != saved.options || self.name.trim() != saved.name
    }
}

#[derive(Debug, Clone)]
pub enum Msg {
    PolicyBatch(Vec<PolicyEvent>),
    Tick(#[allow(dead_code)] Instant),
    Anim(#[allow(dead_code)] Instant),
    SetTab(Tab),
    Files(files::FilesMsg),
    Posts(posts::PostsMsg),
    Priority(priority::PriorityMsg),
    SetSettingsTab(SettingsTab),
    SetNotifyTab(NotifyTab),
    PickNotifySite(SiteId),
    PickNotifyPostSite(crate::policy::posts::SiteId),
    SetDisableNotifications(bool),
    SetDisableWarnings(bool),
    GlobalNtfyEdit(String),
    GlobalNtfySave,
    GlobalNtfyRandomize,
    GlobalNtfyRevert,
    SetLogTab(LogTab),
    Logs(logs::LogsMsg),
    ThemeChosen(ThemeKind),
    OpenConfigDir,
    OpenRecording(RecorderId),
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
    PickSettingsSite(SiteSel),
    DiscardSite,
    SwapSites(SiteId, SiteId),
    BrowseSiteOutput(SiteId),
    SiteOutputChosen(SiteId, Option<std::path::PathBuf>),
    OpenSiteOutput(SiteId),
    Draft(SiteId, DraftField),
    SaveSite(SiteId),
    RevertSite(SiteId),
    DefaultsSite(SiteId),
    DefaultsNotify(SiteId),
    TestNotify(SiteId),
    GlobalOutEdit(String),
    GlobalOutSave,
    BrowseGlobalOut,
    GlobalOutChosen(Option<PathBuf>),
    OpenGlobalOut,
    Recheck,
    ClearWarnings,
    DismissWarning(StreamerId, WarningChip),
    CopyText(String),
    ToggleProc(RecorderId),
    SetWrapOutput(bool),
    CopyOutput(RecorderId),
    SetCompact(bool),
    SetSortItem(prefs::SortItem),
    ToggleSortDir,
    SetRowMin(f32),
    SetRowMax(f32),
    SaveRowRange,
    SetTwoColWidth(f32),
    SaveTwoColWidth,
    SetThreeColWidth(f32),
    SaveThreeColWidth,
    SetFullTwoColWidth(f32),
    SaveFullTwoColWidth,
    ResetFullView,
    SetNameClip(f32),
    SaveNameClip,
    SetLogLines(f32),
    SaveLogLines,
    SetRememberSize(bool),
    ResetWindowSize,
    ResetRowRange,
    Resized(Size),
    /// The monitor size and window position read at startup.
    FitWindow(window::Id, Size, Point),
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

/// What the Settings > Livestreams dropdown can point at: an existing site, a preset with no site yet, or a new blank site.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SiteSel {
    Site(SiteId),
    Template(Preset),
    New,
}

/// One entry of the Settings > Livestreams site dropdown.
#[derive(Debug, Clone, PartialEq)]
pub struct SiteSelOption {
    pub target: SiteSel,
    pub name: String,
}

impl std::fmt::Display for SiteSelOption {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.name)
    }
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

/// Scrollable id of the Livestreams tab.
pub(crate) fn livestreams_scroll_id() -> iced::widget::Id {
    iced::widget::Id::new("livestreams-scroll")
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
    stalls: HashMap<StreamerId, u32>,
    warnings: HashMap<StreamerId, Warnings>,
    live_since: HashMap<StreamerId, u64>,
    last_live: HashMap<StreamerId, u64>,
    resolutions: HashMap<StreamerId, Quality>,
    checks: HashMap<SiteId, CheckInfo>,
    issues: HashMap<StreamerId, String>,
    notices: Vec<Notice>,
    processes: BTreeMap<RecorderId, ProcessInfo>,
    recording_files: HashMap<RecorderId, RecordingFile>,
    log: VecDeque<String>,
    /// Number of lines ever pushed to `log`; gives each line a stable key in the Log tab.
    log_seq: u64,
    drafts: HashMap<SiteId, SiteDraft>,
    new_streamer: String,
    add_site: Option<SiteTarget>,
    open_streamer_settings: HashSet<StreamerId>,
    schedule_picker: Option<(StreamerId, PickerTarget)>,
    streamer_drafts: HashMap<StreamerId, StreamerDraft>,
    settings_tab: SettingsTab,
    notify_tab: NotifyTab,
    notify_site: Option<SiteId>,
    settings_site: Option<SiteSel>,
    notify_post_site: Option<crate::policy::posts::SiteId>,
    global_ntfy: String,
    /// A random topic has been offered in the global topic box this session.
    ntfy_seeded: bool,
    log_tab: LogTab,
    logs: logs::LogsState,
    global_out: String,
    phase: f32,
    /// True while the window has no visible size, so the pulse animation is paused.
    minimized: bool,
    now: u64,
    expanded_procs: HashSet<RecorderId>,
    output: HashMap<RecorderId, OutputLog>,
    wrap_output: bool,
    compact: bool,
    live_first: bool,
    sort_key: prefs::SortKey,
    sort_asc: bool,
    row_min: f32,
    row_max: f32,
    two_col_w: f32,
    three_col_w: f32,
    full_two_col_w: f32,
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
            stalls: HashMap::new(),
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
            log_seq: 0,
            drafts: HashMap::new(),
            new_streamer: String::new(),
            add_site: None,
            pending_jump: None,
            open_streamer_settings: HashSet::new(),
            schedule_picker: None,
            streamer_drafts: HashMap::new(),
            settings_tab: SettingsTab::Sites,
            notify_tab: NotifyTab::Livestreams,
            notify_site: None,
            settings_site: None,
            notify_post_site: None,
            global_ntfy: String::new(),
            ntfy_seeded: false,
            log_tab: LogTab::Engine,
            logs: logs::LogsState::new(),
            global_out: String::new(),
            phase: 0.0,
            minimized: false,
            now: now_secs(),
            expanded_procs: HashSet::new(),
            output: HashMap::new(),
            wrap_output: prefs::load_wrap_output(),
            compact: prefs::load_compact(),
            live_first: prefs::load_sort().0,
            sort_key: prefs::load_sort().1,
            sort_asc: prefs::load_sort().2,
            row_min,
            row_max,
            two_col_w: prefs::load_two_col_width(),
            three_col_w: prefs::load_three_col_width(),
            full_two_col_w: prefs::load_full_two_col_width(),
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
            app.note(e);
        }
        let fit = window::latest().and_then(|id| {
            window::monitor_size(id).and_then(move |monitor| window::position(id).and_then(move |pos| Task::done(Msg::FitWindow(id, monitor, pos))))
        });
        (app, fit)
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
            if self.animating() && !self.minimized { iced::time::every(Duration::from_millis(200)).map(Msg::Anim) } else { Subscription::none() },
            window::resize_events().map(on_resize),
            if self.tab == Tab::Files { iced::keyboard::listen().map(files::key_msg) } else { Subscription::none() },
        ])
    }

    /// Selects a newly added site in the Settings > Livestreams dropdown once it exists.
    fn try_jump(&mut self) -> Option<Task<Msg>> {
        let name = self.pending_jump.clone()?;
        let id = self.settings.sites.iter().find(|s| s.name.eq_ignore_ascii_case(&name))?.id;
        self.pending_jump = None;
        self.settings_site = Some(SiteSel::Site(id));
        self.now = now_secs();
        Some(Task::none())
    }

    /// The length a streamer inherits from its site, if splitting is on there.
    pub(crate) fn inherited_split(&self, st: &Streamer) -> Option<u64> {
        self.settings.sites.iter().find(|s| s.id == st.site_id).and_then(Site::inherited_split_minutes)
    }

    /// Applies one edit to a streamer's draft.
    fn edit_streamer(&mut self, id: StreamerId, field: StreamerField) {
        let Some(st) = self.settings.streamers.iter().find(|s| s.id == id) else { return };
        let d = self.streamer_drafts.entry(id).or_insert_with(|| StreamerDraft::from_streamer(st));
        let digits = |v: &str| v.len() <= 6 && v.chars().all(|c| c.is_ascii_digit());
        match field {
            StreamerField::Name(v) => d.name = v,
            StreamerField::FolderLayout(v) => d.folder_layout = v,
            StreamerField::Notify(v) => d.notify = v,
            StreamerField::AutoSuffix(v) => d.auto_suffix = v,
            StreamerField::UpgradeQuality(v) => d.upgrade_quality = v,
            StreamerField::QuickCheck(v) => d.quick_check = v,
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
        if self.settings.streamers.iter().find(|s| s.id == id).is_some_and(|s| s.name != d.name.trim()) {
            self.client.send(PolicyCommand::SetStreamerName { id, name: d.name.clone() });
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

    /// True while both global disable switches are on, which turns command output parsing off at run time.
    pub(crate) fn parsing_overridden(&self) -> bool {
        let g = &self.settings.notifications;
        g.disable_notifications && g.disable_warnings
    }

    /// The Settings > Livestreams dropdown entries: each named preset (its site once created), then custom sites, then New site.
    pub(crate) fn settings_site_options(&self) -> Vec<SiteSelOption> {
        let mut out: Vec<SiteSelOption> = self
            .site_options()
            .into_iter()
            .map(|o| SiteSelOption {
                target: match o.target {
                    SiteTarget::Site(id) => SiteSel::Site(id),
                    SiteTarget::Template(p) => SiteSel::Template(p),
                },
                name: o.name,
            })
            .collect();
        out.push(SiteSelOption { target: SiteSel::New, name: "New site".to_string() });
        out
    }

    /// The dropdown entry shown in Settings > Livestreams: the picked one, else the first.
    pub(crate) fn settings_site_choice(&self, opts: &[SiteSelOption]) -> Option<SiteSelOption> {
        let picked = self.settings_site.and_then(|t| match t {
            SiteSel::Template(p) => {
                let name = new_site(p).name;
                opts.iter().find(|o| o.target == t || o.name.eq_ignore_ascii_case(&name))
            }
            _ => opts.iter().find(|o| o.target == t),
        });
        picked.or_else(|| opts.iter().find(|o| matches!(o.target, SiteSel::Site(_)))).cloned()
    }

    /// Shows a GUI-originated line in the Log tab and saves it to ui.log.txt.
    fn note(&mut self, line: String) {
        crate::policy::uilog::append(&line);
        self.push_log(line);
    }

    fn push_log(&mut self, line: String) {
        self.log.push_front(format!("{}  {line}", crate::engine::logfile::stamp()));
        self.log_seq = self.log_seq.wrapping_add(1);
        self.log.truncate(self.log_lines as usize);
    }

    fn update(&mut self, msg: Msg) -> Task<Msg> {
        let _t = crate::perf::timer("gui.update", 30);
        crate::perf::count("gui.update");
        self.update_inner(msg)
    }

    fn update_inner(&mut self, msg: Msg) -> Task<Msg> {
        match msg {
            Msg::PolicyBatch(events) => {
                for _ in &events {
                    crate::perf::count("gui.policy_events");
                }
                for ev in events {
                    if matches!(ev, PolicyEvent::Update(UpdateEvent::Restarting)) {
                        return iced::exit();
                    }
                    self.on_event(ev);
                }
            }
            Msg::Anim(_) => {
                self.phase = (self.phase + 0.08) % 1.0;
                if let Some(task) = self.try_jump() {
                    return task;
                }
            }
            Msg::Tick(_) => {
                crate::perf::heartbeat();
                self.now = now_secs();
                if !self.animating() {
                    self.phase = 0.25;
                }
                if let Some(task) = self.try_jump() {
                    return task;
                }
                let posts = self.disks_tick();
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
                if self.tab == Tab::Posts && t != Tab::Posts {
                    self.posts.reset_filters();
                }
                self.tab = t;
                if t == Tab::Livestreams {
                    // Collapse any open streamer settings on entering Livestreams.
                    self.open_streamer_settings.clear();
                    self.schedule_picker = None;
                    return iced::widget::operation::scroll_to(
                        livestreams_scroll_id(),
                        iced::widget::scrollable::AbsoluteOffset { x: 0.0, y: 0.0 },
                    );
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
            Msg::Priority(m) => return self.priority_update(m),
            Msg::SetSettingsTab(w) => self.settings_tab = w,
            Msg::SetNotifyTab(w) => self.notify_tab = w,
            Msg::PickNotifySite(id) => self.notify_site = Some(id),
            Msg::PickNotifyPostSite(id) => self.notify_post_site = Some(id),
            Msg::SetDisableNotifications(v) => {
                let n = GlobalNotify { disable_notifications: v, ..self.settings.notifications.clone() };
                self.client.send(PolicyCommand::SetGlobalNotify(n));
            }
            Msg::SetDisableWarnings(v) => {
                let n = GlobalNotify { disable_warnings: v, ..self.settings.notifications.clone() };
                self.client.send(PolicyCommand::SetGlobalNotify(n));
            }
            Msg::GlobalNtfyEdit(v) => self.global_ntfy = v,
            Msg::GlobalNtfyRandomize => self.global_ntfy = notify::random_topic(),
            Msg::GlobalNtfyRevert => self.global_ntfy = self.settings.notifications.ntfy_topic.clone(),
            Msg::GlobalNtfySave => {
                let n = GlobalNotify { ntfy_topic: self.global_ntfy.trim().to_string(), ..self.settings.notifications.clone() };
                self.client.send(PolicyCommand::SetGlobalNotify(n));
            }
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
            Msg::OpenRecording(id) => self.client.send(PolicyCommand::OpenRecording(id)),
            Msg::NewStreamerUsername(v) => self.new_streamer = v,
            Msg::PickStreamerSite(target) => self.add_site = Some(target),
            Msg::AddStreamer => {
                if let Some(choice) = self.add_choice() {
                    let username = std::mem::take(&mut self.new_streamer);
                    let name = String::new();
                    if username.trim().is_empty() {
                        self.new_streamer = username;
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
                    self.streamer_drafts.insert(id, StreamerDraft::from_streamer(st));
                }
            }
            Msg::DefaultsStreamer(id) => {
                let name = self.streamer_drafts.get(&id).map(|d| d.name.clone()).unwrap_or_default();
                self.streamer_drafts.insert(id, StreamerDraft { name, ..StreamerDraft::from_options(&StreamerOptions::default()) });
            }
            Msg::PickSettingsSite(target) => match target {
                SiteSel::New => {
                    self.drafts.entry(NEW_SITE_ID).or_insert_with(|| SiteDraft::from_defaults("", Preset::Blank));
                    self.settings_site = Some(SiteSel::New);
                }
                SiteSel::Site(id) => self.settings_site = Some(SiteSel::Site(id)),
                SiteSel::Template(preset) => {
                    let site = new_site(preset);
                    // Named presets are added once.
                    if let Some(found) = self.settings.sites.iter().find(|s| s.name.eq_ignore_ascii_case(&site.name)) {
                        self.settings_site = Some(SiteSel::Site(found.id));
                    } else {
                        self.settings_site = Some(SiteSel::Template(preset));
                        self.pending_jump = Some(site.name.clone());
                        self.client.send(PolicyCommand::AddSite(site));
                    }
                }
            },
            Msg::SwapSites(a, b) => self.client.send(PolicyCommand::SwapSites(a, b)),
            Msg::DiscardSite => {
                self.drafts.remove(&NEW_SITE_ID);
                self.settings_site = None;
            }
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
                crate::filemgr::ops::open_detached(dir);
            }
            Msg::Draft(id, field) => {
                if let Some(d) = self.drafts.get_mut(&id) {
                    match field {
                        DraftField::Name(v) => d.name = v,
                        DraftField::Exe(v) => d.exe = v,
                        DraftField::Interval(v) => d.interval = v,
                        DraftField::EndAfter(v) => d.end_after = v,
                        DraftField::WarningClear(v) => d.warning_clear = v,
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
                        DraftField::NotifyLive(v) => d.notify_live = v,
                        DraftField::WarningsEnabled(v) => d.warnings_enabled = v,
                        DraftField::FlashWarnings(v) => d.flash_warnings = v,
                        DraftField::NotifyInterrupts(v) => d.notify_interrupts = v,
                        DraftField::NotifyErrors(v) => d.notify_errors = v,
                        DraftField::WarnStalled(v) => d.warn_stalled = v,
                        DraftField::WarnRestarts(v) => d.warn_restarts = v,
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
                        DraftField::UpgradeQuality(v) => d.upgrade_quality = v,
                        DraftField::RestartStalled(v) => d.restart_stalled = v,
                        DraftField::QuickCheck(v) => d.quick_check = v,
                        DraftField::QuickCheckSecs(v) => d.quick_check_secs = v,
                    }
                }
            }
            Msg::SaveSite(id) => {
                let topic = self.settings.notifications.ntfy_topic.as_str();
                let parsing_off = self.parsing_overridden();
                if let Some(site) = self.drafts.get(&id).and_then(|d| d.to_new_site(topic, parsing_off)) {
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
                    let topic = self.settings.notifications.ntfy_topic.as_str();
                    let targets = d.test_targets(topic);
                    if d.ntfy_ok(topic) && !targets.is_empty() {
                        let site_name = if d.name.trim().is_empty() { "Site".to_string() } else { d.name.trim().to_string() };
                        let folder = PathBuf::from(crate::policy::layout::base_dir(&d.output, &self.settings.default_output_dir));
                        self.client.send(PolicyCommand::TestNotification { site_name, targets, folder });
                    }
                }
            }
            Msg::DefaultsNotify(id) => {
                if let Some(d) = self.drafts.get_mut(&id) {
                    let defaults = SiteDraft::from_defaults(&d.name, preset_for_name(&d.name));
                    d.copy_notify_from(&defaults);
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
                crate::filemgr::ops::open_detached(dir);
            }
            Msg::Recheck => self.client.send(PolicyCommand::Recheck),
            Msg::ClearWarnings => self.client.send(PolicyCommand::ClearWarnings),
            Msg::DismissWarning(id, chip) => self.client.send(PolicyCommand::DismissWarning(id, chip)),
            Msg::CopyText(text) => return iced::clipboard::write(text),
            Msg::SetSortItem(item) => {
                match item {
                    prefs::SortItem::LiveFirst(_) => self.live_first = !self.live_first,
                    prefs::SortItem::Key(k) => self.sort_key = k,
                    prefs::SortItem::Divider => return Task::none(),
                }
                prefs::save_sort(self.live_first, self.sort_key, self.sort_asc);
            }
            Msg::ToggleSortDir => {
                self.sort_asc = !self.sort_asc;
                prefs::save_sort(self.live_first, self.sort_key, self.sort_asc);
            }
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
            Msg::SetFullTwoColWidth(v) => {
                self.full_two_col_w = ((v / 10.0).round() * 10.0).clamp(prefs::TWO_COL_MIN_LIMIT, prefs::TWO_COL_MAX_LIMIT);
            }
            Msg::SaveFullTwoColWidth => prefs::save_full_two_col_width(self.full_two_col_w),
            Msg::ResetFullView => {
                self.full_two_col_w = prefs::FULL_TWO_COL_DEFAULT;
                prefs::save_full_two_col_width(self.full_two_col_w);
            }
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
            Msg::FitWindow(id, monitor, pos) => {
                let (w, h) = prefs::fit_to_monitor(self.win_w, self.win_h, monitor.width, monitor.height);
                if (w, h) != (self.win_w, self.win_h) {
                    let moved = Point::new(pos.x + (self.win_w - w) / 2.0, pos.y + (self.win_h - h) / 2.0);
                    return Task::batch([window::resize(id, Size::new(w, h)), window::move_to(id, moved)]);
                }
            }
            Msg::Resized(size) => {
                self.minimized = size.width < 1.0 || size.height < 1.0;
                if self.minimized {
                    return Task::none();
                }
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
            PolicyEvent::Stalls(map) => self.stalls = map,
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
        if self.global_ntfy == self.settings.notifications.ntfy_topic {
            self.global_ntfy = s.notifications.ntfy_topic.clone();
        }
        if !self.ntfy_seeded {
            self.ntfy_seeded = true;
            if s.notifications.ntfy_topic.trim().is_empty() {
                self.global_ntfy = notify::random_topic();
                let n = GlobalNotify { ntfy_topic: self.global_ntfy.clone(), ..s.notifications.clone() };
                self.client.send(PolicyCommand::SetGlobalNotify(n));
            }
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
                (Some(d), Some(old)) => d.differs_from(old),
                _ => false,
            };
            if !editing {
                self.streamer_drafts.insert(st.id, StreamerDraft::from_streamer(st));
            }
        }
        self.statuses.retain(|id, _| s.streamers.iter().any(|x| x.id == *id));
        self.open_streamer_settings.retain(|id| s.streamers.iter().any(|x| x.id == *id));
        self.checks.retain(|id, _| s.sites.iter().any(|x| x.id == *id));
        self.issues.retain(|id, _| s.streamers.iter().any(|x| x.id == *id));
        self.priority_on_settings(&s);
        self.settings = s;
    }
}

fn on_resize((_, size): (window::Id, Size)) -> Msg {
    Msg::Resized(size)
}

/// Keeps only the newest process update for each recorder in a batch of events.
fn coalesce(batch: Vec<PolicyEvent>) -> Vec<PolicyEvent> {
    let mut last: HashMap<RecorderId, usize> = HashMap::new();
    for (i, ev) in batch.iter().enumerate() {
        if let PolicyEvent::Engine(Event::ProcessUpdate(p)) = ev {
            last.insert(p.id, i);
        }
    }
    batch
        .into_iter()
        .enumerate()
        .filter(|(i, ev)| match ev {
            PolicyEvent::Engine(Event::ProcessUpdate(p)) => last.get(&p.id) == Some(i),
            _ => true,
        })
        .map(|(_, ev)| ev)
        .collect()
}

fn engine_stream() -> impl Stream<Item = Msg> {
    iced::stream::channel(64, async |mut out: iced::futures::channel::mpsc::Sender<Msg>| {
        let rx = EVENTS.lock().unwrap().take();
        if let Some(mut rx) = rx {
            let mut batch = Vec::new();
            while rx.recv_many(&mut batch, 2048).await > 0 {
                if out.send(Msg::PolicyBatch(coalesce(std::mem::take(&mut batch)))).await.is_err() {
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
