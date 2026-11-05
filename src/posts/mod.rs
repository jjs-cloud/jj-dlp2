//! Posts monitor: tracked accounts, the post library and their JSON persistence.

pub mod runner;

use crate::api::DEFAULT_EXE;
use crate::policy::notify;
use crate::settings::config_dir;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::{Path, PathBuf};

pub const DEFAULT_INTERVAL_SECS: u64 = 300;
/// Days before unstarred posts are deleted, in the TikTok and blank templates.
pub const DEFAULT_AUTO_DELETE_DAYS: u64 = 7;
pub const MIN_INTERVAL_SECS: u64 = 15;
/// Checker arguments used when a site's own are blank; `{url}` is the profile (or post) URL.
pub const DEFAULT_CHECKER_ARGS: &str = "--no-warnings --ignore-errors --flat-playlist --dump-json --playlist-end 5 {url}";
/// Download arguments used when a site's own are blank; `{url}` is the post URL and `{dir}` the download folder.
pub const DEFAULT_DOWNLOAD_ARGS: &str = r#"--no-warnings --ignore-errors --no-playlist --no-simulate --print after_move:filepath -P "{dir}" -o "%(uploader)s_%(upload_date)s_%(id)s.%(ext)s" {url}"#;
/// Arguments used when a site's own are blank; `{url}` is the post URL and the printed line is the stream playlist.
pub const DEFAULT_M3U8_ARGS: &str = "--no-warnings --no-playlist --get-url {url}";
const TWITCH_VODS_URL_TEMPLATE: &str = "https://www.twitch.tv/{username}/videos?filter=archives";
const YOUTUBE_SHORTS_URL_TEMPLATE: &str = "https://www.youtube.com/@{username}/shorts";
const TIKTOK_URL_TEMPLATE: &str = "https://www.tiktok.com/@{username}";
const TIKTOK_CHECKER_ARGS: &str = r#"--no-warnings --ignore-errors --user-agent "Mozilla/5.0 (Windows NT 10.0; Win64; x64; rv:153.0) Gecko/20100101 Firefox/153.0" --flat-playlist --dump-json --playlist-end 5 {url}"#;
const TIKTOK_DOWNLOAD_ARGS: &str = r#"--no-warnings --ignore-errors --user-agent "Mozilla/5.0 (Windows NT 10.0; Win64; x64; rv:153.0) Gecko/20100101 Firefox/153.0" --no-playlist --no-simulate --print after_move:filepath -P "{dir}" -o "%(uploader)s_%(upload_date)s_%(id)s_%(resolution)s.%(ext)s" {url}"#;
pub const DEFAULT_CARD_W: f32 = 170.0;
pub const CARD_W_MIN: f32 = 120.0;
pub const CARD_W_MAX: f32 = 260.0;
/// Failed downloads are retried until they have failed this many times.
pub const MAX_ATTEMPTS: u8 = 3;

pub type SiteId = u64;

fn yes() -> bool {
    true
}

/// Starting points offered by the Add site row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Preset {
    Tiktok,
    YoutubeShorts,
    TwitchVods,
    Blank,
}

/// One profile that is checked for new posts.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Account {
    pub name: String,
    pub url: String,
    #[serde(default = "yes")]
    pub enabled: bool,
    /// True once a check of this account has succeeded.
    #[serde(default)]
    pub baselined: bool,
}

/// A website whose accounts are checked, with its own commands and settings.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PostSite {
    pub id: SiteId,
    pub name: String,
    /// Blank uses `DEFAULT_EXE`.
    pub exe_path: String,
    /// Blank uses `DEFAULT_INTERVAL_SECS`.
    pub interval_secs: Option<u64>,
    /// Blank uses `DEFAULT_CHECKER_ARGS`.
    pub checker_args: String,
    /// Blank uses `DEFAULT_DOWNLOAD_ARGS`.
    pub download_args: String,
    /// Profile URL pattern; `{username}` is replaced with the name typed when adding an account.
    pub url_template: String,
    /// Empty means `posts` inside the global output folder.
    pub output_dir: String,
    pub paused: bool,
    pub auto_download: bool,
    pub backfill: bool,
    pub mark_on_open: bool,
    pub notify: bool,
    pub notify_desktop: bool,
    pub notify_ntfy: bool,
    pub ntfy_topic: String,
    /// Play posts that are not downloaded from their stream playlist instead of opening the site.
    pub open_m3u8: bool,
    /// Blank uses `DEFAULT_M3U8_ARGS`.
    pub m3u8_args: String,
    /// Program that plays the playlist; blank opens it with the system default.
    pub player_path: String,
    /// Unstarred posts older than this many days are deleted; `None` keeps them.
    pub auto_delete_days: Option<u64>,
    pub accounts: Vec<Account>,
}

