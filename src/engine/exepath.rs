//! Resolves a site's yt-dlp path against the app's location and fixes the executable bit.

use std::path::{Path, PathBuf};

/// How many directories above the app's own folder are searched.
const MAX_PARENTS: usize = 3;

/// A resolved executable path plus an optional message for the Log tab.
pub struct Resolved {
    pub exe: String,
    pub note: Option<String>,
}

/// Resolves a relative path containing a separator; other values are returned unchanged.
pub fn resolve(raw: &str) -> Resolved {
    let mut bases: Vec<PathBuf> = Vec::new();
    if let Some(dir) = std::env::current_exe().ok().and_then(|p| p.parent().map(Path::to_path_buf)) {
        bases.extend(dir.ancestors().take(MAX_PARENTS + 1).map(Path::to_path_buf));
    }
    if let Ok(cwd) = std::env::current_dir() {
        bases.push(cwd);
    }
    resolve_in(raw, &bases)
}

fn resolve_in(raw: &str, bases: &[PathBuf]) -> Resolved {
    let raw = raw.trim();
    let unchanged = || Resolved { exe: raw.to_string(), note: None };
    if !raw.contains(['/', '\\']) || Path::new(raw).is_absolute() {
        return unchanged();
    }
    let rel: PathBuf = raw.split(['/', '\\']).filter(|p| !p.is_empty()).collect();
    match bases.iter().map(|b| b.join(&rel)).find(|p| p.is_file()) {
        Some(found) => Resolved { note: ensure_executable(&found), exe: found.to_string_lossy().into_owned() },
        None => unchanged(),
    }
}

/// Adds execute permission wherever read is granted; returns a message if it changed anything.
#[cfg(unix)]
fn ensure_executable(path: &Path) -> Option<String> {
    use std::os::unix::fs::PermissionsExt;
    let mode = std::fs::metadata(path).ok()?.permissions().mode();
    let new_mode = mode | ((mode & 0o444) >> 2);
    if new_mode == mode {
        return None;
    }
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(new_mode)).ok()?;
    Some(format!("set executable bit on {}", path.display()))
}

#[cfg(not(unix))]
fn ensure_executable(_path: &Path) -> Option<String> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("jj-exepath-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn bare_and_absolute_paths_are_unchanged() {
        let abs = std::env::temp_dir().join("yt-dlp");
        let abs = abs.to_string_lossy().into_owned();
        assert_eq!(resolve_in(" yt-dlp ", &[]).exe, "yt-dlp");
        assert_eq!(resolve_in(&abs, &[]).exe, abs);
    }

    #[test]
    fn finds_file_in_later_base_with_mixed_separators() {
        let (a, b) = (temp("a"), temp("b"));
        let file = b.join("bin").join("windows").join("yt-dlp.exe");
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(&file, b"x").unwrap();
        let r = resolve_in(r"bin\windows/yt-dlp.exe", &[a.clone(), b.clone()]);
        assert_eq!(PathBuf::from(r.exe), file);
        let _ = std::fs::remove_dir_all(a);
        let _ = std::fs::remove_dir_all(b);
    }

    #[test]
    fn missing_file_falls_back_to_original() {
        let a = temp("missing");
        let r = resolve_in(r"bin\nope\yt-dlp.exe", &[a.clone()]);
        assert_eq!(r.exe, r"bin\nope\yt-dlp.exe");
        assert!(r.note.is_none());
        let _ = std::fs::remove_dir_all(a);
    }

    #[cfg(unix)]
    #[test]
    fn sets_executable_bit_once() {
        use std::os::unix::fs::PermissionsExt;
        let a = temp("chmod");
        let file = a.join("bin").join("yt-dlp");
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(&file, b"x").unwrap();
        std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o644)).unwrap();
        let first = resolve_in("bin/yt-dlp", &[a.clone()]);
        assert!(first.note.unwrap().contains("set executable bit"));
        assert_eq!(std::fs::metadata(&file).unwrap().permissions().mode() & 0o777, 0o755);
        assert!(resolve_in("bin/yt-dlp", &[a.clone()]).note.is_none());
        let _ = std::fs::remove_dir_all(a);
    }
}
