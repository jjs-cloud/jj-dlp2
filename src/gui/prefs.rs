//! GUI-only preferences stored in gui.json.

use super::skin::ThemeKind;
use crate::resets::{reset_field, GuiField, Key, Reset};
use serde::{Deserialize, Serialize};

pub const ROW_MIN_LIMIT: f32 = 15.0;
pub const ROW_MAX_LIMIT: f32 = 48.0;
pub const ROW_MIN_DEFAULT: f32 = 15.0;
pub const ROW_MAX_DEFAULT: f32 = 31.0;
pub const COL_W_MIN_LIMIT: f32 = 300.0;
pub const COL_W_MAX_LIMIT: f32 = 1300.0;
pub const COMPACT_COL_W_DEFAULT: f32 = 540.0;
pub const FULL_COL_W_DEFAULT: f32 = 850.0;
pub const UI_SCALE_MIN: f32 = 0.7;
pub const UI_SCALE_MAX: f32 = 2.0;
pub const UI_SCALE_DEFAULT: f32 = 1.0;
const AUTO_SCALE_MIN: f32 = 0.8;
const AUTO_SCALE_MAX: f32 = 1.5;
const AUTO_REF_W: f32 = 1920.0;
const AUTO_REF_H: f32 = 1080.0;
pub const NAME_CLIP_MIN_LIMIT: f32 = 6.0;
pub const NAME_CLIP_MAX_LIMIT: f32 = 30.0;
pub const NAME_CLIP_DEFAULT: f32 = 16.0;
pub const LOG_LINES_MIN_LIMIT: f32 = 100.0;
pub const LOG_LINES_MAX_LIMIT: f32 = crate::api::LOG_HISTORY_MAX as f32;
pub const LOG_LINES_DEFAULT: f32 = 500.0;
pub const CARD_W_MIN: f32 = 120.0;
pub const CARD_W_MAX: f32 = 260.0;
const CARD_W_DEFAULT: f32 = 170.0;
pub const WIN_DEFAULT_W: f32 = 1180.0;
pub const WIN_DEFAULT_H: f32 = 845.0;
const WIN_MIN_W: f32 = 640.0;
const WIN_MIN_H: f32 = 420.0;
/// Logical pixels kept free around the window for the title bar and taskbar.
const FIT_MARGIN_W: f32 = 40.0;
const FIT_MARGIN_H: f32 = 100.0;

/// How streamers are ordered within a site (and within the live group).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SortKey {
    Name,
    Added,
    #[default]
    LastLive,
    Priority,
}

impl SortKey {
    pub const ALL: [SortKey; 4] = [SortKey::Name, SortKey::Added, SortKey::LastLive, SortKey::Priority];

    fn id(self) -> &'static str {
        match self {
            SortKey::Name => "name",
            SortKey::Added => "added",
            SortKey::LastLive => "last_live",
            SortKey::Priority => "priority",
        }
    }

    fn from_id(id: &str) -> Self {
        Self::ALL.into_iter().find(|k| k.id() == id).unwrap_or_default()
    }
}

impl std::fmt::Display for SortKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            SortKey::Name => "Name",
            SortKey::Added => "Added",
            SortKey::LastLive => "Last live",
            SortKey::Priority => "Priority",
        })
    }
}

/// A row of the Sort dropdown: the Live first and Group by site switches, a divider, or a sort key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortItem {
    LiveFirst(bool),
    GroupBySite(bool),
    Divider,
    Key(SortKey),
}

impl SortItem {
    /// The dropdown's rows: Live first, Group by site, a divider, then every sort key.
    pub fn rows(live_first: bool, group_by_site: bool) -> Vec<SortItem> {
        let mut rows = vec![SortItem::LiveFirst(live_first), SortItem::GroupBySite(group_by_site), SortItem::Divider];
        rows.extend(SortKey::ALL.map(SortItem::Key));
        rows
    }
}

impl std::fmt::Display for SortItem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SortItem::LiveFirst(true) => f.write_str("Live first ✓"),
            SortItem::LiveFirst(false) => f.write_str("Live first"),
            SortItem::GroupBySite(true) => f.write_str("Group by site ✓"),
            SortItem::GroupBySite(false) => f.write_str("Group by site"),
            SortItem::Divider => f.write_str("──────────"),
            SortItem::Key(k) => std::fmt::Display::fmt(k, f),
        }
    }
}

