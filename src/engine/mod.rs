//! The engine: owns settings, per-site checker workers and recorder processes.

// Any changes to the engine should first be approved by the owner.

mod checker;
mod diagnose;
mod exepath;
mod logfile;
mod preflight;
mod recorder;
mod sampler;
mod template;

use crate::api::*;
use crate::settings;
use checker::{CheckReport, LiveEntry, SiteSnapshot, SiteWorker};
use diagnose::Problem;
use recorder::{RecorderHandle, RecorderSpec, SharedTracked, Tracked};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc::{self, UnboundedReceiver, UnboundedSender};

/// How long a recorder must run before its streamer's recording problem is cleared.
const HEALTHY_AFTER: std::time::Duration = std::time::Duration::from_secs(30);

/// Messages from engine background tasks back to the engine loop.
pub(crate) enum Internal {
    CheckResult { site_id: SiteId, at: u64, result: Result<CheckReport, String> },
    RecorderGone(RecorderId),
    Log(String),
    /// An informational line from a recorder, logged once per live session.
    Note { streamer_id: StreamerId, text: String },
    Preflight { notices: Vec<Notice>, manual: bool },
}

/// Prevents a console window from flashing up on Windows.
pub(crate) fn hide_window(cmd: &mut tokio::process::Command) {
    #[cfg(windows)]
    cmd.creation_flags(0x0800_0000 | 0x0000_0200);
    #[cfg(not(windows))]
    let _ = cmd;
}

pub struct EngineParts {
    pub client: EngineClient,
    pub events: UnboundedReceiver<Event>,
    pub thread: std::thread::JoinHandle<()>,
}

/// Starts the engine on its own thread and runtime.
pub fn start() -> EngineParts {
    let (cmd_tx, cmd_rx) = mpsc::unbounded_channel();
    let (ev_tx, ev_rx) = mpsc::unbounded_channel();
    let thread = std::thread::Builder::new()
        .name("engine".into())
        .spawn(move || {
            let rt = tokio::runtime::Builder::new_multi_thread()
                .worker_threads(2)
                .enable_all()
                .build()
                .expect("tokio runtime");
            rt.block_on(Engine::new(ev_tx).run(cmd_rx));
        })
        .expect("spawn engine thread");
    EngineParts { client: EngineClient::new(cmd_tx), events: ev_rx, thread }
}

#[derive(Default)]
struct StreamState {
    live: bool,
    last_checked: Option<u64>,
    entry: Option<LiveEntry>,
}

/// Recording health of one streamer during the current live session.
#[derive(Default)]
struct Health {
    restarts: u32,
    issue: Option<String>,
    last_exit: Option<String>,
}

struct Engine {
    settings: Settings,
    events: UnboundedSender<Event>,
    internal_tx: UnboundedSender<Internal>,
    internal_rx: Option<UnboundedReceiver<Internal>>,
    workers: HashMap<SiteId, SiteWorker>,
    recorders: HashMap<RecorderId, RecorderHandle>,
    states: HashMap<StreamerId, StreamState>,
    tracked: SharedTracked,
    shutting_down: bool,
    next_recorder_id: RecorderId,
    checker_overrides: HashMap<SiteId, String>,
    recorder_overrides: HashMap<StreamerId, String>,
    health: HashMap<StreamerId, Health>,
    site_problems: HashMap<SiteId, String>,
    session_notes: HashSet<(StreamerId, String)>,
    notices: Vec<Notice>,
}

impl Engine {
    fn new(events: UnboundedSender<Event>) -> Self {
        let (internal_tx, internal_rx) = mpsc::unbounded_channel();
        Self {
            settings: settings::load(),
            events,
            internal_tx,
            internal_rx: Some(internal_rx),
            workers: HashMap::new(),
            recorders: HashMap::new(),
            states: HashMap::new(),
            tracked: Arc::new(Mutex::new(HashMap::new())),
            shutting_down: false,
            next_recorder_id: 1,
            checker_overrides: HashMap::new(),
            recorder_overrides: HashMap::new(),
            health: HashMap::new(),
            site_problems: HashMap::new(),
            session_notes: HashSet::new(),
            notices: Vec::new(),
        }
    }

