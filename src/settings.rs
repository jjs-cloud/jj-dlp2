//! Locating, loading and saving the config files in the OS-recommended config directory.

use crate::api::{Settings, Site, Streamer};
use directories::{ProjectDirs, UserDirs};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Contents of settings.json.
#[derive(Serialize, Deserialize)]
struct Globals {
    default_output_dir: String,
}

/// Contents of state.json.
#[derive(Serialize, Deserialize)]
struct State {
    next_id: u64,
}

pub fn config_dir() -> PathBuf {
    ProjectDirs::from("", "", "jj-dlp")
        .map(|d| d.config_dir().to_path_buf())
        .unwrap_or_else(|| PathBuf::from("."))
}

pub fn settings_path() -> PathBuf {
    config_dir().join("settings.json")
}

fn sites_path() -> PathBuf {
    config_dir().join("sites.json")
}

fn streamers_path() -> PathBuf {
    config_dir().join("streamers.json")
}

fn state_path() -> PathBuf {
    config_dir().join("state.json")
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

fn read_json<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T, String> {
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let text = std::fs::read_to_string(path).map_err(|e| format!("cannot read {name}: {e}"))?;
    serde_json::from_str(&text).map_err(|e| format!("{name} is invalid: {e}"))
}

/// Reads all config files, returning an error if any is missing or invalid.
pub fn try_load() -> Result<Settings, String> {
    let globals: Globals = read_json(&settings_path())?;
    let sites: Vec<Site> = read_json(&sites_path())?;
    let streamers: Vec<Streamer> = read_json(&streamers_path())?;
    let state: State = read_json(&state_path())?;
    Ok(Settings { next_id: state.next_id, default_output_dir: globals.default_output_dir, sites, streamers })
}

/// Reads the config files, falling back to defaults if none exist or they are invalid.
pub fn load() -> Settings {
    let any_exists = [settings_path(), sites_path(), streamers_path(), state_path()].iter().any(|p| p.exists());
    if !any_exists {
        return default_settings();
    }
    try_load().unwrap_or_else(|e| {
        eprintln!("{e}; starting with defaults");
        default_settings()
    })
}

/// Writes one file atomically via a temp file.
fn write_json<T: Serialize>(path: &Path, value: &T) -> std::io::Result<()> {
    let tmp = path.with_extension("json.tmp");
    let json = serde_json::to_string_pretty(value).map_err(std::io::Error::other)?;
    std::fs::write(&tmp, json)?;
    std::fs::rename(&tmp, path)
}

/// Writes every config file; state.json goes first so ids are never reused after a partial write.
pub fn save(settings: &Settings) -> std::io::Result<()> {
    std::fs::create_dir_all(config_dir())?;
    write_json(&state_path(), &State { next_id: settings.next_id })?;
    write_json(&settings_path(), &Globals { default_output_dir: settings.default_output_dir.clone() })?;
    write_json(&sites_path(), &settings.sites)?;
    write_json(&streamers_path(), &settings.streamers)
}
