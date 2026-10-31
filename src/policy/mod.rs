//! Policy layer: owns the config files, applies edits and tells the engine to reload.

use crate::api::{
    now_secs, Command, EngineClient, Event, FolderLayout, ProcessInfo, RecorderId, RecorderState, Settings, Site, SiteId,
    Streamer, StreamerId, StreamerOptions, LOG_HISTORY_MAX,
};
use crate::engine::{logfile, EngineParts};
use crate::settings;
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::{Duration, Instant};
use tokio::sync::mpsc::{self, UnboundedReceiver, UnboundedSender};
use tokio::sync::oneshot;

mod defaults;
mod files;
mod gql;
pub mod layout;
pub mod live;
mod outpath;
mod restarts;
pub mod uilog;
pub mod update;
use files::{FileTracker, Thresholds};
pub use defaults::{new_site, preset_for_name, Preset, DEFAULT_SITE_COLOR};
pub use files::RecordingFile;
use live::LiveTracker;
use restarts::RestartTracker;
use update::UpdateConfig;

const UPDATE_CHECK_EVERY: Duration = Duration::from_secs(6 * 3600);

/// Fields supplied when creating a site; the policy layer assigns the id.
#[derive(Debug, Clone, PartialEq)]
pub struct NewSite {
    pub name: String,
    pub exe_path: String,
    pub interval_secs: Option<u64>,
    pub checker_args: String,
    pub recorder_args: String,
    pub url_template: String,
    pub output_dir: String,
    pub folder_layout: FolderLayout,
    pub log_checker_stdout: bool,
    pub log_checker_stderr: bool,
    pub end_after_minutes: Option<u64>,
    pub file_exists_grace_secs: Option<u64>,
    pub growth_grace_secs: Option<u64>,
    pub stall_window_secs: Option<u64>,
    pub file_check_interval_secs: Option<u64>,
    pub restart_clear_minutes: Option<u64>,
    pub restart_threshold: Option<u32>,
    pub color: Option<String>,
}

impl NewSite {
    /// The stored site with the given id.
    pub fn into_site(self, id: SiteId) -> Site {
        Site {
            id,
            name: self.name,
            exe_path: self.exe_path,
            interval_secs: self.interval_secs,
            checker_args: self.checker_args,
            recorder_args: self.recorder_args,
            url_template: self.url_template,
            output_dir: self.output_dir,
            folder_layout: self.folder_layout,
            log_checker_stdout: self.log_checker_stdout,
            log_checker_stderr: self.log_checker_stderr,
            end_after_minutes: self.end_after_minutes,
            file_exists_grace_secs: self.file_exists_grace_secs,
            growth_grace_secs: self.growth_grace_secs,
            stall_window_secs: self.stall_window_secs,
            file_check_interval_secs: self.file_check_interval_secs,
            restart_clear_minutes: self.restart_clear_minutes,
            restart_threshold: self.restart_threshold,
            color: self.color,
        }
    }
}

#[derive(Debug, Clone)]
pub enum PolicyCommand {
    RequestSnapshot,
    AddSite(NewSite),
    UpdateSite(Site),
    RemoveSite(SiteId),
    AddStreamer { site_id: SiteId, name: String, username: String },
    RemoveStreamer(StreamerId),
    SetStreamerEnabled { id: StreamerId, enabled: bool },
    SetStreamerOptions { id: StreamerId, options: StreamerOptions },
    SetGlobalOutputDir(String),
    OpenConfigDir,
    OpenOutputDir(SiteId),
    /// Opens the file a recorder is writing, once its path is verified.
    OpenRecording(RecorderId),
    /// Re-runs the engine's setup checks.
    Recheck,
    /// Resets every streamer's restart count.
    ClearRestarts,
    /// Looks for a new release now, even when automatic checks are off.
    CheckForUpdate,
    /// Downloads the available update, then stops the engine and restarts into it.
    InstallUpdate,
    /// Saves the updater settings and re-checks.
    SetUpdateConfig(UpdateConfig),
    /// Stops the engine gracefully, then ends the policy layer.
    Shutdown,
}

#[derive(Debug, Clone)]
pub enum PolicyEvent {
    Settings(Settings),
    Engine(Event),
    Log(String),
    /// Saved UI log lines from earlier sessions, oldest first; sent once at startup.
    LogHistory(Vec<String>),
    Update(UpdateEvent),
    /// Streamer id to the unix time it was first seen live; absent when not in a live span.
    LiveSince(HashMap<StreamerId, u64>),
    /// Streamer id to the unix time it was last seen live; absent when never seen.
    LastLive(HashMap<StreamerId, u64>),
    /// File state of every active recording; sent whenever it changes.
    RecordingFiles(HashMap<RecorderId, RecordingFile>),
    /// Streamer id to its restart count; streamers with none are absent.
    Restarts(HashMap<StreamerId, u32>),
}