#[derive(Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
struct Prefs {
    live_first: bool,
    group_by_site: bool,
    sort_key: String,
    sort_asc: bool,
    theme: String,
    compact: bool,
    row_min: f32,
    row_max: f32,
    compact_col_w: f32,
    full_col_w: f32,
    ui_scale_auto: bool,
    ui_scale: f32,
    auto_scale: f32,
    name_clip: f32,
    log_lines: f32,
    remember_size: bool,
    win_w: f32,
    win_h: f32,
    wrap_output: bool,
    wrap_logs: bool,
    files: FilesPrefs,
    posts: PostsPrefs,
    extra_drives: Vec<String>,
    drive_order: Vec<String>,
    disk_limits: crate::policy::disks::Limits,
}

/// Preferences of the Files tab.
#[derive(Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct FilesPrefs {
    pub sort: String,
    pub asc: bool,
    pub group_by: String,
    pub delete_empty: bool,
    pub hide_posts: bool,
    pub move_dir: String,
}

/// Preferences of the Posts tab.
#[derive(Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PostsPrefs {
    pub newest_first: bool,
    pub card_w: f32,
}

impl Default for PostsPrefs {
    fn default() -> Self {
        PostsPrefs { newest_first: true, card_w: CARD_W_DEFAULT }
    }
}

impl Default for FilesPrefs {
    fn default() -> Self {
        FilesPrefs { sort: "name".to_string(), asc: true, group_by: "streamer".to_string(), delete_empty: true, hide_posts: false, move_dir: String::new() }
    }
}

impl Default for Prefs {
    fn default() -> Self {
        Prefs { live_first: true, group_by_site: false, sort_key: SortKey::LastLive.id().to_string(), sort_asc: false, theme: String::new(), compact: false, row_min: ROW_MIN_DEFAULT, row_max: ROW_MAX_DEFAULT, compact_col_w: COMPACT_COL_W_DEFAULT,
            full_col_w: FULL_COL_W_DEFAULT,
            ui_scale_auto: true,
            ui_scale: UI_SCALE_DEFAULT,
            auto_scale: UI_SCALE_DEFAULT,
            name_clip: NAME_CLIP_DEFAULT,
            log_lines: LOG_LINES_DEFAULT,
            remember_size: false,
            win_w: WIN_DEFAULT_W,
            win_h: WIN_DEFAULT_H,
            wrap_output: true,
            wrap_logs: true,
            files: FilesPrefs::default(),
            posts: PostsPrefs::default(),
            extra_drives: Vec::new(),
            drive_order: Vec::new(),
            disk_limits: Default::default(),
        }
    }
}

fn path() -> std::path::PathBuf {
    crate::settings::config_dir().join("gui.json")
}

/// Reads gui.json, defaulting when it is missing; an invalid file is kept as gui.json.bad.
fn read() -> Prefs {
    let Ok(text) = std::fs::read_to_string(path()) else { return Prefs::default() };
    serde_json::from_str(&text).unwrap_or_else(|_| {
        crate::settings::backup_bad(&path());
        Prefs::default()
    })
}

/// Writes gui.json through a temp file.
fn try_write(prefs: &Prefs) -> std::io::Result<()> {
    let json = serde_json::to_string_pretty(prefs).map_err(std::io::Error::other)?;
    std::fs::create_dir_all(crate::settings::config_dir())?;
    let tmp = path().with_extension("json.tmp");
    std::fs::write(&tmp, json)?;
    std::fs::rename(&tmp, path())
}

/// Writes gui.json, ignoring failures.
fn write(prefs: &Prefs) {
    let _ = try_write(prefs);
}

