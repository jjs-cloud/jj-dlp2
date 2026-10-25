//! The iced GUI. It only talks to the policy layer through `crate::policy`.

mod prefs;
mod skin;
mod view;

use crate::api::*;
use crate::policy::{NewSite, PolicyClient, PolicyCommand, PolicyEvent};
use iced::futures::{SinkExt, Stream};
use iced::{window, Size, Subscription, Task, Theme};
use skin::{Skin, ThemeKind};
use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};
use std::sync::Mutex;
use std::time::{Duration, Instant};
use tokio::sync::mpsc::UnboundedReceiver;

const LOG_LIMIT: usize = 500;
pub const DEFAULT_JSON_ARGS: &str = "--dump-json --no-warnings --skip-download {urls}";
pub const DEFAULT_SIM_ARGS: &str =
    "--simulate --no-warnings --match-filter is_live --print \"%(original_url)s|%(webpage_url)s|%(id)s|%(title)s\" {urls}";

static EVENTS: Mutex<Option<UnboundedReceiver<PolicyEvent>>> = Mutex::new(None);

/// Hands the policy event stream to the GUI subscription.
pub fn install_event_stream(rx: UnboundedReceiver<PolicyEvent>) {
    *EVENTS.lock().unwrap() = Some(rx);
}

pub fn run(client: PolicyClient) -> iced::Result {
    iced::application(move || App::new(client.clone()), App::update, App::view)
        .title("jj-dlp")
        .subscription(App::subscription)
        .theme(App::theme)
        .window(window::Settings { size: Size::new(1180.0, 820.0), icon: app_icon(), ..Default::default() })
        .antialiasing(true)
        .run()
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

#[derive(Debug, Clone, Copy)]
pub enum Preset {
    Twitch,
    Tiktok,
    Chaturbate,
    Kick,
    Blank,
}

/// Editable copy of a site's settings, kept as strings while the user types.
#[derive(Debug, Clone, PartialEq)]
pub struct SiteDraft {
    pub name: String,
    pub exe: String,
    pub interval: String,
    pub mode: CheckMode,
    pub checker: String,
    pub recorder: String,
    pub url_template: String,
    pub output: String,
    pub log_stdout: bool,
    pub log_stderr: bool,
}

impl SiteDraft {
    fn from_site(s: &Site) -> Self {
        Self {
            name: s.name.clone(),
            exe: s.exe_path.clone(),
            interval: s.interval_secs.to_string(),
            mode: s.check_mode,
            checker: s.checker_args.clone(),
            recorder: s.recorder_args.clone(),
            url_template: s.url_template.clone(),
            output: s.output_dir.clone(),
            log_stdout: s.log_checker_stdout,
            log_stderr: s.log_checker_stderr,
        }
    }

    pub fn interval_secs(&self) -> Option<u64> {
        self.interval.trim().parse().ok().filter(|v| *v >= 5)
    }
}

#[derive(Debug, Clone)]
pub enum DraftField {
    Name(String),
    Exe(String),
    Interval(String),
    Mode(CheckMode),
    Checker(String),
    Recorder(String),
    UrlTemplate(String),
    Output(String),
    LogStdout(bool),
    LogStderr(bool),
}

/// A single option changed in a streamer's settings panel.
#[derive(Debug, Clone, Copy)]
pub enum OptionField {
    LogStdout(bool),
    LogStderr(bool),
}

#[derive(Debug, Clone)]
pub enum Msg {
    Policy(PolicyEvent),
    Tick(#[allow(dead_code)] Instant),
    SetTab(Tab),
    ThemeChosen(ThemeKind),
    OpenConfigDir,
    OpenOutputDir(SiteId),
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
    GlobalOutEdit(String),
    GlobalOutSave,
    Recheck,
    CopyText(String),
}

#[derive(Debug, Clone, Default)]
pub struct CheckInfo {
    pub at: u64,
    pub ok: bool,
    pub problem: Option<String>,
}

pub struct App {
    client: PolicyClient,
    settings: Settings,
    kind: ThemeKind,
    tab: Tab,
    statuses: HashMap<StreamerId, StreamerStatus>,
    checks: HashMap<SiteId, CheckInfo>,
    issues: HashMap<StreamerId, String>,
    notices: Vec<Notice>,
    processes: BTreeMap<RecorderId, ProcessInfo>,
    log: VecDeque<String>,
    drafts: HashMap<SiteId, SiteDraft>,
    new_streamer: HashMap<SiteId, (String, String)>,
    open_streamer_settings: HashSet<StreamerId>,
    global_out: String,
    phase: f32,
    now: u64,
}

impl App {
    fn new(client: PolicyClient) -> (App, Task<Msg>) {
        client.send(PolicyCommand::RequestSnapshot);
        let app = App {
            client,
            settings: crate::settings::default_settings(),
            kind: prefs::load_theme(),
            tab: Tab::Monitor,
            statuses: HashMap::new(),
            checks: HashMap::new(),
            issues: HashMap::new(),
            notices: Vec::new(),
            processes: BTreeMap::new(),
            log: VecDeque::new(),
            drafts: HashMap::new(),
            new_streamer: HashMap::new(),
            open_streamer_settings: HashSet::new(),
            global_out: String::new(),
            phase: 0.0,
            now: now_secs(),
        };
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
        ])
    }

    fn push_log(&mut self, line: String) {
        let t = now_secs() % 86_400;
        self.log.push_front(format!("{:02}:{:02}:{:02}  {line}", t / 3600, (t / 60) % 60, t % 60));
        self.log.truncate(LOG_LIMIT);
    }

    fn update(&mut self, msg: Msg) -> Task<Msg> {
        match msg {
            Msg::Policy(ev) => self.on_event(ev),
            Msg::Tick(_) => {
                self.phase = (self.phase + 0.04) % 1.0;
                self.now = now_secs();
            }
            Msg::SetTab(t) => self.tab = t,
            Msg::ThemeChosen(k) => {
                self.kind = k;
                prefs::save_theme(k);
            }
            Msg::OpenConfigDir => self.client.send(PolicyCommand::OpenConfigDir),
            Msg::OpenOutputDir(site_id) => self.client.send(PolicyCommand::OpenOutputDir(site_id)),
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
                    }
                    self.client.send(PolicyCommand::SetStreamerOptions { id, options });
                }
            }
            Msg::AddSite(preset) => {
                let site = new_site(preset);
                // Named presets are added once; Blank can repeat.
                let exists = !matches!(preset, Preset::Blank)
                    && self.settings.sites.iter().any(|s| s.name.eq_ignore_ascii_case(&site.name));
                if !exists {
                    self.client.send(PolicyCommand::AddSite(site));
                }
            }
            Msg::RemoveSite(id) => self.client.send(PolicyCommand::RemoveSite(id)),
            Msg::Draft(id, field) => {
                if let Some(d) = self.drafts.get_mut(&id) {
                    match field {
                        DraftField::Name(v) => d.name = v,
                        DraftField::Exe(v) => d.exe = v,
                        DraftField::Interval(v) => d.interval = v,
                        DraftField::Mode(m) => {
                            let swap = |cur: &str, from: &str, to: &str| (cur.trim() == from).then(|| to.to_string());
                            let new_args = match m {
                                CheckMode::Simulate => swap(&d.checker, DEFAULT_JSON_ARGS, DEFAULT_SIM_ARGS),
                                CheckMode::DumpJson => swap(&d.checker, DEFAULT_SIM_ARGS, DEFAULT_JSON_ARGS),
                            };
                            if let Some(a) = new_args {
                                d.checker = a;
                            }
                            d.mode = m;
                        }
                        DraftField::Checker(v) => d.checker = v,
                        DraftField::Recorder(v) => d.recorder = v,
                        DraftField::UrlTemplate(v) => d.url_template = v,
                        DraftField::Output(v) => d.output = v,
                        DraftField::LogStdout(v) => d.log_stdout = v,
                        DraftField::LogStderr(v) => d.log_stderr = v,
                    }
                }
            }
            Msg::SaveSite(id) => {
                if let Some(d) = self.drafts.get(&id) {
                    if let Some(interval) = d.interval_secs() {
                        self.client.send(PolicyCommand::UpdateSite(Site {
                            id,
                            name: d.name.trim().to_string(),
                            exe_path: d.exe.trim().to_string(),
                            interval_secs: interval,
                            check_mode: d.mode,
                            checker_args: d.checker.trim().to_string(),
                            recorder_args: d.recorder.trim().to_string(),
                            url_template: d.url_template.trim().to_string(),
                            output_dir: d.output.trim().to_string(),
                            log_checker_stdout: d.log_stdout,
                            log_checker_stderr: d.log_stderr,
                        }));
                    }
                }
            }
            Msg::RevertSite(id) => {
                if let Some(site) = self.settings.sites.iter().find(|s| s.id == id) {
                    self.drafts.insert(id, SiteDraft::from_site(site));
                }
            }
            Msg::GlobalOutEdit(v) => self.global_out = v,
            Msg::GlobalOutSave => self.client.send(PolicyCommand::SetGlobalOutputDir(self.global_out.trim().to_string())),
            Msg::Recheck => self.client.send(PolicyCommand::Recheck),
            Msg::CopyText(text) => return iced::clipboard::write(text),
        }
        Task::none()
    }

    fn on_event(&mut self, ev: PolicyEvent) {
        match ev {
            PolicyEvent::Settings(s) => self.on_settings(s),
            PolicyEvent::Log(line) => self.push_log(line),
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
                }
                Event::Log(line) => self.push_log(line),
            },
        }
    }

    fn on_settings(&mut self, s: Settings) {
        if self.global_out.is_empty() || self.global_out == self.settings.default_output_dir {
            self.global_out = s.default_output_dir.clone();
        }
        self.drafts.retain(|id, _| s.sites.iter().any(|x| x.id == *id));
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

/// Default yt-dlp path for new sites: the bundled binary on Windows/Linux, else PATH.
fn default_exe() -> &'static str {
    if cfg!(windows) {
        r"bin\windows\yt-dlp\yt-dlp.exe"
    } else if cfg!(target_os = "linux") {
        "bin/linux/yt-dlp/yt-dlp"
    } else {
        "yt-dlp"
    }
}

