//! Data model of the Posts monitor: sites, accounts and posts, and the yt-dlp command lines built from them.

use crate::api::DEFAULT_EXE;
use crate::engine::template::tokenize;
use crate::policy::notify;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::{Path, PathBuf};

pub const DEFAULT_INTERVAL_SECS: u64 = 300;
/// Days before unstarred posts are deleted, in the TikTok and blank templates.
pub const DEFAULT_AUTO_DELETE_DAYS: u64 = 7;
/// Megabytes above which automatic downloads are skipped, in every template.
pub const DEFAULT_AUTO_DOWNLOAD_LIMIT_MB: u64 = 15;
/// Seconds after which automatic downloads are cancelled, in every template.
pub const DEFAULT_AUTO_DOWNLOAD_SECS: u64 = 30;
pub const MIN_INTERVAL_SECS: u64 = 15;
/// Checker arguments used when a site's own are blank; `{url}` is the profile (or post) URL.
pub const DEFAULT_CHECKER_ARGS: &str = "--no-warnings --ignore-errors --flat-playlist --dump-json --playlist-end 5 {url}";
/// Download arguments used when a site's own are blank; `{url}` is the post URL and `{dir}` the download folder.
pub const DEFAULT_DOWNLOAD_ARGS: &str = r#"--no-warnings --ignore-errors --no-playlist --no-simulate -P "{dir}" -o "%(uploader)s_%(upload_date)s_%(id)s.%(ext)s" {url}"#;
/// Arguments used when a site's own are blank; `{url}` is the post URL and the printed line is the stream playlist.
pub const DEFAULT_M3U8_ARGS: &str = "--no-warnings --no-playlist --get-url {url}";
const TWITCH_VODS_URL_TEMPLATE: &str = "https://www.twitch.tv/{username}/videos?filter=archives";
const YOUTUBE_SHORTS_URL_TEMPLATE: &str = "https://www.youtube.com/@{username}/shorts";
const YOUTUBE_SHORTS_DOWNLOAD_ARGS: &str = r#"--cookies-from-browser firefox --no-warnings --ignore-errors --no-playlist --no-simulate -P "{dir}" -o "%(uploader)s_%(upload_date)s_%(id)s.%(ext)s" {url}"#;
const TWITTER_X_DOWNLOAD_ARGS: &str = r#"--no-warnings --ignore-errors --no-playlist --no-simulate -P "{dir}" -o "%(uploader)s_%(upload_date)s_%(display_id)s.%(ext)s" {url}"#;
const TWITTER_X_URL_TEMPLATE: &str = "https://x.com/{username}";
const TIKTOK_URL_TEMPLATE: &str = "https://www.tiktok.com/@{username}";
const TIKTOK_CHECKER_ARGS: &str = r#"--no-warnings --ignore-errors --user-agent "Mozilla/5.0 (Windows NT 10.0; Win64; x64; rv:153.0) Gecko/20100101 Firefox/153.0" --flat-playlist --dump-json --playlist-end 5 {url}"#;
const TIKTOK_DOWNLOAD_ARGS: &str = r#"--no-warnings --ignore-errors --user-agent "Mozilla/5.0 (Windows NT 10.0; Win64; x64; rv:153.0) Gecko/20100101 Firefox/153.0" --no-playlist --no-simulate -P "{dir}" -o "%(uploader)s_%(upload_date)s_%(id)s_%(resolution)s.%(ext)s" {url}"#;
/// Failed downloads are retried until they have failed this many times.
pub const MAX_ATTEMPTS: u8 = 3;

pub type SiteId = u64;

/// Starting points offered by the site buttons.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Preset {
    Tiktok,
    YoutubeShorts,
    TwitchVods,
    TwitterX,
    Blank,
}

impl Preset {
    /// Every preset, in the order the site buttons show them.
    pub const ALL: [Preset; 5] = [Preset::Tiktok, Preset::YoutubeShorts, Preset::TwitchVods, Preset::TwitterX, Preset::Blank];

    /// Every named preset, in dropdown order.
    pub const NAMED: [Preset; 4] = [Preset::Tiktok, Preset::YoutubeShorts, Preset::TwitchVods, Preset::TwitterX];

    /// The button label, which is also the site name.
    pub fn label(self) -> &'static str {
        match self {
            Preset::Tiktok => "TikTok",
            Preset::YoutubeShorts => "Youtube Shorts",
            Preset::TwitchVods => "Twitch Vods",
            Preset::TwitterX => "Twitter/X",
            Preset::Blank => "New Site",
        }
    }

    /// The preset whose label is `name`, else blank.
    pub fn from_name(name: &str) -> Preset {
        Preset::NAMED.into_iter().find(|p| p.label().eq_ignore_ascii_case(name.trim())).unwrap_or(Preset::Blank)
    }
}

