//! Remembers the processes of running recorders so a relaunch can stop any a crash left behind.

use crate::settings;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Mutex;
use sysinfo::{Pid, ProcessRefreshKind, ProcessStatus, ProcessesToUpdate, System};

/// A recorder process, identified by pid and start time so a reused pid is never mistaken for it.
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
struct Entry {
    pid: u32,
    start: u64,
}

static LAST: Mutex<Vec<Entry>> = Mutex::new(Vec::new());

fn path() -> PathBuf {
    settings::config_dir().join("recorders.json")
}

/// Saves the given `(pid, start time)` pairs when they differ from the last save.
pub fn update(pids: impl Iterator<Item = (u32, u64)>) {
    let mut entries: Vec<Entry> = pids.filter(|(_, start)| *start != 0).map(|(pid, start)| Entry { pid, start }).collect();
    entries.sort_by_key(|e| (e.pid, e.start));
    entries.dedup();
    let mut last = LAST.lock().unwrap();
    if *last == entries {
        return;
    }
    *last = entries.clone();
    crate::persist::save_json(path(), entries, false);
}

/// Kills recorder processes still running from a session that never shut down, clears the record, and returns how many were killed.
pub fn reap() -> usize {
    let entries: Vec<Entry> = std::fs::read(path()).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default();
    if entries.is_empty() {
        return 0;
    }
    let mut sys = System::new();
    sys.refresh_processes_specifics(ProcessesToUpdate::All, true, ProcessRefreshKind::nothing());
    let mut killed = 0;
    for e in &entries {
        let Some(p) = sys.process(Pid::from_u32(e.pid)) else { continue };
        if e.start == 0 || p.start_time() != e.start || p.status() == ProcessStatus::Zombie {
            continue;
        }
        #[cfg(unix)]
        kill_group(e.pid);
        if p.kill() {
            killed += 1;
        }
    }
    let _ = std::fs::remove_file(path());
    killed
}

/// Kills the whole process group when `pid` leads one, as a recorder's root process does.
#[cfg(unix)]
fn kill_group(pid: u32) {
    let pid = pid as i32;
    unsafe {
        if libc::getpgid(pid) == pid {
            libc::kill(-pid, libc::SIGKILL);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entries_without_a_start_time_are_not_recorded() {
        let before = LAST.lock().unwrap().clone();
        update([(4242, 0)].into_iter());
        assert_eq!(*LAST.lock().unwrap(), before);
    }
}
