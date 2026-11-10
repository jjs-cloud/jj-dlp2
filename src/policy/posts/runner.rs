//! Background work of the Posts monitor: yt-dlp calls, thumbnails and notifications.

use super::model::*;
use super::store::{sidecar_path, thumbs_dir};
use super::xprofile;
use crate::api::now_secs;
use crate::engine::exepath;
use crate::engine::logfile::{self, CmdLog};
use crate::policy::notify;
use std::path::{Path, PathBuf};
use std::collections::HashSet;
use std::io::Read;
use std::process::Stdio;
use std::sync::Mutex;
use std::time::{Duration, Instant};
use tokio::process::Command;

const LIST_TIMEOUT: Duration = Duration::from_secs(180);
const INFO_TIMEOUT: Duration = Duration::from_secs(90);
const M3U8_TIMEOUT: Duration = Duration::from_secs(60);
const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(1800);
/// Start of the error `run` returns when a command exceeds its time limit.
const TIMEOUT_PREFIX: &str = "yt-dlp timed out";
/// Log notes from resolving executables, waiting for the monitor to log them.
static NOTES: Mutex<Vec<String>> = Mutex::new(Vec::new());
const THUMB_MAX_BYTES: u64 = 8 * 1024 * 1024;
const THUMB_BOX: (u32, u32) = (360, 640);

/// What one check found for one account.
#[derive(Debug, Clone)]
pub struct AccountReport {
    pub site_id: SiteId,
    pub name: String,
    pub new_posts: Vec<Post>,
    /// Ids of X posts that have no video.
    pub no_video: Vec<String>,
    pub error: Option<String>,
    /// A TikTok channel ID that is new or changed, to be stored on the account.
    pub channel_id: Option<String>,
    /// Log lines for posts whose details needed an extra yt-dlp call.
    pub lookups: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct CycleReport {
    pub site_ids: Vec<SiteId>,
    pub accounts: Vec<AccountReport>,
}

/// The result of one download attempt.
#[derive(Debug, Clone)]
pub struct DownloadOutcome {
    pub file: Result<String, String>,
    pub has_thumb: bool,
    /// True when the download was stopped by the size or time limit for auto-downloads.
    pub skipped: bool,
    /// Post time and duration yt-dlp reported while downloading.
    pub posted_at: Option<u64>,
    pub duration: Option<u64>,
}

/// Resolves an executable path and queues its note for the Log tab.
fn resolve_and_note(raw: &str) -> String {
    let resolved = exepath::resolve(raw);
    if let (Some(note), Ok(mut notes)) = (resolved.note, NOTES.lock()) {
        notes.push(note);
    }
    resolved.exe
}

/// Takes the notes queued by executable resolution.
pub fn take_notes() -> Vec<String> {
    NOTES.lock().map(|mut n| std::mem::take(&mut *n)).unwrap_or_default()
}

/// The most useful line of yt-dlp's error output.
fn error_line(stderr: &str) -> Option<String> {
    let lines = || stderr.lines().map(str::trim).filter(|l| !l.is_empty());
    let line = lines().rev().find(|l| l.contains("ERROR")).or_else(|| lines().last())?;
    let line = line.trim_start_matches("ERROR:").trim();
    Some(line.chars().take(220).collect())
}

/// The log of an account's checker commands.
fn checker_log(site: &PostSite, acct: &Account) -> CmdLog {
    CmdLog::new(acct.log_stdout, acct.log_stderr, |k| logfile::posts_checker_path(&site.name, &acct.name, k))
}

/// The log of a site's downloader commands.
fn download_log(site: &PostSite) -> CmdLog {
    CmdLog::new(site.log_download_stdout, site.log_download_stderr, |k| logfile::posts_download_path(&site.name, k))
}

async fn run(exe: &str, args: &[String], limit: Duration, log: &CmdLog) -> Result<std::process::Output, String> {
    let resolved = resolve_and_note(exe);
    let mut cmd = Command::new(&resolved);
    cmd.args(args).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped()).kill_on_drop(true);
    crate::engine::hide_window(&mut cmd);
    let started = Instant::now();
    match tokio::time::timeout(limit, cmd.output()).await {
        Ok(Ok(out)) => {
            log.output(&resolved, args, &out, started.elapsed());
            Ok(out)
        }
        Ok(Err(e)) => {
            log.failure(&resolved, args, &format!("failed to start: {e}"));
            Err(format!("failed to start '{exe}': {e}"))
        }
        Err(_) => {
            log.failure(&resolved, args, &format!("timed out after {}s", limit.as_secs()));
            Err(format!("{TIMEOUT_PREFIX} after {}s", limit.as_secs()))
        }
    }
}

