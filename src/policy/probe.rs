//! Reads a recording's video resolution with ffprobe.

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

/// Runs ffprobe on the file and returns its first video stream's (width, height).
pub async fn resolution(path: &Path) -> Result<(u32, u32), ProbeError> {
    let mut cmd = Command::new("ffprobe");
    cmd.args(["-v", "error", "-select_streams", "v:0", "-show_entries", "stream=width,height", "-of", "csv=p=0"])
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

/// Parses ffprobe's `width,height` output.
fn parse(text: &str) -> Option<(u32, u32)> {
    let (w, h) = text.lines().next()?.trim().split_once(',')?;
    let (w, h) = (w.trim().parse().ok()?, h.trim().parse().ok()?);
    (w > 0 && h > 0).then_some((w, h))
}

#[cfg(test)]
mod tests {
    use super::parse;

    #[test]
    fn parses_width_and_height() {
        assert_eq!(parse("1920,1080\n"), Some((1920, 1080)));
        assert_eq!(parse(" 1280 , 720 "), Some((1280, 720)));
        assert_eq!(parse(""), None);
        assert_eq!(parse("N/A,N/A"), None);
        assert_eq!(parse("0,0"), None);
    }
}