/// Applies the gui.json resets in memory; returns log lines.
fn reset_prefs(prefs: &mut Prefs, resets: &[&Reset]) -> Vec<String> {
    let d = Prefs::default();
    let mut log = Vec::new();
    for r in resets {
        let w = &r.when;
        let Key::Gui(f) = r.key else { continue };
        let (field, change) = match f {
            GuiField::Theme => ("theme", reset_field(&mut prefs.theme, d.theme.clone(), w)),
            GuiField::Compact => ("compact", reset_field(&mut prefs.compact, d.compact, w)),
            GuiField::RowMin => ("row_min", reset_field(&mut prefs.row_min, d.row_min, w)),
            GuiField::RowMax => ("row_max", reset_field(&mut prefs.row_max, d.row_max, w)),
            GuiField::NameClip => ("name_clip", reset_field(&mut prefs.name_clip, d.name_clip, w)),
            GuiField::LogLines => ("log_lines", reset_field(&mut prefs.log_lines, d.log_lines, w)),
            GuiField::RememberSize => ("remember_size", reset_field(&mut prefs.remember_size, d.remember_size, w)),
            GuiField::WinW => ("win_w", reset_field(&mut prefs.win_w, d.win_w, w)),
            GuiField::WinH => ("win_h", reset_field(&mut prefs.win_h, d.win_h, w)),
        };
        if let Some(c) = change {
            log.push(format!("gui.json {field} reset: {c}"));
        }
    }
    log
}

/// Applies the gui.json resets and saves once if anything changed; returns log lines.
pub fn apply_resets(resets: &[&Reset]) -> std::io::Result<Vec<String>> {
    let mut prefs = read();
    let before = prefs.clone();
    let lines = reset_prefs(&mut prefs, resets);
    if prefs != before {
        try_write(&prefs)?;
    }
    Ok(lines)
}

/// Reads the saved theme.
pub fn load_theme() -> ThemeKind {
    ThemeKind::from_name(&read().theme)
}

/// Saves the theme.
pub fn save_theme(kind: ThemeKind) {
    let mut prefs = read();
    prefs.theme = kind.name().to_string();
    write(&prefs);
}

/// Reads the saved Files tab preferences.
pub fn load_files() -> FilesPrefs {
    read().files
}

/// Saves the Files tab preferences.
pub fn save_files(files: &FilesPrefs) {
    let mut prefs = read();
    prefs.files = files.clone();
    write(&prefs);
}

/// Reads the saved Posts tab preferences, with the card width clamped to the allowed limits.
pub fn load_posts() -> PostsPrefs {
    let p = read().posts;
    PostsPrefs { card_w: p.card_w.clamp(CARD_W_MIN, CARD_W_MAX), ..p }
}

/// Saves the Posts tab preferences.
pub fn save_posts(posts: &PostsPrefs) {
    let mut prefs = read();
    prefs.posts = posts.clone();
    write(&prefs);
}

/// Reads the drives the user added to the disk-space display.
pub fn load_extra_drives() -> Vec<String> {
    read().extra_drives
}

/// Saves the drives the user added to the disk-space display.
pub fn save_extra_drives(drives: &[String]) {
    let mut prefs = read();
    prefs.extra_drives = drives.to_vec();
    write(&prefs);
}

/// Reads the saved display order of the tracked drives.
pub fn load_drive_order() -> Vec<String> {
    read().drive_order
}

/// Saves the display order of the tracked drives.
pub fn save_drive_order(order: &[String]) {
    let mut prefs = read();
    prefs.drive_order = order.to_vec();
    write(&prefs);
}

/// Reads the saved disk-space display settings.
pub fn load_disk_limits() -> crate::policy::disks::Limits {
    read().disk_limits
}

/// Saves the disk-space display settings.
pub fn save_disk_limits(limits: crate::policy::disks::Limits) {
    let mut prefs = read();
    prefs.disk_limits = limits;
    write(&prefs);
}

/// Reads the saved Live first, Group by site, sort key and ascending settings.
pub fn load_sort() -> (bool, bool, SortKey, bool) {
    let p = read();
    (p.live_first, p.group_by_site, SortKey::from_id(&p.sort_key), p.sort_asc)
}

/// Saves the Live first, Group by site, sort key and ascending settings.
pub fn save_sort(live_first: bool, group_by_site: bool, key: SortKey, asc: bool) {
    let mut prefs = read();
    prefs.live_first = live_first;
    prefs.group_by_site = group_by_site;
    prefs.sort_key = key.id().to_string();
    prefs.sort_asc = asc;
    write(&prefs);
}

/// Reads the saved compact-view setting.
pub fn load_compact() -> bool {
    read().compact
}

/// Saves the compact-view setting.
pub fn save_compact(on: bool) {
    let mut prefs = read();
    prefs.compact = on;
    write(&prefs);
}

