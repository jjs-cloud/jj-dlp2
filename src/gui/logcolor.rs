//! Colours Engine log lines by keywords found in their text.

use super::prefs::LogWords;
use super::skin::Skin;
use iced::Color;

pub const DEFAULT_ERROR: &str = "failed|could not|error|not found|unknown|broken|crash|problem:|unexpectedly|interrupt|denied";
pub const DEFAULT_WARN: &str = "stalled|inactive|ignored|limit|warning|not armed|cannot|already monitored|unmatched|restarting|lower priority";
pub const DEFAULT_GOOD: &str = "is live|new from|ready|up to date|enabled|cleared|no problems";

/// Which keyword list a Settings field edits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WordList {
    Error,
    Warn,
    Good,
}

/// How a log message is coloured.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Level {
    Error,
    Warn,
    Good,
    Info,
}

impl Level {
    pub fn color(self, s: Skin) -> Color {
        match self {
            Level::Error => s.danger,
            Level::Warn => s.warn,
            Level::Good => s.live,
            Level::Info => s.text,
        }
    }
}

/// The keyword lists, split and lowercased once.
#[derive(Clone, Default)]
pub struct Matcher {
    error: Vec<String>,
    warn: Vec<String>,
    good: Vec<String>,
}

fn words(list: &str) -> Vec<String> {
    list.split('|').map(|w| w.trim().to_lowercase()).filter(|w| !w.is_empty()).collect()
}

impl Matcher {
    pub fn new(w: &LogWords) -> Self {
        Matcher { error: words(&w.error), warn: words(&w.warn), good: words(&w.good) }
    }

    /// The level of a message: errors win over warnings, warnings over good news.
    pub fn level(&self, message: &str) -> Level {
        let m = message.to_lowercase();
        let hit = |list: &[String]| list.iter().any(|w| m.contains(w.as_str()));
        if hit(&self.error) {
            Level::Error
        } else if hit(&self.warn) {
            Level::Warn
        } else if hit(&self.good) {
            Level::Good
        } else {
            Level::Info
        }
    }
}

/// Splits `YYYY-MM-DD HH:MM:SS  text` into the timestamp and the text; the timestamp is empty when absent.
pub fn split_stamp(line: &str) -> (&str, &str) {
    let b = line.as_bytes();
    let stamped = b.len() >= 21 && b[4] == b'-' && b[7] == b'-' && b[10] == b' ' && b[13] == b':' && b[16] == b':' && &b[19..21] == b"  ";
    if stamped { (&line[..19], &line[21..]) } else { ("", line) }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn matcher(e: &str, w: &str, g: &str) -> Matcher {
        Matcher::new(&LogWords { error: e.into(), warn: w.into(), good: g.into() })
    }

    #[test]
    fn errors_win_over_warnings_and_good() {
        let m = matcher("failed", "limit", "is live");
        assert_eq!(m.level("x is live"), Level::Good);
        assert_eq!(m.level("limit reached"), Level::Warn);
        assert_eq!(m.level("LIMIT failed"), Level::Error);
        assert_eq!(m.level("plain"), Level::Info);
    }

    #[test]
    fn empty_words_never_match() {
        assert_eq!(matcher("||", " ", "").level("anything"), Level::Info);
    }

    #[test]
    fn splits_the_timestamp() {
        assert_eq!(split_stamp("2026-11-24 10:00:00  hello"), ("2026-11-24 10:00:00", "hello"));
        assert_eq!(split_stamp("no stamp"), ("", "no stamp"));
    }
}
