//! Cancellable Fixup, Trim and Move jobs; ffmpeg is looked up in PATH only.

use super::file_name;
use super::ops::{self, DeleteMode, unique_path};
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;
use std::sync::atomic::{AtomicU8, Ordering};
use std::time::SystemTime;
use tokio::io::AsyncReadExt;
use tokio::process::Command;
use tokio::sync::Notify;

pub const NO_FFMPEG: &str = "ffmpeg not found in PATH";

const RUNNING: u8 = 0;
const CANCELLED: u8 = 1;
const COMMITTING: u8 = 2;

/// Shared cancel switch of one job; it can no longer cancel once the job starts its final steps.
#[derive(Debug, Default)]
pub struct Token {
    state: AtomicU8,
    wake: Notify,
}

impl Token {
    /// Requests cancellation; false when the job is already finishing or cancelled.
    pub fn cancel(&self) -> bool {
        let ok = self.state.compare_exchange(RUNNING, CANCELLED, Ordering::SeqCst, Ordering::SeqCst).is_ok();
        if ok {
            self.wake.notify_one();
        }
        ok
    }

    pub fn is_cancelled(&self) -> bool {
        self.state.load(Ordering::SeqCst) == CANCELLED
    }

    /// True once the job is in its final steps and can no longer be cancelled.
    pub fn finishing(&self) -> bool {
        self.state.load(Ordering::SeqCst) == COMMITTING
    }

    /// Enters the final steps; false when the job was cancelled first.
    fn commit(&self) -> bool {
        self.state.compare_exchange(RUNNING, COMMITTING, Ordering::SeqCst, Ordering::SeqCst).is_ok()
    }

    async fn cancelled(&self) {
        self.wake.notified().await;
    }
}

/// How a job ended; the text is shown in the status line.
#[derive(Debug, Clone, PartialEq)]
pub enum Outcome {
    Done(String),
    Cancelled(String),
    Failed(String),
}

enum Ran {
    Done,
    Cancelled,
    Failed(String),
}

fn os(parts: &[&str]) -> Vec<OsString> {
    parts.iter().map(|p| OsString::from(*p)).collect()
}

/// The extension with its dot (`.ts`), or empty.
fn ext_of(path: &Path) -> String {
    path.extension().map(|e| format!(".{}", e.to_string_lossy())).unwrap_or_default()
}

/// `dir/stem<suffix><ext>` next to `path`.
fn sibling(path: &Path, suffix: &str, ext: &str) -> PathBuf {
    let stem = path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    path.with_file_name(format!("{stem}{suffix}{ext}"))
}

/// Temporary file a move writes before replacing the destination.
pub fn move_tmp(dest: &Path) -> PathBuf {
    sibling(dest, ".moving", &ext_of(dest))
}

fn needs_adts_fix(in_ext: &str, out_ext: &str) -> bool {
    let out = out_ext.to_ascii_lowercase();
    in_ext.eq_ignore_ascii_case(".ts") && [".mp4", ".m4a", ".mov", ".m4v"].contains(&out.as_str())
}

fn push_copy_tail(args: &mut Vec<OsString>, in_ext: &str, output: &Path) {
    args.extend(os(&["-map", "0", "-c", "copy", "-avoid_negative_ts", "make_zero"]));
    if needs_adts_fix(in_ext, &ext_of(output)) {
        args.extend(os(&["-bsf:a", "aac_adtstoasc"]));
    }
    args.push(output.as_os_str().to_owned());
}

/// ffmpeg arguments that rewrite a recording's container without re-encoding.
pub fn fixup_args(input: &Path, output: &Path) -> Vec<OsString> {
    let mut a = os(&["-y", "-hide_banner", "-loglevel", "error", "-nostdin", "-err_detect", "ignore_err", "-fflags", "+genpts", "-i"]);
    a.push(input.as_os_str().to_owned());
    push_copy_tail(&mut a, &ext_of(input), output);
    a
}

/// ffmpeg arguments that cut `start`..`end` (HH:MM:SS) without re-encoding.
pub fn trim_args(input: &Path, output: &Path, start: &str, end: &str) -> Vec<OsString> {
    let mut a = os(&["-y", "-hide_banner", "-loglevel", "error", "-nostdin", "-i"]);
    a.push(input.as_os_str().to_owned());
    a.extend(os(&["-ss", start, "-to", end]));
    push_copy_tail(&mut a, &ext_of(input), output);
    a
}

