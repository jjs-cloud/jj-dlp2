//! Samples each recorder's process tree once a second and confirms when it is fully gone.

use super::recorder::SharedTracked;
use super::Internal;
use crate::api::{Event, ProcSample, ProcessInfo, RecorderState};
use crate::outpath;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::Duration;
use sysinfo::{Pid, ProcessRefreshKind, ProcessStatus, ProcessesToUpdate, System, UpdateKind};
use tokio::sync::mpsc::UnboundedSender;

const ORPHAN_GRACE: Duration = Duration::from_secs(20);

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

fn sampler_loop(tracked: SharedTracked, events: UnboundedSender<Event>, internal: UnboundedSender<Internal>) {
    let mut sys = System::new();
    loop {
        std::thread::sleep(Duration::from_secs(1));
        if events.is_closed() {
            return;
        }
        sys.refresh_processes_specifics(
            ProcessesToUpdate::All,
            true,
            ProcessRefreshKind::nothing().with_cpu().with_memory(),
        );
        refresh_ffmpeg_cmds(&mut sys);
        let children = children_map(&sys);
        let mut map = tracked.lock().unwrap();
        for (&id, t) in map.iter_mut() {
            if t.gone_sent {
                continue;
            }
            let alive = alive_tree(&sys, &children, t.root_pid, &mut t.seen);
            if t.root_name.is_none() {
                t.root_name = sys
                    .process(Pid::from_u32(t.root_pid))
                    .map(|p| p.name().to_string_lossy().to_lowercase());
            }
            let (root, ytdlp, has_bootloader, ffmpeg) = classify(&sys, &alive, t.root_pid, t.root_name.as_deref());

            if t.root_exit.is_some() && alive.is_empty() {
                t.gone_sent = true;
                let _ = internal.send(Internal::RecorderGone(id));
                continue;
            }
            if let Some(at) = t.root_exited_at {
                if at.elapsed() > ORPHAN_GRACE && !alive.is_empty() {
                    for pid in &alive {
                        if let Some(p) = sys.process(*pid) {
                            p.kill();
                        }
                    }
                }
            }

            let ffmpeg_output = ffmpeg.iter().find_map(|f| output_of(&sys, f.pid, &t.cwd));
            let sidecar_paths = t.sidecar.as_deref().map(|p| outpath::read_sidecar(p, &t.cwd)).unwrap_or_default();

            let state = match t.root_exit {
                Some(code) => RecorderState::Exited(code),
                None if t.stopping => RecorderState::Stopping,
                None if root.is_none() && ytdlp.is_none() => RecorderState::Starting,
                None => RecorderState::Running,
            };
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
                ffmpeg_output,
                sidecar_paths,
            }));
        }
    }
}

/// Re-reads the command line and working directory of every ffmpeg process.
fn refresh_ffmpeg_cmds(sys: &mut System) {
    let pids: Vec<Pid> = sys
        .processes()
        .iter()
        .filter(|(_, p)| p.name().to_string_lossy().to_lowercase().contains("ffmpeg"))
        .map(|(pid, _)| *pid)
        .collect();
    if pids.is_empty() {
        return;
    }
    let kind = ProcessRefreshKind::nothing().with_cmd(UpdateKind::Always).with_cwd(UpdateKind::Always);
    sys.refresh_processes_specifics(ProcessesToUpdate::Some(&pids), false, kind);
}

/// The output file on one ffmpeg process's command line, resolved against its working directory.
fn output_of(sys: &System, pid: u32, fallback_cwd: &Path) -> Option<PathBuf> {
    let p = sys.process(Pid::from_u32(pid))?;
    outpath::ffmpeg_output(p.cmd(), p.cwd().unwrap_or(fallback_cwd))
}

fn children_map(sys: &System) -> HashMap<Pid, Vec<Pid>> {
    let mut m: HashMap<Pid, Vec<Pid>> = HashMap::new();
    for (pid, p) in sys.processes() {
        if let Some(parent) = p.parent() {
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
            if !seen.iter().any(|(p, _)| *p == pid.as_u32()) {
                if let Some(p) = sys.process(pid) {
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

fn sample(sys: &System, pid: Pid) -> Option<ProcSample> {
    sys.process(pid).map(|p| ProcSample {
        pid: pid.as_u32(),
        name: p.name().to_string_lossy().into_owned(),
        cpu_percent: p.cpu_usage(),
        mem_bytes: p.memory(),
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
    sys: &System,
    alive: &[Pid],
    root_pid: u32,
    root_name: Option<&str>,
) -> (Option<ProcSample>, Option<ProcSample>, bool, Vec<ProcSample>) {
    let root = Pid::from_u32(root_pid);
    let name_of = |pid: Pid| {
        sys.process(pid).map(|p| p.name().to_string_lossy().to_lowercase()).unwrap_or_default()
    };
    let mut same: Vec<Pid> = alive
        .iter()
        .copied()
        .filter(|p| Some(name_of(*p).as_str()) == root_name && !name_of(*p).contains("ffmpeg"))
        .collect();
    same.sort_by_key(|p| depth(sys, *p, root));
    let has_bootloader = same.len() > 1;
    let ytdlp_pid = same.last().copied();
    let root_sample = if alive.contains(&root) { sample(sys, root) } else { None };
    let ytdlp = ytdlp_pid.and_then(|p| sample(sys, p));
    let mut ffmpeg: Vec<ProcSample> = alive
        .iter()
        .filter(|p| name_of(**p).contains("ffmpeg"))
        .filter_map(|p| sample(sys, *p))
        .collect();
    ffmpeg.sort_by_key(|s| s.pid);
    (root_sample, ytdlp, has_bootloader, ffmpeg)
}
