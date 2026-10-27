//! Recording output paths: ffmpeg's output argument, the yt-dlp sidecar file, and path comparison.

use crate::api::{RecorderId, SidecarPath};
use crate::settings;
use std::ffi::{OsStr, OsString};
use std::path::{Component, Path, PathBuf};

/// Sidecar print forms as (tag, yt-dlp WHEN, template); only stages at or before download start can join the live check.
const FORMS: &[(&str, &str, &str)] = &[("BDP", "before_dl", "%(filepath)s"), ("BDN", "before_dl", "%(filename)s")];
/// Tag of the form built from the user's own `-o` template.
const TEMPLATE_TAG: &str = "BDT";
/// Output-template type prefixes that can precede a `-o` value.
const OUTPUT_KINDS: &[&str] = &[
    "default",
    "subtitle",
    "thumbnail",
    "description",
    "annotation",
    "infojson",
    "link",
    "pl_thumbnail",
    "pl_description",
    "pl_infojson",
    "chapter",
    "pl_video",
];

fn sidecar_dir() -> PathBuf {
    settings::config_dir().join("sidecars")
}

/// Deletes every leftover sidecar file.
pub fn clear_sidecars() {
    let _ = std::fs::remove_dir_all(sidecar_dir());
}

/// Creates an empty sidecar file for a recorder and returns its absolute path.
pub fn prepare_sidecar(id: RecorderId) -> Option<PathBuf> {
    let dir = sidecar_dir();
    std::fs::create_dir_all(&dir).ok()?;
    let path = dir.join(format!("{id}.txt"));
    std::fs::write(&path, b"").ok()?;
    Some(std::path::absolute(&path).unwrap_or(path))
}

pub fn remove_sidecar(path: &Path) {
    let _ = std::fs::remove_file(path);
}

/// Extra yt-dlp arguments that append each print form's path to the sidecar file.
pub fn sidecar_args(args: &[String], sidecar: &Path) -> Vec<String> {
    let Some(file) = sidecar.to_str() else { return Vec::new() };
    let file = file.replace('%', "%%");
    let mut specs: Vec<String> =
        FORMS.iter().map(|(tag, when, template)| format!("{when}:{tag}|{template}")).collect();
    if let Some(template) = output_template(args) {
        specs.push(format!("before_dl:{TEMPLATE_TAG}|{template}"));
    }
    specs.into_iter().flat_map(|spec| ["--print-to-file".to_string(), spec, file.clone()]).collect()
}

/// The last plain `-o` / `--output` template in the arguments.
fn output_template(args: &[String]) -> Option<String> {
    let mut found = None;
    let mut i = 0;
    while i < args.len() {
        let arg = args[i].as_str();
        let value = if arg == "-o" || arg == "--output" {
            i += 1;
            args.get(i).map(String::as_str)
        } else {
            arg.strip_prefix("--output=").or_else(|| arg.strip_prefix("-o"))
        };
        if let Some(template) = value.and_then(plain_template) {
            found = Some(template.to_string());
        }
        i += 1;
    }
    found
}

/// Drops a `default:` prefix and rejects values for other output types.
fn plain_template(value: &str) -> Option<&str> {
    match value.split_once(':') {
        Some((kind, rest)) if OUTPUT_KINDS.contains(&kind) => (kind == "default").then_some(rest),
        _ => Some(value),
    }
}

/// Reads the sidecar file; a missing or unreadable file yields no paths.
pub fn read_sidecar(path: &Path, cwd: &Path) -> Vec<SidecarPath> {
    std::fs::read(path).map(|bytes| parse_sidecar(&bytes, cwd)).unwrap_or_default()
}

/// Parses `TAG|path` lines, resolving paths against `cwd` and skipping invalid UTF-8 and empty values.
fn parse_sidecar(bytes: &[u8], cwd: &Path) -> Vec<SidecarPath> {
    bytes
        .split(|b| *b == b'\n')
        .filter_map(|raw| {
            let line = std::str::from_utf8(raw).ok()?.trim_end_matches('\r');
            let (tag, path) = line.split_once('|')?;
            if tag.is_empty() || path.is_empty() || path == "NA" {
                return None;
            }
            Some(SidecarPath { tag: tag.to_string(), path: normalize(cwd, Path::new(path)) })
        })
        .collect()
}

