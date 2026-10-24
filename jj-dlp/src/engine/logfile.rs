//! Plain-text log files written to the `logs` folder inside the config directory.

use super::template;
use crate::api::now_secs;
use crate::settings;
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

/// A log file larger than this is emptied before the next write.
const MAX_BYTES: u64 = 5 * 1024 * 1024;

/// Joins an executable and its arguments into one display string.
pub fn cmdline(exe: &str, args: &[String]) -> String {
    std::iter::once(exe).chain(args.iter().map(String::as_str)).collect::<Vec<_>>().join(" ")
}

/// Lowercases a name and makes it safe for use in a file name.
fn slug(s: &str) -> String {
    template::sanitize(s).to_lowercase().replace(' ', "-")
}

fn logs_dir() -> PathBuf {
    settings::config_dir().join("logs")
}

/// Path of a site's checker log, e.g. `twitch.checker.log`.
pub fn checker_path(site: &str) -> PathBuf {
    logs_dir().join(format!("{}.checker.log", slug(site)))
}

/// Path of a streamer's downloader log; `kind` is "stdout" or "stderr".
pub fn stream_path(site: &str, streamer: &str, kind: &str) -> PathBuf {
    logs_dir().join(format!("{}.{}.{kind}.log", slug(site), slug(streamer)))
}

/// Appends text to a log file, emptying it first if it is over the size limit; errors are ignored.
pub fn append(path: &Path, text: &str) {
    let _ = try_append(path, text);
}

fn try_append(path: &Path, text: &str) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let over = std::fs::metadata(path).is_ok_and(|m| m.len() > MAX_BYTES);
    let mut file = if over { File::create(path)? } else { OpenOptions::new().create(true).append(true).open(path)? };
    file.write_all(text.as_bytes())
}

/// Appends one timestamped line.
pub fn line(path: &Path, text: &str) {
    append(path, &format!("{}  {text}\n", stamp()));
}

/// The current UTC time as `YYYY-MM-DD HH:MM:SS`.
pub fn stamp() -> String {
    fmt_utc(now_secs())
}

fn fmt_utc(secs: u64) -> String {
    let days = (secs / 86_400) as i64 + 719_468;
    let era = days.div_euclid(146_097);
    let doe = days.rem_euclid(146_097);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    let rem = secs % 86_400;
    format!("{year:04}-{month:02}-{day:02} {:02}:{:02}:{:02}", rem / 3600, (rem / 60) % 60, rem % 60)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_utc() {
        assert_eq!(fmt_utc(0), "1970-01-01 00:00:00");
        assert_eq!(fmt_utc(1_700_000_000), "2023-11-14 22:13:20");
        assert_eq!(fmt_utc(1_782_259_199), "2026-06-23 23:59:59");
        assert_eq!(fmt_utc(951_782_400), "2000-02-29 00:00:00");
    }

    #[test]
    fn names_are_lowercase_and_safe() {
        let p = stream_path("Twitch", "Emi ru/x", "stderr");
        assert_eq!(p.file_name().unwrap(), "twitch.emi-ru_x.stderr.log");
        assert_eq!(checker_path("Twitch").file_name().unwrap(), "twitch.checker.log");
    }

    #[test]
    fn append_empties_an_oversized_file() {
        let path = std::env::temp_dir().join(format!("jj-logfile-{}.log", std::process::id()));
        let _ = std::fs::remove_file(&path);
        append(&path, "a\n");
        append(&path, "b\n");
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "a\nb\n");
        append(&path, &"x".repeat(MAX_BYTES as usize + 1));
        append(&path, "fresh\n");
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "fresh\n");
        let _ = std::fs::remove_file(&path);
    }
}
