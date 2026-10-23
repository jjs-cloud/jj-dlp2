//! Loading and saving settings.json in the OS-recommended config directory.

use crate::api::Settings;
use directories::{ProjectDirs, UserDirs};
use std::path::PathBuf;

pub fn config_dir() -> PathBuf {
    ProjectDirs::from("", "", "stream-recorder")
        .map(|d| d.config_dir().to_path_buf())
        .unwrap_or_else(|| PathBuf::from("."))
}

pub fn settings_path() -> PathBuf {
    config_dir().join("settings.json")
}

fn default_output_dir() -> String {
    UserDirs::new()
        .and_then(|u| u.video_dir().map(|p| p.join("stream-recorder")))
        .unwrap_or_else(|| PathBuf::from("recordings"))
        .to_string_lossy()
        .into_owned()
}

pub fn default_settings() -> Settings {
    Settings {
        next_id: 1,
        theme: "Neon Cyberpunk".into(),
        default_output_dir: default_output_dir(),
        sites: Vec::new(),
        streamers: Vec::new(),
    }
}

pub fn load() -> Settings {
    match std::fs::read_to_string(settings_path()) {
        Ok(text) => serde_json::from_str(&text).unwrap_or_else(|e| {
            eprintln!("settings.json is invalid ({e}); starting with defaults");
            default_settings()
        }),
        Err(_) => default_settings(),
    }
}

/// Writes atomically via a temp file so a crash never truncates the settings.
pub fn save(settings: &Settings) -> std::io::Result<()> {
    let dir = config_dir();
    std::fs::create_dir_all(&dir)?;
    let tmp = dir.join("settings.json.tmp");
    let json = serde_json::to_string_pretty(settings).map_err(std::io::Error::other)?;
    std::fs::write(&tmp, json)?;
    std::fs::rename(&tmp, settings_path())
}