/// Reads the saved compact row-height range, clamped to the allowed limits.
pub fn load_row_range() -> (f32, f32) {
    let prefs = read();
    let min = prefs.row_min.clamp(ROW_MIN_LIMIT, ROW_MAX_LIMIT);
    (min, prefs.row_max.clamp(min, ROW_MAX_LIMIT))
}

/// Saves the compact row-height range.
pub fn save_row_range(min: f32, max: f32) {
    let mut prefs = read();
    prefs.row_min = min;
    prefs.row_max = max;
    write(&prefs);
}

/// Reads the saved minimum column width of compact view, clamped to the allowed limits.
pub fn load_compact_col_width() -> f32 {
    read().compact_col_w.clamp(COL_W_MIN_LIMIT, COL_W_MAX_LIMIT)
}

/// Saves the minimum column width of compact view.
pub fn save_compact_col_width(width: f32) {
    let mut prefs = read();
    prefs.compact_col_w = width;
    write(&prefs);
}

/// Reads the saved minimum column width of full view, clamped to the allowed limits.
pub fn load_full_col_width() -> f32 {
    read().full_col_w.clamp(COL_W_MIN_LIMIT, COL_W_MAX_LIMIT)
}

/// Saves the minimum column width of full view.
pub fn save_full_col_width(width: f32) {
    let mut prefs = read();
    prefs.full_col_w = width;
    write(&prefs);
}

/// Reads the saved UI scale: automatic on or off, the manual scale and the last automatic scale.
pub fn load_ui_scale() -> (bool, f32, f32) {
    let p = read();
    (p.ui_scale_auto, p.ui_scale.clamp(UI_SCALE_MIN, UI_SCALE_MAX), p.auto_scale.clamp(UI_SCALE_MIN, UI_SCALE_MAX))
}

/// Saves whether the UI scale is chosen automatically.
pub fn save_ui_scale_auto(on: bool) {
    let mut prefs = read();
    prefs.ui_scale_auto = on;
    write(&prefs);
}

/// Saves the manual UI scale.
pub fn save_ui_scale(scale: f32) {
    let mut prefs = read();
    prefs.ui_scale = scale;
    write(&prefs);
}

/// Saves the last automatic UI scale.
pub fn save_auto_scale(scale: f32) {
    let mut prefs = read();
    prefs.auto_scale = scale;
    write(&prefs);
}

/// The UI scale in use: the automatic one when `auto` is on, else the manual one.
pub fn effective_scale(auto: bool, manual: f32, automatic: f32) -> f32 {
    let scale = if auto { automatic } else { manual };
    scale.clamp(UI_SCALE_MIN, UI_SCALE_MAX)
}

/// The automatic UI scale for a monitor of `monitor_w` x `monitor_h` logical pixels, 1.0 at 1920 x 1080.
pub fn auto_scale(monitor_w: f32, monitor_h: f32) -> f32 {
    let raw = (monitor_w / AUTO_REF_W).min(monitor_h / AUTO_REF_H);
    ((raw * 20.0).round() / 20.0).clamp(AUTO_SCALE_MIN, AUTO_SCALE_MAX)
}

/// Reads the saved compact-view name length limit.
pub fn load_name_clip() -> f32 {
    read().name_clip.clamp(NAME_CLIP_MIN_LIMIT, NAME_CLIP_MAX_LIMIT)
}

/// Saves the compact-view name length limit.
pub fn save_name_clip(chars: f32) {
    let mut prefs = read();
    prefs.name_clip = chars;
    write(&prefs);
}

/// Reads the saved Log tab line limit, clamped to the allowed limits.
pub fn load_log_lines() -> f32 {
    read().log_lines.clamp(LOG_LINES_MIN_LIMIT, LOG_LINES_MAX_LIMIT)
}

/// Saves the Log tab line limit.
pub fn save_log_lines(lines: f32) {
    let mut prefs = read();
    prefs.log_lines = lines;
    write(&prefs);
}

/// Reads whether command output lines wrap in the Processes tab.
pub fn load_wrap_output() -> bool {
    read().wrap_output
}

/// Saves whether command output lines wrap in the Processes tab.
pub fn save_wrap_output(on: bool) {
    let mut prefs = read();
    prefs.wrap_output = on;
    write(&prefs);
}

/// Reads whether lines wrap in the Output log viewer.
pub fn load_wrap_logs() -> bool {
    read().wrap_logs
}

