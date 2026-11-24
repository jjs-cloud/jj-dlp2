//! One-time resets of stored settings, applied on the first launch after a release that adds them.

use crate::api::{FolderLayout, Settings, Site, SiteKey, Streamer, StreamerOptions};
use crate::gui::prefs;
use crate::policy::posts::{self, PostSite, PostsConfig, Preset as PostsPreset};
use crate::policy::update::{self, UpdateConfig};
use crate::policy::{key_for_name, new_site, Preset};
use crate::settings;
use serde::{Deserialize, Serialize};
use std::fmt::Debug;
use std::path::PathBuf;
use std::sync::Mutex;

/// Config revision of this build; bump it whenever entries are added to `RESETS`.
pub const CONFIG_REV: u32 = 3;

/// Resets to apply; each entry's `rev` is the `CONFIG_REV` it was added at (always above 1).
///
/// A key names the field exactly as it is in the settings files; `.site(..)` limits it to one built-in site.
pub const RESETS: &[Reset] = &[
    // ---- Examples (uncomment one, set its rev to the new CONFIG_REV, and bump CONFIG_REV) ----
    // Every Livestreams site goes back to the default check interval, whatever it is set to now.
    // Reset::always(3, Key::Live(LiveField::IntervalSecs)),
    //
    // Only the Twitch Livestreams site goes back to the default check interval.
    // Reset::always(3, Key::Live(LiveField::IntervalSecs)).site(SiteKey::Twitch),
    //
    // Livestreams sites that still have this exact old text get the site template's checker arguments.
    // Reset::if_equals(3, Key::Live(LiveField::CheckerArgs), Val::Text("--old-args {urls}")),
    //
    // Every Posts site that checks every 60 seconds goes back to its template's interval (the default if it has none).
    // Reset::if_equals(3, Key::Posts(PostsField::IntervalSecs), Val::Num(60)),
    //
    // Posts sites with no download size limit get their template's limit.
    // Reset::if_equals(3, Key::Posts(PostsField::AutoDownloadLimitMb), Val::Unset),
    //
    // Turns notifications back on for every Posts site, even ones where they were switched off.
    // Reset::always(3, Key::Posts(PostsField::Notify)),
    //
    // Only the YouTube Shorts Posts site gets automatic download turned off, if it is on now.
    // Reset::if_equals(3, Key::Posts(PostsField::AutoDownload), Val::Flag(true)).site(SiteKey::YoutubeShorts),
    //
    // The global output folder goes back to its default.
    // Reset::always(3, Key::Settings(SettingsField::DefaultOutputDir)),
    //
    // Every streamer stops logging recorder output to a file.
    // Reset::always(3, Key::Streamer(StreamerField::LogStdout)),
    //
    // Update checks are turned back on.
    // Reset::always(3, Key::Update(UpdateField::Enabled)),
    //
    // Only a Compact view row height of exactly 40 goes back to the default.
    // Reset::if_equals(3, Key::Gui(GuiField::RowMax), Val::Float(40.0)),
    // ---- Entries ----
    // Turns "Use TikTok Photos scraper" on for the TikTok posts site.
    Reset::if_equals(2, Key::Posts(PostsField::UseTiktokPhotos), Val::Flag(false)).site(SiteKey::Tiktok),
    // Switches the Twitter/X posts site to the gallery-dl helper.
    Reset::always(3, Key::Posts(PostsField::XHelper)).site(SiteKey::TwitterX),
    // Sets the Twitter/X posts site to check every 900 seconds.
    Reset::always(3, Key::Posts(PostsField::IntervalSecs)).site(SiteKey::TwitterX),
];

/// A previously stored value to match; `Unset` matches a blank (`None`) field.
#[allow(dead_code)]
#[derive(Debug, Clone, Copy)]
pub enum Val {
    Text(&'static str),
    Num(u64),
    Flag(bool),
    Float(f32),
    Layout(FolderLayout),
    Unset,
}

/// Whether a reset overrides every value or only a known old one.
#[allow(dead_code)]
#[derive(Debug, Clone, Copy)]
pub enum When {
    Always,
    IfEquals(Val),
}

/// A resettable setting; each variant wraps the fields of one settings file, named like the field.
#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    Settings(SettingsField),
    /// A Livestreams site (`sites.json`).
    Live(LiveField),
    Streamer(StreamerField),
    /// A Posts site (`posts.json`).
    Posts(PostsField),
    Update(UpdateField),
    Gui(GuiField),
}

/// Fields of `settings.json`.
#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsField {
    DefaultOutputDir,
}