/// Looks up the channel ID of the TikTok account that posted the video at `url`.
pub async fn resolve_channel_id(exe: String, user_agent: Option<String>, url: String) -> Result<String, String> {
    let mut args = vec!["--no-warnings".to_string()];
    args.extend(user_agent.into_iter().flat_map(|ua| ["--user-agent".to_string(), ua]));
    args.extend(["--no-playlist", "--print", "channel_id"].map(String::from));
    args.push(url);
    let log = CmdLog::new(false, false, |_| PathBuf::new());
    let out = run(&exe, &args, INFO_TIMEOUT, &log).await?;
    let stdout = String::from_utf8_lossy(&out.stdout);
    match stdout.lines().map(str::trim).find(|l| !l.is_empty() && *l != "NA") {
        Some(id) => Ok(id.to_string()),
        None => Err(error_line(&String::from_utf8_lossy(&out.stderr)).unwrap_or_else(|| "no channel ID was returned".into())),
    }
}

/// Checks every enabled account of the given sites once, sequentially.
pub async fn check_cycle(sites: Vec<PostSite>, mut known: HashSet<String>, no_video: HashSet<String>) -> CycleReport {
    let mut accounts = Vec::new();
    for site in &sites {
        for acct in site.accounts.iter().filter(|a| a.enabled) {
            let report = check_account(site, acct, &known, &no_video).await;
            known.extend(report.new_posts.iter().map(|p| p.id.clone()));
            accounts.push(report);
        }
    }
    CycleReport { site_ids: sites.iter().map(|s| s.id).collect(), accounts }
}

async fn check_account(site: &PostSite, acct: &Account, known: &HashSet<String>, no_video: &HashSet<String>) -> AccountReport {
    if let Some(user) = site.use_x_scraper.then(|| xprofile::profile_user(&acct.url)).flatten() {
        return check_x_account(site, acct, &user, known, no_video).await;
    }
    let fail = |e: String| AccountReport { site_id: site.id, name: acct.name.clone(), new_posts: Vec::new(), no_video: Vec::new(), error: Some(e), channel_id: None, lookups: Vec::new() };
    let log = checker_log(site, acct);
    let mut out = match run(site.exe_or_default(), &checker_cmd(site, &acct.url), LIST_TIMEOUT, &log).await {
        Ok(o) => o,
        Err(e) => return fail(e),
    };
    let mut stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    let mut entries: Vec<Entry> = stdout.lines().filter_map(parse_entry).collect();
    if entries.is_empty() && String::from_utf8_lossy(&out.stderr).contains(ID_ERROR_MARKER) {
        let Some(id) = acct.channel_id.as_deref() else { return fail(ID_RESOLUTION_FAILED.into()) };
        out = match run(site.exe_or_default(), &checker_cmd(site, &format!("tiktokuser:{id}")), LIST_TIMEOUT, &log).await {
            Ok(o) => o,
            Err(e) => return fail(e),
        };
        stdout = String::from_utf8_lossy(&out.stdout).into_owned();
        entries = stdout.lines().filter_map(parse_entry).collect();
    }
    let channel_id = if acct.is_tiktok() { stdout.lines().find_map(channel_id_of).filter(|id| acct.channel_id.as_deref() != Some(id)) } else { None };
    if entries.is_empty() && !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr);
        return fail(error_line(&stderr).unwrap_or_else(|| format!("yt-dlp exited with {}", out.status)));
    }
    let (mut new_posts, mut lookups) = (Vec::new(), Vec::new());
    for entry in entries.into_iter().filter(|e| !known.contains(&e.id)) {
        let (post, lookup) = make_post(site, acct, entry).await;
        new_posts.push(post);
        lookups.extend(lookup);
    }
    AccountReport { site_id: site.id, name: acct.name.clone(), new_posts, no_video: Vec::new(), error: None, channel_id, lookups }
}

