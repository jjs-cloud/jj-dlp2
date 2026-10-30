//! Folder layouts: the subfolders recordings go in and the path preview shown in settings.

use super::outpath;
use crate::api::FolderLayout;
use std::path::PathBuf;

/// File name shown at the end of a previewed path.
const PREVIEW_FILE: &str = "<filename>.mp4";

/// Windows device names that cannot be used as folder names.
const RESERVED: &[&str] = &[
    "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8", "COM9", "LPT1", "LPT2",
    "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
];

/// A site or streamer name made safe to use as one folder name.
pub fn folder_name(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| if c.is_control() || "<>:\"/\\|?*".contains(c) { '_' } else { c })
        .collect();
    let trimmed = cleaned.trim().trim_end_matches(['.', ' ']);
    if trimmed.is_empty() {
        return "_".to_string();
    }
    let stem = trimmed.split('.').next().unwrap_or("").trim_end();
    if RESERVED.iter().any(|r| r.eq_ignore_ascii_case(stem)) {
        format!("_{trimmed}")
    } else {
        trimmed.to_string()
    }
}

/// Folder names a layout nests recordings under, outermost first.
pub fn dirs(layout: FolderLayout, site: &str, streamer: &str) -> Vec<String> {
    nest(layout, folder_name(site), folder_name(streamer))
}

/// Orders already-sanitized site and streamer folder names per the layout.
fn nest(layout: FolderLayout, site: String, streamer: String) -> Vec<String> {
    match layout {
        FolderLayout::Flat => Vec::new(),
        FolderLayout::Streamer => vec![streamer],
        FolderLayout::Site => vec![site],
        FolderLayout::StreamerSite => vec![streamer, site],
        FolderLayout::SiteStreamer => vec![site, streamer],
    }
}

/// The site's output folder, or the global one when the site's is blank.
pub fn base_dir<'a>(site_dir: &'a str, global_dir: &'a str) -> &'a str {
    let site_dir = site_dir.trim();
    if site_dir.is_empty() { global_dir.trim() } else { site_dir }
}

/// The path a recording would be written to, ending in a placeholder file name; `None` shows `<streamer>`.
pub fn preview(base: &str, layout: FolderLayout, site: &str, streamer: Option<&str>) -> String {
    let base = PathBuf::from(base);
    let mut path = std::path::absolute(&base).unwrap_or(base);
    let streamer = streamer.map_or_else(|| "<streamer>".to_string(), folder_name);
    path.extend(nest(layout, folder_name(site), streamer));
    path.push(PREVIEW_FILE);
    path.display().to_string()
}

/// True when an output template is an absolute path (drive, root or home).
pub(super) fn is_absolute_template(template: &str) -> bool {
    let b = template.as_bytes();
    template.starts_with(['/', '\\', '~']) || (b.len() >= 2 && b[0].is_ascii_alphabetic() && b[1] == b':')
}

/// True when the recorder arguments' `--output` template is absolute, so subfolders cannot apply.
pub fn has_absolute_template(recorder_args: &str) -> bool {
    outpath::output_template(&outpath::split_args(recorder_args)).is_some_and(|t| is_absolute_template(&t))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn folder_name_replaces_invalid_characters() {
        assert_eq!(folder_name(" a<b>c:d\"e/f\\g|h?i*j "), "a_b_c_d_e_f_g_h_i_j");
    }

    #[test]
    fn folder_name_handles_trailing_dots_and_blanks() {
        assert_eq!(folder_name("name. . "), "name");
        assert_eq!(folder_name(".."), "_");
        assert_eq!(folder_name("   "), "_");
    }

    #[test]
    fn folder_name_avoids_reserved_device_names() {
        assert_eq!(folder_name("con"), "_con");
        assert_eq!(folder_name("NUL.txt"), "_NUL.txt");
        assert_eq!(folder_name("lpt9"), "_lpt9");
        assert_eq!(folder_name("console"), "console");
    }

    #[test]
    fn dirs_follow_the_layout_order() {
        let d = |l| dirs(l, "Twitch", "Emiru");
        assert!(d(FolderLayout::Flat).is_empty());
        assert_eq!(d(FolderLayout::Streamer), ["Emiru"]);
        assert_eq!(d(FolderLayout::Site), ["Twitch"]);
        assert_eq!(d(FolderLayout::StreamerSite), ["Emiru", "Twitch"]);
        assert_eq!(d(FolderLayout::SiteStreamer), ["Twitch", "Emiru"]);
    }

    #[test]
    fn base_dir_falls_back_to_global() {
        assert_eq!(base_dir(" /site ", "/global"), "/site");
        assert_eq!(base_dir("  ", " /global "), "/global");
    }

    #[test]
    fn preview_ends_with_nested_folders_and_placeholder() {
        let sep = std::path::MAIN_SEPARATOR;
        let p = preview("rec", FolderLayout::SiteStreamer, "Twitch", Some("Emiru"));
        assert!(p.ends_with(&format!("{sep}Twitch{sep}Emiru{sep}<filename>.mp4")), "{p}");
        let generic = preview("rec", FolderLayout::SiteStreamer, "Twitch", None);
        assert!(generic.ends_with(&format!("{sep}Twitch{sep}<streamer>{sep}<filename>.mp4")), "{generic}");
        let flat = preview("rec", FolderLayout::Flat, "Twitch", Some("Emiru"));
        assert!(flat.ends_with(&format!("{sep}rec{sep}<filename>.mp4")), "{flat}");
    }

    #[test]
    fn absolute_templates_are_detected() {
        assert!(is_absolute_template("D:\\x\\%(title)s.%(ext)s"));
        assert!(is_absolute_template("C:/x/%(title)s.%(ext)s"));
        assert!(is_absolute_template("/var/rec/%(title)s.%(ext)s"));
        assert!(is_absolute_template("~/rec/%(title)s.%(ext)s"));
        assert!(!is_absolute_template("%(title)s.%(ext)s"));
        assert!(!is_absolute_template("sub/%(title)s.%(ext)s"));
    }

    #[test]
    fn has_absolute_template_reads_the_recorder_arguments() {
        assert!(has_absolute_template("--no-part --output \"D:\\x\\%(title)s.%(ext)s\" {url}"));
        assert!(!has_absolute_template("--no-part --output \"%(title)s.%(ext)s\" {url}"));
        assert!(!has_absolute_template("--no-part {url}"));
    }
}