/// Livestreams site fields; checker args, recorder args, URL template and color re-apply the site template.
#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LiveField {
    ExePath,
    IntervalSecs,
    CheckerArgs,
    RecorderArgs,
    UrlTemplate,
    OutputDir,
    FolderLayout,
    LogCheckerStdout,
    LogCheckerStderr,
    EndAfterMinutes,
    FileExistsGraceSecs,
    GrowthGraceSecs,
    StallWindowSecs,
    FileCheckIntervalSecs,
    WarningClearMinutes,
    RestartThreshold,
    Color,
}

/// Streamer option fields.
#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StreamerField {
    LogStdout,
    LogStderr,
    FolderLayout,
}

/// Posts site fields; the first group re-applies the site template, the rest use the plain default.
#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PostsField {
    CheckerArgs,
    DownloadArgs,
    UrlTemplate,
    XHelper,
    UseTiktokPhotos,
    AutoDownload,
    AutoDownloadLimitMb,
    AutoDownloadSecs,
    Backfill,
    OpenM3u8,
    AutoDeleteDays,
    /// Uses the template's value for a built-in site and the default otherwise.
    IntervalSecs,
    ExePath,
    OutputDir,
    MarkOnOpen,
    Notify,
    NotifyDesktop,
    NotifyNtfy,
    NtfyTopic,
    M3u8Args,
    PlayerPath,
    LogDownloadStdout,
    LogDownloadStderr,
}

/// Fields of `update.json`.
#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UpdateField {
    Enabled,
    IncludeRc,
}

/// Fields of `gui.json`.
#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GuiField {
    Theme,
    Compact,
    RowMin,
    RowMax,
    NameClip,
    LogLines,
    RememberSize,
    WinW,
    WinH,
}

/// One entry of `RESETS`.
#[derive(Debug, Clone, Copy)]
pub struct Reset {
    pub rev: u32,
    pub key: Key,
    pub when: When,
    /// Limits a `Live` or `Posts` reset to the site with this key; `None` means every site.
    pub site: Option<SiteKey>,
}

#[allow(dead_code)]
impl Reset {
    /// Resets the setting whatever its current value.
    pub const fn always(rev: u32, key: Key) -> Self {
        Self { rev, key, when: When::Always, site: None }
    }

    /// Resets the setting only if it still holds `old`.
    pub const fn if_equals(rev: u32, key: Key, old: Val) -> Self {
        Self { rev, key, when: When::IfEquals(old), site: None }
    }

    /// Limits the reset to the built-in site with this key, even if it was renamed; custom sites are skipped.
    pub const fn site(self, key: SiteKey) -> Self {
        Self { rev: self.rev, key: self.key, when: self.when, site: Some(key) }
    }

    /// Whether a site with this stored `site_key` is in scope.
    fn targets(&self, site_key: &str) -> bool {
        match self.site {
            Some(k) => k.slug() == site_key,
            None => true,
        }
    }
}

/// A field type that can be compared with a `Val`.
pub trait Resettable {
    fn eq_val(&self, v: &Val) -> bool;

    fn matches(&self, when: &When) -> bool {
        match when {
            When::Always => true,
            When::IfEquals(v) => self.eq_val(v),
        }
    }
}

impl Resettable for String {
    fn eq_val(&self, v: &Val) -> bool {
        matches!(v, Val::Text(t) if self.trim() == *t)
    }
}

impl Resettable for bool {
    fn eq_val(&self, v: &Val) -> bool {
        matches!(v, Val::Flag(b) if self == b)
    }
}

impl Resettable for f32 {
    fn eq_val(&self, v: &Val) -> bool {
        matches!(v, Val::Float(f) if (*self - *f).abs() < 1e-3)
    }
}

impl Resettable for posts::XHelper {
    fn eq_val(&self, _: &Val) -> bool {
        false
    }
}

impl Resettable for FolderLayout {
    fn eq_val(&self, v: &Val) -> bool {
        matches!(v, Val::Layout(l) if self == l)
    }
}

impl Resettable for Option<u64> {
    fn eq_val(&self, v: &Val) -> bool {
        match v {
            Val::Num(n) => self.is_some_and(|x| x == *n),
            Val::Unset => self.is_none(),
            _ => false,
        }
    }
}

impl Resettable for Option<u32> {
    fn eq_val(&self, v: &Val) -> bool {
        match v {
            Val::Num(n) => self.is_some_and(|x| u64::from(x) == *n),
            Val::Unset => self.is_none(),
            _ => false,
        }
    }
}