/// Parses three hour/minute/second digit fields (empty counts as 0) into seconds.
pub fn parse_hms(parts: &[String; 3]) -> Result<u32, String> {
    let n = |s: &str| if s.is_empty() { Ok(0) } else { s.parse::<u32>().map_err(|_| "Time fields must be numbers".to_string()) };
    let (h, m, s) = (n(&parts[0])?, n(&parts[1])?, n(&parts[2])?);
    if m > 59 || s > 59 {
        return Err("Minutes and seconds must be 59 or less".to_string());
    }
    Ok(h * 3600 + m * 60 + s)
}

/// Formats seconds as HH:MM:SS.
pub fn fmt_hms(secs: u32) -> String {
    format!("{:02}:{:02}:{:02}", secs / 3600, secs / 60 % 60, secs % 60)
}

fn last_line(tail: &[u8]) -> Option<String> {
    let text = String::from_utf8_lossy(tail);
    text.split(['\n', '\r']).map(str::trim).filter(|l| !l.is_empty()).last().map(String::from)
}

async fn run_ffmpeg(args: Vec<OsString>, token: &Token) -> Ran {
    let mut cmd = Command::new("ffmpeg");
    cmd.args(&args).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::piped()).kill_on_drop(true);
    crate::engine::hide_window(&mut cmd);
    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ran::Failed(NO_FFMPEG.to_string()),
        Err(e) => return Ran::Failed(format!("could not run ffmpeg ({e})")),
    };
    let stderr = child.stderr.take();
    let reader = tokio::spawn(async move {
        let mut tail: Vec<u8> = Vec::new();
        if let Some(mut s) = stderr {
            let mut buf = [0u8; 4096];
            while let Ok(n) = s.read(&mut buf).await {
                if n == 0 {
                    break;
                }
                tail.extend_from_slice(&buf[..n]);
                if tail.len() > 8192 {
                    let cut = tail.len() - 8192;
                    tail.drain(..cut);
                }
            }
        }
        tail
    });
    tokio::select! {
        status = child.wait() => {
            let tail = reader.await.unwrap_or_default();
            match status {
                Ok(s) if s.success() => Ran::Done,
                Ok(s) => Ran::Failed(last_line(&tail).unwrap_or_else(|| format!("ffmpeg exited with {s}"))),
                Err(e) => Ran::Failed(format!("could not run ffmpeg ({e})")),
            }
        }
        _ = token.cancelled() => {
            let _ = child.kill().await;
            reader.abort();
            Ran::Cancelled
        }
    }
}

fn valid_output(p: &Path) -> bool {
    std::fs::metadata(p).is_ok_and(|m| m.is_file() && m.len() > 0)
}

fn set_mtime(path: &Path, t: SystemTime) {
    if let Ok(f) = std::fs::OpenOptions::new().write(true).open(path) {
        let _ = f.set_modified(t);
    }
}

fn cleanup(p: &Path) {
    let _ = std::fs::remove_file(p);
}

pub struct FixupSpec {
    pub path: PathBuf,
    pub delete_original: bool,
    pub convert_mp4: bool,
}

/// Rewrites a recording with ffmpeg, optionally to MP4 and replacing the original.
pub async fn fixup(spec: FixupSpec, token: Arc<Token>) -> Outcome {
    let name = file_name(&spec.path).into_owned();
    let src_ext = ext_of(&spec.path);
    let out_ext = if spec.convert_mp4 { ".mp4".to_string() } else { src_ext.clone() };
    let same_ext = out_ext.eq_ignore_ascii_case(&src_ext);
    let (work, fin) = if spec.delete_original && same_ext {
        (unique_path(&sibling(&spec.path, ".fixup_tmp", &src_ext)), spec.path.clone())
    } else {
        let w = if same_ext { unique_path(&sibling(&spec.path, "_fixed", &src_ext)) } else { unique_path(&sibling(&spec.path, "", &out_ext)) };
        (w.clone(), w)
    };
    let Ok(mtime) = std::fs::metadata(&spec.path).and_then(|m| m.modified()) else {
        return Outcome::Failed(format!("Fixup failed: cannot read {name}"));
    };
    match run_ffmpeg(fixup_args(&spec.path, &work), &token).await {
        Ran::Done => {}
        Ran::Cancelled => {
            cleanup(&work);
            return Outcome::Cancelled(format!("Fixup cancelled: {name}"));
        }
        Ran::Failed(e) => {
            cleanup(&work);
            return Outcome::Failed(format!("Fixup failed: {e}"));
        }
    }
    if !valid_output(&work) {
        cleanup(&work);
        return Outcome::Failed("Fixup failed: ffmpeg produced no output".to_string());
    }
    if !token.commit() {
        cleanup(&work);
        return Outcome::Cancelled(format!("Fixup cancelled: {name}"));
    }
    set_mtime(&work, mtime);
    if spec.delete_original {
        if let Err(e) = ops::to_trash(&spec.path) {
            return Outcome::Failed(format!("Fixup finished but the original could not be deleted ({e}); output kept as {}", file_name(&work)));
        }
        if work != fin {
            if let Err(e) = std::fs::rename(&work, &fin) {
                return Outcome::Failed(format!("Fixup finished and the original was deleted, but renaming failed ({e}); output is {}", file_name(&work)));
            }
        }
    }
    Outcome::Done(format!("Fixup complete: {}", file_name(&fin)))
}