    async fn run(mut self, mut cmd_rx: UnboundedReceiver<Command>) {
        let mut internal_rx = self.internal_rx.take().expect("internal rx");
        sampler::spawn_sampler(self.tracked.clone(), self.events.clone(), self.internal_tx.clone());
        self.sync_workers();
        self.start_preflight(false);
        loop {
            tokio::select! {
                cmd = cmd_rx.recv() => match cmd {
                    Some(c) => self.handle_command(c),
                    None => self.begin_shutdown(),
                },
                Some(msg) = internal_rx.recv() => self.handle_internal(msg),
            }
            if self.shutting_down && self.recorders.is_empty() {
                break;
            }
        }
        for (_, w) in self.workers.drain() {
            w.handle.abort();
        }
    }

    fn emit(&self, ev: Event) {
        let _ = self.events.send(ev);
    }

    fn log(&self, msg: impl Into<String>) {
        self.emit(Event::Log(msg.into()));
    }

    fn alloc_recorder_id(&mut self) -> RecorderId {
        let id = self.next_recorder_id;
        self.next_recorder_id += 1;
        id
    }

    fn begin_shutdown(&mut self) {
        self.shutting_down = true;
        let ids: Vec<RecorderId> = self.recorders.keys().copied().collect();
        for id in ids {
            self.stop_recorder(id);
        }
    }

    fn handle_command(&mut self, cmd: Command) {
        match cmd {
            Command::Reload => self.reload(),
            Command::SetStreamerEnabled { id, enabled } => {
                if let Some(s) = self.settings.streamers.iter_mut().find(|s| s.id == id) {
                    s.enabled = enabled;
                    if enabled {
                        self.maybe_start_recording(id);
                    } else {
                        self.stop_streamer_recorders(id);
                    }
                    self.emit_status(id);
                }
            }
            Command::SetCheckerOverride { site_id, args } => {
                if !self.settings.sites.iter().any(|s| s.id == site_id) {
                    self.log(format!("checker override ignored: unknown site {site_id}"));
                    return;
                }
                set_or_clear(&mut self.checker_overrides, site_id, args);
                self.sync_workers();
            }
            Command::SetRecorderOverride { streamer_id, args } => {
                if !self.settings.streamers.iter().any(|s| s.id == streamer_id) {
                    self.log(format!("recorder override ignored: unknown streamer {streamer_id}"));
                    return;
                }
                set_or_clear(&mut self.recorder_overrides, streamer_id, args);
            }
            Command::Recheck => {
                let ids: Vec<StreamerId> = self.health.keys().copied().collect();
                for id in ids {
                    if let Some(h) = self.health.get_mut(&id) {
                        h.last_exit = None;
                        if h.issue.take().is_some() {
                            self.emit(Event::StreamerIssue { id, issue: None });
                        }
                    }
                }
                self.start_preflight(true);
            }
            Command::Shutdown => self.begin_shutdown(),
        }
    }

