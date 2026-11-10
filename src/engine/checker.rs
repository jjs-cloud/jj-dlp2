//! Per-site checker: runs yt-dlp for all of a site's URLs and detects live streams.

use super::{diagnose, exepath, hide_window, logfile, template, Internal};
use crate::api::{now_secs, CheckOutput, Quality, Site};
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
    /// Resolution and frame rate of the format yt-dlp would record; `None` when the output has none.
    pub quality: Option<Quality>,
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
    /// Each unexpected ERROR line on its own; empty when the run failed without one.
    pub errors: Vec<diagnose::Problem>,
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
        if !snap.urls.is_empty() && !snap.site.paused {
            let result = run_check(&snap, &internal).await;
            let msg = Internal::CheckResult { site_id: snap.site.id, at: now_secs(), result };
            if internal.send(msg).is_err() {
                return;
            }
        }
        // The interval starts once the checker process has exited.
        let wait = Duration::from_secs(snap.site.interval_or_default().max(5));
        tokio::select! {
            _ = tokio::time::sleep(wait) => {}
            changed = rx.changed() => if changed.is_err() { return; }
        }
    }
}

async fn run_check(snap: &SiteSnapshot, internal: &UnboundedSender<Internal>) -> Result<CheckReport, String> {
    let template = snap.site.checker_or_default();
    let Some(kind) = CheckOutput::detect(template) else {
        return Ok(CheckReport { entries: Vec::new(), problem: Some(diagnose::missing_output_flag()), errors: Vec::new() });
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
    let log = logfile::CmdLog::new(snap.site.log_checker_stdout, snap.site.log_checker_stderr, |k| logfile::checker_path(&snap.site.name, k));
    let started = Instant::now();
    let output = match tokio::time::timeout(CHECK_TIMEOUT, cmd.output()).await {
        Ok(Ok(output)) => output,
        Ok(Err(e)) => {
            log.failure(&exe.exe, &args, &format!("failed to start: {e}"));
            return Err(format!("failed to start '{}': {e}", exe.exe));
        }
        Err(_) => {
            log.failure(&exe.exe, &args, &format!("timed out after {}s", CHECK_TIMEOUT.as_secs()));
            return Err(format!("checker timed out after {}s", CHECK_TIMEOUT.as_secs()));
        }
    };
    log.output(&exe.exe, &args, &output, started.elapsed());
    let stdout = String::from_utf8_lossy(&output.stdout);
    let entries = parse_output(kind, &stdout);
    let stderr: Vec<String> = String::from_utf8_lossy(&output.stderr).lines().map(str::to_string).collect();
    let status = output.status;
    let problem = diagnose::check_problem(&stderr, status.success(), status.code(), !entries.is_empty());
    let errors = diagnose::error_problems(&stderr);
    Ok(CheckReport { entries, problem, errors })
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
        quality: json_quality(&v),
    })
}

/// The selected format's resolution and frame rate: `width`/`height`, else the `resolution` text.
fn json_quality(v: &Value) -> Option<Quality> {
    let num = |k: &str| v.get(k).and_then(Value::as_u64).and_then(|n| u32::try_from(n).ok());
    let (w, h) = match (num("width"), num("height")) {
        (Some(w), Some(h)) => (w, h),
        _ => parse_resolution(v.get("resolution")?.as_str()?)?,
    };
    (w > 0 && h > 0).then(|| Quality::new(w, h, v.get("fps").and_then(Value::as_f64)))
}

/// Parses `WIDTHxHEIGHT`.
fn parse_resolution(text: &str) -> Option<(u32, u32)> {
    let (w, h) = text.trim().split_once('x')?;
    Some((w.trim().parse().ok()?, h.trim().parse().ok()?))
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
        quality: None,
    })
}

/// True when an error line names the streamer behind `url`, by its address or its last path segment.
pub fn error_names(line: &str, url: &str) -> bool {
    let low = line.to_lowercase();
    let norm = norm_url(url);
    let name = norm.split('?').next().unwrap_or("").rsplit('/').next().unwrap_or("").trim_start_matches('@');
    low.contains(&norm) || (name.len() >= 3 && low.contains(name))
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

    #[test]
    fn reads_quality_from_width_and_height() {
        let l = r#"{"is_live":true,"webpage_url":"https://kick.com/k","width":1920,"height":1080,"fps":60.039,"resolution":"1920x1080"}"#;
        assert_eq!(parse_json_line(l).unwrap().quality, Some(Quality::new(1920, 1080, Some(60.039))));
    }

    #[test]
    fn reads_quality_from_the_resolution_text_without_fps() {
        let l = r#"{"is_live":true,"webpage_url":"https://tiktok.com/@a/live","resolution":"640x1280"}"#;
        assert_eq!(parse_json_line(l).unwrap().quality, Some(Quality::new(640, 1280, None)));
    }

    #[test]
    fn no_quality_without_a_usable_resolution() {
        let audio = r#"{"is_live":true,"webpage_url":"https://x.test/a","resolution":"audio only"}"#;
        assert_eq!(parse_json_line(audio).unwrap().quality, None);
        let bare = r#"{"is_live":true,"webpage_url":"https://x.test/a"}"#;
        assert_eq!(parse_json_line(bare).unwrap().quality, None);
    }

    #[test]
    fn errors_name_their_streamer() {
        let line = "ERROR: [twitch:stream] typo123: typo123 does not exist";
        assert!(error_names(line, "https://www.twitch.tv/typo123"));
        assert!(error_names(line, "https://twitch.tv/TYPO123/"));
        assert!(!error_names(line, "https://www.twitch.tv/emiru"));
        assert!(error_names("ERROR: Unsupported URL: https://x.test/a?b=1", "https://x.test/a?b=1"));
    }
}