pub struct TrimSpec {
    pub path: PathBuf,
    pub start: String,
    pub end: String,
    pub delete_original: bool,
    pub convert_mp4: bool,
}

/// Cuts a section out of a recording with ffmpeg into a new `_trimmed` file.
pub async fn trim(spec: TrimSpec, token: Arc<Token>) -> Outcome {
    let name = file_name(&spec.path).into_owned();
    let out_ext = if spec.convert_mp4 { ".mp4".to_string() } else { ext_of(&spec.path) };
    let work = unique_path(&sibling(&spec.path, "_trimmed", &out_ext));
    match run_ffmpeg(trim_args(&spec.path, &work, &spec.start, &spec.end), &token).await {
        Ran::Done => {}
        Ran::Cancelled => {
            cleanup(&work);
            return Outcome::Cancelled(format!("Trim cancelled: {name}"));
        }
        Ran::Failed(e) => {
            cleanup(&work);
            return Outcome::Failed(format!("Trim failed: {e}"));
        }
    }
    if !valid_output(&work) {
        cleanup(&work);
        return Outcome::Failed("Trim failed: ffmpeg produced no output".to_string());
    }
    if !token.commit() {
        cleanup(&work);
        return Outcome::Cancelled(format!("Trim cancelled: {name}"));
    }
    if spec.delete_original {
        if let Err(e) = ops::to_trash(&spec.path) {
            return Outcome::Failed(format!("Trim finished but the original could not be deleted ({e}); output is {}", file_name(&work)));
        }
    }
    Outcome::Done(format!("Trim complete: {}", file_name(&work)))
}

pub struct MoveSpec {
    pub src: PathBuf,
    pub dest: PathBuf,
    pub fixup: bool,
    /// Folder and output root to clean up when the move leaves the folder empty.
    pub cleanup: Option<(PathBuf, PathBuf)>,
}

fn copy_cancellable(src: &Path, dst: &Path, token: &Token) -> std::io::Result<bool> {
    use std::io::{Read, Write};
    let mut input = std::fs::File::open(src)?;
    let mtime = input.metadata()?.modified().ok();
    let mut out = std::fs::File::create(dst)?;
    let mut buf = vec![0u8; 1 << 20];
    loop {
        if token.is_cancelled() {
            drop(out);
            cleanup(dst);
            return Ok(false);
        }
        let n = input.read(&mut buf)?;
        if n == 0 {
            break;
        }
        out.write_all(&buf[..n])?;
    }
    out.flush()?;
    if let Some(t) = mtime {
        let _ = out.set_modified(t);
    }
    Ok(true)
}

fn moved_note(spec: &MoveSpec) -> String {
    let folder_note = spec.cleanup.as_ref().map(|(f, r)| ops::remove_empty_folder(f, r, DeleteMode::Trash)).unwrap_or_default();
    format!("Moved to {}{folder_note}", spec.dest.display())
}

