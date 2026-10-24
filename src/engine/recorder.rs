//! Launches a yt-dlp recorder, captures its output, and stops it gracefully.

use super::{hide_window, logfile};
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
    /// Files that receive the process's stdout/stderr lines when logging is enabled.
    pub log_stdout: Option<PathBuf>,
    pub log_stderr: Option<PathBuf>,
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

    let cmdline = logfile::cmdline(&spec.exe, &spec.args);
    let logs: Vec<PathBuf> = [&spec.log_stdout, &spec.log_stderr].into_iter().flatten().cloned().collect();
    for path in &logs {
        logfile::line(path, &format!("start: {cmdline} (cwd {}, pid {pid})", spec.cwd.display()));
    }
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
        tokio::spawn(read_lines(out, spec.id, tracked.clone(), spec.log_stdout.clone()));
    }
    if let Some(err) = child.stderr.take() {
        tokio::spawn(read_lines(err, spec.id, tracked.clone(), spec.log_stderr.clone()));
    }

    let (stop_tx, mut stop_rx) = oneshot::channel::<()>();
    let tracked = tracked.clone();
    let id = spec.id;
    let started = Instant::now();
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
        for path in &logs {
            let code = code.map_or("none".to_string(), |c| c.to_string());
            logfile::line(path, &format!("exit: code {code} after {}s", started.elapsed().as_secs()));
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

/// Keeps the newest output line (split on '\n' and '\r') and appends each '\n'-ended line to `log`.
async fn read_lines<R: tokio::io::AsyncRead + Unpin>(mut r: R, id: RecorderId, tracked: SharedTracked, log: Option<PathBuf>) {
    let mut buf = [0u8; 4096];
    let mut line = Vec::<u8>::new();
    // Text ended by a '\r'; it counts as a real line only if a '\n' follows at once (CRLF).
    let mut after_cr: Option<String> = None;
    loop {
        let n = match r.read(&mut buf).await {
            Ok(0) | Err(_) => return,
            Ok(n) => n,
        };
        for &b in &buf[..n] {
            if b != b'\n' && b != b'\r' {
                after_cr = None;
                line.push(b);
                continue;
            }
            let text = String::from_utf8_lossy(&line).trim().to_string();
            line.clear();
            if !text.is_empty() {
                if let Some(t) = tracked.lock().unwrap().get_mut(&id) {
                    t.last_line = text.clone();
                }
            }
            let finished = if b == b'\n' {
                if text.is_empty() { after_cr.take() } else { Some(text) }
            } else {
                after_cr = (!text.is_empty()).then_some(text);
                None
            };
            if let (Some(path), Some(text)) = (&log, finished) {
                logfile::line(path, &text);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn logs_only_newline_ended_lines() {
        let path = std::env::temp_dir().join(format!("jj-readlines-{}.log", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let input: &[u8] = b"one\r\ntwo\rthree\n\nfour";
        let tracked: SharedTracked = Arc::new(Mutex::new(HashMap::new()));
        read_lines(input, 1, tracked, Some(path.clone())).await;
        let text = std::fs::read_to_string(&path).unwrap();
        let lines: Vec<&str> = text.lines().map(|l| l.split_once("  ").unwrap().1).collect();
        assert_eq!(lines, ["one", "three"]);
        let _ = std::fs::remove_file(&path);
    }
}
