//! Reads the end of a log file and follows its growth.

use crate::engine::logfile;
use std::collections::HashSet;
use std::fs::File;
use std::io::{ErrorKind, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

const CHUNK: u64 = 64 * 1024;

/// Path of a site's checker log; `kind` is "stdout" or "stderr".
pub fn checker_file(site: &str, kind: &str) -> PathBuf {
    logfile::checker_path(site, kind)
}

/// Path of a streamer's recorder log; `kind` is "stdout" or "stderr".
pub fn stream_file(site: &str, streamer: &str, kind: &str) -> PathBuf {
    logfile::stream_path(site, streamer, kind)
}

/// Path of a Posts account's checker log; `kind` is "stdout" or "stderr".
pub fn posts_checker_file(site: &str, account: &str, kind: &str) -> PathBuf {
    logfile::posts_checker_path(site, account, kind)
}

/// Path of a Posts site's downloader log; `kind` is "stdout" or "stderr".
pub fn posts_download_file(site: &str, kind: &str) -> PathBuf {
    logfile::posts_download_path(site, kind)
}

/// File names currently in the logs folder.
pub fn existing() -> HashSet<String> {
    let Ok(dir) = std::fs::read_dir(logfile::logs_dir()) else { return HashSet::new() };
    dir.flatten().filter_map(|e| e.file_name().into_string().ok()).collect()
}

/// New complete lines of a log file and where the next read should start.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Chunk {
    pub lines: Vec<String>,
    pub offset: u64,
    /// The lines replace what was shown before instead of extending it.
    pub restart: bool,
}

/// Reads the newest `n` complete lines of a file.
pub fn tail(path: &Path, n: usize) -> Result<Chunk, ErrorKind> {
    let mut file = File::open(path).map_err(|e| e.kind())?;
    let len = file.metadata().map_err(|e| e.kind())?.len();
    let (mut start, mut buf, mut newlines) = (len, Vec::new(), 0);
    while start > 0 && newlines <= n {
        let step = CHUNK.min(start);
        start -= step;
        file.seek(SeekFrom::Start(start)).map_err(|e| e.kind())?;
        let mut part = vec![0; step as usize];
        file.read_exact(&mut part).map_err(|e| e.kind())?;
        newlines += part.iter().filter(|b| **b == b'\n').count();
        part.extend_from_slice(&buf);
        buf = part;
    }
    let Some(end) = buf.iter().rposition(|b| *b == b'\n') else {
        return Ok(Chunk { lines: Vec::new(), offset: start, restart: true });
    };
    let mut lines = split_lines(&buf[..end]);
    if start > 0 && !lines.is_empty() {
        lines.remove(0);
    }
    let skip = lines.len().saturating_sub(n);
    lines.drain(..skip);
    Ok(Chunk { lines, offset: start + end as u64 + 1, restart: true })
}

/// Reads the complete lines written after `offset`, or the newest `n` if the file was emptied.
pub fn follow(path: &Path, offset: u64, n: usize) -> Result<Chunk, ErrorKind> {
    let mut file = File::open(path).map_err(|e| e.kind())?;
    let len = file.metadata().map_err(|e| e.kind())?.len();
    if len < offset {
        return tail(path, n);
    }
    let mut buf = vec![0; (len - offset) as usize];
    file.seek(SeekFrom::Start(offset)).map_err(|e| e.kind())?;
    file.read_exact(&mut buf).map_err(|e| e.kind())?;
    let Some(end) = buf.iter().rposition(|b| *b == b'\n') else {
        return Ok(Chunk { lines: Vec::new(), offset, restart: false });
    };
    let mut lines = split_lines(&buf[..end]);
    let restart = lines.len() > n;
    if restart {
        lines.drain(..lines.len() - n);
    }
    Ok(Chunk { lines, offset: offset + end as u64 + 1, restart })
}

/// Reads from the start (`None`) or from a saved offset.
pub fn read(path: &Path, offset: Option<u64>, n: usize) -> Result<Chunk, ErrorKind> {
    match offset {
        Some(o) => follow(path, o, n),
        None => tail(path, n),
    }
}

fn split_lines(bytes: &[u8]) -> Vec<String> {
    String::from_utf8_lossy(bytes).split('\n').map(|l| l.trim_end_matches('\r').to_string()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn temp(name: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!("jj-logtail-{}-{name}.log", std::process::id()));
        let _ = std::fs::remove_file(&path);
        path
    }

    fn append(path: &Path, text: &str) {
        std::fs::OpenOptions::new().create(true).append(true).open(path).unwrap().write_all(text.as_bytes()).unwrap();
    }

    #[test]
    fn tail_returns_the_newest_lines() {
        let path = temp("newest");
        append(&path, "a\nb\nc\nd\n");
        let c = tail(&path, 2).unwrap();
        assert_eq!(c.lines, ["c", "d"]);
        assert_eq!(c.offset, 8);
        assert!(c.restart);
        assert_eq!(tail(&path, 10).unwrap().lines, ["a", "b", "c", "d"]);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn tail_spans_several_chunks() {
        let path = temp("big");
        let text: String = (0..20_000).map(|i| format!("line {i}\n")).collect();
        append(&path, &text);
        let c = tail(&path, 500).unwrap();
        assert_eq!(c.lines.len(), 500);
        assert_eq!(c.lines[0], "line 19500");
        assert_eq!(c.lines[499], "line 19999");
        assert_eq!(c.offset, text.len() as u64);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn tail_holds_back_a_partial_last_line() {
        let path = temp("partial");
        append(&path, "a\nb\npart");
        let c = tail(&path, 5).unwrap();
        assert_eq!(c.lines, ["a", "b"]);
        assert_eq!(c.offset, 4);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn tail_of_a_missing_file_reports_not_found() {
        assert_eq!(tail(&temp("missing"), 5), Err(ErrorKind::NotFound));
    }

    #[test]
    fn follow_returns_only_new_complete_lines() {
        let path = temp("follow");
        append(&path, "a\nb\n");
        let first = tail(&path, 10).unwrap();
        append(&path, "c\nhal");
        let next = follow(&path, first.offset, 10).unwrap();
        assert_eq!(next.lines, ["c"]);
        assert!(!next.restart);
        append(&path, "f\n");
        let last = follow(&path, next.offset, 10).unwrap();
        assert_eq!(last.lines, ["half"]);
        assert!(follow(&path, last.offset, 10).unwrap().lines.is_empty());
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn follow_restarts_after_the_file_is_emptied() {
        let path = temp("trunc");
        append(&path, "old one\nold two\n");
        let first = tail(&path, 10).unwrap();
        std::fs::write(&path, "new\n").unwrap();
        let next = follow(&path, first.offset, 10).unwrap();
        assert_eq!(next.lines, ["new"]);
        assert!(next.restart);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn follow_keeps_only_the_newest_lines_of_a_big_burst() {
        let path = temp("burst");
        append(&path, "a\n");
        let first = tail(&path, 3).unwrap();
        append(&path, "b\nc\nd\ne\nf\n");
        let next = follow(&path, first.offset, 3).unwrap();
        assert_eq!(next.lines, ["d", "e", "f"]);
        assert!(next.restart);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn crlf_and_invalid_utf8_are_tolerated() {
        let path = temp("bytes");
        std::fs::write(&path, b"a\r\nb\xFFc\n").unwrap();
        assert_eq!(tail(&path, 5).unwrap().lines, ["a", "b\u{FFFD}c"]);
        let _ = std::fs::remove_file(&path);
    }
}
