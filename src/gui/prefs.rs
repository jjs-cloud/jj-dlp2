//! GUI-only preferences stored in gui.json.

use super::skin::ThemeKind;
use crate::resets::{reset_field, Key, Reset};
use serde::{Deserialize, Serialize};

pub const ROW_MIN_LIMIT: f32 = 15.0;
pub const ROW_MAX_LIMIT: f32 = 48.0;
pub const ROW_MIN_DEFAULT: f32 = 15.0;
pub const ROW_MAX_DEFAULT: f32 = 31.0;
pub const TWO_COL_MIN_LIMIT: f32 = 700.0;
pub const TWO_COL_MAX_LIMIT: f32 = 2600.0;
pub const TWO_COL_DEFAULT: f32 = 1100.0;
pub const THREE_COL_DEFAULT: f32 = 1700.0;
pub const NAME_CLIP_MIN_LIMIT: f32 = 6.0;
pub const NAME_CLIP_MAX_LIMIT: f32 = 30.0;
pub const NAME_CLIP_DEFAULT: f32 = 15.0;
pub const LOG_LINES_MIN_LIMIT: f32 = 100.0;
pub const LOG_LINES_MAX_LIMIT: f32 = crate::api::LOG_HISTORY_MAX as f32;
pub const LOG_LINES_DEFAULT: f32 = 500.0;
pub const CARD_W_MIN: f32 = 120.0;
pub const CARD_W_MAX: f32 = 260.0;
const CARD_W_DEFAULT: f32 = 170.0;
pub const WIN_DEFAULT_W: f32 = 1180.0;
pub const WIN_DEFAULT_H: f32 = 845.0;
const WIN_MIN_W: f32 = 640.0;
const WIN_MIN_H: f32 = 420.0;

#[derive(Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
struct Prefs {
    theme: String,
    compact: bool,
    row_min: f32,
    row_max: f32,
    two_col_w: f32,
    three_col_w: f32,
    name_clip: f32,
    log_lines: f32,
    remember_size: bool,
    win_w: f32,
    win_h: f32,
    wrap_output: bool,
    wrap_logs: bool,
    files: FilesPrefs,
    posts: PostsPrefs,
    extra_drives: Vec<String>,
    drive_order: Vec<String>,
    disk_limits: crate::policy::disks::Limits,
}

/// Preferences of the Files tab.
#[derive(Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct FilesPrefs {
    pub sort: String,
    pub asc: bool,
    pub group: bool,
    pub delete_empty: bool,
    pub hide_posts: bool,
    pub move_dir: String,
}

/// Preferences of the Posts tab.
#[derive(Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PostsPrefs {
    pub newest_first: bool,
    pub card_w: f32,
}

impl Default for PostsPrefs {
    fn default() -> Self {
        PostsPrefs { newest_first: true, card_w: CARD_W_DEFAULT }
    }
}

impl Default for FilesPrefs {
    fn default() -> Self {
        FilesPrefs { sort: "name".to_string(), asc: true, group: false, delete_empty: true, hide_posts: false, move_dir: String::new() }
    }
}

impl Default for Prefs {
    fn default() -> Self {
        Prefs { theme: String::new(), compact: false, row_min: ROW_MIN_DEFAULT, row_max: ROW_MAX_DEFAULT, two_col_w: TWO_COL_DEFAULT,
            three_col_w: THREE_COL_DEFAULT,
            name_clip: NAME_CLIP_DEFAULT,
            log_lines: LOG_LINES_DEFAULT,
            remember_size: false,
            win_w: WIN_DEFAULT_W,
            win_h: WIN_DEFAULT_H,
            wrap_output: true,
            wrap_logs: true,
            files: FilesPrefs::default(),
            posts: PostsPrefs::default(),
            extra_drives: Vec::new(),
            drive_order: Vec::new(),
            disk_limits: Default::default(),
        }
    }
}

fn path() -> std::path::PathBuf {
    crate::settings::config_dir().join("gui.json")
}

/// Reads gui.json, defaulting when it is missing; an invalid file is kept as gui.json.bad.
fn read() -> Prefs {
    let Ok(text) = std::fs::read_to_string(path()) else { return Prefs::default() };
    serde_json::from_str(&text).unwrap_or_else(|_| {
        crate::settings::backup_bad(&path());
        Prefs::default()
    })
}

