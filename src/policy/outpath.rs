//! Recording output paths: the sidecar override sent to the engine, ffmpeg's output argument, and path comparison.

use super::layout;
use crate::api::StreamerId;
use crate::settings;
use std::ffi::{OsStr, OsString};
use std::path::{Component, Path, PathBuf};

/// A path yt-dlp reported through the sidecar file, tagged with the print form that produced it.
#[derive(Debug, Clone, PartialEq)]
pub struct SidecarPath {
    pub tag: String,
    pub path: PathBuf,
}

/// Sidecar print forms as (tag, yt-dlp WHEN, template); only stages at or before download start can join the live check.
const FORMS: &[(&str, &str, &str)] = &[("BDP", "before_dl", "%(filepath)s"), ("BDN", "before_dl", "%(filename)s")];
/// Tag of the form built from the user's own `--output` template.
const TEMPLATE_TAG: &str = "BDT";
/// yt-dlp's default output template, used when the arguments have no `--output`.
const DEFAULT_TEMPLATE: &str = "%(title)s [%(id)s].%(ext)s";
/// Output-template type prefixes that can precede a `--output` value.
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

/// Absolute path of a streamer's sidecar file.
pub fn sidecar_path(streamer_id: StreamerId) -> PathBuf {
    let path = sidecar_dir().join(format!("{streamer_id}.txt"));
    std::path::absolute(&path).unwrap_or(path)
}

/// Deletes every leftover sidecar file.
pub fn clear_sidecars() {
    let _ = std::fs::remove_dir_all(sidecar_dir());
}

/// Creates the sidecar folder so yt-dlp can append to files in it.
pub fn ensure_sidecar_dir() {
    let _ = std::fs::create_dir_all(sidecar_dir());
}

/// Deletes a streamer's sidecar file so the next recording starts empty.
pub fn remove_sidecar(streamer_id: StreamerId) {
    let _ = std::fs::remove_file(sidecar_path(streamer_id));
}

/// The recorder arguments to hand the engine: the site's own arguments plus the folder `--output` and sidecar options.
pub fn recorder_override(recorder_args: &str, dirs: &[String], sidecar: &Path) -> Option<String> {
    let mut args = split_args(recorder_args);
    let mut extra = folder_args(&args, dirs);
    args.extend(extra.iter().cloned());
    extra.extend(sidecar_args(&args, sidecar));
    if extra.is_empty() {
        return None;
    }
    let mut out = recorder_args.trim().to_string();
    for arg in extra {
        out.push(' ');
        out.push_str(&quote_arg(&arg));
    }
    Some(out.trim_start().to_string())
}

/// A trailing `--output` that nests the output under `dirs`; empty when there are no folders or the template is absolute.
fn folder_args(args: &[String], dirs: &[String]) -> Vec<String> {
    if dirs.is_empty() {
        return Vec::new();
    }
    let template = output_template(args).unwrap_or_else(|| DEFAULT_TEMPLATE.to_string());
    if layout::is_absolute_template(&template) {
        return Vec::new();
    }
    let prefix: String = dirs.iter().map(|d| format!("{}/", d.replace('%', "%%"))).collect();
    vec!["--output".to_string(), format!("{prefix}{template}")]
}

/// Extra yt-dlp arguments that append each print form's path to the sidecar file.
fn sidecar_args(args: &[String], sidecar: &Path) -> Vec<String> {
    let Some(file) = sidecar.to_str() else { return Vec::new() };
    let file = file.replace('%', "%%");
    let mut specs: Vec<String> =
        FORMS.iter().map(|(tag, when, template)| format!("{when}:{tag}|{template}")).collect();
    if let Some(template) = output_template(args) {
        specs.push(format!("before_dl:{TEMPLATE_TAG}|{template}"));
    }
    specs.into_iter().flat_map(|spec| ["--print-to-file".to_string(), spec, file.clone()]).collect()
}

/// Splits on whitespace honouring "double" and 'single' quotes, the way the engine reads recorder arguments.
pub(super) fn split_args(input: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut in_token = false;
    let mut quote: Option<char> = None;
    let mut chars = input.chars().peekable();
    while let Some(c) = chars.next() {
        match quote {
            Some(q) => {
                if c == q {
                    quote = None;
                } else if c == '\\' && q == '"' && chars.peek() == Some(&'"') {
                    cur.push('"');
                    chars.next();
                } else {
                    cur.push(c);
                }
            }
            None => {
                if c == '"' || c == '\'' {
                    quote = Some(c);
                    in_token = true;
                } else if c.is_whitespace() {
                    if in_token {
                        out.push(std::mem::take(&mut cur));
                        in_token = false;
                    }
                } else {
                    cur.push(c);
                    in_token = true;
                }
            }
        }
    }
    if in_token {
        out.push(cur);
    }
    out
}

/// Quotes one argument so `split_args` returns it unchanged.
fn quote_arg(arg: &str) -> String {
    let pieces: Vec<String> = arg.split('\'').map(|p| format!("'{p}'")).collect();
    pieces.join("\"'\"")
}