    /// Applies settings.json: stops recorders for removed or disabled streamers and resyncs checkers.
    fn reload(&mut self) {
        let new = match settings::try_load() {
            Ok(s) => s,
            Err(e) => {
                self.log(format!("reload failed, keeping current settings: {e}"));
                return;
            }
        };
        let kept: HashSet<StreamerId> = new
            .streamers
            .iter()
            .filter(|s| new.sites.iter().any(|x| x.id == s.site_id))
            .map(|s| s.id)
            .collect();
        let dropped: Vec<StreamerId> =
            self.settings.streamers.iter().map(|s| s.id).filter(|id| !kept.contains(id)).collect();
        for id in dropped {
            self.stop_streamer_recorders(id);
            self.states.remove(&id);
            self.health.remove(&id);
            self.session_notes.retain(|(sid, _)| *sid != id);
        }
        self.settings = new;
        self.checker_overrides.retain(|id, _| self.settings.sites.iter().any(|s| s.id == *id));
        self.recorder_overrides.retain(|id, _| self.settings.streamers.iter().any(|s| s.id == *id));
        self.site_problems.retain(|id, _| self.settings.sites.iter().any(|s| s.id == *id));
        self.sync_workers();
        self.start_preflight(false);
        let streamers = self.settings.streamers.clone();
        for s in streamers.iter().filter(|s| kept.contains(&s.id)) {
            if s.enabled {
                self.maybe_start_recording(s.id);
            } else {
                self.stop_streamer_recorders(s.id);
            }
            self.emit_status(s.id);
        }
    }

    fn handle_internal(&mut self, msg: Internal) {
        match msg {
            Internal::CheckResult { site_id, at, result } => self.on_check_result(site_id, at, result),
            Internal::Log(msg) => self.log(msg),
            Internal::Note { streamer_id, text } => {
                if self.session_notes.insert((streamer_id, text.clone())) {
                    let name = self.settings.streamers.iter().find(|s| s.id == streamer_id).map(|s| s.name.clone());
                    self.log(format!("{}: {text}", name.unwrap_or_default()));
                }
            }
            Internal::Preflight { notices, manual } => self.on_preflight(notices, manual),
            Internal::RecorderGone(id) => {
                let streamer = self.recorders.remove(&id).map(|h| h.streamer_id);
                let done = self.tracked.lock().unwrap().remove(&id);
                self.emit(Event::ProcessRemoved(id));
                if let (Some(sid), Some(t)) = (streamer, done) {
                    self.on_recorder_exit(sid, t);
                }
                if let Some(sid) = streamer {
                    if let Some(st) = self.states.get_mut(&sid) {
                        st.live = false;
                    }
                    self.emit_status(sid);
                }
            }
        }
    }

    /// Creates, updates and removes per-site workers to match the settings.
    fn sync_workers(&mut self) {
        let gone: Vec<SiteId> = self
            .workers
            .keys()
            .copied()
            .filter(|id| !self.settings.sites.iter().any(|s| s.id == *id))
            .collect();
        for id in gone {
            if let Some(w) = self.workers.remove(&id) {
                w.handle.abort();
            }
        }
        for site in self.settings.sites.clone() {
            let snap = self.snapshot_for(&site);
            match self.workers.get(&site.id) {
                Some(w) => {
                    w.tx.send_if_modified(|cur| {
                        if *cur == snap {
                            false
                        } else {
                            *cur = snap;
                            true
                        }
                    });
                }
                None => {
                    let w = checker::spawn_worker(snap, self.internal_tx.clone());
                    self.workers.insert(site.id, w);
                }
            }
        }
    }

    fn snapshot_for(&self, site: &Site) -> SiteSnapshot {
        let urls = self.settings.streamers.iter().filter(|s| s.site_id == site.id).map(|s| s.url.clone()).collect();
        let mut site = site.clone();
        if let Some(args) = self.checker_overrides.get(&site.id) {
            site.checker_args = args.clone();
        }
        SiteSnapshot { site, urls }
    }