/// Updater state changes reported to front ends.
#[derive(Debug, Clone)]
pub enum UpdateEvent {
    Config(UpdateConfig),
    Available { version: String, is_rc: bool },
    /// No newer release; clears any earlier `Available`.
    NoUpdate,
    Downloading,
    Failed(String),
    /// The update is staged and the engine is stopping; the front end should exit.
    Restarting,
}

/// Results of background update work, fed back into the policy loop.
enum UpdateMsg {
    Checked { manual: bool, result: Result<Option<update::Available>, String> },
    Staged(Result<update::Staged, String>),
}

/// Result of a recording-file job running on a blocking thread.
struct FileJob {
    id: RecorderId,
    result: files::JobResult,
}

/// Result of the startup Twitch GQL lookup, with the streamers it was run for.
struct GqlJob {
    targets: Vec<(StreamerId, String)>,
    lookup: gql::Lookup,
}

/// Cheap handle for sending commands to the policy layer.
#[derive(Clone)]
pub struct PolicyClient {
    tx: UnboundedSender<PolicyCommand>,
}

impl PolicyClient {
    pub fn send(&self, cmd: PolicyCommand) {
        let _ = self.tx.send(cmd);
    }
}

pub struct PolicyParts {
    pub client: PolicyClient,
    pub events: UnboundedReceiver<PolicyEvent>,
    pub thread: std::thread::JoinHandle<()>,
}

/// Starts the policy layer on its own thread, taking ownership of the running engine.
pub fn start(engine: EngineParts) -> PolicyParts {
    let (cmd_tx, cmd_rx) = mpsc::unbounded_channel();
    let (ev_tx, ev_rx) = mpsc::unbounded_channel();
    let thread = std::thread::Builder::new()
        .name("policy".into())
        .spawn(move || {
            let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().expect("tokio runtime");
            let EngineParts { client, events, thread } = engine;
            let (done_tx, done_rx) = tokio::sync::oneshot::channel();
            std::thread::spawn(move || {
                let _ = thread.join();
                let _ = done_tx.send(());
            });
            let (upd_tx, upd_rx) = mpsc::unbounded_channel();
            let (file_tx, file_rx) = mpsc::unbounded_channel();
            let (gql_tx, gql_rx) = mpsc::unbounded_channel();
            rt.block_on(Policy::new(client, ev_tx, upd_tx, file_tx, gql_tx).run(
                cmd_rx, events, done_rx, upd_rx, file_rx, gql_rx,
            ));
            // Stops the runtime without waiting on downloads, then installs any staged update.
            rt.shutdown_background();
            update::finish_pending();
        })
        .expect("spawn policy thread");
    PolicyParts { client: PolicyClient { tx: cmd_tx }, events: ev_rx, thread }
}

struct Policy {
    settings: Settings,
    live: LiveTracker,
    engine: EngineClient,
    events: UnboundedSender<PolicyEvent>,
    update_cfg: UpdateConfig,
    available: Option<update::Available>,
    installing: bool,
    upd_tx: UnboundedSender<UpdateMsg>,
    files: FileTracker,
    restarts: RestartTracker,
    last_files: HashMap<RecorderId, RecordingFile>,
    file_tx: UnboundedSender<FileJob>,
    gql_tx: UnboundedSender<GqlJob>,
    /// Running recorders as (streamer id, root pid), used for log toggle markers.
    recorder_pids: HashMap<RecorderId, (StreamerId, u32)>,
}

impl Policy {
    fn new(
        engine: EngineClient,
        events: UnboundedSender<PolicyEvent>,
        upd_tx: UnboundedSender<UpdateMsg>,
        file_tx: UnboundedSender<FileJob>,
        gql_tx: UnboundedSender<GqlJob>,
    ) -> Self {
        let settings = settings::load();
        let live = LiveTracker::load(&settings, now_secs());
        Self {
            settings,
            live,
            engine,
            events,
            update_cfg: update::load_config(),
            available: None,
            installing: false,
            upd_tx,
            files: FileTracker::default(),
            restarts: RestartTracker::default(),
            last_files: HashMap::new(),
            file_tx,
            gql_tx,
            recorder_pids: HashMap::new(),
        }
    }