impl Default for PostSite {
    fn default() -> Self {
        PostSite {
            id: 0,
            name: String::new(),
            exe_path: String::new(),
            interval_secs: None,
            checker_args: String::new(),
            download_args: String::new(),
            url_template: String::new(),
            output_dir: String::new(),
            paused: false,
            auto_download: true,
            backfill: true,
            mark_on_open: true,
            notify: true,
            notify_desktop: true,
            notify_ntfy: false,
            ntfy_topic: String::new(),
            open_m3u8: false,
            m3u8_args: String::new(),
            player_path: String::new(),
            auto_delete_days: Some(DEFAULT_AUTO_DELETE_DAYS),
            accounts: Vec::new(),
        }
    }
}

impl PostSite {
    /// An unsaved site holding a preset's values; the id is assigned when it is added.
    pub fn from_preset(preset: Preset) -> PostSite {
        match preset {
            Preset::Tiktok => PostSite {
                name: "TikTok".into(),
                checker_args: TIKTOK_CHECKER_ARGS.into(),
                download_args: TIKTOK_DOWNLOAD_ARGS.into(),
                url_template: TIKTOK_URL_TEMPLATE.into(),
                ..Default::default()
            },
            Preset::YoutubeShorts => PostSite {
                name: "Youtube Shorts".into(),
                url_template: YOUTUBE_SHORTS_URL_TEMPLATE.into(),
                ..Default::default()
            },
            Preset::TwitchVods => PostSite {
                name: "Twitch Vods".into(),
                url_template: TWITCH_VODS_URL_TEMPLATE.into(),
                auto_download: false,
                backfill: false,
                open_m3u8: true,
                auto_delete_days: None,
                ..Default::default()
            },
            Preset::Blank => PostSite::default(),
        }
    }

    pub fn exe_or_default(&self) -> &str {
        let t = self.exe_path.trim();
        if t.is_empty() { DEFAULT_EXE } else { t }
    }

    pub fn checker_or_default(&self) -> &str {
        let t = self.checker_args.trim();
        if t.is_empty() { DEFAULT_CHECKER_ARGS } else { t }
    }

    pub fn download_or_default(&self) -> &str {
        let t = self.download_args.trim();
        if t.is_empty() { DEFAULT_DOWNLOAD_ARGS } else { t }
    }

    pub fn m3u8_or_default(&self) -> &str {
        let t = self.m3u8_args.trim();
        if t.is_empty() { DEFAULT_M3U8_ARGS } else { t }
    }

    /// Seconds between checks, never below the minimum.
    pub fn interval_or_default(&self) -> u64 {
        self.interval_secs.unwrap_or(DEFAULT_INTERVAL_SECS).max(MIN_INTERVAL_SECS)
    }

    /// The `--user-agent` value in the checker arguments, for thumbnail requests.
    pub fn user_agent(&self) -> Option<String> {
        let toks = tokenize(self.checker_or_default());
        toks.iter().enumerate().find_map(|(i, t)| match t.strip_prefix("--user-agent=") {
            Some(v) => Some(v.to_string()),
            None if t == "--user-agent" => toks.get(i + 1).cloned(),
            None => None,
        })
    }

    /// The download folder: the configured one, else `posts` inside the global output folder.
    pub fn output_folder(&self, global_dir: &str) -> PathBuf {
        let t = self.output_dir.trim();
        if t.is_empty() { Path::new(global_dir.trim()).join("posts") } else { PathBuf::from(t) }
    }