/// The last plain output template in the arguments (`--output`, or user-typed `-o`).
pub(super) fn output_template(args: &[String]) -> Option<String> {
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

/// Reads a streamer's sidecar file; a missing or unreadable file yields no paths.
pub fn read_sidecar(streamer_id: StreamerId, cwd: &Path) -> Vec<SidecarPath> {
    std::fs::read(sidecar_path(streamer_id)).map(|bytes| parse_sidecar(&bytes, cwd)).unwrap_or_default()
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

    fn strings(parts: &[&str]) -> Vec<String> {
        parts.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn split_args_honours_quotes() {
        assert_eq!(split_args(r#"--output "a b/%(id)s" --x 'y z'"#), ["--output", "a b/%(id)s", "--x", "y z"]);
    }

    #[test]
    fn quote_arg_round_trips_awkward_values() {
        for value in ["plain", "a b", "it's", "say \"hi\"", "C:\\rec\\", "'", "", "%(title)s | x", "主播 🎬"] {
            assert_eq!(split_args(&quote_arg(value)), [value], "{value:?}");
        }
    }

    #[test]
    fn override_appends_quoted_sidecar_options_after_the_users_arguments() {
        let got = recorder_override("--no-part --output \"%(title)s.%(ext)s\" {url}", &[], Path::new("/cfg/50%/3.txt")).unwrap();
        assert!(got.starts_with("--no-part --output \"%(title)s.%(ext)s\" {url} '--print-to-file' "), "{got}");
        let args = split_args(&got);
        assert_eq!(args.len(), 4 + 9);
        assert_eq!(&args[4..7], ["--print-to-file", "before_dl:BDP|%(filepath)s", "/cfg/50%%/3.txt"]);
        assert_eq!(args[11], "before_dl:BDT|%(title)s.%(ext)s");
    }

    #[test]
    fn override_works_when_the_site_has_no_arguments() {
        let got = recorder_override("  ", &[], Path::new("/cfg/3.txt")).unwrap();
        assert!(got.starts_with("'--print-to-file' "), "{got}");
        assert_eq!(split_args(&got).len(), 6);
    }

    #[test]
    fn override_nests_the_users_template_under_the_folders() {
        let dirs = strings(&["Twitch", "Emiru"]);
        let got = recorder_override("--no-part --output \"%(title)s.%(ext)s\" {url}", &dirs, Path::new("/cfg/3.txt")).unwrap();
        let args = split_args(&got);
        assert_eq!(args.len(), 4 + 2 + 9);
        assert_eq!(&args[4..6], ["--output", "Twitch/Emiru/%(title)s.%(ext)s"]);
        assert_eq!(args[13], "before_dl:BDT|Twitch/Emiru/%(title)s.%(ext)s");
    }

    #[test]
    fn override_uses_the_default_template_when_there_is_no_output_option() {
        let got = recorder_override("--no-part {url}", &strings(&["Emiru"]), Path::new("/cfg/3.txt")).unwrap();
        let args = split_args(&got);
        assert_eq!(&args[2..4], ["--output", "Emiru/%(title)s [%(id)s].%(ext)s"]);
    }

    #[test]
    fn override_escapes_percent_signs_in_folder_names() {
        let got = recorder_override("--output \"%(id)s.%(ext)s\"", &strings(&["50%"]), Path::new("/cfg/3.txt")).unwrap();
        assert_eq!(split_args(&got)[2..4], ["--output", "50%%/%(id)s.%(ext)s"]);
    }

    #[test]
    fn override_leaves_absolute_templates_alone() {
        let args_in = "--output \"D:\\rec\\%(id)s.%(ext)s\" {url}";
        let got = recorder_override(args_in, &strings(&["Emiru"]), Path::new("/cfg/3.txt")).unwrap();
        assert_eq!(split_args(&got).len(), 3 + 9);
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
    fn output_template_handles_types_and_equals_form() {
        assert_eq!(output_template(&strings(&["--output", "thumbnail:t.%(ext)s", "--output", "x.%(ext)s"])), Some("x.%(ext)s".into()));
        assert_eq!(output_template(&strings(&["--output=default:y.%(ext)s"])), Some("y.%(ext)s".into()));
        assert_eq!(output_template(&strings(&["--output", "C:\\rec\\z.%(ext)s"])), Some("C:\\rec\\z.%(ext)s".into()));
        assert_eq!(output_template(&strings(&["--output", "infojson:i.json"])), None);
        assert_eq!(output_template(&strings(&["-o", "s.%(ext)s"])), Some("s.%(ext)s".into()));
        assert_eq!(output_template(&strings(&["--no-part"])), None);
    }

    #[test]
    fn same_path_compares_exactly_on_unix() {
        assert!(same_path(Path::new("/a/b.ts"), Path::new("/a/b.ts")));
        assert_eq!(same_path(Path::new("/a/B.ts"), Path::new("/a/b.ts")), cfg!(windows));
    }
}