    async fn run(
        mut self,
        mut cmds: UnboundedReceiver<PolicyCommand>,
        mut engine_events: UnboundedReceiver<Event>,
        mut engine_done: oneshot::Receiver<()>,
        mut update_msgs: UnboundedReceiver<UpdateMsg>,
        mut file_msgs: UnboundedReceiver<FileJob>,
        mut gql_msgs: UnboundedReceiver<GqlJob>,
    ) {
        let mut cmds_open = true;
        let mut update_tick = tokio::time::interval(UPDATE_CHECK_EVERY);
        update_tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        let mut restart_tick = tokio::time::interval(Duration::from_secs(30));
        restart_tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        self.emit(PolicyEvent::LogHistory(uilog::tail(LOG_HISTORY_MAX)));
        self.log(format!("--- session started (v{}) ---", update::CURRENT));
        for line in crate::resets::take_log() {
            self.log(line);
        }
        self.emit_settings();
        self.emit_update_config();
        self.emit_live();
        self.emit_last_live();
        self.start_backfill();
        outpath::clear_sidecars();
        self.sync_overrides();
        loop {
            tokio::select! {
                cmd = cmds.recv(), if cmds_open => match cmd {
                    Some(c) => self.handle(c),
                    None => {
                        cmds_open = false;
                        self.engine.send(Command::Shutdown);
                    }
                },
                ev = engine_events.recv() => match ev {
                    Some(e) => {
                        self.on_engine_event(e);
                        while let Ok(e) = engine_events.try_recv() {
                            self.on_engine_event(e);
                        }
                        self.flush_live();
                    }
                    None => break,
                },
                _ = &mut engine_done => break,
                _ = update_tick.tick() => self.check_now(),
                _ = restart_tick.tick() => self.expire_restarts(),
                msg = update_msgs.recv() => {
                    if let Some(m) = msg {
                        self.on_update_msg(m);
                    }
                }
                msg = file_msgs.recv() => {
                    if let Some(m) = msg {
                        self.on_file_job(m);
                    }
                }
                msg = gql_msgs.recv() => {
                    if let Some(m) = msg {
                        self.on_gql_job(m);
                    }
                }
            }
        }
        while let Ok(e) = engine_events.try_recv() {
            self.on_engine_event(e);
        }
        self.flush_live();
    }

    /// Feeds check results to the live tracker, then forwards the event.
    fn on_engine_event(&mut self, ev: Event) {
        match &ev {
            Event::SiteChecked { id, ok, .. } => self.live.site_checked(*id, *ok),
            Event::StreamerStatus { id, status } => {
                if let (Some(at), Some((site_id, secs))) = (status.last_checked, live::lookup(&self.settings, *id)) {
                    self.live.status(*id, site_id, at, status.live, secs);
                }
                self.on_restart_status(*id, status.recording);
            }
            Event::ProcessUpdate(p) => self.on_process_update(p),
            Event::ProcessRemoved(id) => {
                self.recorder_pids.remove(id);
                if let Some(streamer_id) = self.files.remove(*id) {
                    outpath::remove_sidecar(streamer_id);
                }
                self.emit_files();
            }
            _ => {}
        }
        self.emit(PolicyEvent::Engine(ev));
    }

    /// Looks up the file-check timing of the site a streamer belongs to.
    fn thresholds_for(&self, streamer_id: StreamerId) -> Option<Thresholds> {
        let streamer = self.settings.streamers.iter().find(|s| s.id == streamer_id)?;
        let site = self.settings.sites.iter().find(|s| s.id == streamer.site_id)?;
        Some(Thresholds::from_site(site))
    }

    /// Feeds a streamer's recording flag to the restart tracker.
    fn on_restart_status(&mut self, id: StreamerId, recording: bool) {
        let Some((_, end_after_secs)) = live::lookup(&self.settings, id) else { return };
        let enabled = self.settings.streamers.iter().any(|s| s.id == id && s.enabled);
        self.restarts.on_status(id, recording, enabled, Duration::from_secs(end_after_secs), Instant::now());
        self.flush_restarts();
    }

    /// Clears restart counts older than each site's auto-clear time.
    fn expire_restarts(&mut self) {
        let settings = &self.settings;
        self.restarts.expire(Instant::now(), |id| restart_clear_minutes(settings, id));
        self.flush_restarts();
    }

    fn flush_restarts(&mut self) {
        if self.restarts.take_dirty() {
            self.emit_restarts();
        }
    }

