//! Defaults for new sites: the presets and every per-site default value.

use super::NewSite;
use crate::api::{CheckMode, FolderLayout};

pub const DEFAULT_JSON_ARGS: &str = "--dump-json --no-warnings --skip-download {urls}";
pub const DEFAULT_SIM_ARGS: &str =
    "--simulate --no-warnings --match-filter is_live --print \"%(original_url)s|%(webpage_url)s\" {urls}";

/// Colour given to new blank sites.
pub const DEFAULT_SITE_COLOR: &str = "#7AA2F7";

const DEFAULT_INTERVAL_SECS: u64 = 60;
const DEFAULT_END_AFTER_MINUTES: u64 = 30;
const DEFAULT_FILE_EXISTS_GRACE_SECS: u64 = 60;
const DEFAULT_GROWTH_GRACE_SECS: u64 = 60;
const DEFAULT_STALL_WINDOW_SECS: u64 = 120;
const TIKTOK_STALL_WINDOW_SECS: u64 = 360;
const DEFAULT_FILE_CHECK_INTERVAL_SECS: u64 = 30;
const DEFAULT_RESTART_CLEAR_MINUTES: u64 = 1440;
const DEFAULT_RESTART_THRESHOLD: u32 = 1;

/// Starting point for a new site.
#[derive(Debug, Clone, Copy)]
pub enum Preset {
    Twitch,
    Tiktok,
    Chaturbate,
    Kick,
    Blank,
}

/// The named preset whose site name matches `name` (ignoring case), else `Blank`.
pub fn preset_for_name(name: &str) -> Preset {
    [Preset::Twitch, Preset::Tiktok, Preset::Chaturbate, Preset::Kick]
        .into_iter()
        .find(|p| new_site(*p).name.eq_ignore_ascii_case(name.trim()))
        .unwrap_or(Preset::Blank)
}

/// Default yt-dlp path for new sites: the bundled binary on Windows/Linux, else PATH.
fn default_exe() -> &'static str {
    if cfg!(windows) {
        r"bin\windows\yt-dlp\yt-dlp.exe"
    } else if cfg!(target_os = "linux") {
        "bin/linux/yt-dlp/yt-dlp"
    } else {
        "yt-dlp"
    }
}

/// Every setting of a new site for `preset`.
pub fn new_site(preset: Preset) -> NewSite {
    let (name, url_template, checker, recorder, color) = match preset {
        Preset::Twitch => (
            "Twitch",
            "https://twitch.tv/{username}",
            "--cookies-from-browser firefox --dump-json {urls}",
            r#"--cookies-from-browser firefox --fixup never --no-part --output "%(title)s [%(id)s].%(ext)s" {url}"#,
            "#9146FF",
        ),
        Preset::Tiktok => (
            "Tiktok",
            "https://tiktok.com/@{username}/live",
            "--cookies-from-browser firefox --dump-json {urls}",
            r#"--cookies-from-browser firefox --downloader ffmpeg --no-part --retries 0 --output "%(uploader)s %(epoch>%Y-%m-%d %H_%M_%S)s.%(ext)s" {url}"#,
            "#FE2C55",
        ),
        Preset::Chaturbate => (
            "Chaturbate",
            "https://chaturbate.com/{username}",
            "--dump-json {urls}",
            r#"--fixup never --no-part --downloader-args ffmpeg:"-fps_mode passthrough -copyts -avoid_negative_ts make_zero" --output "%(title)s.%(ext)s" {url}"#,
            "#F89C1C",
        ),
        Preset::Kick => (
            "Kick",
            "https://kick.com/{username}",
            "--dump-json {urls}",
            r#"--fixup never --no-part --output "%(uploader)s %(epoch>%Y-%m-%d %H_%M_%S)s.%(ext)s" {url}"#,
            "#53FC18",
        ),
        Preset::Blank => ("New site", "", "", "", DEFAULT_SITE_COLOR),
    };
    NewSite {
        name: name.into(),
        exe_path: default_exe().into(),
        interval_secs: DEFAULT_INTERVAL_SECS,
        check_mode: CheckMode::DumpJson,
        checker_args: checker.into(),
        recorder_args: recorder.into(),
        url_template: url_template.into(),
        output_dir: String::new(),
        folder_layout: FolderLayout::default(),
        log_checker_stdout: false,
        log_checker_stderr: false,
        end_after_minutes: DEFAULT_END_AFTER_MINUTES,
        file_exists_grace_secs: DEFAULT_FILE_EXISTS_GRACE_SECS,
        growth_grace_secs: DEFAULT_GROWTH_GRACE_SECS,
        stall_window_secs: if matches!(preset, Preset::Tiktok) {
            TIKTOK_STALL_WINDOW_SECS
        } else {
            DEFAULT_STALL_WINDOW_SECS
        },
        file_check_interval_secs: DEFAULT_FILE_CHECK_INTERVAL_SECS,
        restart_clear_minutes: DEFAULT_RESTART_CLEAR_MINUTES,
        restart_threshold: DEFAULT_RESTART_THRESHOLD,
        color: Some(color.into()),
    }
}
