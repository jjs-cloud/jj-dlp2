//! Samples each recorder's process tree once a second and confirms when it is fully gone.

use super::pidfile;
use super::recorder::SharedTracked;
use super::Internal;
use crate::api::{Event, ProcSample, ProcessInfo, RecorderId, RecorderState};
use std::collections::{HashMap, HashSet};
use std::time::Duration;
use sysinfo::{Pid, ProcessRefreshKind, ProcessStatus, ProcessesToUpdate, System};
use tokio::sync::mpsc::UnboundedSender;

const ORPHAN_GRACE: Duration = Duration::from_secs(20);
/// Samples between updates for a recorder whose state has not changed.
const IDLE_UPDATE_EVERY: u64 = 2;

pub fn spawn_sampler(
    tracked: SharedTracked,
    events: UnboundedSender<Event>,
    internal: UnboundedSender<Internal>,
) {
    std::thread::Builder::new()
        .name("sampler".into())
        .spawn(move || sampler_loop(tracked, events, internal))
        .expect("spawn sampler thread");
}

/// True while any recorder is still tracked.
fn any_tracked(tracked: &SharedTracked) -> bool {
    tracked.lock().unwrap().values().any(|t| !t.gone_sent)
}

fn sampler_loop(tracked: SharedTracked, events: UnboundedSender<Event>, internal: UnboundedSender<Internal>) {
    let mut sys = System::new();
    let mut primed = false;
    let mut tick: u64 = 0;
    let mut last_state: HashMap<RecorderId, RecorderState> = HashMap::new();
    loop {
        std::thread::sleep(Duration::from_secs(1));
        if events.is_closed() {
            return;
        }
        tick += 1;
        if !any_tracked(&tracked) {
            pidfile::update(std::iter::empty());
            primed = false;
            continue;
        }
        sys.refresh_processes_specifics(ProcessesToUpdate::All, true, ProcessRefreshKind::nothing().with_cpu().with_memory());
        // The first refresh after idling only seeds CPU readings.
        if !primed {
            primed = true;
            register(&tracked);
            continue;
        }
        let fresh = fresh_system();
        let children = children_map(&fresh);
        let mut map = tracked.lock().unwrap();
        last_state.retain(|id, _| map.contains_key(id));
        for (&id, t) in map.iter_mut() {
            if t.gone_sent {
                continue;
            }
            let alive = alive_tree(&fresh, &children, t.root_pid, &mut t.seen);
            if t.root_name.is_none() {
                t.root_name = fresh
                    .process(Pid::from_u32(t.root_pid))
                    .map(|p| p.name().to_string_lossy().to_lowercase());
            }
            let (root, ytdlp, has_bootloader, ffmpeg) = classify(&fresh, &sys, &alive, t.root_pid, t.root_name.as_deref());

            if t.root_exit.is_some() && alive.is_empty() {
                t.gone_sent = true;
                let _ = internal.send(Internal::RecorderGone(id));
                continue;
            }
            if let Some(at) = t.root_exited_at {
                if at.elapsed() > ORPHAN_GRACE && !alive.is_empty() {
                    for pid in &alive {
                        if let Some(p) = fresh.process(*pid) {
                            p.kill();
                        }
                    }
                }
            }

            let state = match t.root_exit {
                Some(code) => RecorderState::Exited(code),
                None if t.stopping => RecorderState::Stopping,
                None if root.is_none() && ytdlp.is_none() => RecorderState::Starting,
                None => RecorderState::Running,
            };
            if last_state.get(&id) == Some(&state) && tick % IDLE_UPDATE_EVERY != 0 {
                continue;
            }
            last_state.insert(id, state);
            let _ = events.send(Event::ProcessUpdate(ProcessInfo {
                id,
                streamer_id: t.streamer_id,
                streamer_name: t.streamer_name.clone(),
                site_name: t.site_name.clone(),
                state,
                uptime_secs: t.started.elapsed().as_secs(),
                root,
                ytdlp,
                has_bootloader,
                ffmpeg,
                last_line: t.last_line.clone(),
                cmdline: t.cmdline.clone(),
            }));
        }
        pidfile::update(map.values().filter(|t| !t.gone_sent).flat_map(|t| t.seen.iter().copied()));
    }
}

/// Saves the processes of the running recorders so a relaunch can clean up after a crash.
fn register(tracked: &SharedTracked) {
    let fresh = fresh_system();
    let children = children_map(&fresh);
    let mut map = tracked.lock().unwrap();
    for t in map.values_mut().filter(|t| !t.gone_sent) {
        alive_tree(&fresh, &children, t.root_pid, &mut t.seen);
    }
    pidfile::update(map.values().filter(|t| !t.gone_sent).flat_map(|t| t.seen.iter().copied()));
}