    fn emit_restarts(&self) {
        self.emit(PolicyEvent::Restarts(self.restarts.counts()));
    }

    fn on_process_update(&mut self, p: &ProcessInfo) {
        if p.state == RecorderState::Stopping {
            self.restarts.stopping(p.streamer_id);
        }
        match (&p.state, &p.root) {
            (RecorderState::Exited(_), _) | (_, None) => {
                self.recorder_pids.remove(&p.id);
            }
            (_, Some(root)) => {
                self.recorder_pids.insert(p.id, (p.streamer_id, root.pid));
            }
        }
        let Some(th) = self.thresholds_for(p.streamer_id) else { return };
        let up = self.files.on_update(p, th, Instant::now());
        self.apply_file_update(p.id, up);
    }

    fn on_file_job(&mut self, m: FileJob) {
        let up = self.files.on_job(m.id, m.result, Instant::now());
        self.apply_file_update(m.id, up);
    }

    /// Logs the tracker's messages, starts a requested job on a blocking thread and publishes changes.
    fn apply_file_update(&mut self, id: RecorderId, up: files::Update) {
        for line in up.logs {
            self.log(line);
        }
        if let Some(job) = up.job {
            let cwd = self.recording_dir(job.streamer_id);
            let tx = self.file_tx.clone();
            tokio::task::spawn_blocking(move || {
                let _ = tx.send(FileJob { id, result: files::run_job(&job, &cwd) });
            });
        }
        self.emit_files();
    }

    /// The absolute folder a streamer's recordings are written to.
    fn recording_dir(&self, streamer_id: StreamerId) -> PathBuf {
        let site_id = self.settings.streamers.iter().find(|s| s.id == streamer_id).map(|s| s.site_id);
        let dir = site_id.map_or(self.settings.default_output_dir.as_str(), |id| output_dir(&self.settings, id));
        let dir = PathBuf::from(dir);
        std::path::absolute(&dir).unwrap_or(dir)
    }

    /// Gives the engine each streamer's recorder arguments, including its subfolders and the sidecar options.
    fn sync_overrides(&self) {
        outpath::ensure_sidecar_dir();
        for streamer in &self.settings.streamers {
            let Some(site) = self.settings.sites.iter().find(|s| s.id == streamer.site_id) else { continue };
            let layout = streamer.options.folder_layout.unwrap_or(site.folder_layout);
            let dirs = layout::dirs(layout, &site.name, &streamer.name);
            let args = outpath::recorder_override(site.recorder_or_default(), &dirs, &outpath::sidecar_path(streamer.id));
            self.engine.send(Command::SetRecorderOverride { streamer_id: streamer.id, args });
        }
    }

    fn emit_files(&mut self) {
        let snap = self.files.snapshot();
        if snap != self.last_files {
            self.last_files = snap.clone();
            self.emit(PolicyEvent::RecordingFiles(snap));
        }
    }

    /// Opens a recording's file, but only through its verified path.
    fn open_recording(&self, id: RecorderId) {
        let Some(path) = self.files.trusted_path(id) else { return };
        if !path.is_file() {
            self.log(format!("recording file not found: {}", path.display()));
            return;
        }
        if let Err(e) = open::that_detached(path) {
            self.log(format!("could not open {}: {e}", path.display()));
        }
    }

    /// Writes live.json and notifies front ends when the tracker changed.
    fn flush_live(&mut self) {
        if self.live.take_dirty() {
            if let Err(e) = self.live.save() {
                self.log(format!("failed to save live.json: {e}"));
            }
        }
        if self.live.take_span_changed() {
            self.emit_live();
        }
        if self.live.take_last_live_changed() {
            self.emit_last_live();
        }
    }

    fn emit_live(&self) {
        self.emit(PolicyEvent::LiveSince(self.live.live_since()));
    }

    fn emit_last_live(&self) {
        self.emit(PolicyEvent::LastLive(self.live.last_live()));
    }

    /// Starts the Twitch GQL lookup of missing last-live times, at most once per 24 hours.
    fn start_backfill(&mut self) {
        if cfg!(test) {
            return;
        }
        let now = now_secs();
        if self.live.gql_checked_at().is_some_and(|at| now.saturating_sub(at) < gql::BACKFILL_EVERY_SECS) {
            return;
        }
        let targets: Vec<(StreamerId, String)> = self
            .settings
            .streamers
            .iter()
            .filter(|s| !self.live.has_last_live(s.id))
            .filter_map(|s| gql::login_from_url(&s.url).map(|login| (s.id, login)))
            .collect();
        if targets.is_empty() {
            return;
        }
        self.live.set_gql_checked_at(now);
        self.flush_live();
        let mut logins: Vec<String> = targets.iter().map(|(_, login)| login.clone()).collect();
        logins.sort();
        logins.dedup();
        let tx = self.gql_tx.clone();
        tokio::task::spawn_blocking(move || {
            let _ = tx.send(GqlJob { targets, lookup: gql::fetch_last_broadcasts(&logins) });
        });
    }

