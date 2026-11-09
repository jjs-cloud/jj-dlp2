//! Plain-text log files written to the `logs` folder inside the config directory.

use super::template;
use crate::api::now_secs;
use crate::settings;
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Output;
use std::time::Duration;

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

/// Folder holding all log files.
pub fn logs_dir() -> PathBuf {
    settings::config_dir().join("logs")
}

/// Path of a site's checker log; `kind` is "stdout" or "stderr", e.g. `twitch.checker.stdout.log`.
pub fn checker_path(site: &str, kind: &str) -> PathBuf {
    logs_dir().join(format!("{}.checker.{kind}.log", slug(site)))
}

/// Path of a streamer's recorder log; `kind` is "stdout" or "stderr".
pub fn stream_path(site: &str, streamer: &str, kind: &str) -> PathBuf {
    logs_dir().join(format!("{}.{}.{kind}.log", slug(site), slug(streamer)))
}

/// Path of a Posts account's checker log, e.g. `posts.tiktok.emiru.checker.stdout.log`.
pub fn posts_checker_path(site: &str, account: &str, kind: &str) -> PathBuf {
    logs_dir().join(format!("posts.{}.{}.checker.{kind}.log", slug(site), slug(account)))
}

/// Path of a Posts site's downloader log, e.g. `posts.tiktok.downloader.stdout.log`.
pub fn posts_download_path(site: &str, kind: &str) -> PathBuf {
    logs_dir().join(format!("posts.{}.downloader.{kind}.log", slug(site)))
}

/// Log files that receive a command's results; a stream's file is `None` when its log is off.
#[derive(Debug, Clone, Default)]
pub struct CmdLog {
    stdout: Option<PathBuf>,
    stderr: Option<PathBuf>,
}

impl CmdLog {
    /// Keeps the file of each stream whose switch is on; `path` maps "stdout" or "stderr" to a file.
    pub fn new(stdout_on: bool, stderr_on: bool, path: impl Fn(&str) -> PathBuf) -> Self {
        CmdLog { stdout: stdout_on.then(|| path("stdout")), stderr: stderr_on.then(|| path("stderr")) }
    }

    /// Appends a finished command's header and output to each enabled log.
    pub fn output(&self, exe: &str, args: &[String], output: &Output, elapsed: Duration) {
        if self.stdout.is_none() && self.stderr.is_none() {
            return;
        }
        let code = output.status.code().map_or("?".to_string(), |c| c.to_string());
        let header = format!("{}  {} (exit {code}, {:.1}s)\n", stamp(), cmdline(exe, args), elapsed.as_secs_f32());
        for (path, bytes) in [(&self.stdout, &output.stdout), (&self.stderr, &output.stderr)] {
            if let Some(path) = path {
                append(path, &format!("{header}{}\n\n", String::from_utf8_lossy(bytes).trim_end()));
            }
        }
    }

    /// Appends a header saying why a command produced no output to each enabled log.
    pub fn failure(&self, exe: &str, args: &[String], reason: &str) {
        let block = format!("{}  {} ({reason})\n\n", stamp(), cmdline(exe, args));
        for path in [&self.stdout, &self.stderr].into_iter().flatten() {
            append(path, &block);
        }
    }
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

/// Rewrites a leading UTC `YYYY-MM-DD HH:MM:SS` stamp in a line as local time.
pub fn to_local(line: &str) -> String {
    let Some(head) = line.get(..19) else { return line.to_string() };
    match chrono::NaiveDateTime::parse_from_str(head, "%Y-%m-%d %H:%M:%S") {
        Ok(t) => format!("{}{}", t.and_utc().with_timezone(&chrono::Local).format("%Y-%m-%d %H:%M:%S"), &line[19..]),
        Err(_) => line.to_string(),
    }
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
    fn to_local_keeps_unstamped_lines() {
        assert_eq!(to_local("plain output"), "plain output");
        assert_eq!(to_local("2026-01-01 00:00:00  cmd").len(), "2026-01-01 00:00:00  cmd".len());
    }

    #[test]
    fn names_are_lowercase_and_safe() {
        let p = stream_path("Twitch", "Emi ru/x", "stderr");
        assert_eq!(p.file_name().unwrap(), "twitch.emi-ru_x.stderr.log");
        assert_eq!(checker_path("Twitch", "stdout").file_name().unwrap(), "twitch.checker.stdout.log");
    }

    #[test]
    fn posts_names_are_prefixed() {
        assert_eq!(posts_checker_path("TikTok", "Emi ru", "stdout").file_name().unwrap(), "posts.tiktok.emi-ru.checker.stdout.log");
        assert_eq!(posts_download_path("TikTok", "stderr").file_name().unwrap(), "posts.tiktok.downloader.stderr.log");
    }

    #[test]
    fn failure_goes_to_enabled_streams_only() {
        let dir = std::env::temp_dir();
        let out = dir.join(format!("jj-cmdlog-out-{}.log", std::process::id()));
        let err = dir.join(format!("jj-cmdlog-err-{}.log", std::process::id()));
        let _ = std::fs::remove_file(&out);
        let _ = std::fs::remove_file(&err);
        let log = CmdLog::new(true, false, |k| if k == "stdout" { out.clone() } else { err.clone() });
        log.failure("yt-dlp", &["--x".to_string()], "timed out after 5s");
        let text = std::fs::read_to_string(&out).unwrap();
        assert!(text.contains("yt-dlp --x (timed out after 5s)"));
        assert!(!err.exists());
        let _ = std::fs::remove_file(&out);
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
