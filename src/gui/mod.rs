//! The iced GUI. It only talks to the policy layer through `crate::policy`.

pub mod prefs;
mod skin;
mod view;

use crate::api::*;
use crate::policy::uilog;
use crate::policy::update::UpdateConfig;
use crate::policy::{
    new_site, preset_for_name, NewSite, PolicyClient, PolicyCommand, PolicyEvent, Preset, RecordingFile, UpdateEvent,
    DEFAULT_SITE_COLOR,
};
use iced::futures::{SinkExt, Stream};
use iced::{window, Size, Subscription, Task, Theme};
use skin::{Skin, ThemeKind};
use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};
use std::sync::Mutex;
use std::time::{Duration, Instant};
use tokio::sync::mpsc::UnboundedReceiver;

/// Quick-pick colours shown under the site colour field.
pub const SITE_SWATCHES: [&str; 10] = [
    "#9146FF", "#FE2C55", "#F89C1C", "#53FC18", "#19F0FF", "#FF2BD6", "#FFD24A", "#7AA2F7", "#F38BA8", "#B8FFC8",
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
    let result = iced::application(move || App::new(client.clone()), App::update, App::view)
        .title("jj-dlp")
        .subscription(App::subscription)
        .theme(App::theme)
        .window(window::Settings { size: Size::new(win_w, win_h), icon: app_icon(), ..Default::default() })
        .antialiasing(true)
        .run();
    save_size_on_exit();
    result
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
    Monitor,
    Processes,
    Settings,
    Log,
}