/// A new `System` read from scratch, so a reused pid never keeps the old process's start time or name.
fn fresh_system() -> System {
    let mut sys = System::new();
    sys.refresh_processes_specifics(ProcessesToUpdate::All, true, ProcessRefreshKind::nothing());
    sys
}

fn children_map(sys: &System) -> HashMap<Pid, Vec<Pid>> {
    let mut m: HashMap<Pid, Vec<Pid>> = HashMap::new();
    for (pid, p) in sys.processes() {
        if let Some(parent) = p.parent() {
            // Skip a link to a pid now held by a process that started after this one.
            let reused = sys.process(parent).is_some_and(|pp| {
                p.start_time() != 0 && pp.start_time() != 0 && p.start_time() < pp.start_time()
            });
            if reused {
                continue;
            }
            m.entry(parent).or_default().push(*pid);
        }
    }
    m
}

/// Returns every live (non-zombie) process in the recorder's tree, remembering pids it has seen.
fn alive_tree(
    sys: &System,
    children: &HashMap<Pid, Vec<Pid>>,
    root_pid: u32,
    seen: &mut Vec<(u32, u64)>,
) -> Vec<Pid> {
    let is_alive = |pid: Pid, start: Option<u64>| {
        sys.process(pid)
            .map(|p| p.status() != ProcessStatus::Zombie && start.is_none_or(|s| s == p.start_time()))
            .unwrap_or(false)
    };
    let mut starts: Vec<Pid> = Vec::new();
    let root = Pid::from_u32(root_pid);
    if sys.process(root).is_some() && !seen.iter().any(|(p, _)| *p == root_pid) {
        seen.push((root_pid, sys.process(root).map(|p| p.start_time()).unwrap_or(0)));
    }
    for (p, st) in seen.iter() {
        if is_alive(Pid::from_u32(*p), Some(*st)) {
            starts.push(Pid::from_u32(*p));
        }
    }
    let mut out: Vec<Pid> = Vec::new();
    let mut stack = starts;
    let mut visited: HashSet<Pid> = HashSet::new();
    while let Some(pid) = stack.pop() {
        if !visited.insert(pid) {
            continue;
        }
        if is_alive(pid, None) {
            out.push(pid);
            if let Some(p) = sys.process(pid) {
                if !seen.contains(&(pid.as_u32(), p.start_time())) {
                    seen.push((pid.as_u32(), p.start_time()));
                }
            }
        }
        if let Some(kids) = children.get(&pid) {
            stack.extend(kids.iter().copied());
        }
    }
    out
}

/// Name from the fresh snapshot; CPU and memory from the long-lived system when it is the same process.
fn sample(fresh: &System, sys: &System, pid: Pid) -> Option<ProcSample> {
    let p = fresh.process(pid)?;
    let live = sys.process(pid).filter(|s| s.start_time() == p.start_time());
    Some(ProcSample {
        pid: pid.as_u32(),
        name: p.name().to_string_lossy().into_owned(),
        cpu_percent: live.map_or(0.0, |s| s.cpu_usage()),
        mem_bytes: live.map_or(0, |s| s.memory()),
    })
}

/// Depth of `pid` below `root` following parent links (root = 0).
fn depth(sys: &System, pid: Pid, root: Pid) -> usize {
    let mut d = 0;
    let mut cur = pid;
    while cur != root && d < 64 {
        match sys.process(cur).and_then(|p| p.parent()) {
            Some(par) => {
                cur = par;
                d += 1;
            }
            None => break,
        }
    }
    d
}

/// Picks the real yt-dlp (the deepest process sharing the spawned process's name), the
/// bootloader above it, and any ffmpeg processes.
fn classify(
    fresh: &System,
    sys: &System,
    alive: &[Pid],
    root_pid: u32,
    root_name: Option<&str>,
) -> (Option<ProcSample>, Option<ProcSample>, bool, Vec<ProcSample>) {
    let root = Pid::from_u32(root_pid);
    let name_of = |pid: Pid| {
        fresh.process(pid).map(|p| p.name().to_string_lossy().to_lowercase()).unwrap_or_default()
    };
    let mut same: Vec<Pid> = alive
        .iter()
        .copied()
        .filter(|p| Some(name_of(*p).as_str()) == root_name && !name_of(*p).contains("ffmpeg"))
        .collect();
    same.sort_by_key(|p| depth(fresh, *p, root));
    let has_bootloader = same.len() > 1;
    let ytdlp_pid = same.last().copied();
    let root_sample = if alive.contains(&root) { sample(fresh, sys, root) } else { None };
    let ytdlp = ytdlp_pid.and_then(|p| sample(fresh, sys, p));
    let mut ffmpeg: Vec<ProcSample> = alive
        .iter()
        .filter(|p| name_of(**p).contains("ffmpeg"))
        .filter_map(|p| sample(fresh, sys, *p))
        .collect();
    ffmpeg.sort_by_key(|s| s.pid);
    (root_sample, ytdlp, has_bootloader, ffmpeg)
}
