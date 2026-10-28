//! GUI-only preferences stored in gui.json.

use super::skin::ThemeKind;
use serde::{Deserialize, Serialize};

pub const ROW_MIN_LIMIT: f32 = 15.0;
pub const ROW_MAX_LIMIT: f32 = 48.0;
pub const ROW_MIN_DEFAULT: f32 = 15.0;
pub const ROW_MAX_DEFAULT: f32 = 34.0;
pub const TWO_COL_MIN_LIMIT: f32 = 700.0;
pub const TWO_COL_MAX_LIMIT: f32 = 2600.0;
pub const TWO_COL_DEFAULT: f32 = 1100.0;
pub const NAME_CLIP_MIN_LIMIT: f32 = 6.0;
pub const NAME_CLIP_MAX_LIMIT: f32 = 30.0;
pub const NAME_CLIP_DEFAULT: f32 = 12.0;
const WIN_DEFAULT_W: f32 = 1180.0;
const WIN_DEFAULT_H: f32 = 820.0;
const WIN_MIN_W: f32 = 640.0;
const WIN_MIN_H: f32 = 420.0;

#[derive(Serialize, Deserialize)]
#[serde(default)]
struct Prefs {
    theme: String,
    compact: bool,
    row_min: f32,
    row_max: f32,
    two_col_w: f32,
    name_clip: f32,
    remember_size: bool,
    win_w: f32,
    win_h: f32,
}

impl Default for Prefs {
    fn default() -> Self {
        Prefs { theme: String::new(), compact: false, row_min: ROW_MIN_DEFAULT, row_max: ROW_MAX_DEFAULT, two_col_w: TWO_COL_DEFAULT,
            name_clip: NAME_CLIP_DEFAULT,
            remember_size: false,
            win_w: WIN_DEFAULT_W,
            win_h: WIN_DEFAULT_H,
        }
    }
}

fn path() -> std::path::PathBuf {
    crate::settings::config_dir().join("gui.json")
}

/// Reads gui.json, defaulting when the file is missing or invalid.
fn read() -> Prefs {
    std::fs::read_to_string(path()).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
}

/// Writes gui.json, ignoring failures.
fn write(prefs: &Prefs) {
    let Ok(json) = serde_json::to_string_pretty(prefs) else { return };
    let _ = std::fs::create_dir_all(crate::settings::config_dir());
    let _ = std::fs::write(path(), json);
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
