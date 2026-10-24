//! GUI-only preferences stored in gui.json.

use super::skin::ThemeKind;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Default)]
struct Prefs {
    theme: String,
}

fn path() -> std::path::PathBuf {
    crate::settings::config_dir().join("gui.json")
}

/// Reads the saved theme, defaulting when the file is missing or invalid.
pub fn load_theme() -> ThemeKind {
    let prefs: Prefs = std::fs::read_to_string(path()).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default();
    ThemeKind::from_name(&prefs.theme)
}

/// Writes the theme to gui.json, ignoring failures.
pub fn save_theme(kind: ThemeKind) {
    let prefs = Prefs { theme: kind.name().to_string() };
    let Ok(json) = serde_json::to_string_pretty(&prefs) else { return };
    let _ = std::fs::create_dir_all(crate::settings::config_dir());
    let _ = std::fs::write(path(), json);
}