/// Writes gui.json through a temp file.
fn try_write(prefs: &Prefs) -> std::io::Result<()> {
    let json = serde_json::to_string_pretty(prefs).map_err(std::io::Error::other)?;
    std::fs::create_dir_all(crate::settings::config_dir())?;
    let tmp = path().with_extension("json.tmp");
    std::fs::write(&tmp, json)?;
    std::fs::rename(&tmp, path())
}

/// Writes gui.json, ignoring failures.
fn write(prefs: &Prefs) {
    let _ = try_write(prefs);
}

/// Applies the gui.json resets in memory; returns log lines.
fn reset_prefs(prefs: &mut Prefs, resets: &[&Reset]) -> Vec<String> {
    let d = Prefs::default();
    let mut log = Vec::new();
    for r in resets {
        let w = &r.when;
        let (field, change) = match r.key {
            Key::GuiTheme => ("theme", reset_field(&mut prefs.theme, d.theme.clone(), w)),
            Key::GuiCompact => ("compact", reset_field(&mut prefs.compact, d.compact, w)),
            Key::GuiRowMin => ("row_min", reset_field(&mut prefs.row_min, d.row_min, w)),
            Key::GuiRowMax => ("row_max", reset_field(&mut prefs.row_max, d.row_max, w)),
            Key::GuiTwoColWidth => ("two_col_w", reset_field(&mut prefs.two_col_w, d.two_col_w, w)),
            Key::GuiNameClip => ("name_clip", reset_field(&mut prefs.name_clip, d.name_clip, w)),
            Key::GuiLogLines => ("log_lines", reset_field(&mut prefs.log_lines, d.log_lines, w)),
            Key::GuiRememberSize => ("remember_size", reset_field(&mut prefs.remember_size, d.remember_size, w)),
            Key::GuiWindowWidth => ("win_w", reset_field(&mut prefs.win_w, d.win_w, w)),
            Key::GuiWindowHeight => ("win_h", reset_field(&mut prefs.win_h, d.win_h, w)),
            _ => continue,
        };
        if let Some(c) = change {
            log.push(format!("gui.json {field} reset: {c}"));
        }
    }
    log
}

/// Applies the gui.json resets and saves once if anything changed; returns log lines.
pub fn apply_resets(resets: &[&Reset]) -> std::io::Result<Vec<String>> {
    let mut prefs = read();
    let before = prefs.clone();
    let lines = reset_prefs(&mut prefs, resets);
    if prefs != before {
        try_write(&prefs)?;
    }
    Ok(lines)
}

/// Reads the saved theme.
pub fn load_theme() -> ThemeKind {
    ThemeKind::from_name(&read().theme)
}

/// Saves the theme.
pub fn save_theme(kind: ThemeKind) {
    let mut prefs = read();
    prefs.theme = kind.name().to_string();
    write(&prefs);
}

/// Reads the saved Files tab preferences.
pub fn load_files() -> FilesPrefs {
    read().files
}

/// Saves the Files tab preferences.
pub fn save_files(files: &FilesPrefs) {
    let mut prefs = read();
    prefs.files = files.clone();
    write(&prefs);
}

/// Reads the saved Posts tab preferences, with the card width clamped to the allowed limits.
pub fn load_posts() -> PostsPrefs {
    let p = read().posts;
    PostsPrefs { card_w: p.card_w.clamp(CARD_W_MIN, CARD_W_MAX), ..p }
}

/// Saves the Posts tab preferences.
pub fn save_posts(posts: &PostsPrefs) {
    let mut prefs = read();
    prefs.posts = posts.clone();
    write(&prefs);
}

/// Reads the drives the user added to the disk-space display.
pub fn load_extra_drives() -> Vec<String> {
    read().extra_drives
}

/// Saves the drives the user added to the disk-space display.
pub fn save_extra_drives(drives: &[String]) {
    let mut prefs = read();
    prefs.extra_drives = drives.to_vec();
    write(&prefs);
}

/// Reads the saved display order of the tracked drives.
pub fn load_drive_order() -> Vec<String> {
    read().drive_order
}

/// Saves the display order of the tracked drives.
pub fn save_drive_order(order: &[String]) {
    let mut prefs = read();
    prefs.drive_order = order.to_vec();
    write(&prefs);
}

/// Reads the saved disk-space display settings.
pub fn load_disk_limits() -> crate::policy::disks::Limits {
    read().disk_limits
}

/// Saves the disk-space display settings.
pub fn save_disk_limits(limits: crate::policy::disks::Limits) {
    let mut prefs = read();
    prefs.disk_limits = limits;
    write(&prefs);
}

