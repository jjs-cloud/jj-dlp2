//! Reads a recording's video resolution and frame rate with ffprobe.

use crate::api::Quality;
use std::path::Path;
use std::process::Stdio;
use std::time::Duration;
use tokio::process::Command;

/// Why ffprobe produced no resolution.
#[derive(Debug, PartialEq, Eq)]
pub enum ProbeError {
    NotFound,
    Failed(String),
}

/// Runs ffprobe on the file and returns its first video stream's resolution and frame rate.
pub async fn quality(path: &Path) -> Result<Quality, ProbeError> {
    let mut cmd = Command::new("ffprobe");
    cmd.args([
        "-v",
        "error",
        "-select_streams",
        "v:0",
        "-show_entries",
        "stream=width,height,avg_frame_rate,r_frame_rate",
        "-of",
        "default=noprint_wrappers=1",
    ])
    .arg(path)
    .stdin(Stdio::null())
    .stdout(Stdio::piped())
    .stderr(Stdio::piped())
    .kill_on_drop(true);
    crate::engine::hide_window(&mut cmd);
    let out = match tokio::time::timeout(Duration::from_secs(20), cmd.output()).await {
        Err(_) => return Err(ProbeError::Failed("timed out".into())),
        Ok(Err(e)) if e.kind() == std::io::ErrorKind::NotFound => return Err(ProbeError::NotFound),
        Ok(Err(e)) => return Err(ProbeError::Failed(e.to_string())),
        Ok(Ok(out)) => out,
    };
    let stdout = String::from_utf8_lossy(&out.stdout);
    parse(&stdout).ok_or_else(|| {
        let err = String::from_utf8_lossy(&out.stderr);
        let line = err.lines().find(|l| !l.trim().is_empty()).unwrap_or("no video stream found");
        ProbeError::Failed(line.trim().to_string())
    })
}

/// Parses ffprobe's `key=value` lines; the average frame rate is preferred over the stream's base rate.
fn parse(text: &str) -> Option<Quality> {
    let (mut width, mut height): (Option<u32>, Option<u32>) = (None, None);
    let (mut avg, mut base): (Option<f64>, Option<f64>) = (None, None);
    for line in text.lines() {
        let Some((key, value)) = line.trim().split_once('=') else { continue };
        match key {
            "width" => width = value.trim().parse().ok(),
            "height" => height = value.trim().parse().ok(),
            "avg_frame_rate" => avg = rate(value),
            "r_frame_rate" => base = rate(value),
            _ => {}
        }
    }
    let (w, h) = (width?, height?);
    (w > 0 && h > 0).then(|| Quality::new(w, h, avg.or(base)))
}

/// Parses a `num/den` frame rate; `0/0` and other unusable values give `None`.
fn rate(text: &str) -> Option<f64> {
    let (n, d) = text.trim().split_once('/')?;
    let (n, d): (f64, f64) = (n.trim().parse().ok()?, d.trim().parse().ok()?);
    (n > 0.0 && d > 0.0).then(|| n / d)
}

#[cfg(test)]
mod tests {
    use super::parse;
    use crate::api::Quality;

    #[test]
    fn parses_resolution_and_frame_rate() {
        let text = "width=1920\nheight=1080\nr_frame_rate=60/1\navg_frame_rate=60/1\n";
        assert_eq!(parse(text), Some(Quality::new(1920, 1080, Some(60.0))));
    }

    #[test]
    fn falls_back_to_the_base_rate_then_to_none() {
        let base = "width=1280\nheight=720\nr_frame_rate=30000/1001\navg_frame_rate=0/0\n";
        assert_eq!(parse(base).and_then(|q| q.fps_rounded()), Some(30));
        let none = "width=640\nheight=1280\nr_frame_rate=0/0\navg_frame_rate=0/0\n";
        assert_eq!(parse(none), Some(Quality::new(640, 1280, None)));
        assert_eq!(parse("width=640\nheight=1280\n"), Some(Quality::new(640, 1280, None)));
    }

    #[test]
    fn rejects_missing_or_empty_sizes() {
        assert_eq!(parse(""), None);
        assert_eq!(parse("width=N/A\nheight=N/A\n"), None);
        assert_eq!(parse("width=0\nheight=0\n"), None);
    }
}
