//! Turns yt-dlp error output into problems, plain-language hints and expected-error filtering.

use super::preflight;

/// ERROR text that is normal during checks because the channel is simply offline.
const BENIGN: [&str; 3] = ["not currently live", "currently offline", "live event will begin"];

/// An unexpected yt-dlp error, with a fix when it is a known setup problem.
#[derive(Debug, Clone, PartialEq)]
pub struct Problem {
    pub line: String,
    pub hint: Option<String>,
}

impl Problem {
    /// Short text for the UI: the hint when known, else the error itself.
    pub fn summary(&self) -> String {
        match &self.hint {
            Some(h) => h.clone(),
            None => self.line.strip_prefix("ERROR:").unwrap_or(&self.line).trim().to_string(),
        }
    }

    /// Log text: the error line plus the hint when there is one.
    pub fn log_text(&self) -> String {
        match &self.hint {
            Some(h) => format!("{} → {h}", self.line),
            None => self.line.clone(),
        }
    }
}

pub fn is_error(line: &str) -> bool {
    line.trim_start().starts_with("ERROR:")
}

fn is_benign(line: &str) -> bool {
    let low = line.to_lowercase();
    BENIGN.iter().any(|p| low.contains(p))
}

fn capitalize(s: &str) -> String {
    let mut c = s.chars();
    c.next().map(|f| f.to_uppercase().collect::<String>() + c.as_str()).unwrap_or_default()
}

/// Plain-language fix for a known setup error.
pub fn hint(line: &str) -> Option<String> {
    let low = line.to_lowercase();
    if low.contains("could not find") && low.contains("cookies database") {
        let browser = low
            .split_once("could not find ")
            .and_then(|(_, rest)| rest.split(" cookies database").next())
            .map(capitalize)
            .unwrap_or_else(|| "The browser".to_string());
        return Some(format!(
            "{browser} has no profile yet. Open it once (log in to the site if you want its cookies); jj-dlp retries on its own."
        ));
    }
    let missing = ["could not be found", "not found", "not installed", "please install", "no such file or directory: 'ffmpeg"];
    if low.contains("ffmpeg") && missing.iter().any(|p| low.contains(p)) {
        let how = match preflight::install_command() {
            Some(cmd) => format!("Install it with: {cmd}."),
            None => "Install it with your package manager.".to_string(),
        };
        let tail = if cfg!(windows) { "Then restart jj-dlp." } else { "jj-dlp retries on its own." };
        return Some(format!("ffmpeg is not installed. {how} {tail}"));
    }
    if low.contains("could not copy") && low.contains("cookie database") {
        return Some("The browser has its cookie database locked. Close the browser (or switch to Firefox); jj-dlp retries on its own.".to_string());
    }
    None
}

/// The most useful unexpected ERROR line in `lines`, preferring one with a known fix.
pub fn find_problem(lines: &[String]) -> Option<Problem> {
    let errors: Vec<&str> = lines.iter().map(|l| l.trim()).filter(|l| is_error(l) && !is_benign(l)).collect();
    let hinted = errors.iter().find_map(|l| hint(l).map(|h| (*l, h)));
    match hinted {
        Some((line, h)) => Some(Problem { line: line.to_string(), hint: Some(h) }),
        None => errors.first().map(|l| Problem { line: l.to_string(), hint: None }),
    }
}

/// Problem for a finished checker run: ERROR lines first, else a generic one if it failed silently.
pub fn check_problem(lines: &[String], success: bool, code: Option<i32>, has_entries: bool) -> Option<Problem> {
    if lines.iter().any(|l| is_error(l)) {
        return find_problem(lines);
    }
    if success || has_entries {
        return None;
    }
    let last = lines.iter().map(|l| l.trim()).rfind(|l| !l.is_empty() && !l.starts_with("WARNING:"));
    let line = last.map_or_else(|| format!("yt-dlp failed ({}) and printed no error", describe_exit(code)), str::to_string);
    Some(Problem { line, hint: None })
}

pub fn describe_exit(code: Option<i32>) -> String {
    code.map_or_else(|| "terminated by a signal".to_string(), |c| format!("exit code {c}"))
}

/// One-off informational note for a recorder output line.
pub fn note(line: &str) -> Option<String> {
    let low = line.to_lowercase();
    let rest = low.split_once("extracted 0 cookies from ")?.1;
    let browser = rest.split_whitespace().next().unwrap_or("the browser");
    Some(format!("no cookies were found in {browser}, so this recording runs without a login"))
}

#[cfg(test)]
mod tests {
    use super::*;

    const COOKIES: &str = "ERROR: could not find firefox cookies database in '/home/user/.config/mozilla/firefox', '/home/user/.mozilla/firefox'";
    const FFMPEG: &str = "ERROR: m3u8 download detected but ffmpeg could not be found. Please install";
    const OFFLINE: &str = "ERROR: [twitch:stream] emiru: The channel is not currently live";

    fn lines(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn known_errors_get_hints() {
        assert!(hint(COOKIES).unwrap().starts_with("Firefox has no profile yet"));
        assert!(hint(FFMPEG).unwrap().starts_with("ffmpeg is not installed"));
        assert!(hint("ERROR: Could not copy Chrome cookie database. See https://example.test").is_some());
        assert!(hint("ERROR: something else entirely").is_none());
    }

    #[test]
    fn offline_errors_are_not_problems() {
        assert!(is_benign(OFFLINE));
        assert!(check_problem(&lines(&[OFFLINE]), false, Some(1), false).is_none());
    }

    #[test]
    fn hinted_error_wins_over_following_noise() {
        let l = lines(&[OFFLINE, FFMPEG, "ERROR: Unable to download video: [Errno 2] No such file or directory: 'x.mp4.part'"]);
        let p = find_problem(&l).unwrap();
        assert_eq!(p.line, FFMPEG);
        assert!(p.hint.is_some());
    }

    #[test]
    fn unknown_error_is_reported_raw() {
        let p = check_problem(&lines(&["ERROR: boom"]), false, Some(1), false).unwrap();
        assert_eq!(p.summary(), "boom");
        assert!(p.hint.is_none());
    }

    #[test]
    fn silent_failure_is_reported_only_without_entries() {
        assert!(check_problem(&[], false, Some(1), false).is_some());
        assert!(check_problem(&[], false, Some(1), true).is_none());
        assert!(check_problem(&[], true, Some(0), false).is_none());
        let p = check_problem(&lines(&["WARNING: x", "Traceback: oops"]), false, Some(1), false).unwrap();
        assert_eq!(p.line, "Traceback: oops");
    }

    #[test]
    fn zero_cookies_note() {
        assert!(note("Extracted 0 cookies from firefox").unwrap().contains("firefox"));
        assert!(note("Extracted 12 cookies from firefox").is_none());
        assert!(note("Extracting cookies from firefox").is_none());
    }
}
