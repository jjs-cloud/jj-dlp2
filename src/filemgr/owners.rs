//! Which streamer and site each recorded file came from, kept in file_owners.json.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, MutexGuard, OnceLock};

/// The streamer and site a file was recorded for.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Owner {
    pub streamer: String,
    pub site: String,
}

/// A file's streamer and site as shown in the Files tab; `None` means "Other".
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Who {
    pub streamer: Option<String>,
    pub site: Option<String>,
}

type Map = HashMap<String, Owner>;

static STATE: OnceLock<Mutex<Map>> = OnceLock::new();
static VERSION: AtomicU64 = AtomicU64::new(0);

fn file() -> PathBuf {
    crate::settings::config_dir().join("file_owners.json")
}

/// Loads file_owners.json, dropping files that are gone from a folder that still exists.
fn load() -> Map {
    let mut map: Map = std::fs::read_to_string(file()).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default();
    map.retain(|k, _| {
        let p = Path::new(k);
        !(p.parent().is_some_and(Path::is_dir) && !p.exists())
    });
    map
}

fn lock() -> MutexGuard<'static, Map> {
    STATE.get_or_init(|| Mutex::new(load())).lock().unwrap_or_else(|e| e.into_inner())
}

/// A path as a map key: absolute, forward slashes and lower case on Windows.
pub fn key(p: &Path) -> String {
    let abs = std::path::absolute(p).unwrap_or_else(|_| p.to_path_buf());
    let s = abs.to_string_lossy().into_owned();
    if cfg!(windows) { s.replace('\\', "/").to_lowercase() } else { s }
}

fn save(map: &Map) {
    VERSION.fetch_add(1, Ordering::Relaxed);
    crate::persist::save_json(file(), map.clone(), false);
}

/// Changes whenever the stored owners change.
pub fn version() -> u64 {
    VERSION.load(Ordering::Relaxed)
}

/// A copy of every stored owner, keyed by `key`.
pub fn snapshot() -> Map {
    lock().clone()
}

/// Stores the owner of a recorded file.
pub fn record(path: &Path, owner: Owner) {
    let k = key(path);
    let mut map = lock();
    if map.get(&k) == Some(&owner) {
        return;
    }
    map.insert(k, owner);
    save(&map);
}

/// Gives `to` the same owner as `from`, when `from` has one.
pub fn inherit(from: &Path, to: &Path) {
    let (from_key, to_key) = (key(from), key(to));
    let mut map = lock();
    let Some(owner) = map.get(&from_key).cloned() else { return };
    if from_key == to_key || map.get(&to_key) == Some(&owner) {
        return;
    }
    map.insert(to_key, owner);
    save(&map);
}

/// True when `name` appears in `hay` (both lower case) with no letter or digit on either side.
fn has_word(hay: &str, name: &str) -> bool {
    hay.match_indices(name).any(|(i, m)| {
        let before = hay[..i].chars().next_back();
        let after = hay[i + m.len()..].chars().next();
        !before.is_some_and(char::is_alphanumeric) && !after.is_some_and(char::is_alphanumeric)
    })
}

/// Finds a current streamer for an unrecorded file from its subfolders, then its file name.
fn match_known(path: &Path, root: &Path, known: &[(String, String)]) -> Who {
    let rel = path.strip_prefix(root).unwrap_or(path);
    let folders: Vec<String> = rel
        .parent()
        .map(|p| p.components().map(|c| c.as_os_str().to_string_lossy().to_lowercase()).collect())
        .unwrap_or_default();
    let stem = path.file_stem().map(|s| s.to_string_lossy().to_lowercase()).unwrap_or_default();
    let mut hits: Vec<(u8, usize, &str, &str)> = Vec::new();
    for (name, site) in known {
        let lower = name.to_lowercase();
        if lower.is_empty() {
            continue;
        }
        let folder = crate::policy::layout::folder_name(name).to_lowercase();
        let score = if folders.iter().any(|f| *f == lower || *f == folder) {
            2
        } else if has_word(&stem, &lower) {
            1
        } else {
            continue;
        };
        hits.push((score, lower.chars().count(), name, site));
    }
    let Some(top) = hits.iter().map(|h| (h.0, h.1)).max() else { return Who::default() };
    let best: Vec<_> = hits.iter().filter(|h| (h.0, h.1) == top).collect();
    let name = best[0].2;
    let same: Vec<&str> = best.iter().filter(|h| h.2.eq_ignore_ascii_case(name)).map(|h| h.3).collect();
    let site = (same.len() == 1).then(|| same[0].to_string());
    Who { streamer: Some(name.to_string()), site }
}

/// Resolves a file's streamer and site: the stored owner first, else a match against current streamers.
pub fn resolve(path: &Path, root: &Path, owners: &Map, known: &[(String, String)]) -> Who {
    match owners.get(&key(path)) {
        Some(o) => Who { streamer: Some(o.streamer.clone()), site: Some(o.site.clone()) },
        None => match_known(path, root, known),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn known() -> Vec<(String, String)> {
        vec![("alice".into(), "Twitch".into()), ("Bob".into(), "Kick".into()), ("al".into(), "Twitch".into())]
    }

    #[test]
    fn name_match_needs_whole_word_and_a_current_streamer() {
        let root = Path::new("/r");
        let w = match_known(Path::new("/r/Alice_2024-01-01.ts"), root, &known());
        assert_eq!(w, Who { streamer: Some("alice".into()), site: Some("Twitch".into()) });
        assert_eq!(match_known(Path::new("/r/malice 2024.ts"), root, &known()), Who::default());
        assert_eq!(match_known(Path::new("/r/carol 2024.ts"), root, &known()), Who::default());
    }

    #[test]
    fn subfolder_beats_file_name() {
        let w = match_known(Path::new("/r/bob/alice stream.ts"), Path::new("/r"), &known());
        assert_eq!(w.streamer.as_deref(), Some("Bob"));
    }

    #[test]
    fn same_name_on_two_sites_leaves_the_site_unknown() {
        let k = vec![("dup".to_string(), "Twitch".to_string()), ("DUP".to_string(), "Kick".to_string())];
        let w = match_known(Path::new("/r/dup live.ts"), Path::new("/r"), &k);
        assert_eq!(w.streamer.as_deref(), Some("dup"));
        assert_eq!(w.site, None);
    }

    #[test]
    fn stored_owner_wins_over_name_matching() {
        let mut map = Map::new();
        map.insert(key(Path::new("/r/bob.ts")), Owner { streamer: "Zed".into(), site: "Old".into() });
        let w = resolve(Path::new("/r/bob.ts"), Path::new("/r"), &map, &known());
        assert_eq!(w, Who { streamer: Some("Zed".into()), site: Some("Old".into()) });
    }
}
