//! Per-site checker: runs yt-dlp for all of a site's URLs and detects live streams.

use super::{diagnose, exepath, hide_window, logfile, template, Internal};
use crate::api::{now_secs, CheckOutput, Site};
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
    pub problem: Option<diagnose::Problem>,
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
        if !snap.urls.is_empty() {
            let result = run_check(&snap, &internal).await;
            let msg = Internal::CheckResult { site_id: snap.site.id, at: now_secs(), result };
            if internal.send(msg).is_err() {
                return;
            }
        }
        let interval = Duration::from_secs(snap.site.interval_or_default().max(5));
        let wait = interval.saturating_sub(started.elapsed());
        tokio::select! {
            _ = tokio::time::sleep(wait) => {}
            changed = rx.changed() => if changed.is_err() { return; }
        }
    }
}

async fn run_check(snap: &SiteSnapshot, internal: &UnboundedSender<Internal>) -> Result<CheckReport, String> {
    let template = snap.site.checker_or_default();
    let Some(kind) = CheckOutput::detect(template) else {
        return Ok(CheckReport { entries: Vec::new(), problem: Some(diagnose::missing_output_flag()) });
    };
    let args = template::expand(template, "urls", &snap.urls);
    let exe = exepath::resolve(snap.site.exe_or_default());
    if let Some(note) = exe.note {
        let _ = internal.send(Internal::Log(note));
    }
    let mut cmd = Command::new(&exe.exe);
    cmd.args(&args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    hide_window(&mut cmd);
    let started = Instant::now();
    let output = tokio::time::timeout(CHECK_TIMEOUT, cmd.output())
        .await
        .map_err(|_| format!("checker timed out after {}s", CHECK_TIMEOUT.as_secs()))?
        .map_err(|e| format!("failed to start '{}': {e}", exe.exe))?;
    log_output(&snap.site, &exe.exe, &args, &output, started.elapsed());
    let stdout = String::from_utf8_lossy(&output.stdout);
    let entries = parse_output(kind, &stdout);
    let stderr: Vec<String> = String::from_utf8_lossy(&output.stderr).lines().map(str::to_string).collect();
    let status = output.status;
    let problem = diagnose::check_problem(&stderr, status.success(), status.code(), !entries.is_empty());
    Ok(CheckReport { entries, problem })
}

/// Appends this check's command, exit code and the enabled output streams to the site's checker log.
fn log_output(site: &Site, exe: &str, args: &[String], output: &std::process::Output, elapsed: Duration) {
    if !site.log_checker_stdout && !site.log_checker_stderr {
        return;
    }
    let code = output.status.code().map_or("?".to_string(), |c| c.to_string());
    let mut block = format!(
        "{}  {} (exit {code}, {:.1}s)\n",
        logfile::stamp(),
        logfile::cmdline(exe, args),
        elapsed.as_secs_f32()
    );
    for (on, label, bytes) in [
        (site.log_checker_stdout, "stdout", &output.stdout),
        (site.log_checker_stderr, "stderr", &output.stderr),
    ] {
        if on {
            block.push_str(&format!("--- {label} ---\n{}\n", String::from_utf8_lossy(bytes).trim_end()));
        }
    }
    block.push('\n');
    logfile::append(&logfile::checker_path(&site.name), &block);
}

fn parse_output(kind: CheckOutput, stdout: &str) -> Vec<LiveEntry> {
    match kind {
        CheckOutput::Json => stdout.lines().filter_map(parse_json_line).collect(),
        CheckOutput::Simulate => stdout.lines().filter_map(parse_simulate_line).collect(),
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
    })
}

/// Expects `original_url|webpage_url`.
fn parse_simulate_line(line: &str) -> Option<LiveEntry> {
    let mut parts = line.trim().splitn(2, '|');
    let original = parts.next()?.trim();
    let webpage = parts.next()?.trim();
    if original.is_empty() && webpage.is_empty() {
        return None;
    }
    Some(LiveEntry {
        candidates: vec![original.to_string(), webpage.to_string()],
        webpage_url: webpage.to_string(),
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
