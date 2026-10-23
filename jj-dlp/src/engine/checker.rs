//! Per-site checker: runs yt-dlp for all of a site's URLs and detects live streams.

use super::{hide_window, template, Internal};
use crate::api::{now_secs, CheckMode, Site};
use serde_json::Value;
use std::process::Stdio;
use std::time::{Duration, Instant};
use tokio::process::Command;
use tokio::sync::{mpsc::UnboundedSender, watch};
use tokio::task::JoinHandle;

const CHECK_TIMEOUT: Duration = Duration::from_secs(300);

/// Everything a worker needs for one check cycle.
#[derive(Debug, Clone, PartialEq)]
pub struct SiteSnapshot {
    pub site: Site,
    pub urls: Vec<String>,
}

/// A live stream reported by the checker.
#[derive(Debug, Clone)]
pub struct LiveEntry {
    pub candidates: Vec<String>,
    pub webpage_url: String,
    pub video_id: String,
    pub title: String,
}

impl LiveEntry {
    pub fn matches(&self, streamer_url: &str) -> bool {
        let want = norm_url(streamer_url);
        self.candidates.iter().any(|c| !c.is_empty() && norm_url(c) == want)
    }
}

#[derive(Debug, Clone)]
pub struct CheckReport {
    pub entries: Vec<LiveEntry>,
    pub message: String,
}

pub struct SiteWorker {
    pub tx: watch::Sender<SiteSnapshot>,
    pub handle: JoinHandle<()>,
}

pub fn spawn_worker(snap: SiteSnapshot, internal: UnboundedSender<Internal>) -> SiteWorker {
    let (tx, rx) = watch::channel(snap);
    let handle = tokio::spawn(worker_loop(rx, internal));
    SiteWorker { tx, handle }
}

async fn worker_loop(mut rx: watch::Receiver<SiteSnapshot>, internal: UnboundedSender<Internal>) {
    loop {
        let snap = rx.borrow_and_update().clone();
        let started = Instant::now();
        if !snap.urls.is_empty() && !snap.site.exe_path.trim().is_empty() {
            let result = run_check(&snap).await;
            let msg = Internal::CheckResult { site_id: snap.site.id, at: now_secs(), result };
            if internal.send(msg).is_err() {
                return;
            }
        }
        let interval = Duration::from_secs(snap.site.interval_secs.max(5));
        let wait = interval.saturating_sub(started.elapsed());
        tokio::select! {
            _ = tokio::time::sleep(wait) => {}
            changed = rx.changed() => if changed.is_err() { return; }
        }
    }
}

async fn run_check(snap: &SiteSnapshot) -> Result<CheckReport, String> {
    let args = template::expand(&snap.site.checker_args, &[], Some(&snap.urls));
    let mut cmd = Command::new(snap.site.exe_path.trim());
    cmd.args(&args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    hide_window(&mut cmd);
    let output = tokio::time::timeout(CHECK_TIMEOUT, cmd.output())
        .await
        .map_err(|_| format!("checker timed out after {}s", CHECK_TIMEOUT.as_secs()))?
        .map_err(|e| format!("failed to start '{}': {e}", snap.site.exe_path))?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let entries = parse_output(snap.site.check_mode, &stdout);
    let last_err = String::from_utf8_lossy(&output.stderr)
        .lines()
        .rev()
        .find(|l| !l.trim().is_empty())
        .unwrap_or("")
        .trim()
        .to_string();
    let message = format!(
        "{} live of {} urls (exit {}){}",
        entries.len(),
        snap.urls.len(),
        output.status.code().map_or("?".into(), |c| c.to_string()),
        if last_err.is_empty() { String::new() } else { format!(" | {last_err}") }
    );
    Ok(CheckReport { entries, message })
}

fn parse_output(mode: CheckMode, stdout: &str) -> Vec<LiveEntry> {
    match mode {
        CheckMode::DumpJson => stdout.lines().filter_map(parse_json_line).collect(),
        CheckMode::Simulate => stdout.lines().filter_map(parse_simulate_line).collect(),
    }
}

fn parse_json_line(line: &str) -> Option<LiveEntry> {
    let line = line.trim();
    if !line.starts_with('{') {
        return None;
    }
    let v: Value = serde_json::from_str(line).ok()?;
    let get = |k: &str| v.get(k).and_then(Value::as_str).unwrap_or("").to_string();
    let live = v.get("is_live").and_then(Value::as_bool).unwrap_or(false)
        || v.get("live_status").and_then(Value::as_str) == Some("is_live");
    if !live {
        return None;
    }
    let webpage_url = get("webpage_url");
    Some(LiveEntry {
        candidates: vec![get("original_url"), webpage_url.clone(), get("channel_url"), get("uploader_url")],
        webpage_url,
        video_id: get("id"),
        title: get("title"),
    })
}

/// Expects `original_url|webpage_url|id|title` (title last, may contain '|').
fn parse_simulate_line(line: &str) -> Option<LiveEntry> {
    let mut parts = line.trim().splitn(4, '|');
    let original = parts.next()?.trim();
    let webpage = parts.next()?.trim();
    let id = parts.next()?.trim();
    let title = parts.next().unwrap_or("").trim();
    if original.is_empty() && webpage.is_empty() {
        return None;
    }
    Some(LiveEntry {
        candidates: vec![original.to_string(), webpage.to_string()],
        webpage_url: webpage.to_string(),
        video_id: id.to_string(),
        title: title.to_string(),
    })
}

/// Lowercases and strips scheme, `www.`, fragments, trailing slashes and `/live`-style suffixes.
pub fn norm_url(u: &str) -> String {
    let mut s = u.trim().to_lowercase();
    for p in ["https://", "http://"] {
        if let Some(r) = s.strip_prefix(p) {
            s = r.to_string();
            break;
        }
    }
    for p in ["www.", "m."] {
        if let Some(r) = s.strip_prefix(p) {
            s = r.to_string();
            break;
        }
    }
    if let Some(i) = s.find('#') {
        s.truncate(i);
    }
    loop {
        let before = s.len();
        s = s.trim_end_matches('/').to_string();
        for suffix in ["/live", "/streams"] {
            if let Some(r) = s.strip_suffix(suffix) {
                s = r.to_string();
            }
        }
        if s.len() == before {
            return s;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes() {
        assert_eq!(norm_url("https://www.YouTube.com/@Foo/live/"), "youtube.com/@foo");
        assert_eq!(norm_url("http://twitch.tv/foo"), "twitch.tv/foo");
    }

    #[test]
    fn detects_live_json() {
        let l = r#"{"id":"abc","title":"t","is_live":true,"webpage_url":"https://youtube.com/watch?v=abc","uploader_url":"https://www.youtube.com/@foo"}"#;
        let e = parse_json_line(l).unwrap();
        assert!(e.matches("https://www.youtube.com/@foo/live"));
        assert!(parse_json_line(r#"{"id":"x","is_live":false}"#).is_none());
    }
}