impl Resettable for Option<FolderLayout> {
    fn eq_val(&self, v: &Val) -> bool {
        match v {
            Val::Layout(l) => self.is_some_and(|x| x == *l),
            Val::Unset => self.is_none(),
            _ => false,
        }
    }
}

impl Resettable for Option<String> {
    fn eq_val(&self, v: &Val) -> bool {
        match v {
            Val::Text(t) => self.as_deref().is_some_and(|s| s.eq_ignore_ascii_case(t)),
            Val::Unset => self.is_none(),
            _ => false,
        }
    }
}

/// Sets `cur` to `to` if the reset applies and the value differs; returns "old -> new" when changed.
pub fn reset_field<T: Resettable + PartialEq + Debug>(cur: &mut T, to: T, when: &When) -> Option<String> {
    if *cur == to || !cur.matches(when) {
        return None;
    }
    let change = format!("{cur:?} -> {to:?}");
    *cur = to;
    Some(change)
}

fn describe(field: &str, change: Option<String>) -> Option<String> {
    change.map(|c| format!("{field} reset: {c}"))
}

/// Like `reset_field` with the template value as target; sites without a template are skipped.
fn reset_preset<T: Resettable + PartialEq + Debug>(cur: &mut T, to: Option<T>, field: &str, when: &When) -> Option<String> {
    to.and_then(|v| describe(field, reset_field(cur, v, when)))
}

/// The Livestreams template the site's key belongs to, if any.
fn preset_for(site: &Site) -> Option<crate::policy::NewSite> {
    let preset = Preset::from_key(site.key()?);
    (!matches!(preset, Preset::Blank)).then(|| new_site(preset))
}

fn reset_site(site: &mut Site, field: LiveField, when: &When) -> Option<String> {
    let p = preset_for(site);
    match field {
        LiveField::ExePath => describe("exe_path", reset_field(&mut site.exe_path, String::new(), when)),
        LiveField::IntervalSecs => describe("interval_secs", reset_field(&mut site.interval_secs, None, when)),
        LiveField::CheckerArgs => reset_preset(&mut site.checker_args, p.map(|p| p.checker_args), "checker_args", when),
        LiveField::RecorderArgs => {
            reset_preset(&mut site.recorder_args, p.map(|p| p.recorder_args), "recorder_args", when)
        }
        LiveField::UrlTemplate => reset_preset(&mut site.url_template, p.map(|p| p.url_template), "url_template", when),
        LiveField::Color => reset_preset(&mut site.color, p.map(|p| p.color), "color", when),
        LiveField::OutputDir => describe("output_dir", reset_field(&mut site.output_dir, String::new(), when)),
        LiveField::FolderLayout => {
            describe("folder_layout", reset_field(&mut site.folder_layout, FolderLayout::default(), when))
        }
        LiveField::LogCheckerStdout => describe("log_checker_stdout", reset_field(&mut site.log_checker_stdout, false, when)),
        LiveField::LogCheckerStderr => describe("log_checker_stderr", reset_field(&mut site.log_checker_stderr, false, when)),
        LiveField::EndAfterMinutes => describe("end_after_minutes", reset_field(&mut site.end_after_minutes, None, when)),
        LiveField::FileExistsGraceSecs => {
            describe("file_exists_grace_secs", reset_field(&mut site.file_exists_grace_secs, None, when))
        }
        LiveField::GrowthGraceSecs => describe("growth_grace_secs", reset_field(&mut site.growth_grace_secs, None, when)),
        LiveField::StallWindowSecs => describe("stall_window_secs", reset_field(&mut site.stall_window_secs, None, when)),
        LiveField::FileCheckIntervalSecs => {
            describe("file_check_interval_secs", reset_field(&mut site.file_check_interval_secs, None, when))
        }
        LiveField::WarningClearMinutes => {
            describe("warning_clear_minutes", reset_field(&mut site.warning_clear_minutes, None, when))
        }
        LiveField::RestartThreshold => describe("restart_threshold", reset_field(&mut site.restart_threshold, None, when)),
    }
}

fn reset_streamer(streamer: &mut Streamer, field: StreamerField, when: &When) -> Option<String> {
    let d = StreamerOptions::default();
    let o = &mut streamer.options;
    match field {
        StreamerField::LogStdout => describe("log_stdout", reset_field(&mut o.log_stdout, d.log_stdout, when)),
        StreamerField::LogStderr => describe("log_stderr", reset_field(&mut o.log_stderr, d.log_stderr, when)),
        StreamerField::FolderLayout => describe("folder_layout", reset_field(&mut o.folder_layout, d.folder_layout, when)),
    }
}