/// Sub-pages of the Settings tab.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsTab {
    Sites,
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

    /// The site this draft describes, or `None` while the name is blank or a field is invalid.
    pub fn to_new_site(&self) -> Option<NewSite> {
        if self.name.trim().is_empty() || !self.color_ok() {
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
            color: self.site_color(),
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
    FileExistsGrace(String),
    GrowthGrace(String),
    StallWindow(String),
    FileCheckInterval(String),
    Checker(String),
    Recorder(String),
    UrlTemplate(String),
    Output(String),
    FolderLayout(FolderLayout),
    LogStdout(bool),
    LogStderr(bool),
    Color(String),
    UseThemeColor(bool),
    PickColor(String),
}

/// A single option changed in a streamer's settings panel.
#[derive(Debug, Clone, Copy)]
pub enum OptionField {
    LogStdout(bool),
    LogStderr(bool),
    FolderLayout(Option<FolderLayout>),
}

#[derive(Debug, Clone)]
pub enum Msg {
    Policy(PolicyEvent),
    Tick(#[allow(dead_code)] Instant),
    SetTab(Tab),
    SetSettingsTab(SettingsTab),
    OpenAddSite,
    ThemeChosen(ThemeKind),
    OpenConfigDir,
    OpenOutputDir(SiteId),
    OpenRecording(RecorderId),
    NewStreamerName(SiteId, String),
    NewStreamerUsername(SiteId, String),
    AddStreamer(SiteId),
    RemoveStreamer(StreamerId),
    ToggleStreamer(StreamerId, bool),
    ToggleStreamerSettings(StreamerId),
    StreamerOption(StreamerId, OptionField),
    AddSite(Preset),
    RemoveSite(SiteId),
    Draft(SiteId, DraftField),
    SaveSite(SiteId),
    RevertSite(SiteId),
    DefaultsSite(SiteId),
    GlobalOutEdit(String),
    GlobalOutSave,
    Recheck,
    ClearRestarts,
    CopyText(String),
    ToggleProc(RecorderId),
    SetCompact(bool),
    SetRowMin(f32),
    SetRowMax(f32),
    SaveRowRange,
    SetTwoColWidth(f32),
    SaveTwoColWidth,
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
}

#[derive(Debug, Clone, Default)]
pub struct CheckInfo {
    pub at: u64,
    pub ok: bool,
    pub problem: Option<String>,
}

/// Widget id of a site's "Streamer username" input.
pub(crate) fn username_input_id(id: SiteId) -> iced::widget::Id {
    iced::widget::Id::from(format!("streamer-username-{:?}", id))
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
    live_since: HashMap<StreamerId, u64>,
    last_live: HashMap<StreamerId, u64>,
    checks: HashMap<SiteId, CheckInfo>,
    issues: HashMap<StreamerId, String>,
    notices: Vec<Notice>,
    processes: BTreeMap<RecorderId, ProcessInfo>,
    recording_files: HashMap<RecorderId, RecordingFile>,
    log: VecDeque<String>,
    drafts: HashMap<SiteId, SiteDraft>,
    new_streamer: HashMap<SiteId, (String, String)>,
    open_streamer_settings: HashSet<StreamerId>,
    settings_tab: SettingsTab,
    global_out: String,
    phase: f32,
    now: u64,
    expanded_procs: HashSet<RecorderId>,
    compact: bool,
    row_min: f32,
    row_max: f32,
    two_col_w: f32,
    name_clip: f32,
    log_lines: f32,
    remember_size: bool,
    win_w: f32,
    win_h: f32,
    update: Option<UpdateInfo>,
    update_stage: UpdateStage,
    update_error: Option<String>,
    update_cfg: UpdateConfig,
}

impl App {
    fn new(client: PolicyClient) -> (App, Task<Msg>) {
        client.send(PolicyCommand::RequestSnapshot);
        let (row_min, row_max) = prefs::load_row_range();
        let (win_w, win_h) = prefs::startup_window_size();
        let mut app = App {
            client,
            settings: crate::settings::default_settings(),
            kind: prefs::load_theme(),
            tab: Tab::Monitor,
            statuses: HashMap::new(),
            restarts: HashMap::new(),
            live_since: HashMap::new(),
            last_live: HashMap::new(),
            checks: HashMap::new(),
            issues: HashMap::new(),
            notices: Vec::new(),
            processes: BTreeMap::new(),
            recording_files: HashMap::new(),
            log: VecDeque::new(),
            drafts: HashMap::new(),
            new_streamer: HashMap::new(),
            pending_jump: None,
            open_streamer_settings: HashSet::new(),
            settings_tab: SettingsTab::Sites,
            global_out: String::new(),
            phase: 0.0,
            now: now_secs(),
            expanded_procs: HashSet::new(),
            compact: prefs::load_compact(),
            row_min,
            row_max,
            two_col_w: prefs::load_two_col_width(),
            name_clip: prefs::load_name_clip(),
            log_lines: prefs::load_log_lines(),
            remember_size: prefs::load_remember_size(),
            win_w,
            win_h,
            update: None,
            update_stage: UpdateStage::Idle,
            update_error: None,
            update_cfg: UpdateConfig::default(),
        };
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
            iced::time::every(Duration::from_millis(100)).map(Msg::Tick),
            window::resize_events().map(on_resize),
        ])
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
            Msg::Tick(_) => {
                self.phase = (self.phase + 0.04) % 1.0;
                self.now = now_secs();
                // Jump on a later tick so the new editor is already laid out.
                if let Some(name) = self.pending_jump.clone() {
                    if let Some(task) = self.jump_to_site(&name) {
                        self.pending_jump = None;
                        return task;
                    }
                }
            }
            Msg::SetTab(t) => self.tab = t,
            Msg::SetSettingsTab(w) => self.settings_tab = w,
            Msg::OpenAddSite => {
                self.tab = Tab::Settings;
                self.settings_tab = SettingsTab::Sites;
            }
            Msg::ThemeChosen(k) => {
                self.kind = k;
                prefs::save_theme(k);
            }
            Msg::OpenConfigDir => self.client.send(PolicyCommand::OpenConfigDir),
            Msg::OpenOutputDir(site_id) => self.client.send(PolicyCommand::OpenOutputDir(site_id)),
            Msg::OpenRecording(id) => self.client.send(PolicyCommand::OpenRecording(id)),
            Msg::NewStreamerName(id, v) => self.new_streamer.entry(id).or_default().0 = v,
            Msg::NewStreamerUsername(id, v) => self.new_streamer.entry(id).or_default().1 = v,
            Msg::AddStreamer(id) => {
                if let Some((name, username)) = self.new_streamer.remove(&id) {
                    if !username.trim().is_empty() {
                        self.client.send(PolicyCommand::AddStreamer { site_id: id, name, username });
                    } else {
                        self.new_streamer.insert(id, (name, username));
                    }
                }
                return iced::widget::operation::focus(username_input_id(id));
            }
            Msg::RemoveStreamer(id) => self.client.send(PolicyCommand::RemoveStreamer(id)),
            Msg::ToggleStreamer(id, enabled) => self.client.send(PolicyCommand::SetStreamerEnabled { id, enabled }),
            Msg::ToggleStreamerSettings(id) => {
                if !self.open_streamer_settings.remove(&id) {
                    self.open_streamer_settings.insert(id);
                }
            }
            Msg::StreamerOption(id, field) => {
                if let Some(st) = self.settings.streamers.iter().find(|s| s.id == id) {
                    let mut options = st.options.clone();
                    match field {
                        OptionField::LogStdout(v) => options.log_stdout = v,
                        OptionField::LogStderr(v) => options.log_stderr = v,
                        OptionField::FolderLayout(v) => options.folder_layout = v,
                    }
                    self.client.send(PolicyCommand::SetStreamerOptions { id, options });
                }
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
                    self.client.send(PolicyCommand::RemoveSite(id));
                }
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
                        DraftField::FileExistsGrace(v) => d.file_exists_grace = v,
                        DraftField::GrowthGrace(v) => d.growth_grace = v,
                        DraftField::StallWindow(v) => d.stall_window = v,
                        DraftField::FileCheckInterval(v) => d.file_check_interval = v,
                        DraftField::Checker(v) => d.checker = v,
                        DraftField::Recorder(v) => d.recorder = v,
                        DraftField::UrlTemplate(v) => d.url_template = v,
                        DraftField::Output(v) => d.output = v,
                        DraftField::FolderLayout(v) => d.folder_layout = v,
                        DraftField::LogStdout(v) => d.log_stdout = v,
                        DraftField::LogStderr(v) => d.log_stderr = v,
                        DraftField::Color(v) => d.color = v,
                        DraftField::UseThemeColor(v) => d.use_theme_color = v,
                        DraftField::PickColor(v) => {
                            d.color = v;
                            d.use_theme_color = false;
                        }
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
            Msg::DefaultsSite(id) => {
                if let Some(site) = self.settings.sites.iter().find(|s| s.id == id) {
                    let draft = SiteDraft::from_defaults(&site.name, preset_for_name(&site.name));
                    self.drafts.insert(id, draft);
                }
            }
            Msg::GlobalOutEdit(v) => self.global_out = v,
            Msg::GlobalOutSave => self.client.send(PolicyCommand::SetGlobalOutputDir(self.global_out.trim().to_string())),
            Msg::Recheck => self.client.send(PolicyCommand::Recheck),
            Msg::ClearRestarts => self.client.send(PolicyCommand::ClearRestarts),
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
                self.name_clip = prefs::NAME_CLIP_DEFAULT;
                prefs::save_row_range(self.row_min, self.row_max);
                prefs::save_two_col_width(self.two_col_w);
                prefs::save_name_clip(self.name_clip);
            }
            Msg::Resized(size) => {
                self.win_w = size.width;
                self.win_h = size.height;
                *LAST_SIZE.lock().unwrap() = Some((size.width, size.height));
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
        }
        Task::none()
    }

    fn on_event(&mut self, ev: PolicyEvent) {
        match ev {
            PolicyEvent::Settings(s) => self.on_settings(s),
            PolicyEvent::Log(line) => self.push_log(line),
            PolicyEvent::LogHistory(lines) => {
                self.log.extend(lines.into_iter().rev());
                self.log.truncate(self.log_lines as usize);
            }
            PolicyEvent::Update(ev) => self.on_update_event(ev),
            PolicyEvent::LiveSince(map) => self.live_since = map,
            PolicyEvent::LastLive(map) => self.last_live = map,
            PolicyEvent::RecordingFiles(map) => self.recording_files = map,
            PolicyEvent::Restarts(map) => self.restarts = map,
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
                }
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
        self.statuses.retain(|id, _| s.streamers.iter().any(|x| x.id == *id));
        self.open_streamer_settings.retain(|id| s.streamers.iter().any(|x| x.id == *id));
        self.checks.retain(|id, _| s.sites.iter().any(|x| x.id == *id));
        self.issues.retain(|id, _| s.streamers.iter().any(|x| x.id == *id));
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
