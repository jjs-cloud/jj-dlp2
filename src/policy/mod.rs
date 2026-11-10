//! Policy layer: owns the config files, applies edits, tells the engine to reload and decides when it records.

use crate::api::{
    now_secs, Command, Control, EngineClient, Event, FolderLayout, GlobalNotify, ProcessInfo, Quality, RecorderId, RecorderState, Settings,
    Site, SiteId, Streamer, StreamerId, StreamerOptions, StreamerStatus, LOG_HISTORY_MAX,
};
use crate::engine::{logfile, EngineParts};
use crate::settings;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use tokio::sync::mpsc::{self, UnboundedReceiver, UnboundedSender};
use tokio::sync::oneshot;

mod decision;
mod defaults;
pub mod disks;
mod files;
mod gql;
pub mod layout;
pub mod live;
pub mod logtail;
pub mod notify;
mod outpath;
pub mod posts;
mod priority;
mod probe;
mod quality;
mod restarts;
mod schedule;
mod segments;
pub mod uilog;
pub mod update;
mod warnings;
use decision::Decision;
use files::{FileTracker, Thresholds};
pub use defaults::{new_site, preset_for_name, Preset, DEFAULT_SITE_COLOR};
pub use files::RecordingFile;
use live::{LiveTracker, PartRecord};
use notify::{Gate, Kind, LiveStatus, Message, Targets};
use quality::QualityTracker;
use restarts::RestartTracker;
use segments::{Plan, Splitter, Step};
use update::UpdateConfig;
pub use warnings::Warnings;
use warnings::WarnTracker;

const UPDATE_CHECK_EVERY: Duration = Duration::from_secs(6 * 3600);
const SCHEDULE_CHECK_EVERY: Duration = Duration::from_secs(10);
const POSTS_TICK_EVERY: Duration = Duration::from_millis(500);

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
    pub forward_output: bool,
    pub interrupt_patterns: String,
    pub error_patterns: String,
    pub warn_interrupts: bool,
    pub warn_errors: bool,
    pub color: Option<String>,
    pub notify: bool,
    pub notify_desktop: bool,
    pub notify_ntfy: bool,
    pub ntfy_topic: String,
    pub notify_cooldown_minutes: Option<u64>,
    pub notify_timeout_secs: Option<u64>,
    pub notify_confirmed: bool,
    pub notify_stalled: bool,
    pub notify_restarted: bool,
    pub notify_on_launch: bool,
    pub notify_live: bool,
    pub warnings_enabled: bool,
    pub notify_interrupts: bool,
    pub notify_errors: bool,
    pub warn_stalled: bool,
    pub warn_restarts: bool,
    pub split_enabled: bool,
    pub split_minutes: Option<u64>,
    pub auto_suffix: bool,
    pub paused: bool,
    pub upgrade_quality: bool,
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
            forward_output: self.forward_output,
            interrupt_patterns: self.interrupt_patterns,
            error_patterns: self.error_patterns,
            warn_interrupts: self.warn_interrupts,
            warn_errors: self.warn_errors,
            color: self.color,
            notify: self.notify,
            notify_desktop: self.notify_desktop,
            notify_ntfy: self.notify_ntfy,
            ntfy_topic: self.ntfy_topic,
            notify_cooldown_minutes: self.notify_cooldown_minutes,
            notify_timeout_secs: self.notify_timeout_secs,
            notify_confirmed: self.notify_confirmed,
            notify_stalled: self.notify_stalled,
            notify_restarted: self.notify_restarted,
            notify_on_launch: self.notify_on_launch,
            notify_live: self.notify_live,
            warnings_enabled: self.warnings_enabled,
            notify_interrupts: self.notify_interrupts,
            notify_errors: self.notify_errors,
            warn_stalled: self.warn_stalled,
            warn_restarts: self.warn_restarts,
            split_enabled: self.split_enabled,
            split_minutes: self.split_minutes,
            auto_suffix: self.auto_suffix,
            paused: self.paused,
            upgrade_quality: self.upgrade_quality,
        }
    }
}

/// A warning chip on a streamer's row that can be dismissed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WarningChip {
    Restarted,
    Interrupt,
    Error,
    Stalled,
}