    /// Stores the looked-up last-live times for streamers that still exist and have none.
    fn on_gql_job(&mut self, job: GqlJob) {
        if let Some(e) = &job.lookup.error {
            self.log(e.clone());
        }
        let found: HashMap<StreamerId, u64> = job
            .targets
            .iter()
            .filter(|(id, _)| live::lookup(&self.settings, *id).is_some())
            .filter_map(|(id, login)| job.lookup.found.get(login).map(|at| (*id, *at)))
            .collect();
        let added = self.live.backfill(found);
        if added > 0 {
            self.log(format!("backfilled last-live time via Twitch GQL for {added} streamer(s)"));
        }
        self.flush_live();
    }

    /// Drops live records of streamers that no longer exist.
    fn prune_live(&mut self) {
        let settings = &self.settings;
        self.live.retain(|id| live::lookup(settings, id).is_some());
        self.restarts.retain(|id| live::lookup(settings, id).is_some());
        self.flush_live();
        self.flush_restarts();
    }

    fn emit(&self, ev: PolicyEvent) {
        if let PolicyEvent::Log(line) | PolicyEvent::Engine(Event::Log(line)) = &ev {
            uilog::append(line);
        }
        let _ = self.events.send(ev);
    }

    fn emit_settings(&self) {
        self.emit(PolicyEvent::Settings(self.settings.clone()));
    }

    fn emit_update_config(&self) {
        self.emit(PolicyEvent::Update(UpdateEvent::Config(self.update_cfg)));
    }

    /// Re-reads update.json (so hand edits apply) and checks for a release.
    fn check_now(&mut self) {
        let cfg = update::load_config();
        if cfg != self.update_cfg {
            self.update_cfg = cfg;
            self.emit_update_config();
        }
        self.run_check(false);
    }

    /// Starts a background release check; `manual` checks run even when automatic ones are off.
    fn run_check(&mut self, manual: bool) {
        if self.installing || cfg!(test) {
            return;
        }
        if !self.update_cfg.enabled && !manual {
            if self.available.take().is_some() {
                self.emit(PolicyEvent::Update(UpdateEvent::NoUpdate));
            }
            return;
        }
        let include_rc = self.update_cfg.include_rc;
        let tx = self.upd_tx.clone();
        tokio::task::spawn_blocking(move || {
            let _ = tx.send(UpdateMsg::Checked { manual, result: update::check(include_rc) });
        });
    }

    /// Starts downloading the available update in the background.
    fn install_update(&mut self) {
        let Some(a) = self.available.clone() else {
            return;
        };
        if self.installing {
            return;
        }
        self.installing = true;
        self.emit(PolicyEvent::Update(UpdateEvent::Downloading));
        self.log(format!("downloading update v{}", a.version));
        let tx = self.upd_tx.clone();
        tokio::task::spawn_blocking(move || {
            let _ = tx.send(UpdateMsg::Staged(update::download(&a)));
        });
    }

    fn on_update_msg(&mut self, msg: UpdateMsg) {
        match msg {
            UpdateMsg::Checked { result: Err(e), .. } => self.log(e),
            UpdateMsg::Checked { result: Ok(Some(a)), .. } => {
                self.emit(PolicyEvent::Update(UpdateEvent::Available { version: a.version.clone(), is_rc: a.is_rc }));
                self.available = Some(a);
            }
            UpdateMsg::Checked { manual, result: Ok(None) } => {
                if manual {
                    self.log(format!("jj-dlp v{} is up to date", update::CURRENT));
                }
                self.available = None;
                self.emit(PolicyEvent::Update(UpdateEvent::NoUpdate));
            }
            UpdateMsg::Staged(Ok(staged)) => {
                self.log(format!("update v{} ready; restarting", staged.version));
                update::set_pending(staged);
                self.emit(PolicyEvent::Update(UpdateEvent::Restarting));
                self.engine.send(Command::Shutdown);
            }
            UpdateMsg::Staged(Err(e)) => {
                self.installing = false;
                self.log(format!("update failed: {e}"));
                self.emit(PolicyEvent::Update(UpdateEvent::Failed(e)));
            }
        }
    }

