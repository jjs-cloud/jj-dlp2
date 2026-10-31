//! One-time resets of stored settings, applied on the first launch after a release that adds them.

use crate::api::{FolderLayout, Settings, Site, Streamer, StreamerOptions};
use crate::gui::prefs;
use crate::policy::update::{self, UpdateConfig};
use crate::policy::{new_site, preset_for_name, Preset};
use crate::settings;
use serde::{Deserialize, Serialize};
use std::fmt::Debug;
use std::path::PathBuf;
use std::sync::Mutex;

/// Config revision of this build; bump it whenever entries are added to `RESETS`.
pub const CONFIG_REV: u32 = 1;

/// Resets to apply; each entry's `rev` is the `CONFIG_REV` it was added at (always above 1).
pub const RESETS: &[Reset] = &[
    // Reset::always(2, Key::SiteIntervalSecs),
    // Reset::if_equals(2, Key::SiteCheckerArgs, Val::Text("--old-args {urls}")),
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

/// A resettable setting; site and streamer keys apply to every site or streamer.
#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    DefaultOutputDir,
    // Site fields; checker args, recorder args, URL template and colour re-apply the name-matched preset.
    SiteExePath,
    SiteIntervalSecs,
    SiteCheckerArgs,
    SiteRecorderArgs,
    SiteUrlTemplate,
    SiteOutputDir,
    SiteFolderLayout,
    SiteLogCheckerStdout,
    SiteLogCheckerStderr,
    SiteEndAfterMinutes,
    SiteFileExistsGraceSecs,
    SiteGrowthGraceSecs,
    SiteStallWindowSecs,
    SiteFileCheckIntervalSecs,
    SiteRestartClearMinutes,
    SiteRestartThreshold,
    SiteColor,
    // Streamer options.
    StreamerLogStdout,
    StreamerLogStderr,
    StreamerFolderLayout,
    // update.json.
    UpdateEnabled,
    UpdateIncludeRc,
    // gui.json.
    GuiTheme,
    GuiCompact,
    GuiRowMin,
    GuiRowMax,
    GuiTwoColWidth,
    GuiNameClip,
    GuiLogLines,
    GuiRememberSize,
    GuiWindowWidth,
    GuiWindowHeight,
}

/// One entry of `RESETS`.
#[derive(Debug, Clone, Copy)]
pub struct Reset {
    pub rev: u32,
    pub key: Key,
    pub when: When,
}

#[allow(dead_code)]
impl Reset {
    /// Resets the setting whatever its current value.
    pub const fn always(rev: u32, key: Key) -> Self {
        Self { rev, key, when: When::Always }
    }