/// Checks an X account: lists its recent posts from the profile page and keeps those yt-dlp finds a video in.
async fn check_x_account(site: &PostSite, acct: &Account, user: &str, known: &HashSet<String>, no_video: &HashSet<String>) -> AccountReport {
    let mut report = AccountReport { site_id: site.id, name: acct.name.clone(), new_posts: Vec::new(), no_video: Vec::new(), error: None, channel_id: None, lookups: Vec::new() };
    let html = match xprofile::fetch_profile(acct.url.clone(), site.user_agent()).await {
        Ok(h) => h,
        Err(e) => return AccountReport { error: Some(e), ..report },
    };
    let ids = xprofile::newest(xprofile::extract_status_ids(&html, user));
    if ids.is_empty() {
        return AccountReport { error: Some("X returned no posts; the profile may be private or need a login".into()), ..report };
    }
    let mut last_error = None;
    let log = checker_log(site, acct);
    for id in ids.into_iter().filter(|id| !known.contains(id) && !no_video.contains(id)) {
        let url = format!("https://x.com/{user}/status/{id}");
        let out = match run(site.exe_or_default(), &checker_cmd(site, &url), INFO_TIMEOUT, &log).await {
            Ok(o) => o,
            Err(e) => {
                last_error = Some(e);
                continue;
            }
        };
        let stdout = String::from_utf8_lossy(&out.stdout);
        let stderr = String::from_utf8_lossy(&out.stderr);
        match stdout.lines().find_map(parse_entry) {
            Some(entry) => {
                let (post, lookup) = make_post(site, acct, Entry { id, url, ..entry }).await;
                report.new_posts.push(post);
                report.lookups.extend(lookup);
            }
            None if stderr.contains("No video could be found") => report.no_video.push(id),
            None => last_error = Some(error_line(&stderr).unwrap_or_else(|| format!("yt-dlp exited with {}", out.status))),
        }
    }
    if report.new_posts.is_empty() && report.no_video.is_empty() {
        report.error = last_error;
    }
    report
}

/// Builds a post from a listed entry, filling missing details and fetching its thumbnail; also returns a log line when it needed an extra yt-dlp call.
async fn make_post(site: &PostSite, acct: &Account, mut entry: Entry) -> (Post, Option<String>) {
    let mut lookup = None;
    if entry.thumb_url.is_none() || entry.title.is_empty() {
        let missing = match (entry.thumb_url.is_none(), entry.title.is_empty()) {
            (true, true) => "thumbnail and title",
            (true, false) => "thumbnail",
            _ => "title",
        };
        lookup = Some(format!("Posts: {} @{}: {missing} missing for {}, running an extra yt-dlp call", site.name, acct.name, entry.id));
        if let Some(full) = fetch_info(site, &entry.url, &checker_log(site, acct)).await {
            entry.thumb_url = entry.thumb_url.or(full.thumb_url);
            entry.posted_at = entry.posted_at.or(full.posted_at);
            entry.duration = entry.duration.or(full.duration);
            if entry.title.is_empty() {
                entry.title = full.title;
            }
        }
    }
    let has_thumb = match entry.thumb_url.clone() {
        Some(url) => fetch_thumbnail(url, site.user_agent(), entry.id.clone()).await,
        None => false,
    };
    let post = Post {
        id: entry.id,
        site_id: site.id,
        account: acct.name.clone(),
        title: entry.title,
        url: entry.url,
        posted_at: entry.posted_at,
        duration: entry.duration,
        found_at: now_secs(),
        has_thumb,
        ..Default::default()
    };
    (post, lookup)
}

/// Full metadata of one post, or `None` when yt-dlp could not read it.
async fn fetch_info(site: &PostSite, url: &str, log: &CmdLog) -> Option<Entry> {
    let out = run(site.exe_or_default(), &checker_cmd(site, url), INFO_TIMEOUT, log).await.ok()?;
    String::from_utf8_lossy(&out.stdout).lines().find_map(parse_entry)
}