#[derive(Debug, Clone)]
pub enum PolicyCommand {
    RequestSnapshot,
    AddSite(NewSite),
    UpdateSite(Site),
    /// Swaps two sites in the saved list, which sets their order in the full and compact views.
    SwapSites(SiteId, SiteId),
    AddStreamer { site_id: SiteId, name: String, username: String },
    /// Adds a streamer to the site named like `preset`, creating that site first when it does not exist.
    AddStreamerToPreset { preset: Preset, name: String, username: String },
    RemoveStreamer(StreamerId),
    SetStreamerEnabled { id: StreamerId, enabled: bool },
    SetStreamerOptions { id: StreamerId, options: StreamerOptions },
    /// Renames a streamer; a blank name uses the one derived from its URL.
    SetStreamerName { id: StreamerId, name: String },
    /// Saves the priority order (highest first), the always-record streamers and the recording limit.
    SetPriority { order: Vec<StreamerId>, always: HashSet<StreamerId>, max: Option<u32> },
    SetGlobalOutputDir(String),
    /// Saves the global notification and warning settings.
    SetGlobalNotify(GlobalNotify),
    OpenConfigDir,
    /// Opens the file a recorder is writing, once its path is verified.
    OpenRecording(RecorderId),
    /// Sends a test notification to the given targets, ignoring cooldowns.
    TestNotification { site_name: String, targets: Targets, folder: PathBuf },
    /// Re-runs the engine's setup checks.
    Recheck,
    /// Resets every streamer's restart, interrupt and error counts and dismisses stall warnings.
    ClearWarnings,
    /// Dismisses one warning chip on one streamer's row.
    DismissWarning(StreamerId, WarningChip),
    /// Looks for a new release now, even when automatic checks are off.
    CheckForUpdate,
    /// Downloads the available update, then stops the engine and restarts into it.
    InstallUpdate,
    /// Writes the replacement config over the broken files that were backed up at launch.
    DiscardBrokenConfig,
    /// Saves the updater settings and re-checks.
    SetUpdateConfig(UpdateConfig),
    /// An action for the Posts monitor.
    Posts(posts::PostsCommand),
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
    /// A streamer's recording resolution and frame rate; `None` once it has no recording.
    Resolution(StreamerId, Option<Quality>),
    /// Streamer id to its restart count; streamers with none are absent.
    Restarts(HashMap<StreamerId, u32>),
    /// Streamer id to its interrupt and error counts; streamers with none are absent.
    Warnings(HashMap<StreamerId, Warnings>),
    /// New output lines per recording, oldest first; batched, and sent only for sites with output on.
    Output(HashMap<RecorderId, Vec<String>>),
    /// A state change of the Posts monitor.
    Posts(posts::PostsEvent),
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

/// Result of an ffprobe run on a streamer's recording, fed back into the policy loop.
struct ProbeMsg {
    streamer_id: StreamerId,
    result: Result<Quality, probe::ProbeError>,
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

/// Starts the policy layer on its own thread, along with the managed engine it controls.
pub fn start() -> PolicyParts {
    let engine = crate::engine::start(Control::Managed);
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
            let (posts_tx, posts_rx) = mpsc::unbounded_channel();
            rt.block_on(Policy::new(client, ev_tx, upd_tx, file_tx, gql_tx, posts_tx).run(
                cmd_rx, events, done_rx, upd_rx, file_rx, gql_rx, posts_rx,
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
    warnings: WarnTracker,
    /// Output lines waiting to be sent to front ends, per recording.
    pending_output: HashMap<RecorderId, Vec<String>>,
    last_files: HashMap<RecorderId, RecordingFile>,
    gate: Gate,
    file_tx: UnboundedSender<FileJob>,
    gql_tx: UnboundedSender<GqlJob>,
    /// Running recorders as (streamer id, root pid), used for log toggle markers.
    recorder_pids: HashMap<RecorderId, (StreamerId, u32)>,
    /// Latest status the engine reported for each streamer.
    statuses: HashMap<StreamerId, StreamerStatus>,
    /// Live streamers whose recording is held back by an intro delay.
    waiting: HashSet<StreamerId>,
    /// Streamers whose current part was prepared but whose recorder has not started yet.
    part_pending: HashSet<StreamerId>,
    /// Follows each streamer's recorders and the split in flight.
    splitter: Splitter,
    /// Recorders stopped on purpose by a split, so their exit is not a restart.
    retiring: HashSet<RecorderId>,
    /// Part-1 files to rename once the recorder writing them has exited.
    rename_on_exit: HashMap<RecorderId, (StreamerId, PathBuf)>,
    /// The part number in the recorder arguments last sent to the engine for each streamer.
    override_part: HashMap<StreamerId, u32>,
    /// Streamers already told to stop for a higher-priority one, so the stop is logged once.
    evicting: HashSet<StreamerId>,
    /// Names of the streamers evicted for a held live notification, by the streamer it is for.
    evicted_notes: HashMap<StreamerId, Vec<String>>,
    /// Arms against each recording's probed quality and finds higher qualities worth restarting for.
    quality: QualityTracker,
    probe_tx: UnboundedSender<ProbeMsg>,
    probe_rx: Option<UnboundedReceiver<ProbeMsg>>,
    posts: posts::PostsMonitor,
}

/// Longest output line, in characters, passed on to front ends.
const MAX_OUTPUT_LINE_CHARS: usize = 2000;

/// The line cut to `MAX_OUTPUT_LINE_CHARS` characters.
fn clip_line(line: &str) -> String {
    match line.char_indices().nth(MAX_OUTPUT_LINE_CHARS) {
        Some((end, _)) => line[..end].to_string(),
        None => line.to_string(),
    }
}

impl Policy {
    fn new(
        engine: EngineClient,
        events: UnboundedSender<PolicyEvent>,
        upd_tx: UnboundedSender<UpdateMsg>,
        file_tx: UnboundedSender<FileJob>,
        gql_tx: UnboundedSender<GqlJob>,
        posts_tx: UnboundedSender<posts::PostsMsg>,
    ) -> Self {
        let settings = settings::load_guarded();
        let live = LiveTracker::load(&settings, now_secs());
        let mut posts = posts::PostsMonitor::new(events.clone(), posts_tx, settings.default_output_dir.clone());
        posts.set_global_notify(&settings.notifications);
        let (probe_tx, probe_rx) = mpsc::unbounded_channel();
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
            warnings: WarnTracker::default(),
            pending_output: HashMap::new(),
            last_files: HashMap::new(),
            gate: Gate::default(),
            file_tx,
            gql_tx,
            recorder_pids: HashMap::new(),
            statuses: HashMap::new(),
            waiting: HashSet::new(),
            part_pending: HashSet::new(),
            splitter: Splitter::default(),
            retiring: HashSet::new(),
            rename_on_exit: HashMap::new(),
            override_part: HashMap::new(),
            evicting: HashSet::new(),
            evicted_notes: HashMap::new(),
            quality: QualityTracker::default(),
            probe_tx,
            probe_rx: Some(probe_rx),
            posts,
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
        mut posts_msgs: UnboundedReceiver<posts::PostsMsg>,
    ) {
        let mut cmds_open = true;
        let mut update_tick = tokio::time::interval(UPDATE_CHECK_EVERY);
        update_tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        let mut restart_tick = tokio::time::interval(Duration::from_secs(30));
        restart_tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        let mut delay_tick = tokio::time::interval(Duration::from_secs(1));
        delay_tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        let mut schedule_tick = tokio::time::interval(SCHEDULE_CHECK_EVERY);
        schedule_tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        let mut posts_tick = tokio::time::interval(POSTS_TICK_EVERY);
        posts_tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        let mut probe_msgs = self.probe_rx.take().expect("probe receiver");
        self.emit(PolicyEvent::LogHistory(uilog::tail(LOG_HISTORY_MAX)));
        self.log(format!("--- session started (v{}) ---", update::CURRENT));
        for line in crate::resets::take_log() {
            self.log(line);
        }
        self.emit_settings();
        self.emit_update_config();
        self.emit_live();
        self.emit_last_live();
        self.posts.emit_snapshot();
        self.start_backfill();
        outpath::clear_sidecars();
        self.sync_overrides();
        self.run_schedules();
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
                        self.flush_warnings();
                        self.flush_output();
                    }
                    None => break,
                },
                _ = &mut engine_done => break,
                _ = update_tick.tick() => self.check_now(),
                _ = restart_tick.tick() => self.expire_restarts(),
                _ = schedule_tick.tick() => self.run_schedules(),
                _ = posts_tick.tick() => self.posts.tick(),
                _ = delay_tick.tick() => {
                    self.release_due_delays();
                    self.split_tick();
                    self.quality_tick();
                }
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
                msg = posts_msgs.recv() => {
                    if let Some(m) = msg {
                        self.posts.on_msg(m);
                    }
                }
                msg = probe_msgs.recv() => {
                    if let Some(m) = msg {
                        self.on_probe(m);
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
        if let Event::Output { id, recorder, line } = &ev {
            self.on_output(*id, line);
            self.pending_output.entry(*recorder).or_default().push(clip_line(line));
            return;
        }
        if matches!(ev, Event::ProcessRemoved(_)) {
            self.flush_output();
        }
        match &ev {
            Event::StreamerStatus { id, status } => {
                self.statuses.insert(*id, *status);
                self.quality.on_check(*id, status.quality);
                self.quality.on_recording(*id, status.recording);
                if status.live && status.quality.is_none() && self.upgrade_on(*id) && self.quality.first_missing_quality(*id) {
                    let name = self.streamer_name(*id);
                    self.log(format!("{name}: quality upgrade inactive: the checker output has no video resolution"));
                }
                self.live.streamer_checked(*id, !status.check_failed);
                if let (Some(at), Some((_, secs))) = (status.last_checked, live::lookup(&self.settings, *id)) {
                    let first = self.gate.first_check(*id);
                    let on_launch = self.site_of(*id).is_some_and(|s| s.notify_on_launch);
                    let started = self.live.status(*id, at, status.live, secs);
                    if started {
                        self.gate.take_deferred(*id);
                    }
                    if started && (!first || on_launch) {
                        self.notify_event(*id, Kind::Live, 0);
                    }
                }
                if status.recording || !status.live {
                    self.part_pending.remove(id);
                }
                if !status.recording {
                    self.evicting.remove(id);
                }
                if !status.live && !status.recording {
                    self.gate.take_held(*id);
                    self.warnings.end_span(*id);
                    self.quality.end_span(*id);
                }
                self.on_restart_status(*id, status.recording);
                self.reconcile(*id);
                self.maybe_upgrade(*id);
            }
            Event::ProcessUpdate(p) => self.on_process_update(p),
            Event::ProcessRemoved(id) => {
                self.recorder_pids.remove(id);
                self.retiring.remove(id);
                if let Some((streamer_id, path)) = self.rename_on_exit.remove(id) {
                    self.rename_part_one(streamer_id, &path);
                }
                if let Some(step) = self.splitter.on_gone(*id, Instant::now()) {
                    self.run_step(step);
                }
                if let Some(streamer_id) = self.files.remove(*id) {
                    if !self.recorder_pids.values().any(|(sid, _)| *sid == streamer_id) {
                        outpath::remove_sidecar(streamer_id);
                        self.emit(PolicyEvent::Resolution(streamer_id, None));
                    }
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
        let before = self.restarts.count(id);
        self.restarts.on_status(id, recording, enabled, Duration::from_secs(end_after_secs), Instant::now());
        let after = self.restarts.count(id);
        let threshold = self.site_of(id).map_or(1, |s| s.restart_threshold_or_default());
        if before < threshold && after >= threshold {
            self.notify_event(id, Kind::Restarted, after);
        }
        self.flush_restarts();
    }

    /// The site a streamer belongs to.
    fn site_of(&self, streamer_id: StreamerId) -> Option<&Site> {
        let site_id = self.settings.streamers.iter().find(|s| s.id == streamer_id)?.site_id;
        self.settings.sites.iter().find(|s| s.id == site_id)
    }

    /// The streamer and site when notifications of `kind` are on for the streamer.
    fn notify_target(&self, id: StreamerId, kind: Kind) -> Option<(Streamer, Site)> {
        if self.settings.notifications.disable_notifications {
            return None;
        }
        let st = self.settings.streamers.iter().find(|s| s.id == id)?;
        let site = self.site_of(id)?;
        let kind_on = match kind {
            Kind::Live => site.notify_live,
            Kind::Stalled => site.notify_stalled,
            Kind::Restarted => site.notify_restarted,
            Kind::Interrupt => site.notify_interrupts,
            Kind::Error => site.notify_errors,
        };
        (st.options.notify.unwrap_or(site.notify) && kind_on).then(|| (st.clone(), site.clone()))
    }

    /// Notifies about a streamer unless it is off or cooling down; a live notification says why it isn't recording, or waits for confirmed growth.
    fn notify_event(&mut self, id: StreamerId, kind: Kind, restarts: u32) {
        let Some((st, site)) = self.notify_target(id, kind) else { return };
        let now = Instant::now();
        let cooldown = Duration::from_secs(site.notify_cooldown_or_default().saturating_mul(60));
        if !self.gate.ready(id, kind, cooldown, now) {
            return;
        }
        let mut live = LiveStatus::Unconfirmed;
        let mut evicted = Vec::new();
        if kind == Kind::Live {
            if let Decision::NotRecording(reason) = self.decision_for(&st) {
                live = LiveStatus::NotRecording(reason);
                if reason.defers_notice() {
                    self.gate.defer(id);
                }
            } else {
                evicted = self.evicted_by(id);
                if site.notify_confirmed {
                    self.evicted_notes.insert(id, evicted);
                    self.gate.hold(id);
                    return;
                }
            }
        }
        self.send_notification(&st, &site, kind, restarts, live, &evicted, now);
    }

    /// Sends the live notification that was waiting for the file to be confirmed growing.
    fn on_growth_confirmed(&mut self, id: StreamerId) {
        let evicted = self.evicted_notes.remove(&id).unwrap_or_default();
        if !self.gate.take_held(id) {
            return;
        }
        let Some((st, site)) = self.notify_target(id, Kind::Live) else { return };
        let now = Instant::now();
        let cooldown = Duration::from_secs(site.notify_cooldown_or_default().saturating_mul(60));
        if self.gate.ready(id, Kind::Live, cooldown, now) {
            self.send_notification(&st, &site, Kind::Live, 0, LiveStatus::Confirmed, &evicted, now);
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn send_notification(&mut self, st: &Streamer, site: &Site, kind: Kind, restarts: u32, live: LiveStatus, evicted: &[String], now: Instant) {
        let targets = Targets {
            desktop_timeout_secs: site.notify_desktop.then(|| site.notify_timeout_or_default()),
            ntfy_topic: if site.notify_ntfy { notify::effective_topic(&site.ntfy_topic, &self.settings.notifications.ntfy_topic) } else { None },
        };
        if targets.is_empty() {
            return;
        }
        self.gate.mark_sent(st.id, kind, now);
        let msg = match kind {
            Kind::Live => Message {
                title: format!("{} is live", st.name),
                body: notify::live_body(live, &site.name, evicted),
                click: Some(st.url.clone()),
            },
            Kind::Stalled => Message {
                title: format!("{}: recording stalled", st.name),
                body: format!("{}: the recording file is missing or has stopped growing", site.name),
                click: Some(st.url.clone()),
            },
            Kind::Restarted => Message {
                title: format!("{}: recording restarted", st.name),
                body: format!("{}: restarted {restarts} time(s)", site.name),
                click: Some(st.url.clone()),
            },
            Kind::Interrupt => Message {
                title: format!("{}: recording interrupted", st.name),
                body: format!("{}: the recorder output matched an interrupt pattern", site.name),
                click: Some(st.url.clone()),
            },
            Kind::Error => Message {
                title: format!("{}: recording error", st.name),
                body: format!("{}: the recorder output matched an error pattern", site.name),
                click: Some(st.url.clone()),
            },
        };
        let folder = self.recording_dir(st.id);
        self.dispatch_notification(msg, targets, false, Some(folder));
    }

    /// Delivers a notification on a blocking thread, adding the free space of `folder`, and logs any failure.
    fn dispatch_notification(&self, msg: Message, targets: Targets, test: bool, folder: Option<PathBuf>) {
        if cfg!(test) {
            return;
        }
        let events = self.events.clone();
        tokio::task::spawn_blocking(move || {
            let msg = match folder {
                Some(folder) => notify::with_disk_space(msg, &folder),
                None => msg,
            };
            for line in notify::deliver(&msg, &targets, test) {
                uilog::append(&line);
                let _ = events.send(PolicyEvent::Log(line));
            }
        });
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

    /// Matches a recorder output line against its site's patterns, logs the first hit of each kind per live span and notifies.
    fn on_output(&mut self, id: StreamerId, line: &str) {
        let settings = &self.settings;
        let site = settings.streamers.iter().find(|s| s.id == id).and_then(|st| settings.sites.iter().find(|x| x.id == st.site_id));
        let Some(site) = site else { return };
        let hits = self.warnings.on_line(id, site, line);
        if hits.is_empty() {
            return;
        }
        let name = self.settings.streamers.iter().find(|s| s.id == id).map(|s| s.name.clone()).unwrap_or_default();
        for hit in hits {
            if hit.fresh {
                self.log(format!("{name}: {}: {}", hit.kind.log_text(), hit.pattern));
            }
            let kind = match hit.kind {
                warnings::WarnKind::Interrupt => Kind::Interrupt,
                warnings::WarnKind::Error => Kind::Error,
            };
            self.notify_event(id, kind, 0);
        }
    }

    /// Sends the batched output lines to front ends.
    fn flush_output(&mut self) {
        if !self.pending_output.is_empty() {
            let output = std::mem::take(&mut self.pending_output);
            self.emit(PolicyEvent::Output(output));
        }
    }

    fn flush_warnings(&mut self) {
        if self.warnings.take_dirty() {
            self.emit_warnings();
        }
    }

    fn emit_warnings(&self) {
        self.emit(PolicyEvent::Warnings(self.warnings.counts()));
    }

    fn on_process_update(&mut self, p: &ProcessInfo) {
        if p.state == RecorderState::Stopping && !self.retiring.contains(&p.id) {
            self.restarts.stopping(p.streamer_id);
        }
        if matches!(p.state, RecorderState::Starting | RecorderState::Running) {
            self.splitter.on_running(p.streamer_id, p.id, Instant::now());
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
        if let Some(streamer_id) = up.stalled_for {
            self.notify_event(streamer_id, Kind::Stalled, 0);
        }
        if let Some(streamer_id) = up.confirmed_for {
            self.on_growth_confirmed(streamer_id);
            if let Some(step) = self.splitter.on_confirmed(streamer_id, id) {
                self.run_step(step);
            }
        }
        if let Some(streamer_id) = up.confirmed_for {
            self.probe_resolution(id, streamer_id);
        }
        self.note_part_one_file(id);
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

    /// Runs ffprobe on the recording's file in the background and feeds its quality back to the policy loop.
    fn probe_resolution(&self, id: RecorderId, streamer_id: StreamerId) {
        let Some(path) = self.files.trusted_path(id).map(Path::to_path_buf) else { return };
        let tx = self.probe_tx.clone();
        tokio::spawn(async move {
            let result = probe::quality(&path).await;
            let _ = tx.send(ProbeMsg { streamer_id, result });
        });
    }

    /// Shows a probed recording's quality or logs the probe failure, and feeds the upgrade tracker.
    fn on_probe(&mut self, m: ProbeMsg) {
        let ProbeMsg { streamer_id, result } = m;
        match result {
            Ok(q) => {
                self.emit(PolicyEvent::Resolution(streamer_id, Some(q)));
                self.quality.on_probe(streamer_id, q, Instant::now());
                self.maybe_upgrade(streamer_id);
            }
            Err(e) => {
                let name = self.streamer_name(streamer_id);
                if self.upgrade_on(streamer_id) && self.quality.first_probe_failure(streamer_id) {
                    self.log(format!("{name}: quality upgrade inactive: ffprobe could not read the recording"));
                }
                self.log(match e {
                    probe::ProbeError::NotFound => format!("{name}: ffprobe not found in PATH; resolution not shown"),
                    probe::ProbeError::Failed(why) => format!("{name}: ffprobe could not read the resolution: {why}"),
                });
            }
        }
    }

    /// True when upgrade-quality is on for the streamer, by its own setting or its site's.
    fn upgrade_on(&self, id: StreamerId) -> bool {
        let Some(streamer) = self.settings.streamers.iter().find(|s| s.id == id) else { return false };
        self.site_of(id).is_some_and(|site| streamer.options.effective_upgrade_quality(site))
    }

    /// Restarts a recording once per live span when the stream now offers a higher quality than is being recorded.
    fn maybe_upgrade(&mut self, id: StreamerId) {
        let Some(streamer) = self.settings.streamers.iter().find(|s| s.id == id).cloned() else { return };
        let on = self.upgrade_on(id);
        let recording = self.statuses.get(&id).is_some_and(|s| s.recording);
        if !on || !recording || self.live.upgraded(id) || self.decision_for(&streamer) != Decision::Record {
            return;
        }
        let Some(up) = self.quality.upgrade(id) else { return };
        self.live.mark_upgraded(id);
        self.log(format!("{}: upgrading quality {} -> {}; restarting the recording", streamer.name, up.from, up.to));
        self.restarts.stopping(id);
        self.splitter.cancel(id);
        self.engine.send(Command::StopRecording { id });
        self.flush_live();
    }

    /// Logs once for each streamer whose quality check is still unarmed after its window.
    fn quality_tick(&mut self) {
        let on: HashSet<StreamerId> = self
            .settings
            .streamers
            .iter()
            .filter(|s| self.settings.sites.iter().any(|x| x.id == s.site_id && s.options.effective_upgrade_quality(x)))
            .map(|s| s.id)
            .collect();
        for u in self.quality.unarmed(Instant::now(), |id| on.contains(&id)) {
            let name = self.streamer_name(u.id);
            self.log(format!(
                "{name}: quality upgrade not armed: the stream reports {} but the recording is {}",
                u.stream, u.recording
            ));
        }
    }

    /// The absolute folder a streamer's recordings are written to.
    fn recording_dir(&self, streamer_id: StreamerId) -> PathBuf {
        let site_id = self.settings.streamers.iter().find(|s| s.id == streamer_id).map(|s| s.site_id);
        let dir = site_id.map_or_else(|| layout::base_dir("", &self.settings.default_output_dir), |id| output_dir(&self.settings, id));
        let dir = PathBuf::from(dir);
        std::path::absolute(&dir).unwrap_or(dir)
    }

    /// Streamers that want a recording slot: enabled ones recording now, or live and past any intro delay.
    fn wanting(&self) -> HashSet<StreamerId> {
        let now = now_secs();
        let wants = |s: &&Streamer| {
            let st = self.statuses.get(&s.id).copied().unwrap_or_default();
            s.enabled && (st.recording || (st.live && decision::decide(s, self.live.span_start(s.id), now) == Decision::Record))
        };
        self.settings.streamers.iter().filter(wants).map(|s| s.id).collect()
    }

    /// The streamers among `wanting` that the priority order and recording limit allow to record.
    fn admitted(&self, wanting: &HashSet<StreamerId>) -> HashSet<StreamerId> {
        let order: Vec<priority::Ranked> = self.settings.streamers.iter().map(|s| (s.id, s.options.always_record)).collect();
        priority::admitted(&order, wanting, self.settings.max_recordings)
    }

    /// Names of the streamers recording now that the priority order no longer lets record, other than `id`.
    fn evicted_by(&self, id: StreamerId) -> Vec<String> {
        let wanting = self.wanting();
        let admitted = self.admitted(&wanting);
        let recording = |s: &Streamer| self.statuses.get(&s.id).is_some_and(|st| st.recording);
        let out = |s: &&Streamer| s.id != id && wanting.contains(&s.id) && !admitted.contains(&s.id) && recording(s);
        self.settings.streamers.iter().filter(out).map(|s| s.name.clone()).collect()
    }

    /// The recording decision for a streamer right now.
    fn decision_for(&self, streamer: &Streamer) -> Decision {
        let base = decision::decide(streamer, self.live.span_start(streamer.id), now_secs());
        let wanting = self.wanting();
        if base != Decision::Record || !wanting.contains(&streamer.id) || self.admitted(&wanting).contains(&streamer.id) {
            return base;
        }
        Decision::NotRecording(notify::NotRecordingReason::LowerPriority)
    }

    /// Applies the recording decision to a streamer, then to the others it affects: recordings the priority order now stops and streamers waiting for a slot.
    fn reconcile(&mut self, id: StreamerId) {
        self.apply_decision(id);
        let affected: Vec<StreamerId> = self
            .settings
            .streamers
            .iter()
            .map(|s| s.id)
            .filter(|o| *o != id && (self.waiting.contains(o) || self.statuses.get(o).is_some_and(|st| st.recording)))
            .collect();
        for other in affected {
            self.apply_decision(other);
        }
    }

    /// Starts a live streamer that should record and stops one that should not.
    fn apply_decision(&mut self, id: StreamerId) {
        let Some(streamer) = self.settings.streamers.iter().find(|s| s.id == id).cloned() else { return };
        let status = self.statuses.get(&id).copied().unwrap_or_default();
        let decision = self.decision_for(&streamer);
        let delayed = matches!(decision, Decision::NotRecording(reason) if reason.defers_notice());
        let was_waiting = if status.live && !status.recording && delayed {
            !self.waiting.insert(id)
        } else {
            self.waiting.remove(&id)
        };
        match decision {
            Decision::Record if status.live && !status.recording => {
                if was_waiting && self.gate.take_deferred(id) {
                    self.gate.clear_sent(id, Kind::Live);
                    self.notify_event(id, Kind::Live, 0);
                }
                self.prepare_part(id);
                self.engine.send(Command::StartRecording { id });
            }
            Decision::NotRecording(reason) if status.recording && !reason.only_delays_start() => {
                if reason == notify::NotRecordingReason::LowerPriority && self.evicting.insert(id) {
                    self.log(format!("Priority: stopped {} (lower priority)", streamer.name));
                }
                self.restarts.stopping(id);
                self.splitter.cancel(id);
                self.engine.send(Command::StopRecording { id });
            }
            _ => {}
        }
    }

    /// Applies the recording decision to every streamer.
    fn reconcile_all(&mut self) {
        let ids: Vec<StreamerId> = self.settings.streamers.iter().map(|s| s.id).collect();
        for id in ids {
            self.apply_decision(id);
        }
    }

    /// Re-applies the decision to streamers held back by an intro delay or a full recording limit.
    fn release_due_delays(&mut self) {
        let ids: Vec<StreamerId> = self.waiting.iter().copied().collect();
        for id in ids {
            self.reconcile(id);
        }
    }

    /// The recorder arguments for a streamer's `part`, including its subfolders and the sidecar options.
    fn override_args(&self, streamer: &Streamer, site: &Site, part: u32) -> Option<String> {
        let layout = streamer.options.folder_layout.unwrap_or(site.folder_layout);
        let dirs = layout::dirs(layout, &site.name, &streamer.name);
        outpath::recorder_override(site.recorder_or_default(), &dirs, &outpath::sidecar_path(streamer.id), part)
    }

    /// Gives the engine each streamer's part 1 recorder arguments.
    fn sync_overrides(&mut self) {
        outpath::ensure_sidecar_dir();
        self.override_part.clear();
        for streamer in &self.settings.streamers {
            let Some(site) = self.settings.sites.iter().find(|s| s.id == streamer.site_id) else { continue };
            let args = self.override_args(streamer, site, 1);
            self.engine.send(Command::SetRecorderOverride { streamer_id: streamer.id, args });
        }
    }

    /// Sends the streamer's recorder arguments for `part` unless the engine already has them.
    fn send_part_override(&mut self, id: StreamerId, part: u32) {
        if self.override_part.get(&id).copied().unwrap_or(1) == part {
            return;
        }
        let Some(streamer) = self.settings.streamers.iter().find(|s| s.id == id) else { return };
        let Some(site) = self.settings.sites.iter().find(|s| s.id == streamer.site_id) else { return };
        let args = self.override_args(streamer, site, part);
        self.engine.send(Command::SetRecorderOverride { streamer_id: id, args });
        self.override_part.insert(id, part);
    }

    /// Before a recorder starts: moves a restarted recording on to its next part and renames part 1 to match.
    fn prepare_part(&mut self, id: StreamerId) {
        let Some(streamer) = self.settings.streamers.iter().find(|s| s.id == id) else { return };
        let Some(site) = self.settings.sites.iter().find(|s| s.id == streamer.site_id) else { return };
        let Some(span) = self.live.span_start(id) else { return };
        if !self.part_pending.insert(id) {
            if let Some(rec) = self.live.part(id).map(|r| r.part) {
                self.send_part_override(id, rec);
            }
            return;
        }
        let opts = &streamer.options;
        let continuing =
            opts.effective_auto_suffix(site) || opts.effective_split_minutes(site).is_some() || opts.intro_split_minutes().is_some();
        let part = match self.live.part(id).cloned() {
            None => {
                self.live.set_part(id, PartRecord { span, part: 1, file: None });
                1
            }
            Some(rec) if continuing => {
                let next = rec.part + 1;
                if let Some(file) = rec.file {
                    self.rename_part_one(id, Path::new(&file));
                }
                self.live.set_part(id, PartRecord { span, part: next, file: None });
                next
            }
            Some(rec) => rec.part,
        };
        self.send_part_override(id, part);
        self.flush_live();
    }

    /// Stores the verified path of a streamer's part 1 file so it can be renamed when a second part appears.
    fn note_part_one_file(&mut self, recorder: RecorderId) {
        let Some(streamer_id) = self.recorder_pids.get(&recorder).map(|(sid, _)| *sid) else { return };
        if self.splitter.current(streamer_id) != Some(recorder) {
            return;
        }
        let Some(path) = self.files.trusted_path(recorder).map(|p| p.to_string_lossy().into_owned()) else { return };
        let Some(rec) = self.live.part(streamer_id).cloned() else { return };
        if rec.part == 1 && rec.file.as_ref() != Some(&path) {
            self.live.set_part(streamer_id, PartRecord { file: Some(path), ..rec });
            self.flush_live();
        }
    }

    /// Renames a streamer's finished part 1 file to `_part1`.
    fn rename_part_one(&self, streamer_id: StreamerId, path: &Path) {
        let Some(target) = segments::suffix_path(path, 1) else { return };
        if !path.is_file() {
            return;
        }
        let name = self.settings.streamers.iter().find(|s| s.id == streamer_id).map(|s| s.name.clone()).unwrap_or_default();
        if target.exists() {
            self.log(format!("{name}: not renaming {}: {} already exists", path.display(), target.display()));
            return;
        }
        match std::fs::rename(path, &target) {
            Ok(()) => self.log(format!("{name}: renamed {} to {}", path.display(), target.display())),
            Err(e) => self.log(format!("{name}: could not rename {}: {e}", path.display())),
        }
    }

    /// Starts the next part of any recording whose part length has passed and gives up on splits that never confirmed.
    fn split_tick(&mut self) {
        let mut plans = HashMap::new();
        for st in &self.settings.streamers {
            let Some(site) = self.settings.sites.iter().find(|s| s.id == st.site_id) else { continue };
            let Some(rec) = self.live.part(st.id) else { continue };
            let minutes = match st.options.intro_split_minutes() {
                Some(m) => (rec.part == 1).then_some(m),
                None => st.options.effective_split_minutes(site),
            };
            let Some(minutes) = minutes else { continue };
            let Some(span) = self.live.span_start(st.id) else { continue };
            let timeout = segments::verify_timeout(Thresholds::from_site(site).interval);
            plans.insert(st.id, Plan { span, minutes, next_part: rec.part + 1, timeout });
        }
        for step in self.splitter.due(Instant::now(), &plans) {
            self.run_step(step);
        }
    }

    /// Carries out one step of a split.
    fn run_step(&mut self, step: Step) {
        match step {
            Step::Start { streamer, part } => {
                self.send_part_override(streamer, part);
                self.log(format!("{}: starting part {part}", self.streamer_name(streamer)));
                self.engine.send(Command::StartSegment { id: streamer });
            }
            Step::Commit { streamer, old, part } => {
                let Some(span) = self.live.span_start(streamer) else { return };
                let first = self.live.part(streamer).and_then(|r| r.file.clone().filter(|_| r.part == 1));
                self.live.set_part(streamer, PartRecord { span, part, file: None });
                self.flush_live();
                self.log(format!("{}: recording part {part}", self.streamer_name(streamer)));
                if self.recorder_pids.contains_key(&old) {
                    self.retiring.insert(old);
                    self.engine.send(Command::StopSegment { id: old });
                    if let Some(file) = first {
                        self.rename_on_exit.insert(old, (streamer, PathBuf::from(file)));
                    }
                } else if let Some(file) = first {
                    self.rename_part_one(streamer, Path::new(&file));
                }
            }
            Step::Abort { streamer, part, stop } => {
                if let Some(id) = stop {
                    self.retiring.insert(id);
                    self.engine.send(Command::StopSegment { id });
                }
                self.log(format!(
                    "{}: part {part} was not confirmed growing; keeping the current recording",
                    self.streamer_name(streamer)
                ));
            }
        }
    }

    /// A streamer's name, or blank when it is unknown.
    fn streamer_name(&self, id: StreamerId) -> String {
        self.settings.streamers.iter().find(|s| s.id == id).map(|s| s.name.clone()).unwrap_or_default()
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
        self.warnings.retain(|id| live::lookup(settings, id).is_some(), |id| settings.sites.iter().any(|s| s.id == id));
        self.gate.retain(|id| live::lookup(settings, id).is_some());
        self.statuses.retain(|id, _| live::lookup(settings, *id).is_some());
        self.waiting.retain(|id| live::lookup(settings, *id).is_some());
        self.evicting.retain(|id| live::lookup(settings, *id).is_some());
        self.evicted_notes.retain(|id, _| live::lookup(settings, *id).is_some());
        self.splitter.retain(|id| live::lookup(settings, id).is_some());
        self.flush_live();
        self.flush_restarts();
        self.flush_warnings();
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

    /// Switches a streamer on or off in memory, releasing its recording state when switched off; false if it is unknown.
    fn apply_enabled(&mut self, id: StreamerId, enabled: bool) -> bool {
        let Some(s) = self.settings.streamers.iter_mut().find(|s| s.id == id) else {
            return false;
        };
        s.enabled = enabled;
        if !enabled {
            self.restarts.stopping(id);
            self.gate.take_held(id);
        }
        true
    }

    /// Applies every schedule start or end that has come due.
    fn run_schedules(&mut self) {
        let now = chrono::Local::now().naive_local();
        let mut changed = Vec::new();
        for i in 0..self.settings.streamers.len() {
            let st = &mut self.settings.streamers[i];
            let Some(due) = schedule::due(&st.options.schedule, &st.schedule_runs, now) else {
                continue;
            };
            if due.stamp_enable {
                st.schedule_runs.last_enable = Some(now);
            }
            if due.stamp_disable {
                st.schedule_runs.last_disable = Some(now);
            }
            let (id, was, name) = (st.id, st.enabled, st.name.clone());
            let want = due.edge == schedule::Edge::Enable;
            if was != want {
                self.apply_enabled(id, want);
                self.log(format!("Schedule: {} {name}", if want { "enabled" } else { "disabled" }));
            }
            changed.push(id);
        }
        if changed.is_empty() {
            return;
        }
        self.save();
        self.emit_settings();
        for id in changed {
            self.reconcile(id);
        }
    }

    /// Saves the config files, tells the engine to reload them, pushes a snapshot and re-applies recording decisions.
    fn commit(&mut self) {
        if self.save() {
            self.engine.send(Command::Reload);
            self.sync_overrides();
        }
        self.emit_settings();
        self.reconcile_all();
    }

    /// Adds a streamer to a site and commits; false when nothing was added.
    fn add_streamer(&mut self, site_id: SiteId, name: String, username: String) -> bool {
        let Some(site) = self.settings.sites.iter().find(|s| s.id == site_id) else {
            return false;
        };
        if username.trim().trim_start_matches('@').is_empty() {
            return false;
        }
        if site.url_template.trim().is_empty() {
            self.log(format!("Set a URL template for {} before adding streamers", site.name));
            return false;
        }
        let url = build_url(&site.url_template, &username);
        if self.settings.streamers.iter().any(|s| s.site_id == site_id && s.url == url) {
            self.log(format!("{url} is already monitored"));
            return false;
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
            schedule_runs: Default::default(),
        });
        self.commit();
        true
    }

    fn handle(&mut self, cmd: PolicyCommand) {
        match cmd {
            PolicyCommand::RequestSnapshot => {
                self.posts.emit_snapshot();
                self.emit_settings();
                self.emit_update_config();
                self.emit_live();
                self.emit_last_live();
                self.emit_restarts();
                self.emit_warnings();
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
            PolicyCommand::SwapSites(a, b) => {
                let sites = &mut self.settings.sites;
                if let (Some(i), Some(j)) = (sites.iter().position(|s| s.id == a), sites.iter().position(|s| s.id == b)) {
                    sites.swap(i, j);
                    self.commit();
                }
            }
            PolicyCommand::AddStreamer { site_id, name, username } => {
                self.add_streamer(site_id, name, username);
            }
            PolicyCommand::AddStreamerToPreset { preset, name, username } => {
                if username.trim().trim_start_matches('@').is_empty() {
                    return;
                }
                let template = new_site(preset);
                let existing = self.settings.sites.iter().find(|s| s.name.eq_ignore_ascii_case(&template.name)).map(|s| s.id);
                let (site_id, created) = match existing {
                    Some(id) => (id, false),
                    None => {
                        let id = self.alloc_id();
                        self.settings.sites.push(template.into_site(id));
                        (id, true)
                    }
                };
                if !self.add_streamer(site_id, name, username) && created {
                    self.commit();
                }
            }
            PolicyCommand::RemoveStreamer(id) => {
                self.settings.streamers.retain(|s| s.id != id);
                self.commit();
                self.prune_live();
            }
            PolicyCommand::SetStreamerEnabled { id, enabled } => {
                if self.apply_enabled(id, enabled) {
                    self.save();
                    self.emit_settings();
                    self.reconcile(id);
                }
            }
            PolicyCommand::SetStreamerOptions { id, options } => {
                if let Some(s) = self.settings.streamers.iter_mut().find(|s| s.id == id) {
                    let options = StreamerOptions { always_record: s.options.always_record, ..options };
                    let old = std::mem::replace(&mut s.options, options.clone());
                    self.mark_log_toggles(id, &old, &options);
                    self.commit();
                    self.run_schedules();
                }
            }
            PolicyCommand::SetStreamerName { id, name } => {
                if let Some(s) = self.settings.streamers.iter_mut().find(|s| s.id == id) {
                    let name = name.trim();
                    s.name = if name.is_empty() { derive_name(&s.url) } else { name.to_string() };
                    self.commit();
                }
            }
            PolicyCommand::SetPriority { order, always, max } => {
                let rank = |id: StreamerId| order.iter().position(|o| *o == id).unwrap_or(usize::MAX);
                self.settings.streamers.sort_by_key(|s| rank(s.id));
                for s in &mut self.settings.streamers {
                    s.options.always_record = always.contains(&s.id);
                }
                self.settings.max_recordings = max.filter(|m| *m > 0);
                self.commit();
            }
            PolicyCommand::SetGlobalOutputDir(dir) => {
                let dir = dir.trim();
                self.settings.default_output_dir = if dir.is_empty() { settings::default_output_dir() } else { dir.to_string() };
                self.posts.set_global_dir(&self.settings.default_output_dir);
                self.commit();
            }
            PolicyCommand::SetGlobalNotify(n) => {
                self.settings.notifications = GlobalNotify { ntfy_topic: n.ntfy_topic.trim().to_string(), ..n };
                self.posts.set_global_notify(&self.settings.notifications);
                self.commit();
            }
            PolicyCommand::OpenConfigDir => {
                let dir = settings::config_dir();
                let _ = std::fs::create_dir_all(&dir);
                if let Err(e) = open::that_detached(&dir) {
                    self.log(format!("could not open {}: {e}", dir.display()));
                }
            }
            PolicyCommand::OpenRecording(id) => self.open_recording(id),
            PolicyCommand::TestNotification { site_name, targets, folder } => {
                let msg = Message {
                    title: "jj-dlp test notification".to_string(),
                    body: format!("{site_name}: notifications are working"),
                    click: None,
                };
                self.log(format!("sending test notification for {site_name}"));
                self.dispatch_notification(msg, targets, true, Some(folder));
            }
            PolicyCommand::Recheck => self.engine.send(Command::Recheck),
            PolicyCommand::ClearWarnings => {
                self.restarts.clear();
                self.flush_restarts();
                self.warnings.clear();
                self.flush_warnings();
                self.files.dismiss_stalls();
                self.emit_files();
            }
            PolicyCommand::DismissWarning(id, chip) => match chip {
                WarningChip::Restarted => {
                    self.restarts.clear_one(id);
                    self.flush_restarts();
                }
                WarningChip::Interrupt | WarningChip::Error => {
                    let kind = if chip == WarningChip::Interrupt { warnings::WarnKind::Interrupt } else { warnings::WarnKind::Error };
                    self.warnings.clear_kind(id, kind);
                    self.flush_warnings();
                }
                WarningChip::Stalled => {
                    self.files.dismiss_stalls_for(id);
                    self.emit_files();
                }
            },
            PolicyCommand::CheckForUpdate => self.run_check(true),
            PolicyCommand::InstallUpdate => self.install_update(),
            PolicyCommand::DiscardBrokenConfig => {
                settings::resume_saving();
                self.posts.resume_saving();
                self.log("replacing the broken config; the old files are kept as .bad copies".to_string());
                self.commit();
            }
            PolicyCommand::SetUpdateConfig(cfg) => {
                if let Err(e) = update::save_config(&cfg) {
                    self.log(format!("failed to save update.json: {e}"));
                }
                self.update_cfg = cfg;
                self.emit_update_config();
                self.run_check(false);
            }
            PolicyCommand::Posts(cmd) => self.posts.handle(cmd),
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
    let site_dir = settings.sites.iter().find(|s| s.id == site_id).map_or("", |s| s.output_dir.as_str());
    layout::base_dir(site_dir, &settings.default_output_dir)
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

    #[test]
    fn clip_line_cuts_on_a_char_boundary() {
        let long = "é".repeat(MAX_OUTPUT_LINE_CHARS + 5);
        assert_eq!(clip_line(&long).chars().count(), MAX_OUTPUT_LINE_CHARS);
        assert_eq!(clip_line("short"), "short");
    }

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
            forward_output: false,
            interrupt_patterns: String::new(),
            error_patterns: String::new(),
            warn_interrupts: true,
            warn_errors: true,
            color: None,
            notify: false,
            notify_desktop: true,
            notify_ntfy: false,
            ntfy_topic: String::new(),
            notify_cooldown_minutes: None,
            notify_timeout_secs: None,
            notify_confirmed: true,
            notify_stalled: true,
            notify_restarted: true,
            notify_on_launch: false,
            notify_live: true,
            warnings_enabled: true,
            notify_interrupts: false,
            notify_errors: false,
            warn_stalled: true,
            warn_restarts: true,
            split_enabled: false,
            split_minutes: None,
            auto_suffix: true,
            paused: false,
            upgrade_quality: false,
        }
    }

    #[test]
    fn output_dir_prefers_site_then_global() {
        let mut s = Settings {
            next_id: 1,
            default_output_dir: "/global".into(),
            max_recordings: None,
            notifications: Default::default(),
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
            let mut p = start();
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
                forward_output: false,
                interrupt_patterns: String::new(),
                error_patterns: String::new(),
                warn_interrupts: true,
                warn_errors: true,
                color: None,
                notify: false,
                notify_desktop: true,
                notify_ntfy: false,
                ntfy_topic: String::new(),
                notify_cooldown_minutes: None,
                notify_timeout_secs: None,
                notify_confirmed: true,
                notify_stalled: true,
                notify_restarted: true,
                notify_on_launch: false,
                notify_live: true,
                warnings_enabled: true,
                notify_interrupts: false,
                notify_errors: false,
                warn_stalled: true,
                warn_restarts: true,
                split_enabled: false,
                split_minutes: None,
                auto_suffix: true,
                paused: false,
                upgrade_quality: false,
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

    fn bare_policy() -> (Policy, UnboundedReceiver<Command>) {
        let (cmd_tx, cmd_rx) = mpsc::unbounded_channel();
        let (ev_tx, _) = mpsc::unbounded_channel();
        let (upd_tx, _) = mpsc::unbounded_channel();
        let (file_tx, _) = mpsc::unbounded_channel();
        let (gql_tx, _) = mpsc::unbounded_channel();
        let (posts_tx, _) = mpsc::unbounded_channel();
        (Policy::new(EngineClient::new(cmd_tx), ev_tx, upd_tx, file_tx, gql_tx, posts_tx), cmd_rx)
    }

    fn one_streamer(p: &mut Policy, options: StreamerOptions, enabled: bool) {
        let site = new_site(Preset::Blank).into_site(1);
        let streamer = Streamer { id: 2, site_id: 1, name: "n".into(), url: "u".into(), enabled, options, schedule_runs: Default::default() };
        p.settings = Settings { next_id: 10, default_output_dir: "out".into(), max_recordings: None, notifications: Default::default(), sites: vec![site], streamers: vec![streamer] };
    }

    fn live_status(p: &mut Policy, recording: bool, checked: u64) {
        let status = StreamerStatus { live: true, recording, last_checked: Some(checked), check_failed: false, quality: None };
        p.on_engine_event(Event::StreamerStatus { id: 2, status });
    }

    fn sent(rx: &mut UnboundedReceiver<Command>) -> Vec<Command> {
        std::iter::from_fn(|| rx.try_recv().ok()).collect()
    }

    fn delay(minutes: u64) -> StreamerOptions {
        StreamerOptions { intro_delay_enabled: true, intro_delay_minutes: minutes, ..Default::default() }
    }

    #[test]
    fn schedule_past_its_end_disables_the_streamer_once() {
        let _sb = Sandbox::new("policy-schedule");
        let (mut p, _rx) = bare_policy();
        let now = chrono::Local::now().naive_local();
        let schedule = crate::api::Schedule {
            enabled: true,
            one_off_start: Some(now - chrono::Duration::hours(2)),
            one_off_end: Some(now - chrono::Duration::hours(1)),
            ..Default::default()
        };
        one_streamer(&mut p, StreamerOptions { schedule, ..Default::default() }, true);
        p.run_schedules();
        assert!(!p.settings.streamers[0].enabled);
        assert!(p.settings.streamers[0].schedule_runs.last_disable.is_some());

        p.settings.streamers[0].enabled = true;
        p.run_schedules();
        assert!(p.settings.streamers[0].enabled);
    }

    fn two_streamers(p: &mut Policy, max: Option<u32>) {
        let site = new_site(Preset::Blank).into_site(1);
        let mk = |id, name: &str| Streamer {
            id,
            site_id: 1,
            name: name.into(),
            url: "u".into(),
            enabled: true,
            options: StreamerOptions::default(),
            schedule_runs: Default::default(),
        };
        p.settings = Settings {
            next_id: 10,
            default_output_dir: "out".into(),
            max_recordings: max,
            notifications: Default::default(),
            sites: vec![site],
            streamers: vec![mk(2, "hi"), mk(3, "lo")],
        };
    }

    fn set_status(p: &mut Policy, id: StreamerId, live: bool, recording: bool) {
        let status = StreamerStatus { live, recording, last_checked: Some(now_secs()), check_failed: false, quality: None };
        p.on_engine_event(Event::StreamerStatus { id, status });
    }

    #[test]
    fn higher_priority_streamer_evicts_the_lowest_at_the_limit() {
        let _sb = Sandbox::new("policy-evict");
        let (mut p, mut rx) = bare_policy();
        two_streamers(&mut p, Some(1));
        set_status(&mut p, 3, true, true);
        sent(&mut rx);
        set_status(&mut p, 2, true, false);
        let cmds = sent(&mut rx);
        assert!(cmds.iter().any(|c| matches!(c, Command::StartRecording { id: 2 })));
        assert!(cmds.iter().any(|c| matches!(c, Command::StopRecording { id: 3 })));
    }

    #[test]
    fn lower_priority_streamer_waits_then_starts_when_a_slot_frees() {
        let _sb = Sandbox::new("policy-wait");
        let (mut p, mut rx) = bare_policy();
        two_streamers(&mut p, Some(1));
        set_status(&mut p, 2, true, true);
        sent(&mut rx);
        set_status(&mut p, 3, true, false);
        assert!(sent(&mut rx).is_empty());
        set_status(&mut p, 2, false, false);
        assert!(sent(&mut rx).iter().any(|c| matches!(c, Command::StartRecording { id: 3 })));
    }

    #[test]
    fn always_record_starts_at_the_limit_and_evicts_the_lowest() {
        let _sb = Sandbox::new("policy-always");
        let (mut p, mut rx) = bare_policy();
        two_streamers(&mut p, Some(1));
        p.settings.streamers[0].options.always_record = true;
        set_status(&mut p, 3, true, true);
        sent(&mut rx);
        set_status(&mut p, 2, true, false);
        let cmds = sent(&mut rx);
        assert!(cmds.iter().any(|c| matches!(c, Command::StartRecording { id: 2 })));
        assert!(cmds.iter().any(|c| matches!(c, Command::StopRecording { id: 3 })));
    }

    #[test]
    fn live_streamer_without_a_delay_is_started() {
        let _sb = Sandbox::new("policy-start");
        let (mut p, mut rx) = bare_policy();
        one_streamer(&mut p, StreamerOptions::default(), true);
        live_status(&mut p, false, now_secs());
        assert!(matches!(sent(&mut rx).as_slice(), [Command::StartRecording { id: 2 }]));
    }

    #[test]
    fn restarted_recording_moves_to_the_next_part() {
        let _sb = Sandbox::new("policy-restart-part");
        let (mut p, mut rx) = bare_policy();
        one_streamer(&mut p, StreamerOptions::default(), true);
        let now = now_secs();
        live_status(&mut p, false, now);
        assert!(matches!(sent(&mut rx).as_slice(), [Command::StartRecording { id: 2 }]));
        live_status(&mut p, true, now + 1);
        sent(&mut rx);
        live_status(&mut p, false, now + 2);
        let cmds = sent(&mut rx);
        assert!(
            matches!(cmds.as_slice(), [Command::SetRecorderOverride { streamer_id: 2, args: Some(a) }, Command::StartRecording { id: 2 }] if a.contains("_part2")),
            "{cmds:?}"
        );
    }

    #[test]
    fn failed_starts_do_not_advance_the_part() {
        let _sb = Sandbox::new("policy-failed-start");
        let (mut p, mut rx) = bare_policy();
        one_streamer(&mut p, StreamerOptions::default(), true);
        let now = now_secs();
        for i in 0..3 {
            live_status(&mut p, false, now + i);
        }
        let cmds = sent(&mut rx);
        assert!(cmds.iter().all(|c| matches!(c, Command::StartRecording { id: 2 })), "{cmds:?}");
        assert_eq!(p.live.part(2).map(|r| r.part), Some(1));
    }

    #[test]
    fn restart_keeps_the_name_when_auto_suffix_and_split_are_off() {
        let _sb = Sandbox::new("policy-restart-plain");
        let (mut p, mut rx) = bare_policy();
        one_streamer(&mut p, StreamerOptions { auto_suffix: Some(false), ..Default::default() }, true);
        let now = now_secs();
        live_status(&mut p, false, now);
        live_status(&mut p, false, now + 1);
        let cmds = sent(&mut rx);
        assert!(matches!(cmds.as_slice(), [Command::StartRecording { id: 2 }, Command::StartRecording { id: 2 }]), "{cmds:?}");
    }

    fn quality_status(p: &mut Policy, recording: bool, checked: u64, quality: Quality) {
        let status = StreamerStatus { live: true, recording, last_checked: Some(checked), check_failed: false, quality: Some(quality) };
        p.on_engine_event(Event::StreamerStatus { id: 2, status });
    }

    fn probed(p: &mut Policy, quality: Quality) {
        p.on_probe(ProbeMsg { streamer_id: 2, result: Ok(quality) });
    }

    /// Runs a live stream offering `first`, a recording probed at `probe`, then a check offering `later`; true when the recording is stopped.
    fn upgrade_stops(options: StreamerOptions, first: Quality, probe: Quality, later: Quality) -> bool {
        let (mut p, mut rx) = bare_policy();
        one_streamer(&mut p, options, true);
        let now = now_secs();
        quality_status(&mut p, false, now, first);
        quality_status(&mut p, true, now + 1, first);
        probed(&mut p, probe);
        quality_status(&mut p, true, now + 2, later);
        sent(&mut rx).iter().any(|c| matches!(c, Command::StopRecording { id: 2 }))
    }

    #[test]
    fn armed_recording_restarts_once_into_the_next_part_when_the_stream_offers_more() {
        let _sb = Sandbox::new("policy-upgrade-quality");
        let (mut p, mut rx) = bare_policy();
        one_streamer(&mut p, StreamerOptions { upgrade_quality: Some(true), ..Default::default() }, true);
        let (low, high) = (Quality::new(1280, 720, None), Quality::new(1920, 1080, None));
        let now = now_secs();
        quality_status(&mut p, false, now, low);
        quality_status(&mut p, true, now + 1, low);
        probed(&mut p, low);
        sent(&mut rx);
        quality_status(&mut p, true, now + 2, high);
        let cmds = sent(&mut rx);
        assert!(cmds.iter().any(|c| matches!(c, Command::StopRecording { id: 2 })), "{cmds:?}");
        assert!(p.live.upgraded(2));
        quality_status(&mut p, false, now + 3, high);
        let cmds = sent(&mut rx);
        assert!(
            matches!(cmds.as_slice(), [Command::SetRecorderOverride { streamer_id: 2, args: Some(a) }, Command::StartRecording { id: 2 }] if a.contains("_part2")),
            "{cmds:?}"
        );
        quality_status(&mut p, true, now + 4, high);
        probed(&mut p, low);
        quality_status(&mut p, true, now + 5, high);
        assert!(!sent(&mut rx).iter().any(|c| matches!(c, Command::StopRecording { .. })));
    }

    #[test]
    fn site_setting_applies_unless_the_streamer_overrides_it() {
        let _sb = Sandbox::new("policy-upgrade-inherit");
        let (low, high) = (Quality::new(1280, 720, None), Quality::new(1920, 1080, None));
        assert!(upgrade_stops(StreamerOptions::default(), low, low, high));
        assert!(!upgrade_stops(StreamerOptions { upgrade_quality: Some(false), ..Default::default() }, low, low, high));
    }

    #[test]
    fn unarmed_recording_is_never_restarted() {
        let _sb = Sandbox::new("policy-upgrade-unarmed");
        let (low, high, higher) = (Quality::new(1280, 720, None), Quality::new(1920, 1080, None), Quality::new(2560, 1440, None));
        assert!(!upgrade_stops(StreamerOptions::default(), high, low, higher));
    }

    #[test]
    fn intro_delay_holds_the_start_until_it_ends() {
        let _sb = Sandbox::new("policy-hold");
        let (mut p, mut rx) = bare_policy();
        one_streamer(&mut p, delay(15), true);
        live_status(&mut p, false, now_secs() - 600);
        assert!(sent(&mut rx).is_empty());
        assert!(p.waiting.contains(&2));

        p.settings.streamers[0].options.intro_delay_minutes = 5;
        p.release_due_delays();
        assert!(matches!(sent(&mut rx).as_slice(), [Command::StartRecording { id: 2 }]));
        assert!(!p.waiting.contains(&2));
    }

    #[test]
    fn intro_delay_never_stops_a_running_recording() {
        let _sb = Sandbox::new("policy-running");
        let (mut p, mut rx) = bare_policy();
        one_streamer(&mut p, delay(15), true);
        live_status(&mut p, true, now_secs() - 60);
        assert!(sent(&mut rx).is_empty());
        assert!(p.waiting.is_empty());
    }

    #[test]
    fn disabled_streamer_is_stopped_and_not_started() {
        let _sb = Sandbox::new("policy-disabled");
        let (mut p, mut rx) = bare_policy();
        one_streamer(&mut p, StreamerOptions::default(), false);
        live_status(&mut p, true, now_secs());
        assert!(matches!(sent(&mut rx).as_slice(), [Command::StopRecording { id: 2 }]));
        live_status(&mut p, false, now_secs() + 60);
        assert!(sent(&mut rx).is_empty());
    }
}