/// Moves a file to `dest`, through ffmpeg when `fixup` is set; copies in chunks across volumes.
pub async fn move_file(spec: MoveSpec, token: Arc<Token>) -> Outcome {
    let name = file_name(&spec.src).into_owned();
    if spec.src == spec.dest {
        return Outcome::Failed("Move failed: source and destination are the same".to_string());
    }
    if let Some(dir) = spec.dest.parent() {
        if let Err(e) = std::fs::create_dir_all(dir) {
            return Outcome::Failed(format!("Move failed: cannot create the destination folder ({e})"));
        }
    }
    let tmp = move_tmp(&spec.dest);
    if spec.fixup {
        let mtime = std::fs::metadata(&spec.src).and_then(|m| m.modified()).ok();
        match run_ffmpeg(fixup_args(&spec.src, &tmp), &token).await {
            Ran::Done => {}
            Ran::Cancelled => {
                cleanup(&tmp);
                return Outcome::Cancelled(format!("Move cancelled: {name}"));
            }
            Ran::Failed(e) => {
                cleanup(&tmp);
                return Outcome::Failed(format!("Move failed: {e}"));
            }
        }
        if !valid_output(&tmp) {
            cleanup(&tmp);
            return Outcome::Failed("Move failed: ffmpeg produced no output".to_string());
        }
        if let Some(t) = mtime {
            set_mtime(&tmp, t);
        }
    } else {
        if std::fs::rename(&spec.src, &spec.dest).is_ok() {
            return Outcome::Done(moved_note(&spec));
        }
        let (s, d, t) = (spec.src.clone(), tmp.clone(), token.clone());
        match tokio::task::spawn_blocking(move || copy_cancellable(&s, &d, &t)).await {
            Ok(Ok(true)) => {}
            Ok(Ok(false)) => return Outcome::Cancelled(format!("Move cancelled: {name}")),
            Ok(Err(e)) => {
                cleanup(&tmp);
                return Outcome::Failed(format!("Move failed: {e}"));
            }
            Err(e) => {
                cleanup(&tmp);
                return Outcome::Failed(format!("Move failed: {e}"));
            }
        }
    }
    if !token.commit() {
        cleanup(&tmp);
        return Outcome::Cancelled(format!("Move cancelled: {name}"));
    }
    if let Err(e) = std::fs::rename(&tmp, &spec.dest) {
        cleanup(&tmp);
        return Outcome::Failed(format!("Move failed: {e}"));
    }
    if let Err(e) = std::fs::remove_file(&spec.src) {
        return Outcome::Failed(format!("Move finished but the original could not be deleted ({e}); the new file is {}", spec.dest.display()));
    }
    Outcome::Done(moved_note(&spec))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn strs(a: &[OsString]) -> Vec<String> {
        a.iter().map(|s| s.to_string_lossy().into_owned()).collect()
    }

    #[test]
    fn fixup_args_add_adts_fix_for_ts_to_mp4() {
        let a = strs(&fixup_args(Path::new("/r/a.ts"), Path::new("/r/a.mp4")));
        assert!(a.windows(2).any(|w| w == ["-bsf:a", "aac_adtstoasc"]));
        assert_eq!(a.last().unwrap(), "/r/a.mp4");
        let b = strs(&fixup_args(Path::new("/r/a.ts"), Path::new("/r/a_fixed.ts")));
        assert!(!b.iter().any(|s| s == "-bsf:a"));
        assert!(b.windows(2).any(|w| w == ["-fflags", "+genpts"]));
    }

    #[test]
    fn trim_args_carry_times() {
        let a = strs(&trim_args(Path::new("/r/a.mkv"), Path::new("/r/a_trimmed.mkv"), "00:01:00", "00:02:30"));
        assert!(a.windows(4).any(|w| w == ["-ss", "00:01:00", "-to", "00:02:30"]));
        assert!(a.windows(2).any(|w| w == ["-c", "copy"]));
    }

    #[test]
    fn hms_parsing() {
        let p = |h: &str, m: &str, s: &str| parse_hms(&[h.to_string(), m.to_string(), s.to_string()]);
        assert_eq!(p("01", "02", "03"), Ok(3723));
        assert_eq!(p("", "", ""), Ok(0));
        assert!(p("", "60", "").is_err());
        assert_eq!(fmt_hms(3723), "01:02:03");
    }

    #[test]
    fn names_and_last_line() {
        assert_eq!(sibling(Path::new("/r/a.b.ts"), ".fixup_tmp", ".ts"), PathBuf::from("/r/a.b.fixup_tmp.ts"));
        assert_eq!(move_tmp(Path::new("/d/x.mp4")), PathBuf::from("/d/x.moving.mp4"));
        assert_eq!(last_line(b"warn\r\n\nbad input\n").as_deref(), Some("bad input"));
        assert_eq!(last_line(b""), None);
    }

    #[test]
    fn token_states() {
        let t = Token::default();
        assert!(t.cancel());
        assert!(t.is_cancelled());
        assert!(!t.commit());
        let t = Token::default();
        assert!(t.commit());
        assert!(t.finishing());
        assert!(!t.cancel());
    }

    #[test]
    fn copy_can_be_cancelled() {
        let dir = std::env::temp_dir().join(format!("jjdlp_copy_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let (src, dst) = (dir.join("s"), dir.join("d"));
        std::fs::write(&src, vec![7u8; 3000]).unwrap();
        let t = Token::default();
        assert!(copy_cancellable(&src, &dst, &t).unwrap());
        assert_eq!(std::fs::read(&dst).unwrap().len(), 3000);
        t.cancel();
        assert!(!copy_cancellable(&src, &dst, &t).unwrap());
        assert!(!dst.exists());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