    fn on_check_result(&mut self, site_id: SiteId, at: u64, result: Result<CheckReport, String>) {
        let report = match result {
            Ok(r) => r,
            Err(message) => {
                self.report_check(site_id, at, Some((message.clone(), message)));
                return;
            }
        };
        let problem = report.problem.as_ref().map(|p| (p.log_text(), p.summary()));
        self.report_check(site_id, at, problem);
        let streamers: Vec<Streamer> =
            self.settings.streamers.iter().filter(|s| s.site_id == site_id).cloned().collect();
        let single = streamers.len() == 1;
        let mut matched = vec![false; report.entries.len()];
        for s in &streamers {
            let found = report.entries.iter().position(|e| e.matches(&s.url));
            if let Some(i) = found {
                matched[i] = true;
            }
            let state = self.states.entry(s.id).or_default();
            let was_live = state.live;
            state.live = found.is_some();
            state.entry = found.map(|i| report.entries[i].clone());
            state.last_checked = Some(at);
            if state.live && !was_live {
                self.log(format!("{} is live", s.name));
            }
        }
        // A lone streamer on a site can't be ambiguous, so accept any live entry for it.
        if single && !matched.is_empty() && !matched[0] {
            let s = &streamers[0];
            let state = self.states.entry(s.id).or_default();
            state.live = true;
            state.entry = Some(report.entries[0].clone());
        } else {
            for (i, m) in matched.iter().enumerate() {
                if !m {
                    self.log(format!("unmatched live entry: {}", report.entries[i].webpage_url));
                }
            }
        }
        for s in &streamers {
            self.refresh_health(s.id);
            self.maybe_start_recording(s.id);
            self.emit_status(s.id);
        }
    }

    /// Reports a check result, logging its problem only when it appears, changes or clears.
    fn report_check(&mut self, site_id: SiteId, at: u64, problem: Option<(String, String)>) {
        let name = self.site_name(site_id);
        let key = problem.as_ref().map(|(log, _)| log.clone());
        if self.site_problems.get(&site_id) != key.as_ref() {
            match &key {
                Some(log) => self.log(format!("[{name}] checker problem: {log}")),
                None => self.log(format!("[{name}] checker problem cleared")),
            }
            match key {
                Some(k) => {
                    self.site_problems.insert(site_id, k);
                }
                None => {
                    self.site_problems.remove(&site_id);
                }
            }
        }
        self.emit(Event::SiteChecked { id: site_id, at, ok: problem.is_none(), problem: problem.map(|(_, summary)| summary) });
    }

    /// Clears a streamer's recording problem once a recorder runs healthily; resets session counters while offline.
    fn refresh_health(&mut self, streamer_id: StreamerId) {
        let healthy = self
            .tracked
            .lock()
            .unwrap()
            .values()
            .any(|t| t.streamer_id == streamer_id && t.root_exit.is_none() && t.started.elapsed() >= HEALTHY_AFTER);
        if healthy {
            self.mark_healthy(streamer_id);
            return;
        }
        let live = self.states.get(&streamer_id).is_some_and(|s| s.live);
        if !live && !self.recording(streamer_id) {
            if let Some(h) = self.health.get_mut(&streamer_id) {
                h.restarts = 0;
                h.last_exit = None;
            }
            self.session_notes.retain(|(id, _)| *id != streamer_id);
        }
    }

    fn mark_healthy(&mut self, streamer_id: StreamerId) {
        let Some(h) = self.health.get_mut(&streamer_id) else { return };
        h.last_exit = None;
        if h.issue.take().is_some() {
            self.emit(Event::StreamerIssue { id: streamer_id, issue: None });
        }
    }

    /// Classifies a finished recorder: clean ends are healthy, known setup errors raise an issue, others are logged once and retried.
    fn on_recorder_exit(&mut self, streamer_id: StreamerId, t: Tracked) {
        if t.stopping {
            return;
        }
        let code = t.root_exit.flatten();
        if code == Some(0) {
            self.mark_healthy(streamer_id);
            return;
        }
        let problem = diagnose::find_problem(&t.errors);
        let key = problem.as_ref().map_or_else(|| diagnose::describe_exit(code), |p| p.line.clone());
        let new_issue = problem.as_ref().filter(|p| p.hint.is_some()).map(Problem::summary);
        let repeat = {
            let h = self.health.entry(streamer_id).or_default();
            h.restarts += 1;
            let repeat = h.last_exit.as_deref() == Some(key.as_str());
            h.last_exit = Some(key);
            if new_issue.is_some() {
                h.issue = new_issue.clone();
            }
            repeat
        };
        if new_issue.is_some() {
            self.emit(Event::StreamerIssue { id: streamer_id, issue: new_issue });
        }
        if !repeat {
            let detail = problem.map(|p| format!(": {}", p.log_text())).unwrap_or_default();
            self.log(format!(
                "{}: recording stopped unexpectedly ({}, after {}s); retrying while live{detail}",
                t.streamer_name,
                diagnose::describe_exit(code),
                t.started.elapsed().as_secs()
            ));
        }
    }