/// One profile that is checked for new posts.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Account {
    pub name: String,
    pub url: String,
    pub enabled: bool,
    /// True once a check of this account has succeeded.
    pub baselined: bool,
    /// Writes the checker's stdout to this account's log file.
    #[serde(default)]
    pub log_stdout: bool,
    /// Writes the checker's stderr to this account's log file.
    #[serde(default)]
    pub log_stderr: bool,
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
    /// Automatic downloads skip posts larger than this many megabytes; `None` means no limit.
    pub auto_download_limit_mb: Option<u64>,
    /// Automatic downloads are cancelled after this many seconds; `None` means no limit.
    pub auto_download_secs: Option<u64>,
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
    /// Writes the downloader's stdout to the site's log file.
    pub log_download_stdout: bool,
    /// Writes the downloader's stderr to the site's log file.
    pub log_download_stderr: bool,
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
            auto_download_limit_mb: Some(DEFAULT_AUTO_DOWNLOAD_LIMIT_MB),
            auto_download_secs: Some(DEFAULT_AUTO_DOWNLOAD_SECS),
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
            log_download_stdout: false,
            log_download_stderr: false,
            accounts: Vec::new(),
        }
    }
}

impl PostSite {
    /// An unsaved site holding a preset's values; the id is assigned when it is added.
    pub fn from_preset(preset: Preset) -> PostSite {
        match preset {
            Preset::Tiktok => PostSite {
                name: Preset::Tiktok.label().into(),
                checker_args: TIKTOK_CHECKER_ARGS.into(),
                download_args: TIKTOK_DOWNLOAD_ARGS.into(),
                url_template: TIKTOK_URL_TEMPLATE.into(),
                ..Default::default()
            },
            Preset::YoutubeShorts => PostSite {
                name: Preset::YoutubeShorts.label().into(),
                download_args: YOUTUBE_SHORTS_DOWNLOAD_ARGS.into(),
                url_template: YOUTUBE_SHORTS_URL_TEMPLATE.into(),
                ..Default::default()
            },
            Preset::TwitchVods => PostSite {
                name: Preset::TwitchVods.label().into(),
                url_template: TWITCH_VODS_URL_TEMPLATE.into(),
                auto_download: false,
                backfill: false,
                open_m3u8: true,
                auto_delete_days: None,
                ..Default::default()
            },
            Preset::TwitterX => PostSite {
                name: Preset::TwitterX.label().into(),
                download_args: TWITTER_X_DOWNLOAD_ARGS.into(),
                url_template: TWITTER_X_URL_TEMPLATE.into(),
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
        (!dup).then_some(Account { name, url, enabled: true, baselined: false, log_stdout: false, log_stderr: false })
    }
}

/// Contents of posts.json.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PostsConfig {
    pub sites: Vec<PostSite>,
}

impl Default for PostsConfig {
    fn default() -> Self {
        PostsConfig { sites: Vec::new() }
    }
}

impl PostsConfig {
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
    /// Unix time the error was set; the error is cleared after a delay.
    pub error_at: Option<u64>,
    pub attempts: u8,
    /// True when the person asked for this download, so the size and time limits do not apply.
    pub manual: bool,
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
        super::store::thumbs_dir().join(thumb_name(&self.id))
    }
}

/// A post id made safe for use in a file name.
pub fn safe_name(id: &str) -> String {
    id.chars().map(|c| if c.is_ascii_alphanumeric() || c == '_' || c == '-' { c } else { '_' }).collect()
}

/// The thumbnail file name for a post id.
pub fn thumb_name(id: &str) -> String {
    format!("{}.jpg", safe_name(id))
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
    let (posted_at, duration) = time_fields(&v);
    Some(Entry { id, url, title, thumb_url, posted_at, duration })
}

/// Post time and duration in seconds from a yt-dlp JSON object.
fn time_fields(v: &Value) -> (Option<u64>, Option<u64>) {
    let posted_at = v
        .get("timestamp")
        .and_then(Value::as_f64)
        .filter(|t| *t > 0.0)
        .map(|t| t as u64)
        .or_else(|| v.get("upload_date").and_then(Value::as_str).and_then(date_to_secs));
    let duration = v.get("duration").and_then(Value::as_f64).filter(|d| *d > 0.0).map(|d| d.round() as u64);
    (posted_at, duration)
}

