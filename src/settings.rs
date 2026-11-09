//! Locating, loading and saving the config files in the OS-recommended config directory.

use crate::api::{GlobalNotify, Settings, Site, Streamer};
use directories::{ProjectDirs, UserDirs};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

/// Holds the reason saving is paused after a config file failed to load.
pub struct SaveGuard(Mutex<Option<String>>);

impl SaveGuard {
    pub const fn new() -> Self {
        SaveGuard(Mutex::new(None))
    }

    /// Pauses saving, adding `reason` to any reason already held.
    pub fn block(&self, reason: String) {
        let mut g = self.0.lock().unwrap_or_else(|e| e.into_inner());
        *g = Some(match g.take() {
            Some(old) => format!("{old}; {reason}"),
            None => reason,
        });
    }

    /// The reason saving is paused, if it is.
    pub fn reason(&self) -> Option<String> {
        self.0.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }

    pub fn clear(&self) {
        *self.0.lock().unwrap_or_else(|e| e.into_inner()) = None;
    }
}

static GUARD: SaveGuard = SaveGuard::new();

/// Copies `path` to `<path>.bad`; false when there is nothing to copy or the copy failed.
pub fn backup_bad(path: &Path) -> bool {
    let mut name = path.as_os_str().to_owned();
    name.push(".bad");
    path.is_file() && std::fs::copy(path, PathBuf::from(name)).is_ok()
}

/// Why the config files could not be loaded, while saving is paused.
pub fn load_problem() -> Option<String> {
    GUARD.reason()
}

/// Lets saving resume once the user has chosen to start fresh.
pub fn resume_saving() {
    GUARD.clear();
}

/// Contents of settings.json.
#[derive(Serialize, Deserialize)]
struct Globals {
    default_output_dir: String,
    max_recordings: Option<u32>,
    #[serde(default)]
    notifications: GlobalNotify,
}

/// Contents of state.json.
#[derive(Serialize, Deserialize)]
struct State {
    next_id: u64,
}

static CONFIG_DIR_OVERRIDE: OnceLock<PathBuf> = OnceLock::new();

/// Uses `dir` as the config directory instead of the platform default; call once at startup.
pub fn set_config_dir(dir: PathBuf) {
    let _ = CONFIG_DIR_OVERRIDE.set(dir);
}

pub fn config_dir() -> PathBuf {
    if let Some(dir) = CONFIG_DIR_OVERRIDE.get() {
        return dir.clone();
    }
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

pub fn default_output_dir() -> String {
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
        max_recordings: None,
        notifications: GlobalNotify::default(),
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
    Ok(Settings {
        next_id: state.next_id,
        default_output_dir: globals.default_output_dir,
        max_recordings: globals.max_recordings,
        notifications: globals.notifications,
        sites,
        streamers,
    })
}

/// Whether any of the four main config files exists.
pub fn any_config_exists() -> bool {
    [settings_path(), sites_path(), streamers_path(), state_path()].iter().any(|p| p.exists())
}

/// Like `load`, but on a failure backs the files up as `.bad` and pauses saving.
pub fn load_guarded() -> Settings {
    if !any_config_exists() {
        return default_settings();
    }
    try_load().unwrap_or_else(|e| {
        for p in [settings_path(), sites_path(), streamers_path(), state_path()] {
            backup_bad(&p);
        }
        GUARD.block(format!("{e}; the old files were kept as .bad copies"));
        default_settings()
    })
}

/// Reads the config files, falling back to defaults if none exist or they are invalid.
pub fn load() -> Settings {
    if !any_config_exists() {
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
    if GUARD.reason().is_some() {
        return Err(std::io::Error::other("saving is paused until the broken config is replaced"));
    }
    std::fs::create_dir_all(config_dir())?;
    write_json(&state_path(), &State { next_id: settings.next_id })?;
    let globals = Globals {
        default_output_dir: settings.default_output_dir.clone(),
        max_recordings: settings.max_recordings,
        notifications: settings.notifications.clone(),
    };
    write_json(&settings_path(), &globals)?;
    write_json(&sites_path(), &settings.sites)?;
    write_json(&streamers_path(), &settings.streamers)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn guard_joins_reasons_and_clears() {
        let g = SaveGuard::new();
        assert_eq!(g.reason(), None);
        g.block("a".into());
        g.block("b".into());
        assert_eq!(g.reason().as_deref(), Some("a; b"));
        g.clear();
        assert_eq!(g.reason(), None);
    }

    #[test]
    fn backup_copies_to_bad_file() {
        let dir = std::env::temp_dir().join(format!("jjdlp_bad_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let f = dir.join("sites.json");
        assert!(!backup_bad(&f));
        std::fs::write(&f, "{ nope").unwrap();
        assert!(backup_bad(&f));
        assert_eq!(std::fs::read_to_string(dir.join("sites.json.bad")).unwrap(), "{ nope");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