/// Reads the saved compact-view setting.
pub fn load_compact() -> bool {
    read().compact
}

/// Saves the compact-view setting.
pub fn save_compact(on: bool) {
    let mut prefs = read();
    prefs.compact = on;
    write(&prefs);
}

/// Reads the saved compact row-height range, clamped to the allowed limits.
pub fn load_row_range() -> (f32, f32) {
    let prefs = read();
    let min = prefs.row_min.clamp(ROW_MIN_LIMIT, ROW_MAX_LIMIT);
    (min, prefs.row_max.clamp(min, ROW_MAX_LIMIT))
}

/// Saves the compact row-height range.
pub fn save_row_range(min: f32, max: f32) {
    let mut prefs = read();
    prefs.row_min = min;
    prefs.row_max = max;
    write(&prefs);
}

/// Reads the saved window width from which compact view may use two columns.
pub fn load_two_col_width() -> f32 {
    read().two_col_w.clamp(TWO_COL_MIN_LIMIT, TWO_COL_MAX_LIMIT)
}

/// Saves the window width from which compact view may use two columns.
pub fn save_two_col_width(width: f32) {
    let mut prefs = read();
    prefs.two_col_w = width;
    write(&prefs);
}

/// Reads the saved window width from which compact view may use three columns.
pub fn load_three_col_width() -> f32 {
    read().three_col_w.clamp(TWO_COL_MIN_LIMIT, TWO_COL_MAX_LIMIT)
}

/// Saves the window width from which compact view may use three columns.
pub fn save_three_col_width(width: f32) {
    let mut prefs = read();
    prefs.three_col_w = width;
    write(&prefs);
}

/// Reads the saved compact-view name length limit.
pub fn load_name_clip() -> f32 {
    read().name_clip.clamp(NAME_CLIP_MIN_LIMIT, NAME_CLIP_MAX_LIMIT)
}

/// Saves the compact-view name length limit.
pub fn save_name_clip(chars: f32) {
    let mut prefs = read();
    prefs.name_clip = chars;
    write(&prefs);
}

/// Reads the saved Log tab line limit, clamped to the allowed limits.
pub fn load_log_lines() -> f32 {
    read().log_lines.clamp(LOG_LINES_MIN_LIMIT, LOG_LINES_MAX_LIMIT)
}

/// Saves the Log tab line limit.
pub fn save_log_lines(lines: f32) {
    let mut prefs = read();
    prefs.log_lines = lines;
    write(&prefs);
}

/// Reads whether command output lines wrap in the Processes tab.
pub fn load_wrap_output() -> bool {
    read().wrap_output
}

/// Saves whether command output lines wrap in the Processes tab.
pub fn save_wrap_output(on: bool) {
    let mut prefs = read();
    prefs.wrap_output = on;
    write(&prefs);
}

/// Reads whether lines wrap in the Output log viewer.
pub fn load_wrap_logs() -> bool {
    read().wrap_logs
}

/// Saves whether lines wrap in the Output log viewer.
pub fn save_wrap_logs(on: bool) {
    let mut prefs = read();
    prefs.wrap_logs = on;
    write(&prefs);
}

/// Reads whether the window size is remembered between launches.
pub fn load_remember_size() -> bool {
    read().remember_size
}

/// Saves whether the window size is remembered between launches.
pub fn save_remember_size(on: bool) {
    let mut prefs = read();
    prefs.remember_size = on;
    write(&prefs);
}

/// Saves the window size for the next launch.
pub fn save_window_size(width: f32, height: f32) {
    let mut prefs = read();
    prefs.win_w = width;
    prefs.win_h = height;
    write(&prefs);
}

/// Returns the saved window size when remembering is on, else the default size.
pub fn startup_window_size() -> (f32, f32) {
    let prefs = read();
    if prefs.remember_size {
        (prefs.win_w.max(WIN_MIN_W), prefs.win_h.max(WIN_MIN_H))
    } else {
        (WIN_DEFAULT_W, WIN_DEFAULT_H)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::resets::Val;

    #[test]
    fn prefs_reset_to_defaults() {
        let mut p = Prefs { row_max: 40.0, compact: true, ..Prefs::default() };
        let a = Reset::always(2, Key::GuiRowMax);
        let b = Reset::if_equals(2, Key::GuiCompact, Val::Flag(false));
        let log = reset_prefs(&mut p, &[&a, &b]);
        assert_eq!(p.row_max, ROW_MAX_DEFAULT);
        assert!(p.compact);
        assert_eq!(log.len(), 1);
    }
}
