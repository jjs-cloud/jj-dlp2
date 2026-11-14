//! Crash reporting: a panic hook that writes crash.log, and a session marker that reveals exits without a clean shutdown.

use crate::engine::logfile::stamp;
use crate::settings;
use serde::{Deserialize, Serialize};
use std::io::Write;
use std::panic::PanicHookInfo;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::Duration;

/// crash.log is emptied before a write once it passes this size.
const MAX_BYTES: u64 = 1024 * 1024;
/// How often the session marker records that the app is still alive.
const BEAT_EVERY: Duration = Duration::from_secs(30);

#[derive(Serialize, Deserialize)]
struct Session {
    pid: u32,
    version: String,
    started: String,
    last_alive: String,
}

static SESSION: Mutex<Option<Session>> = Mutex::new(None);

fn log_path() -> PathBuf {
    settings::config_dir().join("crash.log")
}

fn marker_path() -> PathBuf {
    settings::config_dir().join("session.json")
}

/// Appends text to crash.log immediately, on the calling thread.
fn append(text: &str) {
    let path = log_path();
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if std::fs::metadata(&path).is_ok_and(|m| m.len() > MAX_BYTES) {
        let _ = std::fs::remove_file(&path);
    }
    if let Ok(mut file) = std::fs::OpenOptions::new().create(true).append(true).open(&path) {
        let _ = file.write_all(text.as_bytes());
    }
}

/// Makes every panic, on any thread, write its message, location and backtrace to crash.log.
pub fn install() {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        record_panic(info);
        previous(info);
    }));
}

fn record_panic(info: &PanicHookInfo<'_>) {
    let thread = std::thread::current();
    let name = thread.name().unwrap_or("<unnamed>");
    let location = info.location().map_or_else(|| "unknown location".to_string(), |l| format!("{}:{}:{}", l.file(), l.line(), l.column()));
    let payload = info.payload();
    let message = payload
        .downcast_ref::<&str>()
        .map(|s| s.to_string())
        .or_else(|| payload.downcast_ref::<String>().cloned())
        .unwrap_or_else(|| "non-text panic payload".to_string());
    let backtrace = std::backtrace::Backtrace::force_capture();
    append(&format!(
        "{}  PANIC v{} in thread '{name}' at {location}\n  {message}\n{backtrace}\n\n",
        stamp(),
        env!("CARGO_PKG_VERSION"),
    ));
}

/// Appends one timestamped line to crash.log.
pub fn note(text: &str) {
    append(&format!("{}  {text}\n\n", stamp()));
}

fn write_marker(session: &Session) {
    if let Ok(json) = serde_json::to_vec(session) {
        let _ = std::fs::write(marker_path(), json);
    }
}

/// Reports a previous session that never shut down cleanly, then starts recording this one.
pub fn start_session() {
    if let Some(prev) = std::fs::read(marker_path()).ok().and_then(|b| serde_json::from_slice::<Session>(&b).ok()) {
        let hint = if cfg!(windows) { " On Windows, check Event Viewer > Windows Logs > Application and System around that time." } else { "" };
        let msg = format!(
            "Previous session (pid {}, v{}) started {} and was last seen alive {}; it ended without a clean exit. If no PANIC entry is above this line, the process was killed or crashed natively, or the PC restarted.{hint}",
            prev.pid, prev.version, prev.started, prev.last_alive
        );
        append(&format!("{}  {msg}\n\n", stamp()));
        crate::policy::uilog::append(&msg);
    }
    let now = stamp();
    let session = Session { pid: std::process::id(), version: env!("CARGO_PKG_VERSION").to_string(), started: now.clone(), last_alive: now };
    write_marker(&session);
    *SESSION.lock().unwrap() = Some(session);
    let _ = std::thread::Builder::new().name("crash-beat".into()).spawn(|| loop {
        std::thread::sleep(BEAT_EVERY);
        let mut guard = SESSION.lock().unwrap();
        let Some(session) = guard.as_mut() else { return };
        session.last_alive = stamp();
        write_marker(session);
    });
}

/// Marks this session as cleanly finished by removing the marker.
pub fn end_session() {
    crate::persist::flush();
    if SESSION.lock().unwrap().take().is_some() {
        let _ = std::fs::remove_file(marker_path());
    }
}
