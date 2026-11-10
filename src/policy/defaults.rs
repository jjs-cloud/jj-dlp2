//! Presets for new sites; only site-specific fields are pre-filled, the rest fall back at run time.

use super::NewSite;
use crate::api::FolderLayout;

/// Colour given to new blank sites.
pub const DEFAULT_SITE_COLOR: &str = "#7AA2F7";

/// ffmpeg error phrases every new site starts with.
const DEFAULT_ERROR_PATTERNS: &str = "timestamp discontinuity|Packet corrupt";

/// Ad-break phrases the Twitch preset starts with.
const TWITCH_INTERRUPT_PATTERNS: &str =
    "EXT-X-DISCONTINUITY|amazon|twitch-ad|/ad/|admanifest|/ads/|EXT-X-TWITCH-AD|twitch-stitched-ad";

/// Starting point for a new site.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Preset {
    Twitch,
    Tiktok,
    Chaturbate,
    Kick,
    Youtube,
    Blank,
}

impl Preset {
    /// Every named preset, in dropdown order.
    pub const NAMED: [Preset; 5] = [Preset::Twitch, Preset::Tiktok, Preset::Chaturbate, Preset::Kick, Preset::Youtube];
}

/// The named preset whose site name matches `name` (ignoring case), else `Blank`.
pub fn preset_for_name(name: &str) -> Preset {
    Preset::NAMED
        .into_iter()
        .find(|p| new_site(*p).name.eq_ignore_ascii_case(name.trim()))
        .unwrap_or(Preset::Blank)
}

/// A new site for `preset`: name, URL template, checker/recorder arguments and colour; everything else is blank.
pub fn new_site(preset: Preset) -> NewSite {
    let (name, url_template, checker, recorder, color) = match preset {
        Preset::Twitch => (
            "Twitch",
            "https://twitch.tv/{username}",
            "--cookies-from-browser firefox --dump-json {urls}",
            r#"--verbose --cookies-from-browser firefox --fixup never --no-part --output "%(title)s [%(id)s].%(ext)s" {url}"#,
            "#9146FF",
        ),
        Preset::Tiktok => (
            "Tiktok",
            "https://tiktok.com/@{username}/live",
            "--cookies-from-browser firefox --dump-json {urls}",
            r#"--cookies-from-browser firefox --downloader ffmpeg --no-part --retries 0 --output "%(uploader)s %(epoch>%Y-%m-%d %H_%M_%S)s.%(ext)s" {url}"#,
            "#25F4EE",
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
        Preset::Youtube => (
            "Youtube",
            "https://www.youtube.com/@{username}/live",
            "--dump-json {urls}",
            r#"--fixup never --no-part --replace-in-metadata uploader_id "^@" "" --output "%(uploader_id)s %(epoch>%Y-%m-%d %H_%M_%S)s [%(id)s].%(ext)s" {url}"#,
            "#FF0000",
        ),
        Preset::Blank => ("", "", "", "", DEFAULT_SITE_COLOR),
    };
    NewSite {
        name: name.into(),
        exe_path: String::new(),
        interval_secs: None,
        checker_args: checker.into(),
        recorder_args: recorder.into(),
        url_template: url_template.into(),
        output_dir: String::new(),
        folder_layout: FolderLayout::default(),
        log_checker_stdout: false,
        log_checker_stderr: false,
        end_after_minutes: None,
        file_exists_grace_secs: None,
        growth_grace_secs: None,
        stall_window_secs: None,
        file_check_interval_secs: None,
        restart_clear_minutes: None,
        restart_threshold: None,
        forward_output: true,
        interrupt_patterns: if matches!(preset, Preset::Twitch) { TWITCH_INTERRUPT_PATTERNS.into() } else { String::new() },
        error_patterns: DEFAULT_ERROR_PATTERNS.into(),
        warn_interrupts: true,
        warn_errors: true,
        color: Some(color.into()),
        notify: true,
        notify_desktop: true,
        notify_ntfy: false,
        ntfy_topic: String::new(),
        notify_cooldown_minutes: None,
        notify_timeout_secs: None,
        notify_confirmed: true,
        notify_stalled: false,
        notify_restarted: false,
        notify_on_launch: true,
        notify_live: true,
        warnings_enabled: true,
        notify_interrupts: false,
        notify_errors: false,
        warn_stalled: true,
        warn_restarts: true,
        split_enabled: false,
        split_minutes: None,
        auto_suffix: true,
        paused: false,
        upgrade_quality: !matches!(preset, Preset::Tiktok),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::{DEFAULT_CHECKER_ARGS, DEFAULT_EXE, DEFAULT_INTERVAL_SECS};

    #[test]
    fn blank_site_has_no_values_and_falls_back() {
        let site = new_site(Preset::Blank).into_site(1);
        assert!(site.name.is_empty() && site.url_template.is_empty());
        assert_eq!(site.interval_or_default(), DEFAULT_INTERVAL_SECS);
        assert_eq!(site.checker_or_default(), DEFAULT_CHECKER_ARGS);
        assert_eq!(site.exe_or_default(), DEFAULT_EXE);
    }

    #[test]
    fn warning_patterns_default_on_per_preset() {
        assert!(new_site(Preset::Twitch).interrupt_patterns.contains("twitch-stitched-ad"));
        assert!(new_site(Preset::Kick).interrupt_patterns.is_empty());
        assert!(new_site(Preset::Blank).error_patterns.contains("Packet corrupt"));
    }

    #[test]
    fn output_forwarding_defaults_on() {
        for preset in [Preset::Twitch, Preset::Kick, Preset::Blank] {
            assert!(new_site(preset).forward_output);
        }
    }

    #[test]
    fn upgrade_quality_defaults_on_except_for_tiktok() {
        for preset in [Preset::Twitch, Preset::Chaturbate, Preset::Kick, Preset::Youtube, Preset::Blank] {
            assert!(new_site(preset).upgrade_quality);
        }
        assert!(!new_site(Preset::Tiktok).upgrade_quality);
    }

    #[test]
    fn presets_are_found_by_name() {
        assert!(matches!(preset_for_name("youtube"), Preset::Youtube));
        assert!(matches!(preset_for_name("unknown"), Preset::Blank));
    }
}
