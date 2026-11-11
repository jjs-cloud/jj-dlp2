//! Reads and writes the Posts config and library files.

use super::model::{Post, PostState, PostsConfig};
use crate::settings::config_dir;
use std::path::{Path, PathBuf};

fn path_in_config(name: &str) -> PathBuf {
    config_dir().join(name)
}

pub fn thumbs_dir() -> PathBuf {
    path_in_config("post_thumbs")
}

/// The file yt-dlp writes a post's downloaded path to.
pub fn sidecar_path(id: &str) -> PathBuf {
    let path = path_in_config("post_sidecars").join(format!("{}.txt", super::model::safe_name(id)));
    std::path::absolute(&path).unwrap_or(path)
}

static GUARD: crate::settings::SaveGuard = crate::settings::SaveGuard::new();

/// Why the posts files could not be loaded, while saving is paused.
pub fn load_problem() -> Option<String> {
    GUARD.reason()
}

/// Lets saving resume once the user has chosen to start fresh.
pub fn resume_saving() {
    GUARD.clear();
}

/// Reads a JSON file; a missing file gives `None`, an invalid one is backed up and pauses saving.
fn read_json<T: serde::de::DeserializeOwned>(path: &Path) -> Option<T> {
    let text = std::fs::read_to_string(path).ok()?;
    match serde_json::from_str(&text) {
        Ok(v) => Some(v),
        Err(e) => {
            crate::settings::backup_bad(path);
            let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            GUARD.block(format!("{name} is invalid: {e}; the old file was kept as a .bad copy"));
            None
        }
    }
}

pub fn load_config() -> PostsConfig {
    let path = path_in_config("posts.json");
    if !path.exists() {
        return PostsConfig::default();
    }
    read_json(&path).unwrap_or_default()
}

/// Queues the config for the background writer unless saving is paused.
pub fn save_config(cfg: PostsConfig) {
    if GUARD.reason().is_none() {
        crate::persist::save_json(path_in_config("posts.json"), cfg, true);
    }
}

/// Loads the library, requeuing interrupted downloads and dropping references to missing files.
pub fn load_library() -> Vec<Post> {
    let mut posts: Vec<Post> = read_json(&path_in_config("posts_library.json")).unwrap_or_default();
    for p in &mut posts {
        if p.state == PostState::Downloading {
            p.state = PostState::Queued;
        }
        if p.has_thumb && !p.thumb_path().is_file() {
            p.has_thumb = false;
        }
        if p.state == PostState::Ready && !p.file.as_deref().is_some_and(|f| Path::new(f).is_file()) {
            p.state = PostState::Remote;
            p.file = None;
        }
    }
    posts
}

/// Queues the library for the background writer unless saving is paused.
pub fn save_library(posts: Vec<Post>) {
    if GUARD.reason().is_none() {
        crate::persist::save_json(path_in_config("posts_library.json"), posts, false);
    }
}