fn new_site(preset: Preset) -> NewSite {
    let (name, url_template, checker, recorder) = match preset {
        Preset::Twitch => (
            "Twitch",
            "https://twitch.tv/{username}",
            "--cookies-from-browser firefox --dump-json {urls}",
            r#"--cookies-from-browser firefox --fixup never --no-part -o "%(title)s [%(id)s].%(ext)s" {url}"#,
        ),
        Preset::Tiktok => (
            "Tiktok",
            "https://tiktok.com/@{username}/live",
            "--cookies-from-browser firefox --dump-json {urls}",
            r#"--cookies-from-browser firefox --no-part --retries 0 -o "%(uploader)s %(epoch>%Y-%m-%d %H_%M_%S)s.%(ext)s" {url}"#,
        ),
        Preset::Chaturbate => (
            "Chaturbate",
            "https://chaturbate.com/{username}",
            "--dump-json {urls}",
            r#"--fixup never --no-part --downloader-args ffmpeg:"-fps_mode passthrough -copyts -avoid_negative_ts make_zero" -o "%(title)s.%(ext)s" {url}"#,
        ),
        Preset::Kick => (
            "Kick",
            "https://kick.com/{username}",
            "--dump-json {urls}",
            r#"--fixup never --no-part -o "%(uploader)s %(epoch>%Y-%m-%d %H_%M_%S)s.%(ext)s" {url}"#,
        ),
        Preset::Blank => ("New site", "", "", ""),
    };
    NewSite {
        name: name.into(),
        exe_path: default_exe().into(),
        interval_secs: 60,
        check_mode: CheckMode::DumpJson,
        checker_args: checker.into(),
        recorder_args: recorder.into(),
        url_template: url_template.into(),
        output_dir: String::new(),
    }
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