    pub fn has_enabled_accounts(&self) -> bool {
        self.accounts.iter().any(|a| a.enabled)
    }

    /// An account from a username or profile URL; `None` when blank, malformed or already added.
    pub fn account_from_input(&self, input: &str) -> Option<Account> {
        let input = input.trim();
        let (name, url) = if input.starts_with("http://") || input.starts_with("https://") {
            let path = input.split(['?', '#']).next().unwrap_or(input).trim_end_matches('/');
            let mut segs = path.rsplit('/');
            let mut last = segs.next().unwrap_or("");
            if last.eq_ignore_ascii_case("videos") {
                last = segs.next().filter(|p| !p.is_empty() && !p.contains(['.', ':'])).unwrap_or(last);
            }
            (last.trim_start_matches('@').to_string(), input.to_string())
        } else {
            let tmpl = self.url_template.trim();
            if tmpl.is_empty() {
                return None;
            }
            let name = input.trim_start_matches('@').to_string();
            (name.clone(), tmpl.replace("{username}", &name))
        };
        if name.is_empty() || name.chars().any(char::is_whitespace) {
            return None;
        }
        let dup = self.accounts.iter().any(|a| a.name.eq_ignore_ascii_case(&name) || a.url == url);
        (!dup).then_some(Account { name, url, enabled: true, baselined: false })
    }
}

/// Contents of posts.json.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PostsConfig {
    pub sites: Vec<PostSite>,
    pub newest_first: bool,
    pub card_w: f32,
}

impl Default for PostsConfig {
    fn default() -> Self {
        PostsConfig { sites: Vec::new(), newest_first: true, card_w: DEFAULT_CARD_W }
    }
}

impl PostsConfig {
    /// The config of a first launch: one site per named preset.
    fn first_launch() -> Self {
        let mut cfg = PostsConfig::default();
        for preset in [Preset::Tiktok, Preset::YoutubeShorts, Preset::TwitchVods] {
            cfg.add_site(PostSite::from_preset(preset));
        }
        cfg
    }

    pub fn site(&self, id: SiteId) -> Option<&PostSite> {
        self.sites.iter().find(|s| s.id == id)
    }

    pub fn site_mut(&mut self, id: SiteId) -> Option<&mut PostSite> {
        self.sites.iter_mut().find(|s| s.id == id)
    }

    /// True when a site already uses the name.
    pub fn name_taken(&self, name: &str) -> bool {
        self.sites.iter().any(|s| s.name.trim().eq_ignore_ascii_case(name.trim()))
    }

    /// Adds `site` under a fresh id; `None` when its name is already used.
    pub fn add_site(&mut self, mut site: PostSite) -> Option<SiteId> {
        if self.name_taken(&site.name) {
            return None;
        }
        site.id = self.sites.iter().map(|s| s.id).max().unwrap_or(0) + 1;
        let id = site.id;
        self.sites.push(site);
        Some(id)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum PostState {
    /// Known but not downloaded.
    #[default]
    Remote,
    Queued,
    Downloading,
    Ready,
    Failed,
    /// Dismissed or deleted by the user; kept so it is never fetched again.
    Removed,
}

/// Which slice of the library the main area shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Section {
    Unviewed,
    Viewed,
    Starred,
    Failed,
}

impl Section {
    pub fn label(self) -> &'static str {
        match self {
            Section::Unviewed => "Unviewed",
            Section::Viewed => "Viewed",
            Section::Starred => "Starred",
            Section::Failed => "Failed",
        }
    }
}

/// One post found on an account.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct Post {
    pub id: String,
    pub site_id: SiteId,
    pub account: String,
    pub title: String,
    pub url: String,
    pub posted_at: Option<u64>,
    pub duration: Option<u64>,
    pub found_at: u64,
    /// True while a thumbnail file exists for this post.
    pub has_thumb: bool,
    pub file: Option<String>,
    pub state: PostState,
    pub error: String,
    pub attempts: u8,
    pub viewed: bool,
    pub viewed_at: Option<u64>,
    pub starred: bool,
}

