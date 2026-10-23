//! Launches a yt-dlp recorder, captures its output, and stops it gracefully.

use super::hide_window;
use crate::api::{RecorderId, StreamerId};
use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::io::AsyncReadExt;
use tokio::process::Command;
use tokio::sync::oneshot;

const GRACEFUL_STOP_TIMEOUT: Duration = Duration::from_secs(20);

pub struct RecorderSpec {
    pub id: RecorderId,
    pub streamer_id: StreamerId,
    pub streamer_name: String,
    pub site_name: String,
    pub exe: String,
    pub args: Vec<String>,
    pub cwd: PathBuf,
}

/// State shared between the recorder tasks, the engine and the sampler thread.
pub struct Tracked {
    pub streamer_id: StreamerId,
    pub streamer_name: String,
    pub site_name: String,
    pub started: Instant,
    pub root_pid: u32,
    pub root_name: Option<String>,
    pub seen: Vec<(u32, u64)>,
    pub root_exit: Option<Option<i32>>,
    pub root_exited_at: Option<Instant>,
    pub stopping: bool,
    pub gone_sent: bool,
    pub last_line: String,
    pub cmdline: String,
}

pub type SharedTracked = Arc<Mutex<HashMap<RecorderId, Tracked>>>;

pub struct RecorderHandle {
    pub streamer_id: StreamerId,
    pub stop_tx: Option<oneshot::Sender<()>>,
}

pub fn spawn_recorder(spec: RecorderSpec, tracked: &SharedTracked) -> Result<RecorderHandle, String> {
    std::fs::create_dir_all(&spec.cwd).map_err(|e| format!("cannot create {}: {e}", spec.cwd.display()))?;
    let mut cmd = Command::new(&spec.exe);
    cmd.args(&spec.args)
        .current_dir(&spec.cwd)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(unix)]
    cmd.process_group(0);
    #[cfg(windows)]
    cmd.creation_flags(0x0000_0200); // CREATE_NEW_PROCESS_GROUP
    hide_window(&mut cmd);
    let mut child = cmd.spawn().map_err(|e| format!("failed to start '{}': {e}", spec.exe))?;
    let pid = child.id().ok_or("process exited immediately")?;

    let cmdline = std::iter::once(spec.exe.clone()).chain(spec.args.iter().cloned()).collect::<Vec<_>>().join(" ");
    tracked.lock().unwrap().insert(
        spec.id,
        Tracked {
            streamer_id: spec.streamer_id,
            streamer_name: spec.streamer_name,
            site_name: spec.site_name,
            started: Instant::now(),
            root_pid: pid,
            root_name: None,
            seen: Vec::new(),
            root_exit: None,
            root_exited_at: None,
            stopping: false,
            gone_sent: false,
            last_line: String::new(),
            cmdline,
        },
    );

    if let Some(out) = child.stdout.take() {
        tokio::spawn(read_lines(out, spec.id, tracked.clone()));
    }
    if let Some(err) = child.stderr.take() {
        tokio::spawn(read_lines(err, spec.id, tracked.clone()));
    }

    let (stop_tx, mut stop_rx) = oneshot::channel::<()>();
    let tracked = tracked.clone();
    let id = spec.id;
    tokio::spawn(async move {
        let status = tokio::select! {
            s = child.wait() => s,
            _ = &mut stop_rx => {
                interrupt(pid);
                match tokio::time::timeout(GRACEFUL_STOP_TIMEOUT, child.wait()).await {
                    Ok(s) => s,
                    Err(_) => {
                        force_kill(pid);
                        child.wait().await
                    }
                }
            }
        };
        let code = status.ok().and_then(|s| s.code());
        if let Some(t) = tracked.lock().unwrap().get_mut(&id) {
            t.root_exit = Some(code);
            t.root_exited_at = Some(Instant::now());
        }
    });

    Ok(RecorderHandle { streamer_id: spec.streamer_id, stop_tx: Some(stop_tx) })
}

/// Marks the recorder as stopping and signals its task to interrupt the process.
pub fn request_stop(handle: &mut RecorderHandle, id: RecorderId, tracked: &SharedTracked) {
    if let Some(t) = tracked.lock().unwrap().get_mut(&id) {
        t.stopping = true;
    }
    if let Some(tx) = handle.stop_tx.take() {
        let _ = tx.send(());
    }
}

/// Keeps the newest output line, splitting on both '\n' and '\r' (progress bars).
async fn read_lines<R: tokio::io::AsyncRead + Unpin>(mut r: R, id: RecorderId, tracked: SharedTracked) {
    let mut buf = [0u8; 4096];
    let mut line = Vec::<u8>::new();
    loop {
        let n = match r.read(&mut buf).await {
            Ok(0) | Err(_) => return,
            Ok(n) => n,
        };
        for &b in &buf[..n] {
            if b == b'\n' || b == b'\r' {
                if !line.is_empty() {
                    let text = String::from_utf8_lossy(&line).trim().to_string();
                    if !text.is_empty() {
                        if let Some(t) = tracked.lock().unwrap().get_mut(&id) {
                            t.last_line = text;
                        }
                    }
                    line.clear();
                }
            } else {
                line.push(b);
            }
        }
        if line.len() > 8192 {
            line.clear();
        }
    }
}

#[cfg(unix)]
fn interrupt(pid: u32) {
    unsafe { libc::kill(-(pid as i32), libc::SIGINT) };
}

#[cfg(unix)]
fn force_kill(pid: u32) {
    unsafe { libc::kill(-(pid as i32), libc::SIGKILL) };
}

#[cfg(windows)]
fn interrupt(pid: u32) {
    use windows_sys::Win32::System::Console::{GenerateConsoleCtrlEvent, CTRL_BREAK_EVENT};
    unsafe { GenerateConsoleCtrlEvent(CTRL_BREAK_EVENT, pid) };
}

#[cfg(windows)]
fn force_kill(pid: u32) {
    use std::os::windows::process::CommandExt;
    let _ = std::process::Command::new("taskkill")
        .args(["/PID", &pid.to_string(), "/T", "/F"])
        .creation_flags(0x0800_0000)
        .status();
}