    fn start_preflight(&self, manual: bool) {
        let sites = self.settings.sites.clone();
        let tx = self.internal_tx.clone();
        tokio::spawn(async move {
            let notices = preflight::run(sites).await;
            let _ = tx.send(Internal::Preflight { notices, manual });
        });
    }

    /// Logs and publishes setup notices when they change (or always after a manual re-check).
    fn on_preflight(&mut self, notices: Vec<Notice>, manual: bool) {
        let changed = notices != self.notices;
        if changed {
            for n in &notices {
                let command = n.command.as_ref().map(|c| format!(" Run: {c}")).unwrap_or_default();
                self.log(format!("setup: {}. {}{command}", n.title, n.steps));
            }
        }
        if manual && notices.is_empty() {
            self.log("setup re-check: no problems found");
        }
        if changed || manual {
            self.emit(Event::Preflight(notices.clone()));
        }
        self.notices = notices;
    }

    fn site_name(&self, id: SiteId) -> String {
        self.settings.sites.iter().find(|s| s.id == id).map(|s| s.name.clone()).unwrap_or_default()
    }

    fn recording(&self, streamer_id: StreamerId) -> bool {
        self.recorders.values().any(|r| r.streamer_id == streamer_id)
    }

    fn emit_status(&self, id: StreamerId) {
        if !self.settings.streamers.iter().any(|s| s.id == id) {
            return;
        }
        let st = self.states.get(&id);
        self.emit(Event::StreamerStatus {
            id,
            status: StreamerStatus {
                live: st.is_some_and(|s| s.live),
                recording: self.recording(id),
                last_checked: st.and_then(|s| s.last_checked),
                restarts: self.health.get(&id).map_or(0, |h| h.restarts),
            },
        });
    }

    fn maybe_start_recording(&mut self, streamer_id: StreamerId) {
        if self.shutting_down || self.recording(streamer_id) {
            return;
        }
        let Some(streamer) = self.settings.streamers.iter().find(|s| s.id == streamer_id).cloned() else { return };
        let Some(entry) = self.states.get(&streamer_id).filter(|s| s.live).and_then(|s| s.entry.clone()) else {
            return;
        };
        if !streamer.enabled {
            return;
        }
        let Some(site) = self.settings.sites.iter().find(|s| s.id == streamer.site_id).cloned() else { return };

        let out_dir = if site.output_dir.trim().is_empty() {
            self.settings.default_output_dir.clone()
        } else {
            site.output_dir.clone()
        };
        let url = if entry.webpage_url.is_empty() { streamer.url.clone() } else { entry.webpage_url.clone() };
        let template = self.recorder_overrides.get(&streamer_id).unwrap_or(&site.recorder_args);
        let args = template::expand(template, "url", &[url]);
        let log_path = |on: bool, kind: &str| on.then(|| logfile::stream_path(&site.name, &streamer.name, kind));
        let exe = exepath::resolve(&site.exe_path);
        if let Some(note) = exe.note {
            self.log(note);
        }
        let id = self.alloc_recorder_id();
        let spec = RecorderSpec {
            id,
            streamer_id,
            streamer_name: streamer.name.clone(),
            site_name: site.name.clone(),
            exe: exe.exe,
            args,
            cwd: PathBuf::from(out_dir),
            log_stdout: log_path(streamer.options.log_stdout, "stdout"),
            log_stderr: log_path(streamer.options.log_stderr, "stderr"),
            internal: self.internal_tx.clone(),
        };
        match recorder::spawn_recorder(spec, &self.tracked) {
            Ok(handle) => {
                self.log(format!("recording {}", streamer.name));
                self.recorders.insert(id, handle);
                self.emit_status(streamer_id);
            }
            Err(e) => self.log(format!("could not record {}: {e}", streamer.name)),
        }
    }