impl Post {
    /// When the post was made, else when it was found.
    pub fn sort_time(&self) -> u64 {
        self.posted_at.unwrap_or(self.found_at)
    }

    pub fn thumb_path(&self) -> PathBuf {
        thumbs_dir().join(thumb_name(&self.id))
    }

    pub fn in_section(&self, section: Section) -> bool {
        if self.state == PostState::Removed {
            return false;
        }
        match section {
            Section::Unviewed => !self.viewed,
            Section::Viewed => self.viewed,
            Section::Starred => self.starred,
            Section::Failed => self.state == PostState::Failed,
        }
    }

    /// True when the lowercase `query` is blank or appears in the title or account.
    pub fn matches_query(&self, query: &str) -> bool {
        query.is_empty() || self.title.to_lowercase().contains(query) || self.account.to_lowercase().contains(query)
    }
}

fn path_in_config(name: &str) -> PathBuf {
    config_dir().join(name)
}

pub fn thumbs_dir() -> PathBuf {
    path_in_config("post_thumbs")
}

/// The thumbnail file name for a post id.
pub fn thumb_name(id: &str) -> String {
    let safe: String = id.chars().map(|c| if c.is_ascii_alphanumeric() || c == '_' || c == '-' { c } else { '_' }).collect();
    format!("{safe}.jpg")
}

fn write_atomic<T: Serialize>(path: &Path, value: &T) {
    let _ = std::fs::create_dir_all(config_dir());
    let tmp = path.with_extension("json.tmp");
    if let Ok(json) = serde_json::to_string_pretty(value) {
        if std::fs::write(&tmp, json).is_ok() {
            let _ = std::fs::rename(&tmp, path);
        }
    }
}

fn read_json<T: serde::de::DeserializeOwned + Default>(path: &Path) -> T {
    std::fs::read_to_string(path).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
}

pub fn load_config() -> PostsConfig {
    let path = path_in_config("posts.json");
    if !path.exists() {
        return PostsConfig::first_launch();
    }
    read_json(&path)
}

pub fn save_config(cfg: &PostsConfig) {
    write_atomic(&path_in_config("posts.json"), cfg);
}

/// Loads the library, requeuing interrupted downloads and dropping references to missing files.
pub fn load_library() -> Vec<Post> {
    let mut posts: Vec<Post> = read_json(&path_in_config("posts_library.json"));
    for p in &mut posts {
        if p.state == PostState::Downloading {
            p.state = PostState::Queued;
        }
        if p.has_thumb && !p.thumb_path().is_file() {
            p.has_thumb = false;
        }
        if p.state == PostState::Ready && !p.file.as_deref().is_some_and(|f| Path::new(f).is_file()) {
            p.state = PostState::Remote;
            p.file = None;
        }
    }
    posts
}

pub fn save_library(posts: &[Post]) {
    write_atomic(&path_in_config("posts_library.json"), &posts);
}

/// The fields of one yt-dlp JSON entry that the monitor uses.
#[derive(Debug, Clone, PartialEq)]
pub struct Entry {
    pub id: String,
    pub url: String,
    pub title: String,
    pub thumb_url: Option<String>,
    pub posted_at: Option<u64>,
    pub duration: Option<u64>,
}

/// Parses one line of yt-dlp `--dump-json` output; `None` when it has no id or URL.
pub fn parse_entry(line: &str) -> Option<Entry> {
    let line = line.trim();
    if !line.starts_with('{') {
        return None;
    }
    let v: Value = serde_json::from_str(line).ok()?;
    let text = |k: &str| v.get(k).and_then(Value::as_str).map(str::trim).filter(|s| !s.is_empty());
    let id = match v.get("id")? {
        Value::String(s) if !s.is_empty() => s.clone(),
        Value::Number(n) => n.to_string(),
        _ => return None,
    };
    let url = text("webpage_url").or_else(|| text("url").filter(|u| u.starts_with("http")))?.to_string();
    let title = text("title").or_else(|| text("description")).unwrap_or("").lines().next().unwrap_or("").trim().to_string();
    let thumb_url = text("thumbnail").map(str::to_string).or_else(|| {
        v.get("thumbnails")?.as_array()?.iter().rev().find_map(|t| t.get("url")?.as_str().map(str::to_string))
    });
    let posted_at = v
        .get("timestamp")
        .and_then(Value::as_f64)
        .filter(|t| *t > 0.0)
        .map(|t| t as u64)
        .or_else(|| text("upload_date").and_then(date_to_secs));
    let duration = v.get("duration").and_then(Value::as_f64).filter(|d| *d > 0.0).map(|d| d.round() as u64);
    Some(Entry { id, url, title, thumb_url, posted_at, duration })
}

