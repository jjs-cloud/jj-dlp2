//! Logic behind the Files tab: scanning, sorting, row layout, file operations and ffmpeg jobs.

pub mod jobs;
pub mod ops;
pub mod rows;
pub mod scan;

use chrono::{DateTime, Local};
use std::borrow::Cow;
use std::cmp::Ordering;
use std::path::Path;

/// Formats a byte count such as `1.5 MB`.
pub fn human_size(n: u64) -> String {
    let mut size = n as f64;
    for unit in ["B", "KB", "MB", "GB", "TB"] {
        if size < 1024.0 {
            return if unit == "B" { format!("{} B", size as u64) } else { format!("{size:.1} {unit}") };
        }
        size /= 1024.0;
    }
    format!("{size:.1} PB")
}

/// Formats a write rate in bytes per second, or a dash when it is zero.
pub fn human_rate(bps: f64) -> String {
    if bps == 0.0 {
        return "—".to_string();
    }
    let sign = if bps < 0.0 { "-" } else { "" };
    format!("{sign}{}/s", human_size(bps.abs() as u64))
}

/// Formats a unix timestamp in local time.
pub fn fmt_mtime(secs: u64) -> String {
    DateTime::from_timestamp(secs as i64, 0)
        .map(|d| d.with_timezone(&Local).format("%Y-%m-%d %H:%M:%S").to_string())
        .unwrap_or_default()
}

/// The last path component as text.
pub fn file_name(p: &Path) -> Cow<'_, str> {
    p.file_name().map(|n| n.to_string_lossy()).unwrap_or_default()
}

/// Splits text into alternating digit and non-digit runs.
fn chunks(s: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut start = 0;
    let mut prev: Option<bool> = None;
    for (i, c) in s.char_indices() {
        let digit = c.is_ascii_digit();
        if prev.is_some_and(|p| p != digit) {
            out.push(&s[start..i]);
            start = i;
        }
        prev = Some(digit);
    }
    if start < s.len() {
        out.push(&s[start..]);
    }
    out
}

fn chunk_cmp(p: &str, q: &str) -> Ordering {
    let pd = p.as_bytes()[0].is_ascii_digit();
    let qd = q.as_bytes()[0].is_ascii_digit();
    match (pd, qd) {
        (true, true) => {
            let (p, q) = (p.trim_start_matches('0'), q.trim_start_matches('0'));
            p.len().cmp(&q.len()).then_with(|| p.cmp(q))
        }
        (false, false) => p.to_lowercase().cmp(&q.to_lowercase()),
        (true, false) => Ordering::Less,
        (false, true) => Ordering::Greater,
    }
}

/// Compares names case-insensitively with digit runs ordered numerically.
pub fn natural_cmp(a: &str, b: &str) -> Ordering {
    let (x, y) = (chunks(a), chunks(b));
    for (p, q) in x.iter().zip(y.iter()) {
        let o = chunk_cmp(p, q);
        if o != Ordering::Equal {
            return o;
        }
    }
    x.len().cmp(&y.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sizes_and_rates() {
        assert_eq!(human_size(512), "512 B");
        assert_eq!(human_size(1536), "1.5 KB");
        assert_eq!(human_size(5 * 1024 * 1024 * 1024), "5.0 GB");
        assert_eq!(human_rate(0.0), "—");
        assert_eq!(human_rate(2048.0), "2.0 KB/s");
        assert_eq!(human_rate(-2048.0), "-2.0 KB/s");
    }

    #[test]
    fn natural_order() {
        assert_eq!(natural_cmp("a2", "a10"), Ordering::Less);
        assert_eq!(natural_cmp("A2", "a2"), Ordering::Equal);
        assert_eq!(natural_cmp("file007", "file7"), Ordering::Equal);
        assert_eq!(natural_cmp("b", "a1"), Ordering::Greater);
        assert_eq!(natural_cmp("a", "a1"), Ordering::Less);
        assert_eq!(natural_cmp("1a", "a"), Ordering::Less);
    }
}
