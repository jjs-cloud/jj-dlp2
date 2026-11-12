//! Single-file operations: open, reveal, delete and empty-folder cleanup.

use super::file_name;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeleteMode {
    Trash,
    Permanent,
}

/// Opens a file with the system default program without waiting for the shell.
pub fn open_file(path: &Path) -> Result<(), String> {
    open_detached(path.to_path_buf());
    Ok(())
}

/// Opens a path or URL with the system default program on a background thread; failures go to the UI log.
pub fn open_detached(target: impl AsRef<std::ffi::OsStr> + Send + 'static) {
    let _ = std::thread::Builder::new().name("open".into()).spawn(move || {
        if let Err(e) = open::that_detached(target.as_ref()) {
            crate::policy::uilog::append(&format!("Could not open {}: {e}", target.as_ref().to_string_lossy()));
        }
    });
}

/// Opens a path or URL with the given program on a background thread; failures go to the UI log.
pub fn open_with_detached(target: String, app: String) {
    let _ = std::thread::Builder::new().name("open".into()).spawn(move || {
        if let Err(e) = open::with_detached(&target, app) {
            crate::policy::uilog::append(&format!("Could not play {target}: {e}"));
        }
    });
}

/// Shows the path in the system file manager, selecting it where the platform supports that.
pub fn reveal(path: &Path) -> Result<(), String> {
    let abs = std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf());
    let spawned = if cfg!(windows) {
        Command::new("explorer").arg("/select,").arg(&abs).spawn().map(|_| ())
    } else if cfg!(target_os = "macos") {
        Command::new("open").arg("-R").arg(&abs).spawn().map(|_| ())
    } else {
        open::that_detached(abs.parent().unwrap_or(&abs))
    };
    spawned.map_err(|e| e.to_string())
}

/// Sends a file to the recycle bin.
pub fn to_trash(path: &Path) -> Result<(), String> {
    ::trash::delete(path).map_err(|e| e.to_string())
}

/// Deletes a file as the mode says.
pub fn delete(path: &Path, mode: DeleteMode) -> Result<(), String> {
    match mode {
        DeleteMode::Trash => to_trash(path),
        DeleteMode::Permanent => std::fs::remove_file(path).map_err(|e| e.to_string()),
    }
}

/// Returns `path`, or `name (2).ext`, `name (3).ext`, … when it already exists.
pub fn unique_path(path: &Path) -> PathBuf {
    if !path.exists() {
        return path.to_path_buf();
    }
    let stem = path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    let ext = path.extension().map(|e| format!(".{}", e.to_string_lossy())).unwrap_or_default();
    let dir = path.parent().map(Path::to_path_buf).unwrap_or_default();
    (2..)
        .map(|n| dir.join(format!("{stem} ({n}){ext}")))
        .find(|p| !p.exists())
        .unwrap_or_else(|| path.to_path_buf())
}

/// Deletes `folder` when it is an empty subfolder of `root`; returns a note for the status line.
pub fn remove_empty_folder(folder: &Path, root: &Path, mode: DeleteMode) -> String {
    let abs = |p: &Path| std::path::absolute(p).unwrap_or_else(|_| p.to_path_buf());
    let (folder, root) = (abs(folder), abs(root));
    if folder == root || !folder.starts_with(&root) || !folder.is_dir() {
        return String::new();
    }
    if !std::fs::read_dir(&folder).is_ok_and(|mut d| d.next().is_none()) {
        return String::new();
    }
    let name = file_name(&folder).into_owned();
    let result = match mode {
        DeleteMode::Trash => to_trash(&folder),
        DeleteMode::Permanent => std::fs::remove_dir(&folder).map_err(|e| e.to_string()),
    };
    match result {
        Ok(()) => format!("; removed empty folder {name}"),
        Err(e) => format!("; could not remove empty folder {name}: {e}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("jjdlp_ops_{tag}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn unique_path_counts_up() {
        let dir = temp("unique");
        let p = dir.join("a.ts");
        assert_eq!(unique_path(&p), p);
        std::fs::write(&p, b"x").unwrap();
        assert_eq!(unique_path(&p), dir.join("a (2).ts"));
        std::fs::write(dir.join("a (2).ts"), b"x").unwrap();
        assert_eq!(unique_path(&p), dir.join("a (3).ts"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn empty_folder_rules() {
        let root = temp("empty");
        let sub = root.join("sub");
        std::fs::create_dir_all(&sub).unwrap();
        assert_eq!(remove_empty_folder(&root, &root, DeleteMode::Permanent), "");
        assert_eq!(remove_empty_folder(&root.parent().unwrap().to_path_buf(), &root, DeleteMode::Permanent), "");
        std::fs::write(sub.join("f"), b"x").unwrap();
        assert_eq!(remove_empty_folder(&sub, &root, DeleteMode::Permanent), "");
        std::fs::remove_file(sub.join("f")).unwrap();
        assert_eq!(remove_empty_folder(&sub, &root, DeleteMode::Permanent), "; removed empty folder sub");
        assert!(!sub.exists());
        let _ = std::fs::remove_dir_all(&root);
    }
}
