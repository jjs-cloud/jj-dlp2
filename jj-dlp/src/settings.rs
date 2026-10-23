//! Locating and loading settings.json in the OS-recommended config directory.

use crate::api::Settings;
use directories::{ProjectDirs, UserDirs};
use std::path::PathBuf;

pub fn config_dir() -> PathBuf {
    ProjectDirs::from("", "", "jj-dlp")
        .map(|d| d.config_dir().to_path_buf())
        .unwrap_or_else(|| PathBuf::from("."))
}

pub fn settings_path() -> PathBuf {
    config_dir().join("settings.json")
}

fn default_output_dir() -> String {
    UserDirs::new()
        .and_then(|u| u.video_dir().map(|p| p.join("jj-dlp")))
        .unwrap_or_else(|| PathBuf::from("recordings"))
        .to_string_lossy()
        .into_owned()
}

pub fn default_settings() -> Settings {
    Settings {
        next_id: 1,
        default_output_dir: default_output_dir(),
        sites: Vec::new(),
        streamers: Vec::new(),
    }
}

/// Reads settings.json, returning an error if it is missing or invalid.
pub fn try_load() -> Result<Settings, String> {
    let text = std::fs::read_to_string(settings_path()).map_err(|e| format!("cannot read settings.json: {e}"))?;
    serde_json::from_str(&text).map_err(|e| format!("settings.json is invalid: {e}"))
}

/// Reads settings.json, falling back to defaults if it is missing or invalid.
pub fn load() -> Settings {
    if !settings_path().exists() {
        return default_settings();
    }
    try_load().unwrap_or_else(|e| {
        eprintln!("{e}; starting with defaults");
        default_settings()
    })
}