/// The Posts template the site's key belongs to, if any.
fn post_preset_for(site: &PostSite) -> Option<PostSite> {
    let preset = PostsPreset::from_key(site.key()?);
    (preset != PostsPreset::Blank).then(|| PostSite::from_preset(preset))
}

fn reset_post_site(site: &mut PostSite, field: PostsField, when: &When) -> Option<String> {
    let d = PostSite::default();
    let p = post_preset_for(site);
    match field {
        PostsField::CheckerArgs => reset_preset(&mut site.checker_args, p.map(|p| p.checker_args), "checker_args", when),
        PostsField::DownloadArgs => reset_preset(&mut site.download_args, p.map(|p| p.download_args), "download_args", when),
        PostsField::UrlTemplate => reset_preset(&mut site.url_template, p.map(|p| p.url_template), "url_template", when),
        PostsField::XHelper => reset_preset(&mut site.x_helper, p.map(|p| p.x_helper), "x_helper", when),
        PostsField::UseTiktokPhotos => {
            reset_preset(&mut site.use_tiktok_photos, p.map(|p| p.use_tiktok_photos), "use_tiktok_photos", when)
        }
        PostsField::AutoDownload => reset_preset(&mut site.auto_download, p.map(|p| p.auto_download), "auto_download", when),
        PostsField::AutoDownloadLimitMb => reset_preset(
            &mut site.auto_download_limit_mb,
            p.map(|p| p.auto_download_limit_mb),
            "auto_download_limit_mb",
            when,
        ),
        PostsField::AutoDownloadSecs => {
            reset_preset(&mut site.auto_download_secs, p.map(|p| p.auto_download_secs), "auto_download_secs", when)
        }
        PostsField::Backfill => reset_preset(&mut site.backfill, p.map(|p| p.backfill), "backfill", when),
        PostsField::OpenM3u8 => reset_preset(&mut site.open_m3u8, p.map(|p| p.open_m3u8), "open_m3u8", when),
        PostsField::AutoDeleteDays => {
            reset_preset(&mut site.auto_delete_days, p.map(|p| p.auto_delete_days), "auto_delete_days", when)
        }
        PostsField::IntervalSecs => {
            let to = p.map_or(d.interval_secs, |p| p.interval_secs);
            describe("interval_secs", reset_field(&mut site.interval_secs, to, when))
        }
        PostsField::ExePath => describe("exe_path", reset_field(&mut site.exe_path, d.exe_path, when)),
        PostsField::OutputDir => describe("output_dir", reset_field(&mut site.output_dir, d.output_dir, when)),
        PostsField::MarkOnOpen => describe("mark_on_open", reset_field(&mut site.mark_on_open, d.mark_on_open, when)),
        PostsField::Notify => describe("notify", reset_field(&mut site.notify, d.notify, when)),
        PostsField::NotifyDesktop => describe("notify_desktop", reset_field(&mut site.notify_desktop, d.notify_desktop, when)),
        PostsField::NotifyNtfy => describe("notify_ntfy", reset_field(&mut site.notify_ntfy, d.notify_ntfy, when)),
        PostsField::NtfyTopic => describe("ntfy_topic", reset_field(&mut site.ntfy_topic, d.ntfy_topic, when)),
        PostsField::M3u8Args => describe("m3u8_args", reset_field(&mut site.m3u8_args, d.m3u8_args, when)),
        PostsField::PlayerPath => describe("player_path", reset_field(&mut site.player_path, d.player_path, when)),
        PostsField::LogDownloadStdout => {
            describe("log_download_stdout", reset_field(&mut site.log_download_stdout, d.log_download_stdout, when))
        }
        PostsField::LogDownloadStderr => {
            describe("log_download_stderr", reset_field(&mut site.log_download_stderr, d.log_download_stderr, when))
        }
    }
}

/// Applies the posts.json resets in memory; returns log lines.
fn reset_posts(cfg: &mut PostsConfig, resets: &[&Reset]) -> Vec<String> {
    let mut log = Vec::new();
    for r in resets {
        let Key::Posts(field) = r.key else { continue };
        for site in cfg.sites.iter_mut().filter(|s| r.targets(&s.site_key)) {
            if let Some(m) = reset_post_site(site, field, &r.when) {
                log.push(format!("posts site '{}' {m}", site.name));
            }
        }
    }
    log
}