    fn log(&self, msg: impl Into<String>) {
        self.emit(PolicyEvent::Log(msg.into()));
    }

    /// Writes an enabled/disabled marker to a streamer's log files for each toggled stream with a running recorder.
    fn mark_log_toggles(&self, id: StreamerId, old: &StreamerOptions, new: &StreamerOptions) {
        let Some(streamer) = self.settings.streamers.iter().find(|s| s.id == id) else { return };
        let Some(site) = self.settings.sites.iter().find(|s| s.id == streamer.site_id) else { return };
        let pids: Vec<u32> = self.recorder_pids.values().filter(|(sid, _)| *sid == id).map(|(_, pid)| *pid).collect();
        let streams = [("stdout", old.log_stdout, new.log_stdout), ("stderr", old.log_stderr, new.log_stderr)];
        for (kind, _, now) in streams.into_iter().filter(|(_, was, now)| was != now) {
            let path = logfile::stream_path(&site.name, &streamer.name, kind);
            for pid in &pids {
                let text = if now { format!("logging enabled (pid {pid})") } else { "logging disabled".to_string() };
                logfile::line(&path, &text);
            }
        }
    }

    fn alloc_id(&mut self) -> u64 {
        let id = self.settings.next_id;
        self.settings.next_id += 1;
        id
    }

    /// Writes the config files; returns false (and logs) if the write failed.
    fn save(&self) -> bool {
        match settings::save(&self.settings) {
            Ok(()) => true,
            Err(e) => {
                self.log(format!("failed to save settings: {e}"));
                false
            }
        }
    }

    /// Saves the config files, tells the engine to reload them and pushes a snapshot.
    fn commit(&self) {
        if self.save() {
            self.engine.send(Command::Reload);
            self.sync_overrides();
        }
        self.emit_settings();
    }

    fn handle(&mut self, cmd: PolicyCommand) {
        match cmd {
            PolicyCommand::RequestSnapshot => {
                self.emit_settings();
                self.emit_update_config();
                self.emit_live();
                self.emit_last_live();
                self.emit_restarts();
                if let Some(a) = &self.available {
                    self.emit(PolicyEvent::Update(UpdateEvent::Available { version: a.version.clone(), is_rc: a.is_rc }));
                }
            }
            PolicyCommand::AddSite(n) => {
                let id = self.alloc_id();
                self.settings.sites.push(n.into_site(id));
                self.commit();
            }
            PolicyCommand::UpdateSite(site) => {
                if let Some(slot) = self.settings.sites.iter_mut().find(|s| s.id == site.id) {
                    *slot = site;
                    self.commit();
                }
            }
            PolicyCommand::RemoveSite(id) => {
                self.settings.streamers.retain(|s| s.site_id != id);
                self.settings.sites.retain(|s| s.id != id);
                self.commit();
                self.prune_live();
            }
            PolicyCommand::AddStreamer { site_id, name, username } => {
                let Some(site) = self.settings.sites.iter().find(|s| s.id == site_id) else {
                    return;
                };
                if username.trim().trim_start_matches('@').is_empty() {
                    return;
                }
                if site.url_template.trim().is_empty() {
                    self.log(format!("Set a URL template for {} before adding streamers", site.name));
                    return;
                }
                let url = build_url(&site.url_template, &username);
                if self.settings.streamers.iter().any(|s| s.site_id == site_id && s.url == url) {
                    self.log(format!("{url} is already monitored"));
                    return;
                }
                let name = if name.trim().is_empty() { derive_name(&url) } else { name.trim().to_string() };
                let id = self.alloc_id();
                self.settings.streamers.push(Streamer {
                    id,
                    site_id,
                    name,
                    url,
                    enabled: true,
                    options: StreamerOptions::default(),
                });
                self.commit();
            }
            PolicyCommand::RemoveStreamer(id) => {
                self.settings.streamers.retain(|s| s.id != id);
                self.commit();
                self.prune_live();
            }
            PolicyCommand::SetStreamerEnabled { id, enabled } => {
                if let Some(s) = self.settings.streamers.iter_mut().find(|s| s.id == id) {
                    s.enabled = enabled;
                    if !enabled {
                        self.restarts.stopping(id);
                    }
                    if self.save() {
                        self.engine.send(Command::SetStreamerEnabled { id, enabled });
                    }
                    self.emit_settings();
                }
            }
            PolicyCommand::SetStreamerOptions { id, options } => {
                if let Some(s) = self.settings.streamers.iter_mut().find(|s| s.id == id) {
                    let old = std::mem::replace(&mut s.options, options.clone());
                    self.mark_log_toggles(id, &old, &options);
                    self.commit();
                }
            }
            PolicyCommand::SetGlobalOutputDir(dir) => {
                self.settings.default_output_dir = dir;
                self.commit();
            }
            PolicyCommand::OpenConfigDir => {
                let dir = settings::config_dir();
                let _ = std::fs::create_dir_all(&dir);
                if let Err(e) = open::that_detached(&dir) {
                    self.log(format!("could not open {}: {e}", dir.display()));
                }
            }
            PolicyCommand::OpenOutputDir(site_id) => {
                let _ = open::that_detached(output_dir(&self.settings, site_id));
            }
            PolicyCommand::OpenRecording(id) => self.open_recording(id),
            PolicyCommand::Recheck => self.engine.send(Command::Recheck),
            PolicyCommand::ClearRestarts => {
                self.restarts.clear();
                self.flush_restarts();
            }
            PolicyCommand::CheckForUpdate => self.run_check(true),
            PolicyCommand::InstallUpdate => self.install_update(),
            PolicyCommand::SetUpdateConfig(cfg) => {
                if let Err(e) = update::save_config(&cfg) {
                    self.log(format!("failed to save update.json: {e}"));
                }
                self.update_cfg = cfg;
                self.emit_update_config();
                self.run_check(false);
            }
            PolicyCommand::Shutdown => self.engine.send(Command::Shutdown),
        }
    }
}