    fn stop_recorder(&mut self, id: RecorderId) {
        if let Some(h) = self.recorders.get_mut(&id) {
            recorder::request_stop(h, id, &self.tracked);
        }
    }

    fn stop_streamer_recorders(&mut self, streamer_id: StreamerId) {
        let ids: Vec<RecorderId> =
            self.recorders.iter().filter(|(_, r)| r.streamer_id == streamer_id).map(|(id, _)| *id).collect();
        for id in ids {
            self.stop_recorder(id);
        }
    }
}

fn set_or_clear<K: std::hash::Hash + Eq>(map: &mut HashMap<K, String>, key: K, value: Option<String>) {
    match value {
        Some(v) => {
            map.insert(key, v);
        }
        None => {
            map.remove(&key);
        }
    }
}

#[cfg(all(test, unix))]
mod e2e {
    use super::*;
    use crate::testutil::{Sandbox, FAKE_FAILING};
    use std::time::Duration;

    fn write_settings(dir: &std::path::Path, script: &std::path::Path, enabled: bool) {
        let settings = Settings {
            next_id: 10,
            default_output_dir: dir.join("out").to_string_lossy().into(),
            sites: vec![Site {
                id: 1,
                name: "Fake".into(),
                exe_path: script.to_string_lossy().into(),
                interval_secs: 5,
                check_mode: CheckMode::DumpJson,
                checker_args: "--dump-json {urls}".into(),
                recorder_args: "RECORD {url}".into(),
                url_template: String::new(),
                output_dir: String::new(),
                log_checker_stdout: true,
                log_checker_stderr: true,
            }],
            streamers: vec![Streamer {
                id: 2,
                site_id: 1,
                name: "live1".into(),
                url: "https://x.test/live1".into(),
                enabled,
                options: StreamerOptions { log_stdout: true, log_stderr: true },
            }],
        };
        std::fs::create_dir_all(settings::config_dir()).unwrap();
        std::fs::write(settings::settings_path(), serde_json::to_string(&settings).unwrap()).unwrap();
    }

    async fn wait_for(p: &mut EngineParts, secs: u64, mut pred: impl FnMut(&Event) -> bool) -> Event {
        let deadline = tokio::time::Instant::now() + Duration::from_secs(secs);
        while tokio::time::Instant::now() < deadline {
            if let Ok(Some(ev)) = tokio::time::timeout(Duration::from_secs(1), p.events.recv()).await {
                if pred(&ev) {
                    return ev;
                }
            }
        }
        panic!("timed out waiting for event");
    }

