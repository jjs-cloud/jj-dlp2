//! Samples each recorder's process tree once a second and confirms when it is fully gone.

use super::recorder::SharedTracked;
use super::Internal;
use crate::api::{Event, ProcSample, ProcessInfo, RecorderId, RecorderState, MAX_FFMPEG_PER_STREAMER};
use std::cell::Cell;
use std::collections::{HashMap, HashSet};
use std::time::Duration;
use sysinfo::{Pid, ProcessRefreshKind, ProcessStatus, ProcessesToUpdate, System, UpdateKind};
use tokio::sync::mpsc::UnboundedSender;

const ORPHAN_GRACE: Duration = Duration::from_secs(20);
/// Samples between updates for a recorder whose state has not changed.
const IDLE_UPDATE_EVERY: u64 = 2;
/// Most ids kept in the log-once sets before they are cleared.
const WARNED_MAX: usize = 1000;

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

/// One process as read from a fresh snapshot.
struct Proc {
    parent: Option<Pid>,
    start: u64,
    name: String,
    lname: String,
    zombie: bool,
}

/// Every process read from a brand-new `System`, so reused pids never carry an old process's data.
struct Snap {
    procs: HashMap<Pid, Proc>,
    children: HashMap<Pid, Vec<Pid>>,
    sys: System,
}

impl Snap {
    fn take() -> Snap {
        let mut sys = System::new();
        sys.refresh_processes_specifics(ProcessesToUpdate::All, true, ProcessRefreshKind::nothing());
        let procs = sys
            .processes()
            .iter()
            .map(|(pid, p)| {
                let name = p.name().to_string_lossy().into_owned();
                let proc = Proc {
                    parent: p.parent(),
                    start: p.start_time(),
                    lname: name.to_lowercase(),
                    name,
                    zombie: p.status() == ProcessStatus::Zombie,
                };
                (*pid, proc)
            })
            .collect();
        Snap::build(procs, sys)
    }

    /// Links each process to its parent unless it started before the process now holding the parent's pid.
    fn build(procs: HashMap<Pid, Proc>, sys: System) -> Snap {
        let mut children: HashMap<Pid, Vec<Pid>> = HashMap::new();
        for (pid, p) in &procs {
            let Some(parent) = p.parent else { continue };
            let Some(pp) = procs.get(&parent) else { continue };
            if p.start != 0 && pp.start != 0 && p.start < pp.start {
                continue;
            }
            children.entry(parent).or_default().push(*pid);
        }
        Snap { procs, children, sys }
    }

    /// The process with this pid when it exists and is not a zombie.
    fn live(&self, pid: Pid) -> Option<&Proc> {
        self.procs.get(&pid).filter(|p| !p.zombie)
    }

    fn kill(&self, pid: Pid) {
        if let Some(p) = self.sys.process(pid) {
            p.kill();
        }
    }
}

