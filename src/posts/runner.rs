//! Background work of the Posts monitor: yt-dlp calls, thumbnails and notifications.

use super::*;
use crate::api::now_secs;
use std::collections::HashSet;
use std::io::Read;
use std::process::Stdio;
use std::time::Duration;
use tokio::process::Command;

const LIST_TIMEOUT: Duration = Duration::from_secs(180);
const INFO_TIMEOUT: Duration = Duration::from_secs(90);
const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(1800);
const THUMB_MAX_BYTES: u64 = 8 * 1024 * 1024;
const THUMB_BOX: (u32, u32) = (360, 640);

/// What one check found for one account.
#[derive(Debug, Clone)]
pub struct AccountReport {
    pub name: String,
    pub new_posts: Vec<Post>,
    pub error: Option<String>,
}

#[derive(Debug, Clone)]
pub struct CycleReport {
    pub at: u64,
    pub accounts: Vec<AccountReport>,
}

/// The result of one download attempt.
#[derive(Debug, Clone)]
pub struct DownloadOutcome {
    pub file: Result<String, String>,
    pub has_thumb: bool,
}

/// Resolves a relative path with a separator against the app folder and its parents.
fn resolve_exe(raw: &str) -> String {
    let raw = raw.trim();
    if !raw.contains(['/', '\\']) || Path::new(raw).is_absolute() {
        return raw.to_string();
    }
    let rel: PathBuf = raw.split(['/', '\\']).filter(|p| !p.is_empty()).collect();
    let mut bases: Vec<PathBuf> = Vec::new();
    if let Some(dir) = std::env::current_exe().ok().and_then(|p| p.parent().map(Path::to_path_buf)) {
        bases.extend(dir.ancestors().take(4).map(Path::to_path_buf));
    }
    if let Ok(cwd) = std::env::current_dir() {
        bases.push(cwd);
    }
    bases.iter().map(|b| b.join(&rel)).find(|p| p.is_file()).map_or_else(|| raw.to_string(), |p| p.to_string_lossy().into_owned())
}

/// The most useful line of yt-dlp's error output.
fn error_line(stderr: &str) -> Option<String> {
    let lines = || stderr.lines().map(str::trim).filter(|l| !l.is_empty());
    let line = lines().rev().find(|l| l.contains("ERROR")).or_else(|| lines().last())?;
    let line = line.trim_start_matches("ERROR:").trim();
    Some(line.chars().take(220).collect())
}

async fn run(exe: &str, args: &[String], limit: Duration) -> Result<std::process::Output, String> {
    let mut cmd = Command::new(resolve_exe(exe));
    cmd.args(args).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped()).kill_on_drop(true);
    crate::engine::hide_window(&mut cmd);
    match tokio::time::timeout(limit, cmd.output()).await {
        Ok(Ok(out)) => Ok(out),
        Ok(Err(e)) => Err(format!("failed to start '{exe}': {e}")),
        Err(_) => Err(format!("yt-dlp timed out after {}s", limit.as_secs())),
    }
}

/// Checks every enabled account once, sequentially.
pub async fn check_cycle(cfg: PostsConfig, mut known: HashSet<String>) -> CycleReport {
    let mut accounts = Vec::new();
    for acct in cfg.accounts.iter().filter(|a| a.enabled) {
        let report = check_account(&cfg, acct, &known).await;
        known.extend(report.new_posts.iter().map(|p| p.id.clone()));
        accounts.push(report);
    }
    CycleReport { at: now_secs(), accounts }
}

async fn check_account(cfg: &PostsConfig, acct: &Account, known: &HashSet<String>) -> AccountReport {
    let fail = |e: String| AccountReport { name: acct.name.clone(), new_posts: Vec::new(), error: Some(e) };
    let out = match run(cfg.exe_or_default(), &checker_cmd(cfg, &acct.url), LIST_TIMEOUT).await {
        Ok(o) => o,
        Err(e) => return fail(e),
    };
    let stdout = String::from_utf8_lossy(&out.stdout);
    let entries: Vec<Entry> = stdout.lines().filter_map(parse_entry).collect();
    if entries.is_empty() && !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr);
        return fail(error_line(&stderr).unwrap_or_else(|| format!("yt-dlp exited with {}", out.status)));
    }
    let mut new_posts = Vec::new();
    for mut entry in entries.into_iter().filter(|e| !known.contains(&e.id)) {
        if entry.thumb_url.is_none() || entry.title.is_empty() {
            if let Some(full) = fetch_info(cfg, &entry.url).await {
                entry.thumb_url = entry.thumb_url.or(full.thumb_url);
                entry.posted_at = entry.posted_at.or(full.posted_at);
                entry.duration = entry.duration.or(full.duration);
                if entry.title.is_empty() {
                    entry.title = full.title;
                }
            }
        }
        let has_thumb = match entry.thumb_url.clone() {
            Some(url) => fetch_thumbnail(url, cfg.user_agent(), entry.id.clone()).await,
            None => false,
        };
        new_posts.push(Post {
            id: entry.id,
            account: acct.name.clone(),
            title: entry.title,
            url: entry.url,
            posted_at: entry.posted_at,
            duration: entry.duration,
            found_at: now_secs(),
            has_thumb,
            ..Default::default()
        });
    }
    AccountReport { name: acct.name.clone(), new_posts, error: None }
}

/// Full metadata of one post, or `None` when yt-dlp could not read it.
async fn fetch_info(cfg: &PostsConfig, url: &str) -> Option<Entry> {
    let out = run(cfg.exe_or_default(), &checker_cmd(cfg, url), INFO_TIMEOUT).await.ok()?;
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
fn find_by_id(dir: &Path, id: &str, depth: u32) -> Option<PathBuf> {
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

/// Downloads one post, then makes sure it has a thumbnail.
pub async fn download(cfg: PostsConfig, post: Post, global_dir: String) -> DownloadOutcome {
    let folder = cfg.output_folder(&global_dir);
    let _ = std::fs::create_dir_all(&folder);
    let args = download_cmd(&cfg, &post.url, &folder.to_string_lossy());
    let out = match run(cfg.exe_or_default(), &args, DOWNLOAD_TIMEOUT).await {
        Ok(o) => o,
        Err(e) => return DownloadOutcome { file: Err(e), has_thumb: post.has_thumb },
    };
    let stdout = String::from_utf8_lossy(&out.stdout);
    let printed = stdout.lines().map(str::trim).rev().map(PathBuf::from).find(|p| p.is_file());
    let path = printed.or_else(|| find_by_id(&folder, &post.id, 3));
    let Some(path) = path else {
        let stderr = String::from_utf8_lossy(&out.stderr);
        let msg = error_line(&stderr).unwrap_or_else(|| format!("yt-dlp exited with {} and no file", out.status));
        return DownloadOutcome { file: Err(msg), has_thumb: post.has_thumb };
    };
    let has_thumb = post.has_thumb || frame_thumbnail(&path, &post.id).await;
    DownloadOutcome { file: Ok(path.to_string_lossy().into_owned()), has_thumb }
}

/// Sends a notification off the async threads and returns the log lines for failures.
pub async fn send_notification(msg: notify::Message, targets: notify::Targets) -> Vec<String> {
    tokio::task::spawn_blocking(move || notify::deliver(&msg, &targets, false)).await.unwrap_or_default()
}