/// Post time and duration from the JSON line `download_cmd` writes to its sidecar.
pub fn parse_meta(text: &str) -> (Option<u64>, Option<u64>) {
    text.lines()
        .map(str::trim)
        .filter(|l| l.starts_with('{'))
        .find_map(|l| serde_json::from_str::<Value>(l).ok())
        .map_or((None, None), |v| time_fields(&v))
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

/// Download arguments for one post into `dir`, plus options that write the final file path and the post time and duration to `sidecar` and cap the size at `max_mb`.
pub fn download_cmd(site: &PostSite, url: &str, dir: &str, sidecar: &Path, max_mb: Option<u64>) -> Vec<String> {
    let mut args = expand(site.download_or_default(), url, dir);
    if let Some(mb) = max_mb {
        args.extend(["--max-filesize".to_string(), format!("{mb}M")]);
    }
    let sidecar = sidecar.to_string_lossy().replace('%', "%%");
    for template in ["after_move:filepath", "after_move:%(.{timestamp,upload_date,duration})j"] {
        args.extend(["--print-to-file".to_string(), template.to_string(), sidecar.clone()]);
    }
    args
}

/// Paths in yt-dlp output or a sidecar file, last line first.
pub fn path_lines(text: &str) -> impl Iterator<Item = PathBuf> + '_ {
    text.lines().map(str::trim).rev().filter(|l| !l.is_empty()).map(PathBuf::from)
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
    fn meta_line_gives_time_and_duration() {
        let t = "C:\\a.mp4\n{\"timestamp\": 1700000000, \"upload_date\": \"20231114\", \"duration\": 20.6}\n";
        assert_eq!(parse_meta(t), (Some(1700000000), Some(21)));
        assert_eq!(parse_meta("{\"timestamp\": null, \"upload_date\": \"20240101\", \"duration\": null}"), (Some(1704067200), None));
        assert_eq!(parse_meta("a.mp4"), (None, None));
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
        let dl = download_cmd(&site, "https://x/v/1", "C:\\out dir", Path::new("s%1.txt"), None);
        assert!(dl.contains(&"C:\\out dir".to_string()) && dl.contains(&"https://x/v/1".to_string()));
        assert_eq!(&dl[dl.len() - 3..], ["--print-to-file", "after_move:filepath", "s%%1.txt"]);
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
    fn named_presets_are_every_preset_but_blank() {
        let names: Vec<&str> = Preset::NAMED.into_iter().map(Preset::label).collect();
        assert_eq!(names, ["TikTok", "Youtube Shorts", "Twitch Vods", "Twitter/X"]);
        assert!(!Preset::NAMED.contains(&Preset::Blank));
    }

    #[test]
    fn auto_download_limits_default_everywhere_and_size_becomes_an_argument() {
        for preset in Preset::ALL {
            let site = PostSite::from_preset(preset);
            assert_eq!((site.auto_download_limit_mb, site.auto_download_secs), (Some(15), Some(30)));
        }
        let dl = download_cmd(&tiktok(), "u", "d", Path::new("s.txt"), Some(15));
        assert!(dl.windows(2).any(|w| w == ["--max-filesize", "15M"]));
    }

    #[test]
    fn path_lines_are_newest_first_and_skip_blanks() {
        let got: Vec<PathBuf> = path_lines("a.mp4\r\n\n b.mp4 \n").collect();
        assert_eq!(got, [PathBuf::from("b.mp4"), PathBuf::from("a.mp4")]);
    }

    #[test]
    fn twitter_x_downloads_are_named_by_post_id() {
        let site = PostSite::from_preset(Preset::TwitterX);
        assert!(site.download_args.contains("%(display_id)s"));
        assert!(!site.download_args.contains("--print"));
    }

    #[test]
    fn twitter_x_preset_builds_profile_urls() {
        let site = PostSite::from_preset(Preset::TwitterX);
        assert_eq!(site.name, "Twitter/X");
        assert_eq!(site.account_from_input("@smeepsx").unwrap().url, "https://x.com/smeepsx");
        assert_eq!(site.account_from_input("https://x.com/ash/").unwrap().name, "ash");
    }

    #[test]
    fn presets_are_found_by_name() {
        assert_eq!(Preset::from_name(" twitter/x "), Preset::TwitterX);
        assert_eq!(Preset::from_name("Blank"), Preset::Blank);
        assert_eq!(Preset::from_name("other"), Preset::Blank);
    }

    #[test]
    fn m3u8_command_fills_the_url() {
        assert_eq!(m3u8_cmd(&tiktok(), "https://x/v/1"), vec!["--no-warnings", "--no-playlist", "--get-url", "https://x/v/1"]);
    }

    #[test]
    fn thumb_names_are_filesystem_safe() {
        assert_eq!(thumb_name("a/b:1"), "a_b_1.jpg");
    }
}