/// The output file named by an ffmpeg command line, resolved against `cwd`; `None` for pipes and URLs.
pub fn ffmpeg_output(cmd: &[OsString], cwd: &Path) -> Option<PathBuf> {
    if cmd.len() < 2 {
        return None;
    }
    let arg = output_arg(cmd.last()?)?;
    Some(normalize(cwd, Path::new(&arg)))
}

/// Strips ffmpeg's `file:` prefix; rejects options, stdout, pipes and network URLs.
fn output_arg(arg: &OsStr) -> Option<OsString> {
    if let Some(rest) = strip_prefix(arg, "file:") {
        return Some(rest);
    }
    let text = arg.to_string_lossy();
    if text.starts_with('-') || text.starts_with("pipe:") || text.contains("://") {
        return None;
    }
    Some(arg.to_os_string())
}

#[cfg(unix)]
fn strip_prefix(arg: &OsStr, prefix: &str) -> Option<OsString> {
    use std::os::unix::ffi::OsStrExt;
    arg.as_bytes().strip_prefix(prefix.as_bytes()).map(|rest| OsStr::from_bytes(rest).to_os_string())
}

#[cfg(windows)]
fn strip_prefix(arg: &OsStr, prefix: &str) -> Option<OsString> {
    use std::os::windows::ffi::{OsStrExt, OsStringExt};
    let wide: Vec<u16> = arg.encode_wide().collect();
    let pre: Vec<u16> = prefix.encode_utf16().collect();
    wide.strip_prefix(pre.as_slice()).map(OsString::from_wide)
}

/// Joins a relative path onto `base` and removes `.`, `..` and repeated separators without touching the filesystem.
pub fn normalize(base: &Path, path: &Path) -> PathBuf {
    let joined = if path.is_absolute() { path.to_path_buf() } else { base.join(path) };
    let mut out = PathBuf::new();
    for part in joined.components() {
        match part {
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            other => out.push(other.as_os_str()),
        }
    }
    strip_verbatim(out)
}

/// Rewrites `\\?\C:\x` as `C:\x` and `\\?\UNC\s\x` as `\\s\x`.
#[cfg(windows)]
fn strip_verbatim(path: PathBuf) -> PathBuf {
    let Some(text) = path.to_str() else { return path };
    if let Some(rest) = text.strip_prefix(r"\\?\UNC\") {
        return PathBuf::from(format!(r"\\{rest}"));
    }
    match text.strip_prefix(r"\\?\") {
        Some(rest) => PathBuf::from(rest),
        None => path,
    }
}

#[cfg(not(windows))]
fn strip_verbatim(path: PathBuf) -> PathBuf {
    path
}