    /// Resets the setting only if it still holds `old`.
    pub const fn if_equals(rev: u32, key: Key, old: Val) -> Self {
        Self { rev, key, when: When::IfEquals(old) }
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

/// Like `reset_field` with the preset value as target; reports sites that have no preset.
fn reset_preset<T: Resettable + PartialEq + Debug>(cur: &mut T, to: Option<T>, field: &str, when: &When) -> Option<String> {
    match to {
        Some(v) => describe(field, reset_field(cur, v, when)),
        None => cur.matches(when).then(|| format!("{field} not reset: no preset")),
    }
}

fn preset_for(site_name: &str) -> Option<crate::policy::NewSite> {
    let preset = preset_for_name(site_name);
    (!matches!(preset, Preset::Blank)).then(|| new_site(preset))
}

fn reset_site(site: &mut Site, key: Key, when: &When) -> Option<String> {
    match key {
        Key::SiteExePath => describe("exe_path", reset_field(&mut site.exe_path, String::new(), when)),
        Key::SiteIntervalSecs => describe("interval_secs", reset_field(&mut site.interval_secs, None, when)),
        Key::SiteCheckerArgs => {
            reset_preset(&mut site.checker_args, preset_for(&site.name).map(|p| p.checker_args), "checker_args", when)
        }
        Key::SiteRecorderArgs => {
            reset_preset(&mut site.recorder_args, preset_for(&site.name).map(|p| p.recorder_args), "recorder_args", when)
        }
        Key::SiteUrlTemplate => {
            reset_preset(&mut site.url_template, preset_for(&site.name).map(|p| p.url_template), "url_template", when)
        }
        Key::SiteColor => reset_preset(&mut site.color, preset_for(&site.name).map(|p| p.color), "color", when),
        Key::SiteOutputDir => describe("output_dir", reset_field(&mut site.output_dir, String::new(), when)),
        Key::SiteFolderLayout => {
            describe("folder_layout", reset_field(&mut site.folder_layout, FolderLayout::default(), when))
        }
        Key::SiteLogCheckerStdout => describe("log_checker_stdout", reset_field(&mut site.log_checker_stdout, false, when)),
        Key::SiteLogCheckerStderr => describe("log_checker_stderr", reset_field(&mut site.log_checker_stderr, false, when)),
        Key::SiteEndAfterMinutes => describe("end_after_minutes", reset_field(&mut site.end_after_minutes, None, when)),
        Key::SiteFileExistsGraceSecs => {
            describe("file_exists_grace_secs", reset_field(&mut site.file_exists_grace_secs, None, when))
        }
        Key::SiteGrowthGraceSecs => describe("growth_grace_secs", reset_field(&mut site.growth_grace_secs, None, when)),
        Key::SiteStallWindowSecs => describe("stall_window_secs", reset_field(&mut site.stall_window_secs, None, when)),
        Key::SiteFileCheckIntervalSecs => {
            describe("file_check_interval_secs", reset_field(&mut site.file_check_interval_secs, None, when))
        }
        Key::SiteRestartClearMinutes => {
            describe("restart_clear_minutes", reset_field(&mut site.restart_clear_minutes, None, when))
        }
        Key::SiteRestartThreshold => describe("restart_threshold", reset_field(&mut site.restart_threshold, None, when)),
        _ => None,
    }
}

fn reset_streamer(streamer: &mut Streamer, key: Key, when: &When) -> Option<String> {
    let d = StreamerOptions::default();
    let o = &mut streamer.options;
    match key {
        Key::StreamerLogStdout => describe("log_stdout", reset_field(&mut o.log_stdout, d.log_stdout, when)),
        Key::StreamerLogStderr => describe("log_stderr", reset_field(&mut o.log_stderr, d.log_stderr, when)),
        Key::StreamerFolderLayout => describe("folder_layout", reset_field(&mut o.folder_layout, d.folder_layout, when)),
        _ => None,
    }
}

/// Applies the settings, site and streamer resets in memory; returns log lines.
fn reset_settings(s: &mut Settings, resets: &[&Reset]) -> Vec<String> {
    let mut log = Vec::new();
    for r in resets {
        if r.key == Key::DefaultOutputDir {
            let to = settings::default_settings().default_output_dir;
            if let Some(m) = describe("default_output_dir", reset_field(&mut s.default_output_dir, to, &r.when)) {
                log.push(format!("settings {m}"));
            }
        }
        for site in &mut s.sites {
            if let Some(m) = reset_site(site, r.key, &r.when) {
                log.push(format!("site '{}' {m}", site.name));
            }
        }
        for streamer in &mut s.streamers {
            if let Some(m) = reset_streamer(streamer, r.key, &r.when) {
                log.push(format!("streamer '{}' {m}", streamer.name));
            }
        }
    }
    log
}

/// Applies the update.json resets in memory; returns log lines.
fn reset_update(cfg: &mut UpdateConfig, resets: &[&Reset]) -> Vec<String> {
    let d = UpdateConfig::default();
    let mut log = Vec::new();
    for r in resets {
        let msg = match r.key {
            Key::UpdateEnabled => describe("enabled", reset_field(&mut cfg.enabled, d.enabled, &r.when)),
            Key::UpdateIncludeRc => describe("include_rc", reset_field(&mut cfg.include_rc, d.include_rc, &r.when)),
            _ => None,
        };
        if let Some(m) = msg {
            log.push(format!("update.json {m}"));
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

/// Applies resets added since the stored config rev, then stamps the current rev; call once at startup.
pub fn run() {
    let from = match read_rev() {
        Some(rev) => rev,
        None if settings::any_config_exists() => 0,
        None => {
            let _ = write_rev(CONFIG_REV);
            return;
        }
    };
    if from >= CONFIG_REV {
        return;
    }
    let pending: Vec<&Reset> = RESETS.iter().filter(|r| r.rev > from && r.rev <= CONFIG_REV).collect();
    let (mut log, mut ok) = apply(&pending);
    if ok {
        if let Err(e) = write_rev(CONFIG_REV) {
            failed(&mut log, &mut ok, "config_rev.json", e);
        }
    }
    LOG.lock().unwrap().extend(log);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn site(name: &str) -> Site {
        let mut s = new_site(Preset::Blank).into_site(1);
        s.name = name.into();
        s
    }

    fn settings_with(sites: Vec<Site>) -> Settings {
        Settings { next_id: 10, default_output_dir: "x".into(), sites, streamers: Vec::new() }
    }

    #[test]
    fn table_revs_are_within_current_rev() {
        assert!(RESETS.iter().all(|r| r.rev > 1 && r.rev <= CONFIG_REV));
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
    fn preset_reset_reapplies_preset_and_skips_unknown_sites() {
        let mut twitch = site("Twitch");
        twitch.checker_args = "custom".into();
        let mut mine = site("Mine");
        mine.checker_args = "custom".into();
        let mut s = settings_with(vec![twitch, mine]);
        let r = Reset::always(2, Key::SiteCheckerArgs);
        let log = reset_settings(&mut s, &[&r]);
        assert_eq!(s.sites[0].checker_args, new_site(Preset::Twitch).checker_args);
        assert_eq!(s.sites[1].checker_args, "custom");
        assert_eq!(log.len(), 2);
        assert!(log[1].contains("no preset"));
    }

    #[test]
    fn site_fields_reset_to_fallback_for_every_site() {
        let mut a = site("A");
        a.interval_secs = Some(10);
        let mut b = site("B");
        b.interval_secs = Some(20);
        let mut s = settings_with(vec![a, b]);
        let r = Reset::if_equals(2, Key::SiteIntervalSecs, Val::Num(10));
        reset_settings(&mut s, &[&r]);
        assert_eq!(s.sites[0].interval_secs, None);
        assert_eq!(s.sites[1].interval_secs, Some(20));
    }

    #[test]
    fn update_config_resets_to_default() {
        let mut cfg = UpdateConfig { enabled: false, include_rc: true };
        let r = Reset::always(2, Key::UpdateEnabled);
        reset_update(&mut cfg, &[&r]);
        assert!(cfg.enabled && cfg.include_rc);
    }
}
