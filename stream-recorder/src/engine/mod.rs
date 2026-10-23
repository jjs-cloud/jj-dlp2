//! The engine: owns settings, per-site checker workers and recorder processes.

// Any changes to the engine should first be approved by the owner.

mod checker;
mod recorder;
mod sampler;
mod template;

use crate::api::*;
use crate::settings;
use checker::{CheckReport, LiveEntry, SiteSnapshot, SiteWorker};
use recorder::{RecorderHandle, RecorderSpec, SharedTracked};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc::{self, UnboundedReceiver, UnboundedSender};

/// Messages from engine background tasks back to the engine loop.
pub(crate) enum Internal {
    CheckResult { site_id: SiteId, at: u64, result: Result<CheckReport, String> },
    RecorderGone(RecorderId),
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
        }
    }

    async fn run(mut self, mut cmd_rx: UnboundedReceiver<Command>) {
        let mut internal_rx = self.internal_rx.take().expect("internal rx");
        sampler::spawn_sampler(self.tracked.clone(), self.events.clone(), self.internal_tx.clone());
        self.emit(Event::Snapshot(self.settings.clone()));
        self.sync_workers();
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

    fn alloc_id(&mut self) -> u64 {
        let id = self.settings.next_id;
        self.settings.next_id += 1;
        id
    }

    fn persist(&self) {
        if let Err(e) = settings::save(&self.settings) {
            self.log(format!("failed to save settings: {e}"));
        }
        self.emit(Event::Snapshot(self.settings.clone()));
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
            Command::RequestSnapshot => self.emit(Event::Snapshot(self.settings.clone())),
            Command::AddSite(n) => {
                let id = self.alloc_id();
                self.settings.sites.push(Site {
                    id,
                    name: n.name,
                    exe_path: n.exe_path,
                    interval_secs: n.interval_secs,
                    check_mode: n.check_mode,
                    checker_args: n.checker_args,
                    downloader_args: n.downloader_args,
                    output_dir: n.output_dir,
                });
                self.persist();
                self.sync_workers();
            }
            Command::UpdateSite(site) => {
                if let Some(slot) = self.settings.sites.iter_mut().find(|s| s.id == site.id) {
                    *slot = site;
                    self.persist();
                    self.sync_workers();
                }
            }
            Command::RemoveSite(id) => {
                let doomed: Vec<StreamerId> =
                    self.settings.streamers.iter().filter(|s| s.site_id == id).map(|s| s.id).collect();
                for sid in &doomed {
                    self.stop_streamer_recorders(*sid);
                    self.states.remove(sid);
                }
                self.settings.streamers.retain(|s| s.site_id != id);
                self.settings.sites.retain(|s| s.id != id);
                self.persist();
                self.sync_workers();
            }
            Command::AddStreamer { site_id, name, url } => {
                let url = url.trim().to_string();
                if url.is_empty() || !self.settings.sites.iter().any(|s| s.id == site_id) {
                    return;
                }
                if self.settings.streamers.iter().any(|s| s.site_id == site_id && s.url == url) {
                    self.log(format!("{url} is already monitored"));
                    return;
                }
                let name = if name.trim().is_empty() { derive_name(&url) } else { name.trim().to_string() };
                let id = self.alloc_id();
                self.settings.streamers.push(Streamer { id, site_id, name, url, enabled: true });
                self.persist();
                self.sync_workers();
            }
            Command::RemoveStreamer(id) => {
                self.stop_streamer_recorders(id);
                self.states.remove(&id);
                self.settings.streamers.retain(|s| s.id != id);
                self.persist();
                self.sync_workers();
            }
            Command::SetStreamerEnabled { id, enabled } => {
                if let Some(s) = self.settings.streamers.iter_mut().find(|s| s.id == id) {
                    s.enabled = enabled;
                }
                if enabled {
                    self.maybe_start_recording(id);
                } else {
                    self.stop_streamer_recorders(id);
                }
                self.persist();
                self.emit_status(id);
            }
            Command::SetGlobalOutputDir(dir) => {
                self.settings.default_output_dir = dir;
                self.persist();
            }
            Command::SetTheme(name) => {
                self.settings.theme = name;
                self.persist();
            }
            Command::OpenConfigDir => {
                let dir = settings::config_dir();
                let _ = std::fs::create_dir_all(&dir);
                if let Err(e) = open::that_detached(&dir) {
                    self.log(format!("could not open {}: {e}", dir.display()));
                }
            }
            Command::Shutdown => self.begin_shutdown(),
        }
    }

    fn handle_internal(&mut self, msg: Internal) {
        match msg {
            Internal::CheckResult { site_id, at, result } => self.on_check_result(site_id, at, result),
            Internal::RecorderGone(id) => {
                let streamer = self.recorders.remove(&id).map(|h| h.streamer_id);
                self.tracked.lock().unwrap().remove(&id);
                self.emit(Event::ProcessRemoved(id));
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
        SiteSnapshot { site: site.clone(), urls }
    }

    fn on_check_result(&mut self, site_id: SiteId, at: u64, result: Result<CheckReport, String>) {
        let report = match result {
            Ok(r) => r,
            Err(message) => {
                self.log(format!("[{}] check failed: {message}", self.site_name(site_id)));
                self.emit(Event::SiteChecked { id: site_id, at, ok: false, message });
                return;
            }
        };
        self.emit(Event::SiteChecked { id: site_id, at, ok: true, message: report.message.clone() });
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
            self.maybe_start_recording(s.id);
            self.emit_status(s.id);
        }
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
        let vars = [
            ("url", url),
            ("streamer_url", streamer.url.clone()),
            ("output_dir", out_dir.clone()),
            ("id", entry.video_id.clone()),
            ("title", template::sanitize(&entry.title)),
            ("name", template::sanitize(&streamer.name)),
        ];
        let args = template::expand(&site.downloader_args, &vars, None);
        let id = self.alloc_id();
        let spec = RecorderSpec {
            id,
            streamer_id,
            streamer_name: streamer.name.clone(),
            site_name: site.name.clone(),
            exe: site.exe_path.trim().to_string(),
            args,
            cwd: PathBuf::from(out_dir),
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

#[cfg(all(test, unix))]
mod e2e {
    use super::*;
    use std::os::unix::fs::PermissionsExt;
    use std::time::Duration;

    const FAKE: &str = r#"#!/bin/bash
case "$1" in
  --dump-json)
    for u in "$@"; do
      if [[ $u == *live1* ]]; then
        echo "{\"id\":\"vid1\",\"title\":\"T\",\"is_live\":true,\"webpage_url\":\"https://x.test/watch/vid1\",\"original_url\":\"$u\"}"
      fi
    done
    exit 1;;
  RECORD)
    if [[ -z "$BOOT" ]]; then BOOT=1 "$0" "$@" & wait; exit $?; fi
    "$(dirname "$0")/ffmpeg" 300 & FF=$!
    trap 'kill $FF 2>/dev/null; sleep 1; exit 0' INT TERM
    wait;;
esac
"#;

    #[test]
    fn records_and_confirms_exit() {
        let dir = std::env::temp_dir().join(format!("sr-e2e-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        unsafe {
            std::env::set_var("XDG_CONFIG_HOME", dir.join("cfg"));
            std::env::set_var("HOME", &dir);
        }
        let script = dir.join("fake-ytdlp");
        std::fs::write(&script, FAKE).unwrap();
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
        std::fs::copy("/bin/sleep", dir.join("ffmpeg")).unwrap();

        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let mut p = start();
            p.client.send(Command::AddSite(NewSite {
                name: "Fake".into(),
                exe_path: script.to_string_lossy().into(),
                interval_secs: 5,
                check_mode: CheckMode::DumpJson,
                checker_args: "--dump-json {urls}".into(),
                downloader_args: "RECORD {url} {id}".into(),
                output_dir: dir.join("out").to_string_lossy().into(),
            }));
            let mut site_id = None;
            let mut seen_pids: Vec<u32> = Vec::new();
            let mut full_tree = false;
            let mut disabled = false;
            let deadline = tokio::time::Instant::now() + Duration::from_secs(60);
            let mut removed = false;
            while tokio::time::Instant::now() < deadline && !removed {
                let ev = tokio::time::timeout(Duration::from_secs(5), p.events.recv()).await;
                let Ok(Some(ev)) = ev else { continue };
                match ev {
                    Event::Snapshot(s) => {
                        if site_id.is_none() {
                            if let Some(site) = s.sites.first() {
                                site_id = Some(site.id);
                                p.client.send(Command::AddStreamer {
                                    site_id: site.id,
                                    name: String::new(),
                                    url: "https://x.test/live1".into(),
                                });
                            }
                        }
                    }
                    Event::ProcessUpdate(info) => {
                        for s in info.root.iter().chain(info.ytdlp.iter()).chain(info.ffmpeg.iter()) {
                            if !seen_pids.contains(&s.pid) {
                                seen_pids.push(s.pid);
                            }
                        }
                        if info.has_bootloader && info.ytdlp.is_some() && !info.ffmpeg.is_empty() {
                            assert_ne!(info.root.as_ref().unwrap().pid, info.ytdlp.as_ref().unwrap().pid);
                            full_tree = true;
                            if !disabled {
                                disabled = true;
                                let sid = info.streamer_id;
                                p.client.send(Command::SetStreamerEnabled { id: sid, enabled: false });
                            }
                        }
                    }
                    Event::ProcessRemoved(_) => {
                        for pid in &seen_pids {
                            let state = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok().and_then(|t| t.rsplit(") ").next().and_then(|r| r.chars().next()));
                            assert!(matches!(state, None | Some('Z')), "pid {pid} still alive at removal (state {state:?})");
                        }
                        removed = true;
                    }
                    _ => {}
                }
            }
            assert!(full_tree, "never saw bootloader + yt-dlp + ffmpeg");
            assert!(removed, "recorder was never removed after disable");
            p.client.send(Command::Shutdown);
            let thread = p.thread;
            tokio::task::spawn_blocking(move || thread.join()).await.unwrap().unwrap();
        });
        let _ = std::fs::remove_dir_all(&dir);
    }
}