fn sampler_loop(tracked: SharedTracked, events: UnboundedSender<Event>, internal: UnboundedSender<Internal>) {
    let mut sys = System::new();
    let mut primed = false;
    let mut tick: u64 = 0;
    let mut last_state: HashMap<RecorderId, RecorderState> = HashMap::new();
    let mut shared_warned: HashSet<u32> = HashSet::new();
    let mut cap_warned: HashSet<RecorderId> = HashSet::new();
    loop {
        std::thread::sleep(Duration::from_secs(1));
        if events.is_closed() {
            return;
        }
        tick += 1;
        if !any_tracked(&tracked) {
            primed = false;
            continue;
        }
        sys.refresh_processes_specifics(ProcessesToUpdate::All, true, ProcessRefreshKind::nothing().with_cpu().with_memory());
        // The first refresh after idling only seeds CPU readings.
        if !primed {
            primed = true;
            continue;
        }
        let snap = Snap::take();
        let mut map = tracked.lock().unwrap();
        last_state.retain(|id, _| map.contains_key(id));
        cap_warned.retain(|id| map.contains_key(id));
        if shared_warned.len() > WARNED_MAX {
            shared_warned.clear();
        }
        let mut trees: Vec<(RecorderId, Vec<Pid>)> = Vec::new();
        for (&id, t) in map.iter_mut() {
            if !t.gone_sent {
                trees.push((id, alive_tree(&snap, t.root_pid, &mut t.seen)));
            }
        }
        let labels: HashMap<RecorderId, String> = map
            .iter()
            .map(|(id, t)| (*id, recorder_label(*id, t.streamer_name.as_str(), t.root_pid, t.started.elapsed().as_secs())))
            .collect();
        warn_shared(&snap, &trees, &labels, &mut shared_warned, &internal);
        let stale = Cell::new(false);
        for (id, alive) in trees {
            let Some(t) = map.get_mut(&id) else { continue };
            if t.root_name.is_none() {
                t.root_name = snap.procs.get(&Pid::from_u32(t.root_pid)).map(|p| p.lname.clone());
            }
            let (root, ytdlp, has_bootloader, ffmpeg) =
                classify(&snap, &sys, &stale, &alive, t.root_pid, t.root_name.as_deref());

            if ffmpeg.len() > MAX_FFMPEG_PER_STREAMER {
                if cap_warned.insert(id) {
                    let pids: Vec<Pid> = alive.iter().copied().collect();
                    let cmds = cmdlines(&pids);
                    let state = match (t.root_exit, t.stopping) {
                        (Some(code), _) => format!("root process exited with {code:?}"),
                        (None, true) => "stopping".to_string(),
                        (None, false) => "running".to_string(),
                    };
                    let _ = internal.send(Internal::Log(format!(
                        "warning: {}: this one recording owns {} ffmpeg processes, but a streamer should have at most {MAX_FFMPEG_PER_STREAMER} \
                         (and only briefly while a new part starts). Nothing was killed or stopped. {}; state: {state}; \
                         {} processes in its tree. ffmpeg processes: [{}]. Whole tree: [{}]. If these ffmpeg processes belong to other \
                         streamers, process attribution is broken (see the parent and start values); if they all belong to this streamer, \
                         the recorder is spawning too many.",
                        t.streamer_name,
                        ffmpeg.len(),
                        recorder_label(id, &t.streamer_name, t.root_pid, t.started.elapsed().as_secs()),
                        alive.len(),
                        describe_all(&snap, ffmpeg.iter().map(|f| Pid::from_u32(f.pid)), &cmds),
                        describe_all(&snap, alive.iter().copied(), &cmds),
                    )));
                }
            } else {
                cap_warned.remove(&id);
            }

            if t.root_exit.is_some() && alive.is_empty() {
                t.gone_sent = true;
                let _ = internal.send(Internal::RecorderGone(id));
                continue;
            }
            if let Some(at) = t.root_exited_at {
                if at.elapsed() > ORPHAN_GRACE && !alive.is_empty() {
                    for pid in &alive {
                        snap.kill(*pid);
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
        drop(map);
        // A reused pid left old data in the long-lived System; start it over.
        if stale.get() {
            sys = System::new();
            primed = false;
        }
    }
}

/// Logs each process that more than one recorder claims; every claimant keeps it.
fn warn_shared(
    snap: &Snap,
    trees: &[(RecorderId, Vec<Pid>)],
    labels: &HashMap<RecorderId, String>,
    warned: &mut HashSet<u32>,
    internal: &UnboundedSender<Internal>,
) {
    let mut claims: HashMap<Pid, Vec<RecorderId>> = HashMap::new();
    for (id, tree) in trees {
        for pid in tree {
            claims.entry(*pid).or_default().push(*id);
        }
    }
    for (pid, ids) in claims {
        if ids.len() > 1 && warned.insert(pid.as_u32()) {
            let cmds = cmdlines(&[pid]);
            let owners: Vec<&str> = ids.iter().filter_map(|id| labels.get(id).map(String::as_str)).collect();
            let _ = internal.send(Internal::Log(format!(
                "warning: a process is in the process tree of {} recordings at once, which should never happen. Nothing was killed or stopped; \
                 each recording keeps counting it, so their process counts are wrong and the 20s orphan cleanup of any of them could kill it. \
                 Process: {}. Claimed by: [{}].",
                ids.len(),
                describe(snap, pid, &cmds),
                owners.join("; "),
            )));
        }
    }
}

/// Short identity of a recorder for log lines.
fn recorder_label(id: RecorderId, streamer: &str, root_pid: u32, running_secs: u64) -> String {
    format!("recording {id} of {streamer} (root pid {root_pid}, running {running_secs}s)")
}

/// Command lines of the given pids, read fresh; used only when writing diagnostics.
fn cmdlines(pids: &[Pid]) -> HashMap<Pid, String> {
    let mut sys = System::new();
    sys.refresh_processes_specifics(ProcessesToUpdate::Some(pids), false, ProcessRefreshKind::nothing().with_cmd(UpdateKind::Always));
    pids.iter()
        .filter_map(|pid| {
            let p = sys.process(*pid)?;
            let cmd = p.cmd().iter().map(|a| a.to_string_lossy()).collect::<Vec<_>>().join(" ");
            Some((*pid, cmd.chars().take(600).collect()))
        })
        .collect()
}

/// One-line description of a process: name, start time, parent, whether the parent link is trusted, command line.
fn describe(snap: &Snap, pid: Pid, cmds: &HashMap<Pid, String>) -> String {
    let Some(p) = snap.procs.get(&pid) else { return format!("pid {pid} (no longer running)") };
    let parent = match p.parent {
        None => "none".to_string(),
        Some(pp) => match snap.procs.get(&pp) {
            Some(x) => format!("{pp} '{}' started {}", x.name, x.start),
            None => format!("{pp} (not running)"),
        },
    };
    let trusted = p.parent.is_some_and(|pp| snap.children.get(&pp).is_some_and(|k| k.contains(&pid)));
    let cmd = cmds.get(&pid).map(String::as_str).unwrap_or("unavailable");
    format!("pid {pid} '{}' started {} (unix seconds), parent {parent}, parent link trusted: {trusted}, command: {cmd}", p.name, p.start)
}

fn describe_all(snap: &Snap, pids: impl Iterator<Item = Pid>, cmds: &HashMap<Pid, String>) -> String {
    pids.map(|p| describe(snap, p, cmds)).collect::<Vec<_>>().join(" | ")
}

/// Returns every live (non-zombie) process in the recorder's tree, remembering each as (pid, start time).
fn alive_tree(snap: &Snap, root_pid: u32, seen: &mut Vec<(u32, u64)>) -> Vec<Pid> {
    if !seen.iter().any(|(p, _)| *p == root_pid) {
        if let Some(p) = snap.live(Pid::from_u32(root_pid)) {
            seen.push((root_pid, p.start));
        }
    }
    let mut stack: Vec<Pid> = seen
        .iter()
        .filter(|(p, s)| snap.live(Pid::from_u32(*p)).is_some_and(|x| x.start == *s))
        .map(|(p, _)| Pid::from_u32(*p))
        .collect();
    let mut out: Vec<Pid> = Vec::new();
    let mut visited: HashSet<Pid> = HashSet::new();
    while let Some(pid) = stack.pop() {
        if !visited.insert(pid) {
            continue;
        }
        let Some(p) = snap.procs.get(&pid) else { continue };
        if !p.zombie {
            out.push(pid);
            if !seen.contains(&(pid.as_u32(), p.start)) {
                seen.push((pid.as_u32(), p.start));
            }
        }
        if let Some(kids) = snap.children.get(&pid) {
            stack.extend(kids.iter().copied());
        }
    }
    out
}

fn sample(snap: &Snap, sys: &System, stale: &Cell<bool>, pid: Pid) -> Option<ProcSample> {
    let p = snap.procs.get(&pid)?;
    let (cpu_percent, mem_bytes) = match sys.process(pid) {
        Some(s) if s.start_time() == p.start || s.start_time() == 0 || p.start == 0 => (s.cpu_usage(), s.memory()),
        Some(_) => {
            stale.set(true);
            (0.0, 0)
        }
        None => (0.0, 0),
    };
    Some(ProcSample { pid: pid.as_u32(), name: p.name.clone(), cpu_percent, mem_bytes })
}

/// Depth of `pid` below `root` following parent links (root = 0).
fn depth(snap: &Snap, pid: Pid, root: Pid) -> usize {
    let mut d = 0;
    let mut cur = pid;
    while cur != root && d < 64 {
        match snap.procs.get(&cur).and_then(|p| p.parent) {
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
    snap: &Snap,
    sys: &System,
    stale: &Cell<bool>,
    alive: &[Pid],
    root_pid: u32,
    root_name: Option<&str>,
) -> (Option<ProcSample>, Option<ProcSample>, bool, Vec<ProcSample>) {
    let root = Pid::from_u32(root_pid);
    let name_of = |pid: Pid| snap.procs.get(&pid).map(|p| p.lname.as_str()).unwrap_or_default();
    let mut same: Vec<Pid> = alive
        .iter()
        .copied()
        .filter(|p| Some(name_of(*p)) == root_name && !name_of(*p).contains("ffmpeg"))
        .collect();
    same.sort_by_key(|p| depth(snap, *p, root));
    let has_bootloader = same.len() > 1;
    let ytdlp_pid = same.last().copied();
    let root_sample = if alive.contains(&root) { sample(snap, sys, stale, root) } else { None };
    let ytdlp = ytdlp_pid.and_then(|p| sample(snap, sys, stale, p));
    let mut ffmpeg: Vec<ProcSample> = alive
        .iter()
        .filter(|p| name_of(**p).contains("ffmpeg"))
        .filter_map(|p| sample(snap, sys, stale, *p))
        .collect();
    ffmpeg.sort_by_key(|s| s.pid);
    (root_sample, ytdlp, has_bootloader, ffmpeg)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn proc(parent: Option<u32>, start: u64, name: &str) -> Proc {
        Proc { parent: parent.map(Pid::from_u32), start, name: name.into(), lname: name.to_lowercase(), zombie: false }
    }

    fn snap(list: Vec<(u32, Proc)>) -> Snap {
        Snap::build(list.into_iter().map(|(pid, p)| (Pid::from_u32(pid), p)).collect(), System::new())
    }

    fn pids(v: Vec<Pid>) -> Vec<u32> {
        let mut out: Vec<u32> = v.into_iter().map(|p| p.as_u32()).collect();
        out.sort();
        out
    }

    #[test]
    fn follows_a_normal_tree() {
        let s = snap(vec![
            (5, proc(Some(1), 100, "yt-dlp.exe")),
            (6, proc(Some(5), 100, "yt-dlp.exe")),
            (7, proc(Some(6), 101, "ffmpeg.exe")),
            (8, proc(Some(1), 100, "other.exe")),
        ]);
        assert_eq!(pids(alive_tree(&s, 5, &mut Vec::new())), [5, 6, 7]);
    }

    #[test]
    fn a_reused_parent_pid_does_not_adopt_older_children() {
        // pid 5 now belongs to a new process; pid 10 is an older orphan whose parent field still says 5.
        let s = snap(vec![
            (5, proc(Some(1), 200, "yt-dlp.exe")),
            (10, proc(Some(5), 100, "ffmpeg.exe")),
        ]);
        assert_eq!(pids(alive_tree(&s, 5, &mut Vec::new())), [5]);
    }

    #[test]
    fn an_orphan_is_still_followed_through_seen() {
        let s = snap(vec![(10, proc(Some(5), 100, "ffmpeg.exe"))]);
        let mut seen = vec![(5, 90), (10, 100)];
        assert_eq!(pids(alive_tree(&s, 5, &mut seen)), [10]);
    }

    #[test]
    fn a_seen_pid_reused_by_another_process_is_ignored() {
        let s = snap(vec![(10, proc(Some(1), 300, "ffmpeg.exe"))]);
        let mut seen = vec![(10, 100)];
        assert!(alive_tree(&s, 5, &mut seen).is_empty());
    }

    #[test]
    fn a_new_process_on_a_seen_pid_is_remembered_with_its_own_start() {
        let s = snap(vec![
            (5, proc(Some(1), 100, "yt-dlp.exe")),
            (10, proc(Some(5), 300, "ffmpeg.exe")),
        ]);
        let mut seen = vec![(10, 100)];
        assert_eq!(pids(alive_tree(&s, 5, &mut seen)), [5, 10]);
        assert!(seen.contains(&(10, 300)));
    }
}