/// Saves whether lines wrap in the Output log viewer.
pub fn save_wrap_logs(on: bool) {
    let mut prefs = read();
    prefs.wrap_logs = on;
    write(&prefs);
}

/// Reads whether the window size is remembered between launches.
pub fn load_remember_size() -> bool {
    read().remember_size
}

/// Saves whether the window size is remembered between launches.
pub fn save_remember_size(on: bool) {
    let mut prefs = read();
    prefs.remember_size = on;
    write(&prefs);
}

/// Saves the window size for the next launch.
pub fn save_window_size(width: f32, height: f32) {
    let mut prefs = read();
    prefs.win_w = width;
    prefs.win_h = height;
    write(&prefs);
}

/// Returns the saved window size when remembering is on, else the default size.
pub fn startup_window_size() -> (f32, f32) {
    let prefs = read();
    if prefs.remember_size {
        (prefs.win_w.max(WIN_MIN_W), prefs.win_h.max(WIN_MIN_H))
    } else {
        (WIN_DEFAULT_W, WIN_DEFAULT_H)
    }
}

/// The window size reduced to fit a monitor of `monitor_w` x `monitor_h` logical pixels.
pub fn fit_to_monitor(w: f32, h: f32, monitor_w: f32, monitor_h: f32) -> (f32, f32) {
    let max_w = (monitor_w - FIT_MARGIN_W).max(WIN_MIN_W);
    let max_h = (monitor_h - FIT_MARGIN_H).max(WIN_MIN_H);
    (w.min(max_w), h.min(max_h))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::resets::Val;

    #[test]
    fn window_shrinks_to_fit_a_small_monitor() {
        assert_eq!(fit_to_monitor(1180.0, 845.0, 1280.0, 720.0), (1180.0, 620.0));
        assert_eq!(fit_to_monitor(1180.0, 845.0, 1920.0, 1080.0), (1180.0, 845.0));
        assert_eq!(fit_to_monitor(1180.0, 845.0, 500.0, 300.0), (WIN_MIN_W, WIN_MIN_H));
    }

    #[test]
    fn auto_scale_follows_the_monitor_and_stays_in_range() {
        assert_eq!(auto_scale(1920.0, 1080.0), 1.0);
        assert_eq!(auto_scale(2560.0, 1440.0), 1.35);
        assert_eq!(auto_scale(2048.0, 1152.0), 1.05);
        assert_eq!(auto_scale(1280.0, 720.0), 0.8);
        assert_eq!(auto_scale(3840.0, 2160.0), 1.5);
    }

    #[test]
    fn effective_scale_picks_automatic_or_manual_within_limits() {
        assert_eq!(effective_scale(true, 1.2, 1.35), 1.35);
        assert_eq!(effective_scale(false, 1.2, 1.35), 1.2);
        assert_eq!(effective_scale(false, 5.0, 1.0), UI_SCALE_MAX);
    }

    #[test]
    fn sort_rows_start_with_the_switches_then_a_divider_then_every_key() {
        let rows = SortItem::rows(true, true);
        assert_eq!(rows[..3], [SortItem::LiveFirst(true), SortItem::GroupBySite(true), SortItem::Divider]);
        assert_eq!(rows[3..], SortKey::ALL.map(SortItem::Key));
        assert_eq!(SortItem::LiveFirst(true).to_string(), "Live first ✓");
        assert_eq!(SortItem::LiveFirst(false).to_string(), "Live first");
        assert_eq!(SortItem::GroupBySite(true).to_string(), "Group by site ✓");
        assert_eq!(SortItem::GroupBySite(false).to_string(), "Group by site");
        assert_eq!(SortItem::Key(SortKey::LastLive).to_string(), "Last live");
    }

    #[test]
    fn prefs_reset_to_defaults() {
        let mut p = Prefs { row_max: 40.0, compact: true, ..Prefs::default() };
        let a = Reset::always(2, Key::Gui(GuiField::RowMax));
        let b = Reset::if_equals(2, Key::Gui(GuiField::Compact), Val::Flag(false));
        let log = reset_prefs(&mut p, &[&a, &b]);
        assert_eq!(p.row_max, ROW_MAX_DEFAULT);
        assert!(p.compact);
        assert_eq!(log.len(), 1);
    }
}
