//! The built-in changelog and the record of which version's changelog was last shown.

use crate::settings;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};

/// The full changelog, embedded at build time.
const TEXT: &str = include_str!("../CHANGELOG.md");

static PENDING: AtomicBool = AtomicBool::new(false);

#[derive(Default, Serialize, Deserialize)]
#[serde(default)]
struct Seen {
    version: String,
}

fn seen_path() -> PathBuf {
    settings::config_dir().join("changelog_seen.json")
}

fn read_seen() -> Option<String> {
    let text = std::fs::read_to_string(seen_path()).ok()?;
    serde_json::from_str::<Seen>(&text).ok().map(|s| s.version)
}

/// The changelog text, newest version first.
pub fn text() -> &'static str {
    TEXT
}

/// Decides at startup whether the changelog window opens; call before any config file is created.
pub fn init() {
    let current = crate::policy::update::CURRENT;
    match read_seen() {
        Some(v) if v == current => {}
        None if !settings::any_config_exists() => mark_seen(),
        _ => PENDING.store(true, Ordering::Relaxed),
    }
}

/// Whether the changelog window should open at launch.
pub fn pending() -> bool {
    PENDING.load(Ordering::Relaxed)
}

/// Records that the changelog of the running version has been shown.
pub fn mark_seen() {
    let seen = Seen { version: crate::policy::update::CURRENT.to_string() };
    let Ok(json) = serde_json::to_string_pretty(&seen) else {
        return;
    };
    let _ = std::fs::create_dir_all(settings::config_dir());
    let _ = std::fs::write(seen_path(), json);
}