/// The streamer's site auto-clear time for restart warnings, in minutes; 0 when the site is unknown.
fn restart_clear_minutes(settings: &Settings, id: StreamerId) -> u64 {
    let site_id = settings.streamers.iter().find(|s| s.id == id).map(|s| s.site_id);
    site_id.and_then(|sid| settings.sites.iter().find(|s| s.id == sid)).map_or(0, |s| s.restart_clear_or_default())
}

/// The site's output folder, or the global one when the site has none.
fn output_dir(settings: &Settings, site_id: SiteId) -> &str {
    let site_dir = settings.sites.iter().find(|s| s.id == site_id).map(|s| s.output_dir.trim());
    site_dir.filter(|d| !d.is_empty()).unwrap_or(&settings.default_output_dir)
}

/// Builds a stream URL by replacing `{username}` in the template; a leading `@` is dropped.
fn build_url(template: &str, username: &str) -> String {
    template.trim().replace("{username}", username.trim().trim_start_matches('@'))
}

/// Uses the last meaningful path segment of a URL as a display name.
fn derive_name(url: &str) -> String {
    let trimmed = url.trim_end_matches('/');
    let mut segs: Vec<&str> = trimmed.split('/').collect();
    while segs.len() > 3 && matches!(segs.last(), Some(&"live") | Some(&"streams")) {
        segs.pop();
    }
    let last = segs.last().copied().unwrap_or(url);
    last.trim_start_matches('@').split(['?', '#']).next().unwrap_or(last).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn site(id: SiteId, output_dir: &str) -> Site {
        Site {
            id,
            name: "s".into(),
            exe_path: "yt-dlp".into(),
            interval_secs: Some(60),
            checker_args: String::new(),
            recorder_args: String::new(),
            url_template: String::new(),
            output_dir: output_dir.into(),
            folder_layout: FolderLayout::default(),
            log_checker_stdout: false,
            log_checker_stderr: false,
            end_after_minutes: Some(30),
            file_exists_grace_secs: Some(60),
            growth_grace_secs: Some(60),
            stall_window_secs: Some(120),
            file_check_interval_secs: Some(30),
            restart_clear_minutes: Some(0),
            restart_threshold: Some(1),
            color: None,
        }
    }

    #[test]
    fn output_dir_prefers_site_then_global() {
        let mut s = Settings {
            next_id: 1,
            default_output_dir: "/global".into(),
            sites: vec![site(1, " /custom "), site(2, "  ")],
            streamers: Vec::new(),
        };
        assert_eq!(output_dir(&s, 1), "/custom");
        assert_eq!(output_dir(&s, 2), "/global");
        assert_eq!(output_dir(&s, 99), "/global");
        s.default_output_dir = "/other".into();
        assert_eq!(output_dir(&s, 2), "/other");
    }

    #[test]
    fn site_and_streamer_without_optional_fields_still_load() {
        let sites = r#"[{"id":1,"name":"s","exe_path":"y","interval_secs":60,"check_mode":"DumpJson","checker_args":"","recorder_args":"","url_template":"","output_dir":"","end_after_minutes":30,"file_exists_grace_secs":60,"growth_grace_secs":60,"stall_window_secs":120,"file_check_interval_secs":30,"restart_clear_minutes":0,"restart_threshold":1,"color":null}]"#;
        let streamers = r#"[{"id":2,"site_id":1,"name":"n","url":"u","enabled":true}]"#;
        let sites: Vec<Site> = serde_json::from_str(sites).unwrap();
        let streamers: Vec<Streamer> = serde_json::from_str(streamers).unwrap();
        assert!(!sites[0].log_checker_stdout && !sites[0].log_checker_stderr);
        assert_eq!(streamers[0].options, StreamerOptions::default());
        assert_eq!(sites[0].folder_layout, FolderLayout::Streamer);
        assert_eq!(streamers[0].options.folder_layout, None);
    }

    #[test]
    fn build_url_fills_username() {
        assert_eq!(build_url("https://tiktok.com/@{username}/live", "@name"), "https://tiktok.com/@name/live");
        assert_eq!(build_url("https://kick.com/{username}", " name "), "https://kick.com/name");
    }

    #[test]
    fn derive_name_uses_last_segment() {
        assert_eq!(derive_name("https://www.youtube.com/@someone/live"), "someone");
        assert_eq!(derive_name("https://www.twitch.tv/name/"), "name");
    }
}