/// Downloads a thumbnail and stores a resized JPEG; true on success.
async fn fetch_thumbnail(url: String, user_agent: Option<String>, id: String) -> bool {
    tokio::task::spawn_blocking(move || {
        let agent = ureq::AgentBuilder::new()
            .timeout_connect(Duration::from_secs(10))
            .timeout_read(Duration::from_secs(20))
            .build();
        let mut req = agent.get(&url);
        if let Some(ua) = user_agent.as_deref().filter(|u| !u.trim().is_empty()) {
            req = req.set("User-Agent", ua.trim());
        }
        let Ok(resp) = req.call() else { return false };
        let mut buf = Vec::new();
        if resp.into_reader().take(THUMB_MAX_BYTES).read_to_end(&mut buf).is_err() {
            return false;
        }
        save_thumbnail(&buf, &id)
    })
    .await
    .unwrap_or(false)
}

/// Decodes image bytes, shrinks them and writes the post's thumbnail file.
fn save_thumbnail(bytes: &[u8], id: &str) -> bool {
    let Ok(img) = image::load_from_memory(bytes) else { return false };
    let rgb = image::DynamicImage::ImageRgb8(img.thumbnail(THUMB_BOX.0, THUMB_BOX.1).to_rgb8());
    let mut jpeg = std::io::Cursor::new(Vec::new());
    if rgb.write_to(&mut jpeg, image::ImageFormat::Jpeg).is_err() {
        return false;
    }
    let _ = std::fs::create_dir_all(thumbs_dir());
    std::fs::write(thumbs_dir().join(thumb_name(id)), jpeg.into_inner()).is_ok()
}

/// Grabs a frame of the video as the post's thumbnail with ffmpeg; true on success.
async fn frame_thumbnail(video: &Path, id: &str) -> bool {
    let _ = std::fs::create_dir_all(thumbs_dir());
    let out = thumbs_dir().join(thumb_name(id));
    let mut cmd = Command::new("ffmpeg");
    cmd.args(["-y", "-loglevel", "error", "-ss", "0.5", "-i"])
        .arg(video)
        .args(["-frames:v", "1", "-vf", "scale=-2:640"])
        .arg(&out)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .kill_on_drop(true);
    crate::engine::hide_window(&mut cmd);
    let ok = matches!(tokio::time::timeout(Duration::from_secs(30), cmd.status()).await, Ok(Ok(s)) if s.success());
    ok && out.is_file()
}

/// Looks for a finished download whose name contains the post id.
pub(super) fn find_by_id(dir: &Path, id: &str, depth: u32) -> Option<PathBuf> {
    const SKIP: [&str; 8] = ["part", "ytdl", "json", "jpg", "jpeg", "webp", "png", "temp"];
    for entry in std::fs::read_dir(dir).ok()?.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if depth > 0 {
                if let Some(found) = find_by_id(&path, id, depth - 1) {
                    return Some(found);
                }
            }
            continue;
        }
        let ext = path.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
        let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        if !SKIP.contains(&ext.as_str()) && name.contains(id) {
            return Some(path);
        }
    }
    None
}

/// Deletes unfinished download files of a post.
fn remove_partials(dir: &Path, id: &str, depth: u32) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for path in entries.flatten().map(|e| e.path()) {
        if path.is_dir() {
            if depth > 0 {
                remove_partials(&path, id, depth - 1);
            }
            continue;
        }
        let ext = path.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
        let named = path.file_name().is_some_and(|n| n.to_string_lossy().contains(id));
        if named && ["part", "ytdl", "temp"].contains(&ext.as_str()) {
            let _ = std::fs::remove_file(path);
        }
    }
}

/// Removes the unfinished files of a cancelled download once its process has exited.
pub async fn discard_partials(folder: PathBuf, id: String) {
    tokio::time::sleep(Duration::from_secs(1)).await;
    remove_partials(&folder, &id, 3);
    let _ = std::fs::remove_file(sidecar_path(&id));
}