/// True when two normalised paths name the same file; Windows ignores case.
pub fn same_path(a: &Path, b: &Path) -> bool {
    if a == b {
        return true;
    }
    if cfg!(windows) {
        if let (Some(x), Some(y)) = (a.to_str(), b.to_str()) {
            return x.to_lowercase() == y.to_lowercase();
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    fn os(parts: &[&str]) -> Vec<OsString> {
        parts.iter().map(OsString::from).collect()
    }

    #[test]
    fn normalize_resolves_relative_dots_and_repeats() {
        let base = Path::new("/rec");
        assert_eq!(normalize(base, Path::new("a/./b//c.ts")), PathBuf::from("/rec/a/b/c.ts"));
        assert_eq!(normalize(base, Path::new("../x.ts")), PathBuf::from("/x.ts"));
        assert_eq!(normalize(base, Path::new("/abs/x.ts")), PathBuf::from("/abs/x.ts"));
    }

    #[test]
    fn ffmpeg_output_keeps_unicode_names_intact() {
        let cmd = os(&["ffmpeg", "-i", "https://x/y.m3u8", "-f", "mpegts", "file:主播 🎬 [vid].ts"]);
        assert_eq!(ffmpeg_output(&cmd, Path::new("/rec")), Some(PathBuf::from("/rec/主播 🎬 [vid].ts")));
    }

    #[test]
    fn ffmpeg_output_ignores_pipes_options_and_urls() {
        let cwd = Path::new("/rec");
        assert_eq!(ffmpeg_output(&os(&["ffmpeg", "-i", "x", "-"]), cwd), None);
        assert_eq!(ffmpeg_output(&os(&["ffmpeg", "-i", "x", "pipe:1"]), cwd), None);
        assert_eq!(ffmpeg_output(&os(&["ffmpeg", "-i", "x", "-f", "flv", "rtmp://h/app"]), cwd), None);
        assert_eq!(ffmpeg_output(&os(&["ffmpeg", "-i", "x", "-y"]), cwd), None);
        assert_eq!(ffmpeg_output(&os(&["ffmpeg"]), cwd), None);
    }

    #[test]
    fn ffmpeg_output_accepts_file_prefix_with_leading_dash() {
        let cmd = os(&["ffmpeg", "-i", "x", "file:-weird.ts"]);
        assert_eq!(ffmpeg_output(&cmd, Path::new("/rec")), Some(PathBuf::from("/rec/-weird.ts")));
    }

    #[test]
    fn sidecar_lines_parse_with_cjk_and_skip_empty_values() {
        let text = "BDP|主播 2026.ts\nBDN|NA\nBDT|\r\nbad line\nBDN|/abs/a b.ts\r\n";
        let got = parse_sidecar(text.as_bytes(), Path::new("/rec"));
        let want = vec![
            SidecarPath { tag: "BDP".into(), path: PathBuf::from("/rec/主播 2026.ts") },
            SidecarPath { tag: "BDN".into(), path: PathBuf::from("/abs/a b.ts") },
        ];
        assert_eq!(got, want);
    }

    #[test]
    fn sidecar_parse_skips_invalid_utf8_lines() {
        let mut bytes = b"BDP|ok.ts\nBDN|".to_vec();
        bytes.extend([0xff, 0xfe]);
        bytes.extend(b"\n");
        let got = parse_sidecar(&bytes, Path::new("/rec"));
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].tag, "BDP");
    }

    #[test]
    fn sidecar_args_escape_percent_and_include_output_template() {
        let args: Vec<String> = ["--no-part", "-o", "%(title)s.%(ext)s", "URL"].iter().map(|s| s.to_string()).collect();
        let out = sidecar_args(&args, Path::new("/cfg/50%/1.txt"));
        assert_eq!(out.len(), 9);
        assert_eq!(&out[0..3], ["--print-to-file", "before_dl:BDP|%(filepath)s", "/cfg/50%%/1.txt"]);
        assert_eq!(out[7], "before_dl:BDT|%(title)s.%(ext)s");
    }

    #[test]
    fn output_template_handles_types_and_equals_form() {
        let a = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(output_template(&a(&["-o", "thumbnail:t.%(ext)s", "-o", "x.%(ext)s"])), Some("x.%(ext)s".into()));
        assert_eq!(output_template(&a(&["--output=default:y.%(ext)s"])), Some("y.%(ext)s".into()));
        assert_eq!(output_template(&a(&["-o", "C:\\rec\\z.%(ext)s"])), Some("C:\\rec\\z.%(ext)s".into()));
        assert_eq!(output_template(&a(&["-o", "infojson:i.json"])), None);
        assert_eq!(output_template(&a(&["--no-part"])), None);
    }

    #[test]
    fn same_path_compares_exactly_on_unix() {
        assert!(same_path(Path::new("/a/b.ts"), Path::new("/a/b.ts")));
        assert_eq!(same_path(Path::new("/a/B.ts"), Path::new("/a/b.ts")), cfg!(windows));
    }
}
