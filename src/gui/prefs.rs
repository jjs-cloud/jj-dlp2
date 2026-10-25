//! GUI-only preferences stored in gui.json.

use super::skin::ThemeKind;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Default)]
#[serde(default)]
struct Prefs {
    theme: String,
    compact: bool,
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