/// Applies the settings, site and streamer resets in memory; returns log lines.
fn reset_settings(s: &mut Settings, resets: &[&Reset]) -> Vec<String> {
    let mut log = Vec::new();
    for r in resets {
        match r.key {
            Key::Settings(SettingsField::DefaultOutputDir) => {
                let to = settings::default_settings().default_output_dir;
                if let Some(m) = describe("default_output_dir", reset_field(&mut s.default_output_dir, to, &r.when)) {
                    log.push(format!("settings {m}"));
                }
            }
            Key::Live(field) => {
                for site in s.sites.iter_mut().filter(|x| r.targets(&x.site_key)) {
                    if let Some(m) = reset_site(site, field, &r.when) {
                        log.push(format!("site '{}' {m}", site.name));
                    }
                }
            }
            Key::Streamer(field) => {
                for streamer in &mut s.streamers {
                    if let Some(m) = reset_streamer(streamer, field, &r.when) {
                        log.push(format!("streamer '{}' {m}", streamer.name));
                    }
                }
            }
            _ => {}
        }
    }
    log
}

/// Applies the update.json resets in memory; returns log lines.
fn reset_update(cfg: &mut UpdateConfig, resets: &[&Reset]) -> Vec<String> {
    let d = UpdateConfig::default();
    let mut log = Vec::new();
    for r in resets {
        let Key::Update(field) = r.key else { continue };
        let msg = match field {
            UpdateField::Enabled => describe("enabled", reset_field(&mut cfg.enabled, d.enabled, &r.when)),
            UpdateField::IncludeRc => describe("include_rc", reset_field(&mut cfg.include_rc, d.include_rc, &r.when)),
        };
        if let Some(m) = msg {
            log.push(format!("update.json {m}"));
        }
    }
    log
}

/// Sets `site_key` on Livestreams sites that have none and are named like a built-in site; returns log lines.
fn backfill_live(sites: &mut [Site]) -> Vec<String> {
    let mut log = Vec::new();
    for site in sites.iter_mut().filter(|s| s.site_key.is_empty()) {
        let key = key_for_name(&site.name);
        if !key.is_empty() {
            log.push(format!("site '{}' site_key set: {key}", site.name));
            site.site_key = key;
        }
    }
    log
}

/// Sets `site_key` on Posts sites that have none and are named like a built-in site; returns log lines.
fn backfill_posts(sites: &mut [PostSite]) -> Vec<String> {
    let mut log = Vec::new();
    for site in sites.iter_mut().filter(|s| s.site_key.is_empty()) {
        let key = posts::key_for_name(&site.name);
        if !key.is_empty() {
            log.push(format!("posts site '{}' site_key set: {key}", site.name));
            site.site_key = key;
        }
    }
    log
}

/// Keys every unkeyed site named like a built-in site and saves only if something changed; returns log lines.
fn backfill_site_keys() -> Vec<String> {
    let (mut log, mut ok) = (Vec::new(), true);
    if let Ok(mut s) = settings::try_load() {
        let lines = backfill_live(&mut s.sites);
        if !lines.is_empty() {
            match settings::save(&s) {
                Ok(()) => log.extend(lines),
                Err(e) => failed(&mut log, &mut ok, "sites.json", e),
            }
        }
    }
    if let Some(mut cfg) = posts::read_config_quiet() {
        let lines = backfill_posts(&mut cfg.sites);
        if !lines.is_empty() {
            match posts::write_config_now(&cfg) {
                Ok(()) => log.extend(lines),
                Err(e) => failed(&mut log, &mut ok, "posts.json", e),
            }
        }
    }
    log
}

#[derive(Serialize, Deserialize)]
struct RevFile {
    config_rev: u32,
}

fn rev_path() -> PathBuf {
    settings::config_dir().join("config_rev.json")
}

fn read_rev() -> Option<u32> {
    let text = std::fs::read_to_string(rev_path()).ok()?;
    serde_json::from_str::<RevFile>(&text).ok().map(|f| f.config_rev)
}

fn write_rev(rev: u32) -> std::io::Result<()> {
    std::fs::create_dir_all(settings::config_dir())?;
    let json = serde_json::to_string_pretty(&RevFile { config_rev: rev }).map_err(std::io::Error::other)?;
    std::fs::write(rev_path(), json)
}

static LOG: Mutex<Vec<String>> = Mutex::new(Vec::new());

/// Returns and clears the log lines left by `run`.
pub fn take_log() -> Vec<String> {
    std::mem::take(&mut *LOG.lock().unwrap())
}

fn failed(log: &mut Vec<String>, ok: &mut bool, what: &str, e: std::io::Error) {
    log.push(format!("config reset: cannot save {what} ({e}); will retry next launch"));
    *ok = false;
}

