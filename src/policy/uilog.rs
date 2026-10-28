//! The persistent UI log (`ui.log`) in the config directory.

use crate::api::LOG_HISTORY_MAX;
use crate::engine::logfile;
use crate::settings;
use std::path::{Path, PathBuf};

/// The file is cut back to its newest lines once it grows past this size.
const MAX_BYTES: u64 = 4 * 1024 * 1024;

fn path() -> PathBuf {
    settings::config_dir().join("ui.log")
}

/// The current UTC time as `YYYY-MM-DD HH:MM:SS`.
pub fn stamp() -> String {
    logfile::stamp()
}

/// Appends one timestamped line, then trims the file to its newest lines if it is too large.
pub fn append(text: &str) {
    let path = path();
    logfile::line(&path, &text.replace(['\r', '\n'], " "));
    if std::fs::metadata(&path).is_ok_and(|m| m.len() > MAX_BYTES) {
        trim(&path);
    }
}

/// The newest `n` saved lines, oldest first.
pub fn tail(n: usize) -> Vec<String> {
    let Ok(bytes) = std::fs::read(path()) else { return Vec::new() };
    newest(&String::from_utf8_lossy(&bytes), n).into_iter().map(str::to_string).collect()
}

/// Rewrites the file with only its newest `LOG_HISTORY_MAX` lines.
fn trim(path: &Path) {
    let Ok(bytes) = std::fs::read(path) else { return };
    let text = String::from_utf8_lossy(&bytes);
    let kept = newest(&text, LOG_HISTORY_MAX);
    let _ = std::fs::write(path, kept.join("\n") + "\n");
}

/// The last `n` non-blank lines of `text`.
fn newest(text: &str, n: usize) -> Vec<&str> {
    let lines: Vec<&str> = text.lines().filter(|l| !l.trim().is_empty()).collect();
    lines[lines.len().saturating_sub(n)..].to_vec()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn newest_keeps_the_last_lines() {
        assert_eq!(newest("a\nb\n\nc\n", 2), ["b", "c"]);
        assert_eq!(newest("a\nb\n", 5), ["a", "b"]);
    }
}