/// Unix seconds of midnight UTC for a `YYYYMMDD` date.
fn date_to_secs(s: &str) -> Option<u64> {
    if s.len() != 8 || !s.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    let (y, m, d) = (s[0..4].parse().ok()?, s[4..6].parse().ok()?, s[6..8].parse().ok()?);
    let secs = chrono::NaiveDate::from_ymd_opt(y, m, d)?.and_hms_opt(0, 0, 0)?.and_utc().timestamp();
    u64::try_from(secs).ok()
}

/// Splits on whitespace, honouring "double" and 'single' quotes.
pub fn tokenize(input: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut in_token = false;
    let mut quote: Option<char> = None;
    for c in input.chars() {
        match quote {
            Some(q) if c == q => quote = None,
            Some(_) => cur.push(c),
            None if c == '"' || c == '\'' => {
                quote = Some(c);
                in_token = true;
            }
            None if c.is_whitespace() => {
                if in_token {
                    out.push(std::mem::take(&mut cur));
                    in_token = false;
                }
            }
            None => {
                cur.push(c);
                in_token = true;
            }
        }
    }
    if in_token {
        out.push(cur);
    }
    out
}

/// Replaces `{url}` and `{dir}` in each argument; the URL is appended if `{url}` is absent.
fn expand(template: &str, url: &str, dir: &str) -> Vec<String> {
    let mut args: Vec<String> = tokenize(template).into_iter().map(|t| t.replace("{url}", url).replace("{dir}", dir)).collect();
    if !template.contains("{url}") {
        args.push(url.to_string());
    }
    args
}

/// Checker arguments for a profile or single post URL.
pub fn checker_cmd(site: &PostSite, url: &str) -> Vec<String> {
    expand(site.checker_or_default(), url, "")
}

/// Download arguments for one post into `dir`.
pub fn download_cmd(site: &PostSite, url: &str, dir: &str) -> Vec<String> {
    expand(site.download_or_default(), url, dir)
}

/// Arguments that print the stream playlist URL of one post.
pub fn m3u8_cmd(site: &PostSite, url: &str) -> Vec<String> {
    expand(site.m3u8_or_default(), url, "")
}