/// Applies the pending resets and returns their log lines plus whether every write succeeded.
fn apply(pending: &[&Reset]) -> (Vec<String>, bool) {
    let (mut log, mut ok) = (Vec::new(), true);
    if let Ok(mut s) = settings::try_load() {
        let before = s.clone();
        let lines = reset_settings(&mut s, pending);
        let saved = if s != before { settings::save(&s) } else { Ok(()) };
        match saved {
            Ok(()) => log.extend(lines),
            Err(e) => failed(&mut log, &mut ok, "settings", e),
        }
    }
    if let Some(mut cfg) = posts::read_config_quiet() {
        let before = cfg.clone();
        let lines = reset_posts(&mut cfg, pending);
        let saved = if cfg != before { posts::write_config_now(&cfg) } else { Ok(()) };
        match saved {
            Ok(()) => log.extend(lines),
            Err(e) => failed(&mut log, &mut ok, "posts.json", e),
        }
    }
    let mut cfg = update::load_config();
    let before = cfg;
    let lines = reset_update(&mut cfg, pending);
    let saved = if cfg != before { update::save_config(&cfg) } else { Ok(()) };
    match saved {
        Ok(()) => log.extend(lines),
        Err(e) => failed(&mut log, &mut ok, "update.json", e),
    }
    match prefs::apply_resets(pending) {
        Ok(lines) => log.extend(lines),
        Err(e) => failed(&mut log, &mut ok, "gui.json", e),
    }
    (log, ok)
}

/// Applies resets added since the stored config rev, then stamps the current rev; returns log lines.
fn run_pending() -> Vec<String> {
    let from = match read_rev() {
        Some(rev) => rev,
        None if settings::any_config_exists() => 0,
        None => {
            let _ = write_rev(CONFIG_REV);
            return Vec::new();
        }
    };
    if from >= CONFIG_REV {
        return Vec::new();
    }
    let pending: Vec<&Reset> = RESETS.iter().filter(|r| r.rev > from && r.rev <= CONFIG_REV).collect();
    let (mut log, mut ok) = apply(&pending);
    if ok {
        if let Err(e) = write_rev(CONFIG_REV) {
            failed(&mut log, &mut ok, "config_rev.json", e);
        }
    }
    log
}