/// Downloads one post, then makes sure it has a thumbnail.
pub async fn download(site: PostSite, post: Post, global_dir: String, max_mb: Option<u64>, max_secs: Option<u64>) -> DownloadOutcome {
    let folder = site.output_folder(&global_dir);
    let _ = std::fs::create_dir_all(&folder);
    let sidecar = sidecar_path(&post.id);
    if let Some(parent) = sidecar.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = std::fs::remove_file(&sidecar);
    let args = download_cmd(&site, &post.url, &folder.to_string_lossy(), &sidecar, max_mb);
    let limit = max_secs.map_or(DOWNLOAD_TIMEOUT, Duration::from_secs);
    let out = match run(site.exe_or_default(), &args, limit, &download_log(&site)).await {
        Ok(o) => o,
        Err(e) if max_secs.is_some() && e.starts_with(TIMEOUT_PREFIX) => {
            remove_partials(&folder, &post.id, 3);
            let msg = "Over the time limit for auto-downloads".to_string();
            return DownloadOutcome { file: Err(msg), has_thumb: post.has_thumb, skipped: true, posted_at: None, duration: None };
        }
        Err(e) => return DownloadOutcome { file: Err(e), has_thumb: post.has_thumb, skipped: false, posted_at: None, duration: None },
    };
    let reported = std::fs::read(&sidecar).ok().and_then(|b| String::from_utf8(b).ok());
    let _ = std::fs::remove_file(&sidecar);
    let (posted_at, duration) = reported.as_deref().map_or((None, None), parse_meta);
    let stdout = String::from_utf8_lossy(&out.stdout);
    let from_sidecar = reported.as_deref().and_then(|t| path_lines(t).find(|p| p.is_file()));
    let path = from_sidecar.or_else(|| path_lines(&stdout).find(|p| p.is_file())).or_else(|| find_by_id(&folder, &post.id, 3));
    let Some(path) = path else {
        let stderr = String::from_utf8_lossy(&out.stderr);
        let skipped = max_mb.is_some() && (stdout.contains("larger than max-filesize") || stderr.contains("larger than max-filesize"));
        let msg = if skipped {
            "Over the size limit for auto-downloads".to_string()
        } else {
            error_line(&stderr).unwrap_or_else(|| format!("yt-dlp exited with {} and no file", out.status))
        };
        return DownloadOutcome { file: Err(msg), has_thumb: post.has_thumb, skipped, posted_at: None, duration: None };
    };
    let has_thumb = post.has_thumb || frame_thumbnail(&path, &post.id).await;
    DownloadOutcome { file: Ok(path.to_string_lossy().into_owned()), has_thumb, skipped: false, posted_at, duration }
}

/// The stream playlist URL yt-dlp reports for a post.
pub async fn resolve_m3u8(site: PostSite, url: String) -> Result<String, String> {
    let out = run(site.exe_or_default(), &m3u8_cmd(&site, &url), M3U8_TIMEOUT, &download_log(&site)).await?;
    let stdout = String::from_utf8_lossy(&out.stdout);
    if let Some(line) = stdout.lines().map(str::trim).find(|l| l.starts_with("http")) {
        return Ok(line.to_string());
    }
    let stderr = String::from_utf8_lossy(&out.stderr);
    Err(error_line(&stderr).unwrap_or_else(|| format!("yt-dlp exited with {} and no URL", out.status)))
}

/// Plays a stream URL in `player`, or through a temporary .m3u8 file when `player` is blank.
pub fn play_stream(player: &str, url: &str, id: &str) -> Result<(), String> {
    if !player.trim().is_empty() {
        return open::with_detached(url, resolve_and_note(player)).map_err(|e| e.to_string());
    }
    let file = std::env::temp_dir().join(Path::new(&thumb_name(id)).with_extension("m3u8"));
    std::fs::write(&file, format!("#EXTM3U\n{url}\n")).map_err(|e| e.to_string())?;
    open::that_detached(file).map_err(|e| e.to_string())
}

/// Sends a notification with the free space of `folder` off the async threads and returns the log lines for failures.
pub async fn send_notification(msg: notify::Message, targets: notify::Targets, folder: PathBuf) -> Vec<String> {
    tokio::task::spawn_blocking(move || notify::deliver(&notify::with_disk_space(msg, &folder), &targets, false))
        .await
        .unwrap_or_default()
}