/// The notification for new posts of one account.
pub fn new_posts_message(account: &str, posts: &[Post]) -> notify::Message {
    let first = posts.first().map(|p| p.title.as_str()).filter(|t| !t.is_empty()).unwrap_or("New post");
    let body = if posts.len() > 1 { format!("{first}\n+{} more", posts.len() - 1) } else { first.to_string() };
    let title = if posts.len() > 1 { format!("{} new posts from @{account}", posts.len()) } else { format!("New post from @{account}") };
    notify::Message { title, body, click: posts.first().map(|p| p.url.clone()) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entry_reads_core_fields() {
        let l = r#"{"id":"123","webpage_url":"https://tiktok.com/@a/video/123","title":"hi\nthere","thumbnails":[{"url":"u1"},{"url":"u2"}],"timestamp":1700000000.0,"duration":12.4}"#;
        let e = parse_entry(l).unwrap();
        assert_eq!((e.id.as_str(), e.title.as_str(), e.thumb_url.as_deref()), ("123", "hi", Some("u2")));
        assert_eq!((e.posted_at, e.duration), (Some(1700000000), Some(12)));
        assert!(parse_entry("not json").is_none());
        assert!(parse_entry(r#"{"id":"1"}"#).is_none());
    }

    #[test]
    fn upload_date_parses() {
        assert_eq!(date_to_secs("20240101"), Some(1704067200));
        assert_eq!(date_to_secs("2024"), None);
    }

    fn tiktok() -> PostSite {
        PostSite::from_preset(Preset::Tiktok)
    }

    #[test]
    fn accounts_from_names_and_urls() {
        let mut site = tiktok();
        let a = site.account_from_input(" @Emiru ").unwrap();
        assert_eq!((a.name.as_str(), a.url.as_str()), ("Emiru", "https://www.tiktok.com/@Emiru"));
        site.accounts.push(a);
        assert!(site.account_from_input("emiru").is_none());
        assert!(site.account_from_input("two words").is_none());
        let b = site.account_from_input("https://www.tiktok.com/@ash/?lang=en").unwrap();
        assert_eq!(b.name, "ash");
    }

    #[test]
    fn blank_site_needs_a_url_for_accounts() {
        let site = PostSite::from_preset(Preset::Blank);
        assert!(site.account_from_input("someone").is_none());
        assert!(site.account_from_input("https://example.com/u/someone").is_some());
    }

    #[test]
    fn site_names_are_unique() {
        let mut cfg = PostsConfig::default();
        assert!(cfg.add_site(tiktok()).is_some());
        assert!(cfg.add_site(tiktok()).is_none());
    }

    #[test]
    fn templates_default_to_seven_day_cleanup() {
        assert_eq!(tiktok().auto_delete_days, Some(7));
        assert_eq!(PostSite::from_preset(Preset::Blank).auto_delete_days, Some(7));
        assert_eq!(tiktok().interval_or_default(), 300);
    }

    #[test]
    fn commands_fill_placeholders() {
        let mut site = tiktok();
        let dl = download_cmd(&site, "https://x/v/1", "C:\\out dir");
        assert!(dl.contains(&"C:\\out dir".to_string()) && dl.last().map(String::as_str) == Some("https://x/v/1"));
        assert!(site.user_agent().unwrap().starts_with("Mozilla/5.0"));
        site.checker_args = "--dump-json".into();
        assert_eq!(checker_cmd(&site, "u"), vec!["--dump-json", "u"]);
        assert_eq!(site.user_agent(), None);
    }

    #[test]
    fn twitch_vods_preset_is_manual_and_streams() {
        let site = PostSite::from_preset(Preset::TwitchVods);
        assert!(!site.auto_download && !site.backfill && site.open_m3u8 && site.auto_delete_days.is_none());
        assert!(!tiktok().open_m3u8 && !PostSite::from_preset(Preset::Blank).open_m3u8);
        let a = site.account_from_input("Emiru").unwrap();
        assert_eq!(a.url, "https://www.twitch.tv/Emiru/videos?filter=archives");
        let b = site.account_from_input("https://www.twitch.tv/ash/videos?filter=archives").unwrap();
        assert_eq!(b.name, "ash");
    }

    #[test]
    fn first_launch_has_every_named_site() {
        let names: Vec<String> = PostsConfig::first_launch().sites.into_iter().map(|s| s.name).collect();
        assert_eq!(names, ["TikTok", "Youtube Shorts", "Twitch Vods"]);
    }

    #[test]
    fn m3u8_command_fills_the_url() {
        assert_eq!(m3u8_cmd(&tiktok(), "https://x/v/1"), vec!["--no-warnings", "--no-playlist", "--get-url", "https://x/v/1"]);
    }

    #[test]
    fn tokenizer_keeps_quoted_values() {
        assert_eq!(tokenize(r#"--a "b c" 'd e' f"#), vec!["--a", "b c", "d e", "f"]);
    }

    #[test]
    fn sections_filter_posts() {
        let mut p = Post { viewed: true, starred: true, ..Default::default() };
        assert!(p.in_section(Section::Viewed) && p.in_section(Section::Starred) && !p.in_section(Section::Unviewed));
        p.state = PostState::Removed;
        assert!(!p.in_section(Section::Viewed));
    }

    #[test]
    fn thumb_names_are_filesystem_safe() {
        assert_eq!(thumb_name("a/b:1"), "a_b_1.jpg");
    }
}