/// Keys built-in sites at every launch, then applies the pending resets; call once at startup.
pub fn run() {
    let mut log = backfill_site_keys();
    log.extend(run_pending());
    LOG.lock().unwrap().extend(log);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn site(name: &str) -> Site {
        let mut s = new_site(Preset::Blank).into_site(1);
        s.name = name.into();
        s.site_key = key_for_name(name);
        s
    }

    fn settings_with(sites: Vec<Site>) -> Settings {
        Settings { next_id: 10, default_output_dir: "x".into(), max_recordings: None, notifications: Default::default(), sites, streamers: Vec::new() }
    }

    #[test]
    fn table_revs_are_within_current_rev() {
        assert!(RESETS.iter().all(|r| r.rev > 1 && r.rev <= CONFIG_REV));
    }

    #[test]
    fn site_filter_is_only_used_on_site_fields() {
        assert!(RESETS.iter().all(|r| r.site.is_none() || matches!(r.key, Key::Live(_) | Key::Posts(_))));
    }

    #[test]
    fn always_changes_value_and_skips_when_equal() {
        let mut v = Some(30u64);
        assert!(reset_field(&mut v, None, &When::Always).is_some());
        assert_eq!(v, None);
        assert!(reset_field(&mut v, None, &When::Always).is_none());
    }

    #[test]
    fn if_equals_only_matches_old_value() {
        let mut v = Some(30u64);
        assert!(reset_field(&mut v, None, &When::IfEquals(Val::Num(45))).is_none());
        assert_eq!(v, Some(30));
        assert!(reset_field(&mut v, None, &When::IfEquals(Val::Num(30))).is_some());
        assert_eq!(v, None);
    }

    #[test]
    fn unset_matches_blank_fields() {
        let mut v: Option<u64> = None;
        assert!(reset_field(&mut v, Some(5), &When::IfEquals(Val::Unset)).is_some());
        assert_eq!(v, Some(5));
    }

    #[test]
    fn preset_reset_reapplies_preset_and_skips_custom_sites() {
        let mut twitch = site("Twitch");
        twitch.checker_args = "custom".into();
        let mut mine = site("Mine");
        mine.checker_args = "custom".into();
        let mut s = settings_with(vec![twitch, mine]);
        let r = Reset::always(2, Key::Live(LiveField::CheckerArgs));
        let log = reset_settings(&mut s, &[&r]);
        assert_eq!(s.sites[0].checker_args, new_site(Preset::Twitch).checker_args);
        assert_eq!(s.sites[1].checker_args, "custom");
        assert_eq!(log.len(), 1);
    }

    #[test]
    fn site_fields_reset_to_fallback_for_every_site() {
        let mut a = site("A");
        a.interval_secs = Some(10);
        let mut b = site("B");
        b.interval_secs = Some(20);
        let mut s = settings_with(vec![a, b]);
        let r = Reset::if_equals(2, Key::Live(LiveField::IntervalSecs), Val::Num(10));
        reset_settings(&mut s, &[&r]);
        assert_eq!(s.sites[0].interval_secs, None);
        assert_eq!(s.sites[1].interval_secs, Some(20));
    }

    #[test]
    fn live_site_filter_only_touches_the_keyed_site_even_when_renamed() {
        let mut twitch = site("Twitch");
        twitch.interval_secs = Some(10);
        let mut renamed = site("My Twitch");
        renamed.site_key = SiteKey::Twitch.slug().into();
        renamed.interval_secs = Some(10);
        let mut kick = site("Kick");
        kick.interval_secs = Some(10);
        let mut custom = site("Twitch clone");
        custom.interval_secs = Some(10);
        let mut s = settings_with(vec![twitch, renamed, kick, custom]);
        let r = Reset::always(2, Key::Live(LiveField::IntervalSecs)).site(SiteKey::Twitch);
        let log = reset_settings(&mut s, &[&r]);
        let intervals: Vec<_> = s.sites.iter().map(|x| x.interval_secs).collect();
        assert_eq!(intervals, vec![None, None, Some(10), Some(10)]);
        assert_eq!(log.len(), 2);
    }

    fn post_site(name: &str) -> PostSite {
        PostSite { name: name.into(), site_key: posts::key_for_name(name), ..Default::default() }
    }

    #[test]
    fn tiktok_photos_reset_enables_only_the_tiktok_site() {
        let mut cfg = PostsConfig { sites: vec![post_site("TikTok"), post_site("Twitter/X"), post_site("Mine")] };
        let r = Reset::if_equals(2, Key::Posts(PostsField::UseTiktokPhotos), Val::Flag(false)).site(SiteKey::Tiktok);
        let log = reset_posts(&mut cfg, &[&r]);
        assert!(cfg.sites[0].use_tiktok_photos);
        assert!(!cfg.sites[1].use_tiktok_photos && !cfg.sites[2].use_tiktok_photos);
        assert_eq!(log.len(), 1);
        assert!(log[0].contains("posts site 'TikTok' use_tiktok_photos reset"));
    }

    #[test]
    fn tiktok_photos_reset_finds_a_renamed_tiktok_site() {
        let mut renamed = post_site("My TikTok");
        renamed.site_key = SiteKey::Tiktok.slug().into();
        let mut cfg = PostsConfig { sites: vec![renamed] };
        let r = Reset::if_equals(2, Key::Posts(PostsField::UseTiktokPhotos), Val::Flag(false)).site(SiteKey::Tiktok);
        reset_posts(&mut cfg, &[&r]);
        assert!(cfg.sites[0].use_tiktok_photos);
    }

    #[test]
    fn tiktok_photos_reset_is_skipped_when_already_on() {
        let mut site = post_site("TikTok");
        site.use_tiktok_photos = true;
        let mut cfg = PostsConfig { sites: vec![site] };
        let r = Reset::always(2, Key::Posts(PostsField::UseTiktokPhotos)).site(SiteKey::Tiktok);
        assert!(reset_posts(&mut cfg, &[&r]).is_empty());
    }

    #[test]
    fn x_helper_and_interval_resets_only_touch_the_twitter_x_site() {
        let mut x = post_site("Twitter/X");
        x.interval_secs = Some(60);
        let mut tiktok = post_site("TikTok");
        tiktok.interval_secs = Some(60);
        let mut mine = post_site("Mine");
        mine.interval_secs = Some(60);
        let mut cfg = PostsConfig { sites: vec![x, tiktok, mine] };
        let r1 = Reset::always(3, Key::Posts(PostsField::XHelper)).site(SiteKey::TwitterX);
        let r2 = Reset::always(3, Key::Posts(PostsField::IntervalSecs)).site(SiteKey::TwitterX);
        let log = reset_posts(&mut cfg, &[&r1, &r2]);
        assert_eq!((cfg.sites[0].x_helper, cfg.sites[0].interval_secs), (posts::XHelper::GalleryDl, Some(900)));
        assert_eq!((cfg.sites[1].x_helper, cfg.sites[1].interval_secs), (posts::XHelper::Off, Some(60)));
        assert_eq!((cfg.sites[2].x_helper, cfg.sites[2].interval_secs), (posts::XHelper::Off, Some(60)));
        assert_eq!(log.len(), 2);
    }

    #[test]
    fn x_resets_are_skipped_when_already_applied() {
        let mut x = PostSite::from_preset(PostsPreset::TwitterX);
        x.interval_secs = Some(900);
        let mut cfg = PostsConfig { sites: vec![x] };
        let r1 = Reset::always(3, Key::Posts(PostsField::XHelper)).site(SiteKey::TwitterX);
        let r2 = Reset::always(3, Key::Posts(PostsField::IntervalSecs)).site(SiteKey::TwitterX);
        assert!(reset_posts(&mut cfg, &[&r1, &r2]).is_empty());
    }

    #[test]
    fn posts_default_fields_reset_for_every_site() {
        let mut a = post_site("A");
        a.notify = false;
        a.interval_secs = Some(60);
        let mut cfg = PostsConfig { sites: vec![a] };
        let r1 = Reset::always(2, Key::Posts(PostsField::Notify));
        let r2 = Reset::if_equals(2, Key::Posts(PostsField::IntervalSecs), Val::Num(60));
        reset_posts(&mut cfg, &[&r1, &r2]);
        assert!(cfg.sites[0].notify);
        assert_eq!(cfg.sites[0].interval_secs, None);
    }

    #[test]
    fn posts_interval_reset_uses_the_template_value_for_built_in_sites() {
        let mut x = post_site("Twitter/X");
        x.interval_secs = Some(60);
        let mut tiktok = post_site("TikTok");
        tiktok.interval_secs = Some(60);
        let mut mine = post_site("Mine");
        mine.interval_secs = Some(60);
        let mut cfg = PostsConfig { sites: vec![x, tiktok, mine] };
        let r = Reset::if_equals(3, Key::Posts(PostsField::IntervalSecs), Val::Num(60));
        reset_posts(&mut cfg, &[&r]);
        let intervals: Vec<_> = cfg.sites.iter().map(|s| s.interval_secs).collect();
        assert_eq!(intervals, vec![Some(900), None, None]);
    }

    #[test]
    fn update_config_resets_to_default() {
        let mut cfg = UpdateConfig { enabled: false, include_rc: true };
        let r = Reset::always(2, Key::Update(UpdateField::Enabled));
        reset_update(&mut cfg, &[&r]);
        assert!(cfg.enabled && cfg.include_rc);
    }

    #[test]
    fn backfill_keys_live_sites_by_name_and_leaves_the_rest() {
        let mut named = site("x");
        named.name = " twitch ".into();
        named.site_key = String::new();
        let mut custom = site("x");
        custom.name = "Mine".into();
        custom.site_key = String::new();
        let mut keyed = site("x");
        keyed.name = "Renamed".into();
        keyed.site_key = "kick".into();
        let mut sites = vec![named, custom, keyed];
        let log = backfill_live(&mut sites);
        let keys: Vec<_> = sites.iter().map(|s| s.site_key.as_str()).collect();
        assert_eq!(keys, vec!["twitch", "", "kick"]);
        assert_eq!(log.len(), 1);
        assert!(backfill_live(&mut sites).is_empty());
    }

    #[test]
    fn backfill_keys_posts_sites_by_name_and_leaves_the_rest() {
        let mut sites = vec![
            PostSite { name: "twitter/x".into(), ..Default::default() },
            PostSite { name: "Mine".into(), ..Default::default() },
            PostSite { name: "Renamed".into(), site_key: "tiktok".into(), ..Default::default() },
        ];
        let log = backfill_posts(&mut sites);
        let keys: Vec<_> = sites.iter().map(|s| s.site_key.as_str()).collect();
        assert_eq!(keys, vec!["twitter_x", "", "tiktok"]);
        assert_eq!(log.len(), 1);
        assert!(backfill_posts(&mut sites).is_empty());
    }

    #[test]
    fn live_and_posts_templates_map_to_their_keys_and_back() {
        for p in Preset::NAMED {
            assert_eq!(Preset::from_key(p.site_key().unwrap()), p);
        }
        for p in PostsPreset::NAMED {
            assert_eq!(PostsPreset::from_key(p.site_key().unwrap()), p);
            assert_eq!(PostSite::from_preset(p).key(), p.site_key());
        }
    }
}