    #[test]
    fn records_reloads_and_confirms_exit() {
        let sb = Sandbox::new("engine");
        let (dir, script) = (sb.dir.clone(), sb.script.clone());
        write_settings(&dir, &script, true);

        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let mut p = start();
            let mut seen_pids: Vec<u32> = Vec::new();
            let full = |info: &ProcessInfo| info.has_bootloader && info.ytdlp.is_some() && !info.ffmpeg.is_empty();
            let ev = wait_for(&mut p, 60, |e| matches!(e, Event::ProcessUpdate(i) if full(i))).await;
            let Event::ProcessUpdate(info) = ev else { unreachable!() };
            assert_ne!(info.root.as_ref().unwrap().pid, info.ytdlp.as_ref().unwrap().pid);
            for s in info.root.iter().chain(info.ytdlp.iter()).chain(info.ffmpeg.iter()) {
                seen_pids.push(s.pid);
            }

            let logs = settings::config_dir().join("logs");
            let checker_log = std::fs::read_to_string(logs.join("fake.checker.log")).unwrap();
            assert!(checker_log.contains("--- stdout ---") && checker_log.contains("vid1"), "{checker_log}");
            assert!(checker_log.contains("--- stderr ---"), "{checker_log}");

            p.client.send(Command::SetStreamerEnabled { id: info.streamer_id, enabled: false });
            wait_for(&mut p, 30, |e| matches!(e, Event::ProcessRemoved(_))).await;
            for kind in ["stdout", "stderr"] {
                let path = logs.join(format!("fake.live1.{kind}.log"));
                let mut text = String::new();
                for _ in 0..20 {
                    text = std::fs::read_to_string(&path).unwrap();
                    if text.contains("exit: code") {
                        break;
                    }
                    tokio::time::sleep(Duration::from_millis(100)).await;
                }
                assert!(text.contains("start: ") && text.contains("exit: code"), "{kind}: {text}");
            }
            for pid in &seen_pids {
                let state = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok().and_then(|t| t.rsplit(") ").next().and_then(|r| r.chars().next()));
                assert!(matches!(state, None | Some('Z')), "pid {pid} still alive at removal (state {state:?})");
            }

            std::fs::write(settings::settings_path(), "{ not json").unwrap();
            p.client.send(Command::Reload);
            wait_for(&mut p, 10, |e| matches!(e, Event::Log(m) if m.contains("reload failed"))).await;

            write_settings(&dir, &script, true);
            p.client.send(Command::Reload);
            wait_for(&mut p, 40, |e| matches!(e, Event::ProcessUpdate(i) if full(i))).await;

            p.client.send(Command::Shutdown);
            let thread = p.thread;
            tokio::task::spawn_blocking(move || thread.join()).await.unwrap().unwrap();
        });
    }

    #[test]
    fn reports_problems_and_ignores_offline_errors() {
        let sb = Sandbox::with_script("engine-issues", FAKE_FAILING);
        let streamer = |id: u64, name: &str| Streamer {
            id,
            site_id: 1,
            name: name.into(),
            url: format!("https://x.test/{name}"),
            enabled: true,
            options: StreamerOptions::default(),
        };
        let settings = Settings {
            next_id: 10,
            default_output_dir: sb.dir.join("out").to_string_lossy().into(),
            sites: vec![Site {
                id: 1,
                name: "Fake".into(),
                exe_path: sb.script.to_string_lossy().into(),
                interval_secs: 5,
                check_mode: CheckMode::DumpJson,
                checker_args: "--dump-json {urls}".into(),
                recorder_args: "RECORD {url}".into(),
                url_template: String::new(),
                output_dir: String::new(),
                log_checker_stdout: false,
                log_checker_stderr: false,
            }],
            streamers: vec![streamer(2, "offline1"), streamer(3, "broken1"), streamer(4, "live1")],
        };
        std::fs::create_dir_all(settings::config_dir()).unwrap();
        std::fs::write(settings::settings_path(), serde_json::to_string(&settings).unwrap()).unwrap();

        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let mut p = start();
            let ev = wait_for(&mut p, 30, |e| matches!(e, Event::SiteChecked { ok: false, .. })).await;
            let Event::SiteChecked { problem, .. } = ev else { unreachable!() };
            assert!(problem.unwrap().contains("Something unexpected"));

            let ev = wait_for(&mut p, 40, |e| matches!(e, Event::StreamerIssue { issue: Some(m), .. } if m.contains("ffmpeg"))).await;
            let Event::StreamerIssue { id, .. } = ev else { unreachable!() };
            assert_eq!(id, 4);
            wait_for(&mut p, 20, |e| matches!(e, Event::StreamerStatus { status, .. } if status.restarts >= 1)).await;

            p.client.send(Command::Shutdown);
            let thread = p.thread;
            tokio::task::spawn_blocking(move || thread.join()).await.unwrap().unwrap();
        });
    }
}