#[cfg(all(test, unix))]
mod e2e {
    use super::*;
    use crate::testutil::Sandbox;
    use std::time::Duration;

    async fn wait_for(p: &mut PolicyParts, secs: u64, mut pred: impl FnMut(&PolicyEvent) -> bool) -> PolicyEvent {
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
    fn edits_are_saved_and_reach_the_engine() {
        let sb = Sandbox::new("policy");
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let mut p = start(crate::engine::start());
            p.client.send(PolicyCommand::AddSite(NewSite {
                name: "Fake".into(),
                exe_path: sb.script.to_string_lossy().into(),
                interval_secs: Some(5),
                checker_args: "--dump-json {urls}".into(),
                recorder_args: "RECORD {url}".into(),
                url_template: "https://x.test/{username}".into(),
                output_dir: sb.dir.join("out").to_string_lossy().into(),
                folder_layout: FolderLayout::default(),
                log_checker_stdout: false,
                log_checker_stderr: false,
                end_after_minutes: Some(30),
                file_exists_grace_secs: Some(60),
                growth_grace_secs: Some(60),
                stall_window_secs: Some(120),
                file_check_interval_secs: Some(30),
                restart_clear_minutes: Some(1440),
                restart_threshold: Some(1),
                color: None,
            }));
            let ev = wait_for(&mut p, 10, |e| matches!(e, PolicyEvent::Settings(s) if !s.sites.is_empty())).await;
            let PolicyEvent::Settings(s) = ev else { unreachable!() };
            let site_id = s.sites[0].id;

            p.client.send(PolicyCommand::AddStreamer { site_id, name: String::new(), username: "live1".into() });
            let ev = wait_for(&mut p, 60, |e| matches!(e, PolicyEvent::Engine(Event::ProcessUpdate(i)) if i.ytdlp.is_some())).await;
            let PolicyEvent::Engine(Event::ProcessUpdate(info)) = ev else { unreachable!() };
            assert_eq!(info.streamer_name, "live1");
            let saved = settings::try_load().unwrap();
            assert_eq!(saved.streamers.len(), 1);

            let options = StreamerOptions { log_stdout: true, log_stderr: false, ..Default::default() };
            p.client.send(PolicyCommand::SetStreamerOptions { id: info.streamer_id, options: options.clone() });
            wait_for(&mut p, 10, |e| matches!(e, PolicyEvent::Settings(s) if s.streamers[0].options == options)).await;
            assert_eq!(settings::try_load().unwrap().streamers[0].options, options);

            p.client.send(PolicyCommand::SetStreamerEnabled { id: info.streamer_id, enabled: false });
            wait_for(&mut p, 30, |e| matches!(e, PolicyEvent::Engine(Event::ProcessRemoved(_)))).await;
            assert!(!settings::try_load().unwrap().streamers[0].enabled);

            p.client.send(PolicyCommand::Shutdown);
            let thread = p.thread;
            tokio::task::spawn_blocking(move || thread.join()).await.unwrap().unwrap();
        });
    }
}
