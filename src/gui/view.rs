//! View functions for every screen.

use super::skin::{self, BtnKind, Skin, ThemeKind, pulse};
use super::{
    App, DraftField, Msg, NEW_SITE_ID, OutputLog, PickerTarget, SITE_SWATCHES, ScheduleIssue, SettingsTab, SiteDraft, SiteOption, SiteSel, SiteSelOption, StreamerDraft,
    LogTab, StreamerField, Tab, UpdateStage, prefs,
};
use crate::api::*;
use chrono::{Local, NaiveDate, NaiveDateTime, NaiveTime, Timelike};
use crate::policy::{layout, WarningChip, Warnings};
use iced::widget::text::{IntoFragment, Wrapping};
use iced::widget::{
    Column, Row, Space, Text, button, column, container, pick_list, row, scrollable, slider,
    text_input, toggler, tooltip,
};
use iced::{Alignment, Color, Element, Length};
use iced_aw::date_picker::Date;
use iced_aw::time_picker::{Period, Time};
use iced_aw::{DatePicker, TimePicker};

const DAY_NAMES: [&str; 7] = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];

/// `t` with its seconds dropped.
fn to_minute(t: NaiveTime) -> NaiveTime {
    NaiveTime::from_hms_opt(t.hour(), t.minute(), 0).unwrap_or(t)
}

pub(super) fn t<'a>(s: &Skin, v: impl IntoFragment<'a>) -> Text<'a> {
    iced::widget::text(v).font(s.font).color(s.text)
}

/// Width of the "Add streamer" button.
const STREAMER_BTN_W: f32 = 130.0;

/// Button label that inherits the button style's text colour.
pub(super) fn btn_label<'a>(s: &Skin, v: impl IntoFragment<'a>) -> Text<'a> {
    iced::widget::text(v).font(s.font)
}

pub(super) fn muted<'a>(s: &Skin, v: impl IntoFragment<'a>) -> Text<'a> {
    iced::widget::text(v).font(s.font).color(s.muted).size(12)
}

pub(super) fn chip<'a>(s: Skin, label: impl IntoFragment<'a>, color: Color) -> Element<'a, Msg> {
    container(iced::widget::text(label).font(s.font).size(11).color(color))
        .padding([3.0, 10.0])
        .style(skin::chip(s, color))
        .into()
}

/// The REC/RECORDING chip; clicking opens the file when one is confirmed.
fn rec_chip<'a>(s: Skin, label: &'static str, size: f32, pad: [f32; 2], open: Option<RecorderId>) -> Element<'a, Msg> {
    let text = iced::widget::text(label).font(s.font).size(size).color(s.accent);
    match open {
        Some(id) => button(text)
            .on_press(Msg::OpenRecording(id))
            .padding(pad)
            .style(skin::chip_btn(s, s.accent))
            .into(),
        None => container(text).padding(pad).style(skin::chip(s, s.accent)).into(),
    }
}

/// A glowing warning chip; clicking dismisses that warning for the streamer.
fn warn_chip<'a>(s: Skin, label: impl IntoFragment<'a>, color: Color, size: f32, pad: [f32; 2], id: StreamerId, kind: WarningChip) -> Element<'a, Msg> {
    button(iced::widget::text(label).font(s.font).size(size).color(color))
        .on_press(Msg::DismissWarning(id, kind))
        .padding(pad)
        .style(skin::chip_btn(s, color))
        .into()
}

fn dot<'a>(s: Skin, color: Color, glow: f32) -> Element<'a, Msg> {
    let _ = s;
    container(Space::new().width(10).height(10))
        .style(skin::dot(color, glow))
        .into()
}

fn fmt_mem(bytes: u64) -> String {
    let mb = bytes as f64 / (1024.0 * 1024.0);
    if mb >= 1024.0 {
        format!("{:.2} GB", mb / 1024.0)
    } else {
        format!("{mb:.1} MB")
    }
}

fn fmt_dur(secs: u64) -> String {
    let (h, m, s) = (secs / 3600, (secs / 60) % 60, secs % 60);
    if h > 0 {
        format!("{h}h {m:02}m {s:02}s")
    } else if m > 0 {
        format!("{m}m {s:02}s")
    } else {
        format!("{s}s")
    }
}

/// Largest whole unit of an elapsed time, e.g. `5m ago`, `3h ago`, `2d ago`.
pub(super) fn fmt_ago(secs: u64) -> String {
    match secs {
        0..=59 => "<1m ago".to_string(),
        60..=3599 => format!("{}m ago", secs / 60),
        3600..=86399 => format!("{}h ago", secs / 3600),
        _ => format!("{}d ago", secs / 86400),
    }
}

pub(super) fn clip(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let mut out: String = s.chars().take(max).collect();
        out.push('…');
        out
    }
}

const ROW_GAP: f32 = 2.0;
const ROW_FLOOR: f32 = 15.0;
const ROW_CEIL: f32 = 34.0;
const COMPACT_CHROME_H: f32 = 200.0;
const COMPACT_SLACK_H: f32 = 16.0;
/// Height of the add-streamer bar under the compact list, with its gap.
const ADD_BAR_H: f32 = 48.0;
const COLUMN_FRAME: f32 = 1.0;
const COLUMN_GAP: f32 = 14.0;
const SITE_CLIP_CHARS: usize = 12;
const SITE_CHAR_W: f32 = 0.6;
const PW_SITE: u32 = 104;
const PW_STATE: u32 = 170;
const PW_OPEN: u32 = 56;
const PW_UP: u32 = 80;
const PW_YT: u32 = 140;
const PW_FF: u32 = 140;

/// Sizes of one compact row, interpolated from its height; the site column fits the longest site chip and the name column takes the remaining width.
#[derive(Clone, Copy)]
struct RowMetrics {
    row_h: f32,
    name: f32,
    info: f32,
    chip: f32,
    chip_pad_v: f32,
    chip_pad_h: f32,
    dot: f32,
    toggler: f32,
    site_w: f32,
    spacing: f32,
    pad_h: f32,
}

impl RowMetrics {
    fn new(row_h: f32, site_chars: usize, pad_trim: f32) -> RowMetrics {
        let t = ((row_h - ROW_FLOOR) / (ROW_CEIL - ROW_FLOOR)).max(0.0);
        let at = |floor: f32, ceil: f32| floor + (ceil - floor) * t;
        let text = |floor: f32, ceil: f32| (at(floor, ceil) * 2.0).round() / 2.0;
        let name = text(11.0, 16.0);
        let chip = text(9.0, 11.0);
        let chip_pad_h = at(6.0, 10.0);
        RowMetrics {
            row_h,
            name,
            info: text(10.0, 12.0),
            chip,
            chip_pad_v: at(0.0, 4.0),
            chip_pad_h,
            dot: at(7.0, 10.0).round(),
            toggler: at(12.0, 18.0).round(),
            site_w: (site_chars as f32 * chip * SITE_CHAR_W + 2.0 * chip_pad_h).ceil() + 2.0,
            spacing: at(8.0, 12.0),
            pad_h: at(6.0, 12.0) - pad_trim,
        }
    }
}

fn mini_dot<'a>(m: RowMetrics, color: Color, glow: f32) -> Element<'a, Msg> {
    container(Space::new().width(m.dot).height(m.dot))
        .style(skin::dot(color, glow * 0.5))
        .into()
}

fn mini_chip<'a>(
    s: Skin,
    m: RowMetrics,
    label: impl IntoFragment<'a>,
    color: Color,
) -> Element<'a, Msg> {
    container(
        iced::widget::text(label)
            .font(s.font)
            .size(m.chip)
            .color(color),
    )
    .padding([m.chip_pad_v, m.chip_pad_h])
    .style(skin::chip(s, color))
    .into()
}

fn compact_chips<'a>(
    s: Skin,
    m: RowMetrics,
    st: &Streamer,
    status: StreamerStatus,
    stalled: bool,
    restarts: u32,
    warns: Warnings,
    open: Option<RecorderId>,
) -> Row<'a, Msg> {
    let mut chips: Row<'a, Msg> = Row::new()
        .spacing(m.spacing / 2.0)
        .align_y(Alignment::Center);
    if status.recording && st.enabled {
        chips = chips.push(rec_chip(s, "REC", m.chip, [m.chip_pad_v, m.chip_pad_h], open));
    }
    if stalled {
        chips = chips.push(warn_chip(s, "STALLED", s.danger, m.chip, [m.chip_pad_v, m.chip_pad_h], st.id, WarningChip::Stalled));
    }
    if !st.enabled {
        chips = chips.push(mini_chip(s, m, "PAUSED", s.warn));
    }
    if st.options.schedule.enabled {
        chips = chips.push(mini_chip(s, m, "SCH", s.muted));
    }
    if status.live {
        chips = chips.push(mini_chip(s, m, "LIVE", s.live));
    }
    if restarts > 0 {
        chips = chips.push(warn_chip(s, format!("RESTARTED {restarts}×"), s.warn, m.chip, [m.chip_pad_v, m.chip_pad_h], st.id, WarningChip::Restarted));
    }
    if warns.interrupts > 0 {
        chips = chips.push(warn_chip(s, format!("INTERRUPT {}×", warns.interrupts), s.warn, m.chip, [m.chip_pad_v, m.chip_pad_h], st.id, WarningChip::Interrupt));
    }
    if warns.errors > 0 {
        chips = chips.push(warn_chip(s, format!("ERROR {}×", warns.errors), s.warn, m.chip, [m.chip_pad_v, m.chip_pad_h], st.id, WarningChip::Error));
    }
    if st.options.log_stdout || st.options.log_stderr {
        chips = chips.push(mini_chip(s, m, "LOG", s.muted));
    }
    chips
}

/// Total CPU and memory across every process of one recording.
fn proc_totals(p: &ProcessInfo) -> (f32, u64) {
    let mut cpu = 0.0_f32;
    let mut mem = 0_u64;
    let mut add = |x: &ProcSample| {
        cpu += x.cpu_percent;
        mem += x.mem_bytes;
    };
    if p.has_bootloader {
        if let Some(r) = &p.root {
            add(r);
        }
    }
    let ytdlp = p.ytdlp.as_ref().or(if p.has_bootloader {
        None
    } else {
        p.root.as_ref()
    });
    if let Some(y) = ytdlp {
        add(y);
    }
    for f in &p.ffmpeg {
        add(f);
    }
    (cpu, mem)
}

impl App {
    pub(super) fn view(&self) -> Element<'_, Msg> {
        let s = self.skin();
        let body: Element<'_, Msg> = match self.tab {
            Tab::Livestreams => self.monitor(s),
            Tab::Posts => self.posts_view(s),
            Tab::Processes => self.processes(s),
            Tab::Files => self.files_view(s),
            Tab::Settings => self.settings_tab(s),
            Tab::Log => self.log_tab(s),
        };
        let mut page = Column::new().spacing(14).push(self.header(s));
        if let Some(banner) = self.config_banner(s) {
            page = page.push(banner);
        }
        if let Some(banner) = self.update_banner(s) {
            page = page.push(banner);
        }
        container(page.push(self.tabs(s)).push(body))
            .padding(18)
            .width(Length::Fill)
            .height(Length::Fill)
            .style(skin::app_bg(s))
            .into()
    }

    fn header(&self, s: Skin) -> Element<'_, Msg> {
        let live = self.statuses.values().filter(|x| x.live).count();
        let rec = self.processes.len();
        let monitoring = self
            .settings
            .streamers
            .iter()
            .filter(|st| st.enabled && !self.settings.sites.iter().any(|site| site.id == st.site_id && site.paused))
            .count();
        let p = pulse(self.phase);
        let title = row![
            dot(s, s.accent, 0.4 + 0.6 * p),
            row![
                iced::widget::text("jj-dlp")
                    .font(s.font)
                    .size(22)
                    .color(s.accent2),
                iced::widget::text(format!("v{}", crate::policy::update::CURRENT))
                    .font(s.font)
                    .size(12)
                    .color(s.muted),
            ]
            .spacing(8)
            .align_y(Alignment::End),
        ]
        .spacing(12)
        .align_y(Alignment::Center);
        let chips = row![
            chip(
                s,
                format!("{monitoring} monitoring"),
                if monitoring > 0 { s.accent2 } else { s.muted }
            ),
            chip(
                s,
                format!("{live} live"),
                if live > 0 { s.live } else { s.muted }
            ),
            chip(
                s,
                format!("{rec} recording"),
                if rec > 0 { s.accent } else { s.muted }
            ),
        ]
        .spacing(8);
        let mut right = Row::new().spacing(10).align_y(Alignment::Center);
        if let Some(disks) = self.disk_chips(s) {
            right = right.push(disks);
        }
        let right = right.push(pick_list(ThemeKind::ALL, Some(self.kind), Msg::ThemeChosen).padding(8));
        container(
            row![title, chips, Space::new().width(Length::Fill), right]
                .spacing(18)
                .align_y(Alignment::Center),
        )
        .padding([12.0, 18.0])
        .width(Length::Fill)
        .style(skin::header(s))
        .into()
    }

    /// Warns that a config file failed to load and saving is paused; `None` when all loaded.
    fn config_banner(&self, s: Skin) -> Option<Element<'_, Msg>> {
        let reasons: Vec<String> = [crate::settings::load_problem(), crate::policy::posts::load_problem()].into_iter().flatten().collect();
        if reasons.is_empty() {
            return None;
        }
        let left = Column::new()
            .spacing(4)
            .push(iced::widget::text("A config file could not be loaded, so changes are not being saved").font(s.font).size(15).color(s.danger))
            .push(iced::widget::text(reasons.join("; ")).font(s.font).size(12).color(s.warn))
            .push(muted(&s, "Fix or restore the file from its .bad copy and restart, or start fresh to overwrite it."));
        let buttons = row![
            button(t(&s, "Open config folder").size(13))
                .on_press(Msg::OpenConfigDir)
                .padding([6.0, 14.0])
                .style(skin::btn(s, BtnKind::Ghost)),
            button(t(&s, "Start fresh").size(13))
                .on_press(Msg::StartFresh)
                .padding([6.0, 14.0])
                .style(skin::btn(s, BtnKind::Danger)),
        ]
        .spacing(8);
        Some(
            container(row![left, Space::new().width(Length::Fill), buttons].spacing(12).align_y(Alignment::Center))
                .padding([10.0, 14.0])
                .width(Length::Fill)
                .style(skin::card(s, Some((s.danger, pulse(self.phase)))))
                .into(),
        )
    }

    /// The "Update available!" banner with its confirm step; `None` when up to date.
    fn update_banner(&self, s: Skin) -> Option<Element<'_, Msg>> {
        let a = self.update.as_ref()?;
        let kind = if a.is_rc { " (release candidate)" } else { "" };
        let heading = iced::widget::text(format!(
            "Update available! v{}{kind} (running v{})",
            a.version,
            crate::policy::update::CURRENT
        ))
        .font(s.font)
        .size(15)
        .color(s.accent2);
        let (detail, buttons): (Option<String>, Element<'_, Msg>) = match self.update_stage {
            UpdateStage::Idle => (
                self.update_error.as_ref().map(|e| format!("Last attempt failed: {e}")),
                button(t(&s, "Update").size(13))
                    .on_press(Msg::UpdateClicked)
                    .padding([6.0, 14.0])
                    .style(skin::btn(s, BtnKind::Primary))
                    .into(),
            ),
            UpdateStage::Confirm => {
                let n = self.processes.len();
                let text = if n > 0 {
                    format!("Installing will stop {n} active recording(s) and restart jj-dlp.")
                } else {
                    "jj-dlp will restart to install the update.".to_string()
                };
                (
                    Some(text),
                    row![
                        button(t(&s, "Install & restart").size(13))
                            .on_press(Msg::UpdateConfirm)
                            .padding([6.0, 14.0])
                            .style(skin::btn(s, BtnKind::Danger)),
                        button(t(&s, "Cancel").size(13))
                            .on_press(Msg::UpdateCancel)
                            .padding([6.0, 14.0])
                            .style(skin::btn(s, BtnKind::Ghost)),
                    ]
                    .spacing(8)
                    .into(),
                )
            }
            UpdateStage::Installing => (
                Some("Downloading the update; jj-dlp restarts when it is ready.".to_string()),
                Space::new().into(),
            ),
        };
        let mut left = Column::new().spacing(4).push(heading);
        if let Some(d) = detail {
            left = left.push(iced::widget::text(d).font(s.font).size(12).color(s.warn));
        }
        Some(
            container(
                row![left, Space::new().width(Length::Fill), buttons]
                    .spacing(12)
                    .align_y(Alignment::Center),
            )
            .padding([10.0, 14.0])
            .width(Length::Fill)
            .style(skin::card(s, Some((s.accent, pulse(self.phase)))))
            .into(),
        )
    }

    fn tabs(&self, s: Skin) -> Element<'_, Msg> {
        let alert = self.monitor_alert();
        let tab = |label: String, which: Tab| {
            let pulsing = alert && which == Tab::Livestreams;
            let kind = if self.tab == which {
                BtnKind::TabActive
            } else {
                BtnKind::TabIdle
            };
            let b = button(
                t(&s, label)
                    .size(14)
                    .color(if self.tab == which || pulsing { s.bg } else { s.muted }),
            )
            .on_press(Msg::SetTab(which))
            .padding([7.0, 18.0]);
            if pulsing {
                b.style(skin::tab_alert(s, self.phase))
            } else {
                b.style(skin::btn(s, kind))
            }
        };
        row![
            tab("Livestreams".to_string(), Tab::Livestreams),
            tab(self.posts_tab_label(), Tab::Posts),
            tab("Processes".to_string(), Tab::Processes),
            tab("Files".to_string(), Tab::Files),
            tab("Settings".to_string(), Tab::Settings),
            tab("Log".to_string(), Tab::Log)
        ]
        .spacing(8)
        .into()
    }

    pub(super) fn section_title<'a>(&self, s: Skin, label: &'a str) -> Element<'a, Msg> {
        iced::widget::text(label)
            .font(s.font)
            .size(13)
            .color(s.accent2)
            .into()
    }

    fn monitor(&self, s: Skin) -> Element<'_, Msg> {
        let mut title = row![Space::new().width(Length::Fill)]
            .spacing(8)
            .padding(iced::Padding {
                right: 8.0,
                ..Default::default()
            })
            .align_y(Alignment::Center);
        if self.monitor_alert() {
            title = title.push(
                button(btn_label(&s, "Clear warnings").size(12))
                    .on_press(Msg::ClearWarnings)
                    .padding([3.0, 10.0])
                    .style(skin::btn(s, BtnKind::Ghost)),
            );
        }
        title = title.push(toggler(self.live_first).label("Live first").on_toggle(Msg::SetLiveFirst).text_size(12));
        title = title.push(
            button(btn_label(&s, if self.compact { "Full View" } else { "Compact View" }).size(12))
                .on_press(Msg::SetCompact(!self.compact))
                .padding([5.0, 10.0])
                .style(skin::btn(s, BtnKind::Ghost)),
        );
        title = title.push(
            pick_list(prefs::SortKey::ALL.to_vec(), Some(self.sort_key), Msg::SetSortKey)
                .padding([3.0, 8.0])
                .text_size(12),
        );
        title = title.push(
            button(self.sort_dir_icon(s))
                .on_press(Msg::ToggleSortDir)
                .padding([5.0, 8.0])
                .style(skin::btn(s, BtnKind::Ghost)),
        );
        let mut col = Column::new().spacing(12).padding(iced::Padding {
            right: 8.0,
            ..Default::default()
        });
        if !self.notices.is_empty() {
            col = col.push(self.setup_notices(s));
        }
        if self.compact {
            col = self.compact_list(s, col);
        } else {
            for p in &self.check_problems() {
                col = col.push(self.warning_line(s, p));
            }
            if self.settings.streamers.is_empty() {
                col = col.push(muted(&s, "No streamers yet. Pick a site and add a username below."));
            }
            let (live, rest) = self.split_live();
            if !live.is_empty() {
                col = col.push(self.live_block(s, live));
            }
            let shown: Vec<(&Site, Vec<&Streamer>)> = self
                .settings
                .sites
                .iter()
                .map(|site| (site, rest.iter().copied().filter(|x| x.site_id == site.id).collect::<Vec<_>>()))
                .filter(|(_, list)| !list.is_empty())
                .collect();
            let ids: Vec<SiteId> = shown.iter().map(|(site, _)| site.id).collect();
            for (i, (site, list)) in shown.into_iter().enumerate() {
                let up = i.checked_sub(1).map(|k| ids[k]);
                let down = ids.get(i + 1).copied();
                col = col.push(self.site_block(s, site, list, up, down));
            }
        }
        column![title, scrollable(col).height(Length::Fill)]
            .spacing(12)
            .push(container(self.streamer_add_bar(s)).padding(iced::Padding { right: 8.0, ..Default::default() }))
            .into()
    }

    /// Sort-direction icon: an arrow beside horizontal lines, longest at the top for ascending.
    fn sort_dir_icon(&self, s: Skin) -> Element<'_, Msg> {
        let bar = move |w: f32| -> Element<'static, Msg> {
            container(Space::new())
                .width(w)
                .height(2.0)
                .style(move |_| container::Style { background: Some(s.text.into()), ..Default::default() })
                .into()
        };
        let widths = if self.sort_asc { [14.0, 10.0, 6.0] } else { [6.0, 10.0, 14.0] };
        let bars = column![bar(widths[0]), bar(widths[1]), bar(widths[2])].spacing(2);
        row![iced::widget::text(if self.sort_asc { "▲" } else { "▼" }).font(s.font).size(10).color(s.text), bars]
            .spacing(4)
            .align_y(Alignment::Center)
            .into()
    }

    /// Streamers in the chosen sort order, in settings (priority) order for ties.
    fn sorted_streamers(&self) -> Vec<&Streamer> {
        let mut v: Vec<(usize, &Streamer)> = self.settings.streamers.iter().enumerate().collect();
        v.sort_by(|(ia, a), (ib, b)| {
            let o = match self.sort_key {
                prefs::SortKey::Name => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
                prefs::SortKey::Added => a.id.cmp(&b.id),
                prefs::SortKey::LastLive => self.last_live.get(&a.id).cmp(&self.last_live.get(&b.id)),
                prefs::SortKey::Priority => ia.cmp(ib),
            };
            (if self.sort_asc { o } else { o.reverse() }).then(ia.cmp(ib))
        });
        v.into_iter().map(|(_, st)| st).collect()
    }

    /// Splits the sorted streamers into the currently live ones (only with Live first on) and the rest.
    fn split_live(&self) -> (Vec<&Streamer>, Vec<&Streamer>) {
        let sorted = self.sorted_streamers();
        if !self.live_first {
            return (Vec::new(), sorted);
        }
        sorted.into_iter().partition(|st| self.statuses.get(&st.id).is_some_and(|x| x.live))
    }

    /// The "Live" panel shown above the site panels.
    fn live_block<'a>(&'a self, s: Skin, streamers: Vec<&'a Streamer>) -> Element<'a, Msg> {
        let head = iced::widget::text("Live").font(s.font).size(18).color(s.live);
        let top = self.push_streamers(s, Column::new().spacing(8).push(head), streamers);
        container(top).padding(14).width(Length::Fill).style(skin::card(s, None)).into()
    }

    /// Adds the streamer rows to a panel, in two columns when the window is wide enough.
    fn push_streamers<'a>(&'a self, s: Skin, mut top: Column<'a, Msg>, streamers: Vec<&'a Streamer>) -> Column<'a, Msg> {
        if self.win_w >= self.full_two_col_w && streamers.len() > 1 {
            let per_col = streamers.len().div_ceil(2);
            let mut grid = Row::new().spacing(12).align_y(Alignment::Start);
            for chunk in streamers.chunks(per_col) {
                let mut c = Column::new().spacing(8).width(Length::Fill);
                for &st in chunk {
                    c = c.push(self.streamer_row(s, st));
                }
                grid = grid.push(c);
            }
            top = top.push(grid);
        } else {
            for st in streamers {
                top = top.push(self.streamer_row(s, st));
            }
        }
        top
    }

    /// Site selector, name and username boxes and Add button under the site panels.
    fn streamer_add_bar(&self, s: Skin) -> Element<'_, Msg> {
        let choices = self.site_options();
        let selected = self.add_choice();
        let pulsing = self.settings.streamers.is_empty();
        let (calm, glow) = (skin::input(s), skin::input_pulse(s, pulse(self.phase)));
        row![
            pick_list(choices, selected, |o: SiteOption| Msg::PickStreamerSite(o.target)).placeholder("Site").padding(8).text_size(13).width(150),
            text_input("Streamer username", &self.new_streamer)
                .id(super::username_input_id())
                .on_input(Msg::NewStreamerUsername)
                .on_submit(Msg::AddStreamer)
                .padding(8)
                .width(Length::Fill)
                .style(move |th, st| if pulsing { glow(th, st) } else { calm(th, st) }),
            button(btn_label(&s, "Add streamer").size(13).width(Length::Fill).align_x(Alignment::Center))
                .on_press(Msg::AddStreamer)
                .padding([8.0, 14.0])
                .width(STREAMER_BTN_W)
                .style(skin::btn(s, BtnKind::Primary)),
        ]
        .spacing(8)
        .align_y(Alignment::Center)
        .into()
    }

    /// Picks three or two columns when the window is at least that threshold wide, else one; sizes rows to fit.
    fn compact_layout(&self, rows: usize, extra_h: f32) -> (usize, f32) {
        let avail = (self.win_h - COMPACT_CHROME_H - COMPACT_SLACK_H - extra_h - 2.0 * COLUMN_FRAME).max(0.0);
        let cols = if self.win_w >= self.three_col_w {
            3
        } else if self.win_w >= self.two_col_w {
            2
        } else {
            1
        };
        let per_col = rows.div_ceil(cols).max(1);
        let fit = ((avail + ROW_GAP) / per_col as f32 - ROW_GAP).floor();
        (cols, fit.clamp(self.row_min, self.row_max))
    }

    /// One line per site whose last check reported a problem or failed.
    fn check_problems(&self) -> Vec<String> {
        let mut problems: Vec<String> = Vec::new();
        for site in &self.settings.sites {
            if let Some(c) = self.checks.get(&site.id) {
                if let Some(p) = &c.problem {
                    problems.push(format!("{}: {p}", site.name));
                } else if !c.ok {
                    problems.push(format!("{}: last check failed", site.name));
                }
            }
        }
        problems
    }

    fn compact_list<'a>(&'a self, s: Skin, mut col: Column<'a, Msg>) -> Column<'a, Msg> {
        let problems = self.check_problems();
        for p in &problems {
            col = col.push(self.warning_line(s, p));
        }
        let mut entries: Vec<(&'a Site, &'a Streamer)> = Vec::new();
        let (live, rest) = self.split_live();
        for st in live {
            if let Some(site) = self.settings.sites.iter().find(|x| x.id == st.site_id) {
                entries.push((site, st));
            }
        }
        for site in &self.settings.sites {
            for st in rest.iter().copied().filter(|x| x.site_id == site.id) {
                entries.push((site, st));
            }
        }
        if entries.is_empty() {
            col = col.push(muted(&s, "No streamers yet. Pick a site and add a username below."));
            return col;
        }
        let notices_h = if self.notices.is_empty() {
            0.0
        } else {
            48.0 + 80.0 * self.notices.len() as f32
        };
        let banner_h = if self.update.is_some() { 70.0 } else { 0.0 };
        let extra_h = problems.len() as f32 * 32.0 + notices_h + banner_h + ADD_BAR_H;
        let (cols, row_h) = self.compact_layout(entries.len(), extra_h);
        let longest_site = entries
            .iter()
            .map(|(site, _)| clip(&site.name, SITE_CLIP_CHARS).chars().count())
            .max()
            .unwrap_or(0);
        let frame = COLUMN_FRAME;
        let pad_trim = if cols == 1 { frame } else { 0.0 };
        let m = RowMetrics::new(row_h, longest_site, pad_trim);
        let per_col = entries.len().div_ceil(cols);
        let col_h = per_col as f32 * row_h + (per_col - 1) as f32 * ROW_GAP + 2.0 * frame;
        let mut grid = Row::new().spacing(COLUMN_GAP - 4.0 * frame);
        for chunk in entries.chunks(per_col) {
            let mut c = Column::new().spacing(ROW_GAP).width(Length::Fill);
            for (i, &(site, st)) in chunk.iter().enumerate() {
                c = c.push(self.compact_row(s, m, site, st, i % 2 == 1));
            }
            grid = grid.push(
                container(c)
                    .padding(COLUMN_FRAME)
                    .width(Length::Fill)
                    .height(col_h)
                    .clip(true)
                    .style(skin::column_frame(s)),
            );
        }
        col.push(grid)
    }

    /// True when warnings picked by `on` show for the streamer's site; false while all warnings are disabled.
    fn warn_shown(&self, streamer_id: StreamerId, on: impl Fn(&Site) -> bool) -> bool {
        let on = |x: &Site| x.warnings_enabled && on(x);
        if self.settings.notifications.disable_warnings {
            return false;
        }
        let st = self.settings.streamers.iter().find(|st| st.id == streamer_id);
        st.and_then(|st| self.settings.sites.iter().find(|x| x.id == st.site_id)).is_none_or(on)
    }

    fn recording_stalled(&self, id: RecorderId) -> bool {
        self.recording_files.get(&id).is_some_and(|f| f.stalled)
            && self.processes.get(&id).is_none_or(|p| self.warn_shown(p.streamer_id, |x| x.warn_stalled))
    }

    fn recording_openable(&self, id: RecorderId) -> bool {
        self.recording_files.get(&id).is_some_and(|f| f.openable)
    }

    /// The streamer's restart count, or 0 while it is below the site's threshold.
    fn shown_restarts(&self, st: &Streamer) -> u32 {
        if !self.warn_shown(st.id, |x| x.warn_restarts) {
            return 0;
        }
        let count = self.restarts.get(&st.id).copied().unwrap_or(0);
        let threshold = self.settings.sites.iter().find(|x| x.id == st.site_id).map_or(1, |x| x.restart_threshold_or_default());
        if count >= threshold { count } else { 0 }
    }

    /// The streamer's INTERRUPT and ERROR counts.
    fn warns(&self, id: StreamerId) -> Warnings {
        let w = self.warnings.get(&id).copied().unwrap_or_default();
        Warnings {
            interrupts: if self.warn_shown(id, |x| x.warn_interrupts) { w.interrupts } else { 0 },
            errors: if self.warn_shown(id, |x| x.warn_errors) { w.errors } else { 0 },
        }
    }

    /// True when at least one RESTARTED, INTERRUPT or ERROR chip is showing.
    /// True while the Clear warnings button applies.
    /// True while something on screen pulses, so the fast animation tick is needed.
    pub(super) fn animating(&self) -> bool {
        let p = &self.posts;
        !self.processes.is_empty()
            || self.statuses.values().any(|s| s.live || s.recording)
            || self.pending_jump.is_some()
            || self.update.is_some()
            || crate::settings::load_problem().is_some()
            || crate::policy::posts::load_problem().is_some()
            || self.monitor_alert()
            || (p.add_choice().is_some() && p.cfg.sites.iter().all(|x| x.accounts.is_empty()))
            || (self.tab == Tab::Livestreams && self.settings.streamers.is_empty())
    }

    fn monitor_alert(&self) -> bool {
        self.any_warning_chip() || self.settings.streamers.iter().any(|st| self.streamer_stalled(st.id))
    }

    fn any_warning_chip(&self) -> bool {
        self.settings.streamers.iter().any(|st| {
            let w = self.warns(st.id);
            self.shown_restarts(st) > 0 || w.interrupts > 0 || w.errors > 0
        })
    }

    /// True when any active recording of the streamer is stalled.
    /// The recording of the streamer whose file is confirmed and can be opened.
    fn openable_recording(&self, id: StreamerId) -> Option<RecorderId> {
        self.processes.values().find(|p| p.streamer_id == id && self.recording_openable(p.id)).map(|p| p.id)
    }

    fn streamer_stalled(&self, id: StreamerId) -> bool {
        self.processes.values().any(|p| p.streamer_id == id && self.recording_stalled(p.id))
    }

    /// Row tint: red while STALLED shows, else yellow while RESTARTED, INTERRUPT or ERROR shows.
    fn row_tint(&self, s: Skin, st: &Streamer) -> Option<Color> {
        let w = self.warns(st.id);
        if self.streamer_stalled(st.id) {
            Some(s.danger)
        } else if self.shown_restarts(st) > 0 || w.interrupts > 0 || w.errors > 0 {
            Some(s.warn)
        } else {
            None
        }
    }

    /// Seconds the streamer has been live; only while the checker reports it live.
    fn live_for(&self, id: StreamerId, status: StreamerStatus) -> Option<u64> {
        if !status.live {
            return None;
        }
        self.live_since.get(&id).map(|t| self.now.saturating_sub(*t))
    }

    /// Seconds since the streamer was last seen live; hidden while live or when unknown.
    fn last_live_ago(&self, id: StreamerId, status: StreamerStatus) -> Option<u64> {
        if status.live {
            return None;
        }
        self.last_live.get(&id).map(|t| self.now.saturating_sub(*t))
    }

    fn compact_row<'a>(
        &'a self,
        s: Skin,
        m: RowMetrics,
        site: &'a Site,
        st: &'a Streamer,
        shaded: bool,
    ) -> Element<'a, Msg> {
        let status = self.statuses.get(&st.id).copied().unwrap_or_default();
        let ph = pulse(self.phase);
        let color = if status.live && !st.enabled {
            s.muted
        } else if status.recording {
            s.accent
        } else if status.live {
            s.live
        } else {
            s.muted
        };
        let glow = if status.live || status.recording {
            0.4 + 0.6 * ph
        } else {
            0.0
        };
        let id = st.id;
        let name_color = if st.enabled { s.text } else { s.muted };
        let shown_name = clip(&st.name, self.name_clip.round() as usize);
        let was_clipped = shown_name != st.name;
        let name_cell = container(
            iced::widget::text(shown_name)
                .font(s.font)
                .size(m.name)
                .color(name_color)
                .wrapping(Wrapping::None),
        )
        .width(Length::Fill)
        .clip(true);
        // Full name on hover, only when it was cut.
        let name_el: Element<'a, Msg> = if was_clipped {
            tooltip(
                name_cell,
                container(iced::widget::text(st.name.as_str()).font(s.font).size(12).color(s.text))
                    .padding([4.0, 8.0])
                    .style(skin::card(s, None)),
                tooltip::Position::Bottom,
            )
            .into()
        } else {
            name_cell.into()
        };
        // Live duration while live, otherwise time since last live.
        let (time_text, time_color, time_font) = match self.live_for(id, status) {
            Some(d) => (format!("{:>12}", fmt_dur(d)), s.live, iced::Font::MONOSPACE),
            None => (self.last_live_ago(id, status).map(fmt_ago).unwrap_or_default(), s.muted, s.font),
        };
        let line = row![
            mini_dot(m, color, glow),
            container(mini_chip(
                s,
                m,
                clip(&site.name, SITE_CLIP_CHARS),
                s.site_color(site.color.as_deref())
            ))
            .width(m.site_w)
            .clip(true),
            name_el,
            {
                let mut times = Row::new().spacing(4).align_y(Alignment::Center);
                if let Some(q) = self.resolutions.get(&id) {
                    let fps = q.fps_rounded().map(|f| f.to_string()).unwrap_or_default();
                    times = times.push(iced::widget::text(format!("{}p{fps} ", q.width.min(q.height))).font(s.font).size(m.info).color(s.muted).wrapping(Wrapping::None));
                }
                times.push(iced::widget::text(time_text).font(time_font).size(m.info).color(time_color).wrapping(Wrapping::None))
            },
            compact_chips(s, m, st, status, self.streamer_stalled(id), self.shown_restarts(st), self.warns(id), self.openable_recording(id)),
            toggler(st.enabled)
                .on_toggle(move |v| Msg::ToggleStreamer(id, v))
                .size(m.toggler),
        ]
        .spacing(m.spacing)
        .align_y(Alignment::Center);
        container(line)
            .center_y(m.row_h)
            .width(Length::Fill)
            .padding([0.0, m.pad_h])
            .clip(true)
            .style(skin::stripe_tinted(s, shaded, self.row_tint(s, st)))
            .into()
    }

    fn warning_line<'a>(&self, s: Skin, msg: &str) -> Element<'a, Msg> {
        row![
            iced::widget::text("⚠").font(s.font).size(13).color(s.warn),
            iced::widget::text(clip(msg, 300))
                .font(s.font)
                .size(12)
                .color(s.warn)
                .width(Length::Fill),
        ]
        .spacing(8)
        .into()
    }

    fn setup_notices(&self, s: Skin) -> Element<'_, Msg> {
        let header = row![
            self.section_title(s, "SETUP NEEDED"),
            Space::new().width(Length::Fill),
            button(t(&s, "Re-check").size(13))
                .on_press(Msg::Recheck)
                .padding([6.0, 12.0])
                .style(skin::btn(s, BtnKind::Ghost)),
        ]
        .align_y(Alignment::Center);
        let mut col = Column::new().spacing(10).push(header);
        for n in &self.notices {
            let mut item = Column::new()
                .spacing(4)
                .push(
                    iced::widget::text(n.title.as_str())
                        .font(s.font)
                        .size(14)
                        .color(s.warn),
                )
                .push(muted(&s, n.steps.as_str()));
            if let Some(cmd) = &n.command {
                item = item.push(
                    row![
                        container(
                            iced::widget::text(cmd.as_str())
                                .font(iced::Font::MONOSPACE)
                                .size(12)
                                .color(s.text)
                        )
                        .padding([6.0, 10.0])
                        .style(skin::inset(s)),
                        button(t(&s, "Copy").size(12))
                            .on_press(Msg::CopyText(cmd.clone()))
                            .padding([5.0, 10.0])
                            .style(skin::btn(s, BtnKind::Ghost)),
                    ]
                    .spacing(8)
                    .align_y(Alignment::Center),
                );
            }
            col = col.push(item);
        }
        container(col)
            .padding(14)
            .width(Length::Fill)
            .style(skin::card(s, None))
            .into()
    }

    fn processes(&self, s: Skin) -> Element<'_, Msg> {
        let mut col = Column::new().spacing(6).padding(iced::Padding {
            right: 8.0,
            ..Default::default()
        });
        col = col.push(self.section_title(s, "PROCESSES"));
        if self.processes.is_empty() {
            col = col.push(
                container(muted(
                    &s,
                    "No active recordings. Streams are recorded automatically when they go live.",
                ))
                .padding(16)
                .width(Length::Fill)
                .style(skin::card(s, None)),
            );
        } else {
            col = col.push(self.proc_summary(s));
            col = col.push(self.proc_header(s));
            for p in self.processes.values() {
                col = col.push(self.proc_line(s, p));
                if self.expanded_procs.contains(&p.id) {
                    col = col.push(self.proc_detail(s, p));
                }
            }
        }
        scrollable(col).height(Length::Fill).into()
    }

    fn proc_summary(&self, s: Skin) -> Element<'_, Msg> {
        let (cpu, mem) = self.processes.values().fold((0.0_f32, 0_u64), |(c, m), p| {
            let (pc, pm) = proc_totals(p);
            (c + pc, m + pm)
        });
        container(
            row![
                t(&s, format!("{} recording(s)", self.processes.len())).size(13),
                muted(&s, format!("CPU {cpu:.1}% total")),
                muted(&s, format!("MEM {} total", fmt_mem(mem))),
                Space::new().width(Length::Fill),
                muted(&s, "Click a row for PIDs and the full command line."),
            ]
            .spacing(16)
            .align_y(Alignment::Center),
        )
        .padding([8.0, 14.0])
        .width(Length::Fill)
        .style(skin::inset(s))
        .into()
    }

    fn proc_header(&self, s: Skin) -> Element<'_, Msg> {
        let h = |label: &'static str| -> Text<'static> {
            iced::widget::text(label)
                .font(s.font)
                .size(11)
                .color(s.muted)
        };
        container(
            row![
                Space::new().width(10),
                container(h("STREAMER")).width(Length::FillPortion(3)),
                container(h("SITE")).width(PW_SITE),
                container(h("STATE")).width(PW_STATE),
                container(h("UPTIME")).width(PW_UP),
                container(h("YT-DLP CPU · MEM")).width(PW_YT),
                container(h("FFMPEG CPU · MEM")).width(PW_FF),
                container(h("LAST OUTPUT")).width(Length::FillPortion(4)),
                Space::new().width(PW_OPEN),
            ]
            .spacing(8),
        )
        .padding([0.0, 12.0])
        .into()
    }

    fn proc_row<'a>(
        &self,
        s: Skin,
        label: &'a str,
        color: Color,
        p: &ProcSample,
    ) -> Element<'a, Msg> {
        row![
            container(iced::widget::text(label).font(s.font).size(12).color(color)).width(88),
            iced::widget::text(format!("PID {}", p.pid))
                .font(s.font)
                .size(12)
                .color(s.text)
                .width(96),
            iced::widget::text(format!("CPU {:.1}%", p.cpu_percent))
                .font(s.font)
                .size(12)
                .color(s.text)
                .width(96),
            iced::widget::text(format!("MEM {}", fmt_mem(p.mem_bytes)))
                .font(s.font)
                .size(12)
                .color(s.text),
        ]
        .spacing(8)
        .into()
    }

    fn proc_line<'a>(&'a self, s: Skin, p: &'a ProcessInfo) -> Element<'a, Msg> {
        let ph = pulse(self.phase);
        let (state_label, state_color) = match p.state {
            RecorderState::Starting => ("STARTING".to_string(), s.warn),
            RecorderState::Running => ("RUNNING".to_string(), s.accent),
            RecorderState::Stopping => ("STOPPING".to_string(), s.warn),
            RecorderState::Exited(code) => (
                format!("EXITED {}", code.map_or("?".into(), |c| c.to_string())),
                s.muted,
            ),
        };
        let glow = match p.state {
            RecorderState::Exited(_) => 0.0,
            _ => 0.4 + 0.6 * ph,
        };
        let stalled = self.recording_stalled(p.id);
        let tint = if stalled { s.danger } else { state_color };
        let mut state_chips: Row<'a, Msg> =
            Row::new().spacing(6).align_y(Alignment::Center).push(chip(s, state_label, state_color));
        if stalled {
            state_chips = state_chips.push(warn_chip(s, "STALLED", s.danger, 11.0, [3.0, 10.0], p.streamer_id, WarningChip::Stalled));
        }
        let yt = p.ytdlp.as_ref().or(p.root.as_ref());
        let yt_text = yt.map_or("—".to_string(), |y| {
            format!("{:.1}% · {}", y.cpu_percent, fmt_mem(y.mem_bytes))
        });
        let ff_text = if p.ffmpeg.is_empty() {
            "—".to_string()
        } else {
            let cpu: f32 = p.ffmpeg.iter().map(|f| f.cpu_percent).sum();
            let mem: u64 = p.ffmpeg.iter().map(|f| f.mem_bytes).sum();
            format!("{cpu:.1}% · {}", fmt_mem(mem))
        };
        let cell = |v: String, w: u32| -> iced::widget::Container<'a, Msg> {
            container(
                iced::widget::text(v)
                    .font(s.font)
                    .size(12)
                    .color(s.text)
                    .wrapping(Wrapping::None),
            )
            .width(w)
            .clip(true)
        };
        let site_color = self
            .settings
            .streamers
            .iter()
            .find(|x| x.id == p.streamer_id)
            .and_then(|st| self.settings.sites.iter().find(|x| x.id == st.site_id))
            .map_or(s.accent2, |site| s.site_color(site.color.as_deref()));
        let line = row![
            dot(s, tint, glow),
            container(
                iced::widget::text(clip(&p.streamer_name, 28))
                    .font(s.font)
                    .size(13)
                    .color(s.text)
                    .wrapping(Wrapping::None)
            )
            .width(Length::FillPortion(3))
            .clip(true),
            container(chip(s, clip(&p.site_name, 12), site_color))
                .width(PW_SITE)
                .clip(true),
            container(state_chips).width(PW_STATE),
            cell(fmt_dur(p.uptime_secs), PW_UP),
            cell(yt_text, PW_YT),
            cell(ff_text, PW_FF),
            container(
                iced::widget::text(p.last_line.as_str())
                    .font(s.font)
                    .size(12)
                    .color(s.muted)
                    .wrapping(Wrapping::None)
            )
            .width(Length::FillPortion(4))
            .clip(true),
        ]
        .spacing(8)
        .align_y(Alignment::Center);
        let open = self.expanded_procs.contains(&p.id);
        let tile = button(line)
            .on_press(Msg::ToggleProc(p.id))
            .padding([6.0, 12.0])
            .width(Length::Fill)
            .style(skin::tile(
                s,
                if matches!(p.state, RecorderState::Exited(_)) {
                    s.muted
                } else {
                    tint
                },
                open,
            ));
        let open_file = button(container(btn_label(&s, "Open").size(12)).center_x(Length::Fill))
            .on_press_maybe(self.recording_openable(p.id).then_some(Msg::OpenRecording(p.id)))
            .padding([6.0, 0.0])
            .width(PW_OPEN)
            .style(skin::btn(s, BtnKind::Ghost));
        row![tile, open_file].spacing(8).align_y(Alignment::Center).into()
    }

    fn proc_detail<'a>(&'a self, s: Skin, p: &'a ProcessInfo) -> Element<'a, Msg> {
        let mut body = Column::new().spacing(5);
        if p.has_bootloader {
            if let Some(root) = &p.root {
                body = body.push(self.proc_row(s, "bootloader", s.muted, root));
            }
        }
        match p.ytdlp.as_ref().or(p.root.as_ref()) {
            Some(y) => body = body.push(self.proc_row(s, "yt-dlp", s.accent, y)),
            None => body = body.push(muted(&s, "yt-dlp: resolving process…")),
        }
        for f in &p.ffmpeg {
            body = body.push(self.proc_row(s, "ffmpeg", s.accent2, f));
        }
        if p.ffmpeg.is_empty() && matches!(p.state, RecorderState::Running) {
            body = body.push(muted(&s, "ffmpeg: not running"));
        }
        if matches!(p.state, RecorderState::Exited(_)) {
            body = body.push(muted(
                &s,
                "Root process exited; confirming every child process is gone.",
            ));
        }
        let output = self.output.get(&p.id).filter(|l| !l.text.is_empty());
        if let Some(log) = output {
            body = body.push(self.output_panel(s, p.id, log));
        }
        if !p.last_line.is_empty() {
            let line = iced::widget::text(p.last_line.as_str()).font(s.font).size(12).color(s.text);
            let inner: Element<'a, Msg> = if output.is_some() {
                column![muted(&s, "Last line"), line].spacing(4).into()
            } else {
                line.into()
            };
            body = body.push(container(inner).padding([6.0, 10.0]).width(Length::Fill).style(skin::inset(s)));
        }
        body = body.push(muted(&s, p.cmdline.as_str()));
        container(body)
            .padding([8.0, 14.0])
            .width(Length::Fill)
            .style(skin::inset(s))
            .into()
    }

    /// The scrollable command output of a recording, with wrap and copy controls.
    fn output_panel<'a>(&'a self, s: Skin, id: RecorderId, log: &'a OutputLog) -> Element<'a, Msg> {
        let wrap = self.wrap_output;
        let lines = iced::widget::text(log.text.as_str()).font(s.font).size(12).color(s.text).wrapping(if wrap {
            Wrapping::WordOrGlyph
        } else {
            Wrapping::None
        });
        let pad = iced::Padding { right: 12.0, bottom: if wrap { 0.0 } else { 12.0 }, ..iced::Padding::ZERO };
        let scroll = scrollable(container(lines).padding(pad)).width(Length::Fill).anchor_bottom();
        let scroll = if wrap {
            scroll
        } else {
            scroll.direction(scrollable::Direction::Both {
                vertical: scrollable::Scrollbar::default(),
                horizontal: scrollable::Scrollbar::default(),
            })
            .anchor_bottom()
        };
        let header = row![
            muted(&s, "Output"),
            Space::new().width(Length::Fill),
            toggler(wrap).label("Wrap").on_toggle(Msg::SetWrapOutput).text_size(12),
            button(btn_label(&s, "Copy").size(12))
                .on_press(Msg::CopyOutput(id))
                .padding([3.0, 10.0])
                .style(skin::btn(s, BtnKind::Ghost)),
        ]
        .spacing(12)
        .align_y(Alignment::Center);
        container(column![header, container(scroll).max_height(300.0)].spacing(6))
            .padding([6.0, 10.0])
            .width(Length::Fill)
            .style(skin::inset(s))
            .into()
    }

    fn site_block<'a>(
        &'a self,
        s: Skin,
        site: &'a Site,
        streamers: Vec<&'a Streamer>,
        up: Option<SiteId>,
        down: Option<SiteId>,
    ) -> Element<'a, Msg> {
        let check = match self.checks.get(&site.id) {
            Some(c) => {
                let color = if c.ok { s.live } else { s.danger };
                row![
                    iced::widget::text(if c.ok { "●" } else { "✕" })
                        .font(s.font)
                        .size(12)
                        .color(color),
                    muted(&s, format!("{}s ago", self.now.saturating_sub(c.at))),
                ]
                .spacing(6)
                .align_y(Alignment::Center)
            }
            None => row![muted(&s, "waiting for first check")],
        };
        let mut head: Row<'_, Msg> = Row::new().spacing(10).align_y(Alignment::Center).push(
            iced::widget::text(clip(&site.name, 30))
                .font(s.font)
                .size(18)
                .color(s.site_color(site.color.as_deref())),
        );
        if site.log_checker_stdout || site.log_checker_stderr {
            head = head.push(chip(s, "LOGGING", s.muted));
        }
        if site.paused {
            head = head.push(chip(s, "PAUSED", s.warn));
        }
        let arrow = |label: &'static str, other: Option<SiteId>| {
            button(btn_label(&s, label).size(11))
                .on_press_maybe(other.map(|o| Msg::SwapSites(site.id, o)))
                .padding([2.0, 6.0])
                .style(skin::btn(s, BtnKind::Ghost))
        };
        let head = head.push(Space::new().width(Length::Fill)).push(check).push(arrow("▲", up)).push(arrow("▼", down));

        let top = Column::new().spacing(8).push(head);
        let top = self.push_streamers(s, top, streamers);
        container(top)
            .padding(14)
            .width(Length::Fill)
            .style(skin::card(s, None))
            .into()
    }

    fn streamer_row<'a>(&'a self, s: Skin, st: &'a Streamer) -> Element<'a, Msg> {
        let status = self.statuses.get(&st.id).copied().unwrap_or_default();
        let ph = pulse(self.phase);
        let color = if status.live && !st.enabled {
            s.muted
        } else if status.recording {
            s.accent
        } else if status.live {
            s.live
        } else {
            s.muted
        };
        let glow = if status.live || status.recording {
            0.4 + 0.6 * ph
        } else {
            0.0
        };
        let id = st.id;
        let mut chips: Row<'_, Msg> = Row::new().spacing(6);
        if status.recording && st.enabled {
            chips = chips.push(rec_chip(s, "RECORDING", 11.0, [3.0, 10.0], self.openable_recording(id)));
        }
        if self.streamer_stalled(id) {
            chips = chips.push(warn_chip(s, "STALLED", s.danger, 11.0, [3.0, 10.0], id, WarningChip::Stalled));
        }
        if status.live {
            chips = chips.push(chip(s, "LIVE", s.live));
        }
        if !status.live && !status.recording {
            chips = chips.push(chip(s, "OFFLINE", s.muted));
        }
        if !st.enabled {
            chips = chips.push(chip(s, "PAUSED", s.warn));
        }
        if st.options.schedule.enabled {
            chips = chips.push(chip(s, "SCHEDULED", s.muted));
        }
        let restarts = self.shown_restarts(st);
        if restarts > 0 {
            chips = chips.push(warn_chip(s, format!("RESTARTED {restarts}×"), s.warn, 11.0, [3.0, 10.0], id, WarningChip::Restarted));
        }
        let warns = self.warns(id);
        if warns.interrupts > 0 {
            chips = chips.push(warn_chip(s, format!("INTERRUPT {}×", warns.interrupts), s.warn, 11.0, [3.0, 10.0], id, WarningChip::Interrupt));
        }
        if warns.errors > 0 {
            chips = chips.push(warn_chip(s, format!("ERROR {}×", warns.errors), s.warn, 11.0, [3.0, 10.0], id, WarningChip::Error));
        }
        if st.options.log_stdout || st.options.log_stderr {
            chips = chips.push(chip(s, "LOGGING", s.muted));
        }
        let line = row![
            dot(s, color, glow),
            column![
                {
                    let mut name = Row::new().spacing(12).align_y(Alignment::Center).push(
                        iced::widget::text(clip(&st.name, 40))
                            .font(s.font)
                            .size(15)
                            .color(if st.enabled { s.text } else { s.muted }),
                    );
                    if let Some(q) = self.resolutions.get(&id) {
                        let fps = q.fps_rounded().map(|f| format!(" · {f} fps")).unwrap_or_default();
                        name = name.push(muted(&s, format!("{} x {}{fps}", q.width, q.height)));
                    }
                    if let Some(d) = self.live_for(id, status) {
                        name = name.push(
                            row![muted(&s, "live for "), muted(&s, fmt_dur(d)).font(iced::Font::MONOSPACE)]
                        );
                    }
                    name
                },
                {
                    let url = clip(&st.url, 70);
                    match self.last_live_ago(id, status) {
                        Some(d) => muted(&s, format!("{url} · last live {}", fmt_ago(d))),
                        None => muted(&s, url),
                    }
                },
            ]
            .spacing(2)
            .width(Length::Fill),
            chips,
            toggler(st.enabled)
                .label("Record")
                .on_toggle(move |v| Msg::ToggleStreamer(id, v))
                .text_size(12),
            button(btn_label(&s, "Settings").size(12))
                .on_press(Msg::ToggleStreamerSettings(id))
                .padding([5.0, 10.0])
                .style(skin::btn(
                    s,
                    if self.open_streamer_settings.contains(&id) { BtnKind::Primary } else { BtnKind::Ghost }
                )),
            button(t(&s, "Remove").size(12))
                .on_press(Msg::RemoveStreamer(id))
                .padding([5.0, 10.0])
                .style(skin::btn(s, BtnKind::Danger)),
        ]
        .spacing(12)
        .align_y(Alignment::Center);
        let mut body = Column::new().spacing(10).push(line);
        if let Some(msg) = self.issues.get(&id) {
            body = body.push(self.warning_line(s, msg));
        }
        if self.open_streamer_settings.contains(&id) {
            body = body.push(self.streamer_settings_panel(s, st));
        }
        container(body)
            .padding([8.0, 12.0])
            .width(Length::Fill)
            .style(skin::inset_tinted(s, self.row_tint(s, st)))
            .into()
    }

    fn streamer_settings_panel<'a>(&'a self, s: Skin, st: &'a Streamer) -> Element<'a, Msg> {
        let id = st.id;
        let d = self
            .streamer_drafts
            .get(&id)
            .cloned()
            .unwrap_or_else(|| StreamerDraft::from_streamer(st));
        let labeled = |label: &'static str, input: Element<'a, Msg>| -> Element<'a, Msg> {
            column![muted(&s, label), input].spacing(2).into()
        };
        let cell = |e: Element<'a, Msg>, w: Length| -> Element<'a, Msg> { container(e).width(w).into() };
        let toggle = |label: &'static str, on: bool, f: fn(bool) -> StreamerField| {
            toggler(on)
                .label(label)
                .on_toggle(move |v| Msg::StreamerEdit(id, f(v)))
                .text_size(12)
        };
        let inherit_choices = |inherited: bool| {
            vec![
                NotifyChoice { value: None, label: format!("Inherit ({})", on_off(inherited)) },
                NotifyChoice { value: Some(true), label: "On".to_string() },
                NotifyChoice { value: Some(false), label: "Off".to_string() },
            ]
        };
        let inherit_pick = |inherited: bool, current: Option<bool>, f: fn(Option<bool>) -> StreamerField| {
            let choices = inherit_choices(inherited);
            let selected = choices.iter().find(|c| c.value == current).cloned();
            pick_list(choices, selected, move |c: NotifyChoice| Msg::StreamerEdit(id, f(c.value)))
                .padding(5)
                .text_size(13)
                .width(Length::Fill)
        };

        let site = self.settings.sites.iter().find(|x| x.id == st.site_id);
        let site_layout = site.map(|x| x.folder_layout).unwrap_or_default();
        let layout_choices: Vec<LayoutChoice> = std::iter::once(LayoutChoice {
            value: None,
            label: format!("Inherit ({site_layout})"),
        })
        .chain(FolderLayout::ALL.into_iter().map(|l| LayoutChoice {
            value: Some(l),
            label: l.to_string(),
        }))
        .collect();
        let layout_selected = layout_choices.iter().find(|c| c.value == d.folder_layout).cloned();
        let effective_layout = d.folder_layout.unwrap_or(site_layout);
        let base = layout::base_dir(
            site.map_or("", |x| x.output_dir.as_str()),
            &self.settings.default_output_dir,
        );
        let shown_name = if d.name.trim().is_empty() { st.name.as_str() } else { d.name.trim() };
        let path = layout::preview(base, effective_layout, site.map_or("", |x| x.name.as_str()), Some(shown_name));
        let site_notify = site.is_some_and(|x| x.notify);
        let site_suffix = site.is_some_and(|x| x.auto_suffix);
        let site_upgrade = site.is_some_and(|x| x.upgrade_quality);
        let site_quick = site.is_some_and(|x| x.quick_check);

        let name_row = row![
            cell(
                labeled(
                    "Display Name",
                    text_input("Defaults to the username", &d.name)
                        .on_input(move |v| Msg::StreamerEdit(id, StreamerField::Name(v)))
                        .padding(5)
                        .size(13)
                        .style(skin::input(s))
                        .into()
                ),
                Length::FillPortion(1)
            ),
            cell(Space::new().into(), Length::FillPortion(2)),
        ]
        .spacing(8);

        let top = row![
            cell(
                labeled(
                    "Subfolders",
                    pick_list(layout_choices, layout_selected, move |c: LayoutChoice| Msg::StreamerEdit(
                        id,
                        StreamerField::FolderLayout(c.value)
                    ))
                    .padding(5)
                    .text_size(13)
                    .width(Length::Fill)
                    .into(),
                ),
                Length::FillPortion(1)
            ),
            cell(
                labeled(
                    "Notifications",
                    inherit_pick(site_notify, d.notify, StreamerField::Notify).into()
                ),
                Length::FillPortion(1)
            ),
            cell(Space::new().into(), Length::FillPortion(1)),
        ]
        .spacing(8);
        let mut paths = column![muted(&s, format!("Effective path: {path}"))].spacing(2);
        if effective_layout != FolderLayout::Flat && site.is_some_and(|x| layout::has_absolute_template(&x.recorder_args)) {
            paths = paths.push(warn_text(&s, ABSOLUTE_TEMPLATE_WARNING));
        }

        let inherited_split = self.inherited_split(st);
        let split_invalid = d.split_invalid();
        let split_hint = if d.split.is_none() { inherited_split.map(|m| m.to_string()).unwrap_or_default() } else { String::new() };
        let splitting = row![
            cell(
                labeled(
                    "Split recordings",
                    inherit_pick(inherited_split.is_some(), d.split, StreamerField::Split).into()
                ),
                Length::FillPortion(1)
            ),
            cell(
                labeled(
                    "Split the recording after (minutes)",
                    text_input(&split_hint, &d.split_minutes)
                        .on_input(move |v| Msg::StreamerEdit(id, StreamerField::SplitMinutes(v)))
                        .padding(5)
                        .size(13)
                        .style(skin::input_required(s, split_invalid))
                        .into()
                ),
                Length::FillPortion(1)
            ),
            cell(Space::new().into(), Length::FillPortion(1)),
        ]
        .spacing(8);

        let suffix_row = row![
            cell(
                labeled(
                    "Name restarted recordings _partN",
                    inherit_pick(site_suffix, d.auto_suffix, StreamerField::AutoSuffix).into()
                ),
                Length::FillPortion(1)
            ),
            cell(Space::new().into(), Length::FillPortion(2)),
        ]
        .spacing(8);

        let upgrade_row = row![
            cell(
                labeled(
                    "Upgrade quality when available",
                    inherit_pick(site_upgrade, d.upgrade_quality, StreamerField::UpgradeQuality).into()
                ),
                Length::FillPortion(1)
            ),
            cell(Space::new().into(), Length::FillPortion(2)),
        ]
        .spacing(8);

        let quick_row = row![
            cell(
                labeled(
                    "Quick check ended streams",
                    inherit_pick(site_quick, d.quick_check, StreamerField::QuickCheck).into()
                ),
                Length::FillPortion(1)
            ),
            cell(Space::new().into(), Length::FillPortion(2)),
        ]
        .spacing(8);

        let intro_delay = column![
            row![
                toggle("Intro Delay", d.intro_enabled, StreamerField::IntroEnabled),
                toggler(d.intro_split && d.intro_enabled)
                    .label("Split the intro into its own file instead of waiting")
                    .on_toggle_maybe(
                        d.intro_enabled.then(|| move |v: bool| Msg::StreamerEdit(id, StreamerField::IntroSplit(v)))
                    )
                    .text_size(12),
            ]
            .spacing(24),
            row![
                cell(
                    labeled(
                        "Delay starting the recording by (minutes)",
                        text_input("", &d.intro_minutes)
                            .on_input(move |v| Msg::StreamerEdit(id, StreamerField::IntroMinutes(v)))
                            .padding(5)
                            .size(13)
                            .style(skin::input_required(s, d.intro_invalid()))
                            .into()
                    ),
                    Length::FillPortion(1)
                ),
                cell(Space::new().into(), Length::FillPortion(2)),
            ]
            .spacing(8),
        ]
        .spacing(6);

        let schedule = self.schedule_block(s, id, &d);

        let dirty = d.differs_from(st);
        let valid = !d.invalid();
        let mut actions: Row<'a, Msg> = row![Space::new().width(Length::Fill)]
            .spacing(10)
            .align_y(Alignment::Center);
        if dirty {
            actions = actions.push(chip(s, "unsaved changes", s.warn));
        }
        actions = actions
            .push(
                button(btn_label(&s, "Save changes").size(13))
                    .on_press_maybe((dirty && valid).then_some(Msg::SaveStreamer(id)))
                    .padding([5.0, 12.0])
                    .style(skin::btn(s, BtnKind::Primary)),
            )
            .push(
                button(t(&s, "Revert").size(13))
                    .on_press_maybe(dirty.then_some(Msg::RevertStreamer(id)))
                    .padding([5.0, 12.0])
                    .style(skin::btn(s, BtnKind::Ghost)),
            )
            .push(
                button(t(&s, "Defaults").size(13))
                    .on_press(Msg::DefaultsStreamer(id))
                    .padding([5.0, 12.0])
                    .style(skin::btn(s, BtnKind::Ghost)),
            );

        column![
            name_row,
            top,
            paths,
            splitting,
            intro_delay,
            schedule,
            suffix_row,
            upgrade_row,
            quick_row,
            actions,
        ]
        .spacing(6)
        .into()
    }

    /// The Schedule block of a streamer's settings panel.
    fn schedule_block<'a>(&self, s: Skin, id: StreamerId, d: &StreamerDraft) -> Element<'a, Msg> {
        let sc = &d.schedule;
        let issue = d.schedule_issue();
        let incomplete = issue == Some(ScheduleIssue::Incomplete);
        let is_open = |t: PickerTarget| self.schedule_picker == Some((id, t));
        let field_btn = |label: String, target: PickerTarget, bad: bool| {
            button(btn_label(&s, label).size(13))
                .on_press(Msg::OpenSchedulePicker(id, target))
                .padding([5.0, 10.0])
                .style(skin::btn(s, if bad { BtnKind::Danger } else { BtnKind::Ghost }))
        };
        let date_field = |target: PickerTarget,
                          cur: Option<NaiveDateTime>,
                          make: fn(NaiveDate) -> StreamerField,
                          bad: bool|
         -> Element<'a, Msg> {
            let label = cur.map_or_else(|| "Pick date".to_string(), |c| c.format("%Y-%m-%d").to_string());
            let shown = cur.map_or_else(|| Local::now().date_naive(), |c| c.date());
            DatePicker::new(
                is_open(target),
                Date::from(shown),
                field_btn(label, target, bad),
                Msg::CloseSchedulePicker,
                move |picked: Date| Msg::SchedulePicked(id, make(NaiveDate::from(picked))),
            )
            .into()
        };
        let time_field = |target: PickerTarget,
                          cur: Option<NaiveTime>,
                          make: fn(NaiveTime) -> StreamerField,
                          bad: bool|
         -> Element<'a, Msg> {
            let label = cur.map_or_else(|| "Pick time".to_string(), |c| c.format("%H:%M").to_string());
            let shown = cur.unwrap_or_else(|| Local::now().time());
            let initial = Time::Hm { hour: shown.hour(), minute: shown.minute(), period: Period::H24 };
            TimePicker::new(
                is_open(target),
                initial,
                field_btn(label, target, bad),
                Msg::CloseSchedulePicker,
                move |picked: Time| Msg::SchedulePicked(id, make(to_minute(NaiveTime::from(picked)))),
            )
            .use_24h()
            .into()
        };
        let label = |text: &'static str| container(muted(&s, text)).width(44);

        let header = row![
            toggler(sc.enabled)
                .label("Schedule")
                .on_toggle(move |v| Msg::StreamerEdit(id, StreamerField::ScheduleEnabled(v)))
                .text_size(12),
            pick_list(ScheduleMode::ALL, Some(sc.mode), move |m: ScheduleMode| {
                Msg::StreamerEdit(id, StreamerField::ScheduleMode(m))
            })
            .padding(5)
            .text_size(13),
        ]
        .spacing(24)
        .align_y(Alignment::Center);

        let body: Element<'a, Msg> = match sc.mode {
            ScheduleMode::OneOff => {
                let start_bad = incomplete && sc.one_off_start.is_none();
                let end_bad = (incomplete && sc.one_off_end.is_none()) || issue == Some(ScheduleIssue::EndNotAfterStart);
                column![
                    row![
                        label("Start"),
                        date_field(PickerTarget::OneOffStartDate, sc.one_off_start, StreamerField::OneOffStartDate, start_bad),
                        time_field(
                            PickerTarget::OneOffStartTime,
                            sc.one_off_start.map(|c| c.time()),
                            StreamerField::OneOffStartTime,
                            start_bad
                        ),
                    ]
                    .spacing(8)
                    .align_y(Alignment::Center),
                    row![
                        label("End"),
                        date_field(PickerTarget::OneOffEndDate, sc.one_off_end, StreamerField::OneOffEndDate, end_bad),
                        time_field(
                            PickerTarget::OneOffEndTime,
                            sc.one_off_end.map(|c| c.time()),
                            StreamerField::OneOffEndTime,
                            end_bad
                        ),
                    ]
                    .spacing(8)
                    .align_y(Alignment::Center),
                ]
                .spacing(6)
                .into()
            }
            ScheduleMode::Recurring => {
                let same = issue == Some(ScheduleIssue::SameTime);
                let start_bad = (incomplete && sc.start_time.is_none()) || same;
                let end_bad = (incomplete && sc.end_time.is_none()) || same;
                let mut days: Row<'a, Msg> = Row::new().spacing(4).align_y(Alignment::Center);
                for (i, name) in DAY_NAMES.into_iter().enumerate() {
                    let on = sc.days[i];
                    days = days.push(
                        button(btn_label(&s, name).size(12))
                            .on_press(Msg::StreamerEdit(id, StreamerField::ScheduleDay(i, !on)))
                            .padding([4.0, 8.0])
                            .style(skin::btn(s, if on { BtnKind::TabActive } else { BtnKind::Ghost })),
                    );
                }
                column![
                    days,
                    row![
                        label("From"),
                        time_field(PickerTarget::RecurringStart, sc.start_time, StreamerField::RecurringStart, start_bad),
                        label("To"),
                        time_field(PickerTarget::RecurringEnd, sc.end_time, StreamerField::RecurringEnd, end_bad),
                    ]
                    .spacing(8)
                    .align_y(Alignment::Center),
                ]
                .spacing(6)
                .into()
            }
        };

        let mut block: Column<'a, Msg> = column![header, body].spacing(6);
        if let Some(issue) = issue {
            block = block.push(iced::widget::text(issue.text()).font(s.font).size(12).color(s.danger));
        }
        block.into()
    }

    fn settings_tab(&self, s: Skin) -> Element<'_, Msg> {
        let switch = |label: &'static str, which: SettingsTab| {
            let on = self.settings_tab == which;
            let kind = if on { BtnKind::TabActive } else { BtnKind::TabIdle };
            button(t(&s, label).size(13).color(if on { s.bg } else { s.muted }))
                .on_press(Msg::SetSettingsTab(which))
                .padding([6.0, 16.0])
                .style(skin::btn(s, kind))
        };
        let switcher = row![
            switch("Livestreams", SettingsTab::Sites),
            switch("Posts", SettingsTab::Posts),
            switch("Priority", SettingsTab::Priority),
            switch("Notifications", SettingsTab::Notifications),
            switch("General", SettingsTab::General)
        ]
        .spacing(8);
        let body = match self.settings_tab {
            SettingsTab::Sites => self.settings_sites(s),
            SettingsTab::Posts => self.posts_settings(s),
            SettingsTab::Priority => self.priority_settings(s),
            SettingsTab::Notifications => self.notifications_settings(s),
            SettingsTab::General => self.settings_general(s),
        };
        column![switcher, body].spacing(12).height(Length::Fill).into()
    }

    fn settings_sites(&self, s: Skin) -> Element<'_, Msg> {
        let options = self.settings_site_options();
        let selected = self.settings_site_choice(&options);
        let target = selected.as_ref().map(|o| o.target);
        let picker = pick_list(options, selected, |o: SiteSelOption| Msg::PickSettingsSite(o.target))
            .placeholder("Site")
            .padding(8)
            .text_size(13)
            .width(240);
        let mut col = Column::new().spacing(10).padding(iced::Padding {
            right: 8.0,
            ..Default::default()
        });
        col = col.push(picker);
        match target {
            Some(SiteSel::New) => {
                if let Some(d) = self.drafts.get(&NEW_SITE_ID) {
                    col = col.push(self.site_editor(s, NEW_SITE_ID, None, d));
                }
            }
            Some(SiteSel::Site(id)) => {
                if let (Some(site), Some(d)) = (self.settings.sites.iter().find(|x| x.id == id), self.drafts.get(&id)) {
                    col = col.push(self.site_editor(s, id, Some(site), d));
                }
            }
            _ => {}
        }
        scrollable(col).id(super::sites_scroll_id()).height(Length::Fill).into()
    }

    fn settings_general(&self, s: Skin) -> Element<'_, Msg> {
        let mut col = Column::new().spacing(12).padding(iced::Padding {
            right: 8.0,
            ..Default::default()
        });

        let folders = column![
            row![
                self.section_title(s, "FOLDERS"),
                Space::new().width(Length::Fill),
                button(t(&s, "Open settings folder").size(13))
                    .on_press(Msg::OpenConfigDir)
                    .padding([8.0, 14.0])
                    .style(skin::btn(s, BtnKind::Ghost)),
            ]
            .align_y(Alignment::Center),
            muted(&s, "Global output folder, used by any site that has no output folder of its own."),
            row![
                text_input(&self.default_out, &self.global_out)
                    .on_input(Msg::GlobalOutEdit)
                    .on_submit(Msg::GlobalOutSave)
                    .padding(8)
                    .style(skin::input(s)),
                button(btn_label(&s, "Browse").size(13))
                    .on_press(Msg::BrowseGlobalOut)
                    .padding([8.0, 16.0])
                    .style(skin::btn(s, BtnKind::Ghost)),
                button(btn_label(&s, "Open").size(13))
                    .on_press(Msg::OpenGlobalOut)
                    .padding([8.0, 16.0])
                    .style(skin::btn(s, BtnKind::Ghost)),
                button(btn_label(&s, "Save").size(13))
                    .on_press_maybe(
                        (self.global_out.trim() != self.settings.default_output_dir)
                            .then_some(Msg::GlobalOutSave)
                    )
                    .padding([8.0, 16.0])
                    .style(skin::btn(s, BtnKind::Primary)),
            ]
            .spacing(8),
        ]
        .spacing(8);
        col = col.push(card_box(s, folders));
        col = col.push(self.drives_card(s));

        let row_size = |label: &'static str, value: f32, on_change: fn(f32) -> Msg| {
            row![
                t(&s, label).size(13).width(150),
                slider(
                    prefs::ROW_MIN_LIMIT..=prefs::ROW_MAX_LIMIT,
                    value,
                    on_change
                )
                .step(1.0_f32)
                .on_release(Msg::SaveRowRange)
                .width(Length::Fill),
                muted(&s, format!("{value:.0} px")).width(90),
            ]
            .spacing(12)
            .align_y(Alignment::Center)
        };
        let two_col = row![
            t(&s, "Two column threshold").size(13).width(150),
            slider(
                prefs::TWO_COL_MIN_LIMIT..=prefs::TWO_COL_MAX_LIMIT,
                self.two_col_w,
                Msg::SetTwoColWidth
            )
            .step(10.0_f32)
            .on_release(Msg::SaveTwoColWidth)
            .width(Length::Fill),
            muted(&s, format!("{:.0} px", self.two_col_w)).width(90),
        ]
        .spacing(12)
        .align_y(Alignment::Center);
        let three_col = row![
            t(&s, "Three column threshold").size(13).width(150),
            slider(
                prefs::TWO_COL_MIN_LIMIT..=prefs::TWO_COL_MAX_LIMIT,
                self.three_col_w,
                Msg::SetThreeColWidth
            )
            .step(10.0_f32)
            .on_release(Msg::SaveThreeColWidth)
            .width(Length::Fill),
            muted(&s, format!("{:.0} px", self.three_col_w)).width(90),
        ]
        .spacing(12)
        .align_y(Alignment::Center);
        let name_clip = row![
            t(&s, "Name Length").size(13).width(150),
            slider(
                prefs::NAME_CLIP_MIN_LIMIT..=prefs::NAME_CLIP_MAX_LIMIT,
                self.name_clip,
                Msg::SetNameClip
            )
            .step(1.0_f32)
            .on_release(Msg::SaveNameClip)
            .width(Length::Fill),
            muted(&s, format!("{:.0} chars", self.name_clip)).width(90),
        ]
        .spacing(12)
        .align_y(Alignment::Center);
        let log_lines = row![
            t(&s, "Buffer Length").size(13).width(150),
            slider(prefs::LOG_LINES_MIN_LIMIT..=prefs::LOG_LINES_MAX_LIMIT, self.log_lines, Msg::SetLogLines)
                .step(100.0_f32)
                .on_release(Msg::SaveLogLines)
                .width(Length::Fill),
            muted(&s, format!("{:.0} lines", self.log_lines)).width(90),
        ]
        .spacing(12)
        .align_y(Alignment::Center);
        let full_two_col = row![
            t(&s, "Two column threshold").size(13).width(150),
            slider(
                prefs::TWO_COL_MIN_LIMIT..=prefs::TWO_COL_MAX_LIMIT,
                self.full_two_col_w,
                Msg::SetFullTwoColWidth
            )
            .step(10.0_f32)
            .on_release(Msg::SaveFullTwoColWidth)
            .width(Length::Fill),
            muted(&s, format!("{:.0} px", self.full_two_col_w)).width(90),
        ]
        .spacing(12)
        .align_y(Alignment::Center);
        let compact = column![
            row![
                self.section_title(s, "FULL VIEW"),
                Space::new().width(Length::Fill),
                button(t(&s, "Reset").size(12))
                    .on_press(Msg::ResetFullView)
                    .padding([5.0, 10.0])
                    .style(skin::btn(s, BtnKind::Ghost)),
            ]
            .align_y(Alignment::Center),
            full_two_col,
            Space::new().height(8),
            row![
                self.section_title(s, "COMPACT VIEW"),
                Space::new().width(Length::Fill),
                button(t(&s, "Reset").size(12))
                    .on_press(Msg::ResetRowRange)
                    .padding([5.0, 10.0])
                    .style(skin::btn(s, BtnKind::Ghost)),
            ]
            .align_y(Alignment::Center),
            row_size("Row Size Max", self.row_max, Msg::SetRowMax),
            row_size("Row Size Min", self.row_min, Msg::SetRowMin),
            two_col,
            three_col,
            name_clip,
        ]
        .spacing(8);

        let updates = column![
            row![
                self.section_title(s, "UPDATES"),
                Space::new().width(Length::Fill),
                button(t(&s, "Check now").size(12))
                    .on_press(Msg::CheckUpdateNow)
                    .padding([5.0, 10.0])
                    .style(skin::btn(s, BtnKind::Ghost)),
            ]
            .align_y(Alignment::Center),
            toggler(self.update_cfg.enabled)
                .label("Check for updates automatically")
                .on_toggle(Msg::SetUpdateChecks)
                .text_size(12),
            toggler(self.update_cfg.include_rc)
                .label("Include release candidates")
                .on_toggle(Msg::SetIncludeRc)
                .text_size(12),
            muted(&s, format!("Running v{}. Automatic checks run every 6 hours.", crate::policy::update::CURRENT)),
        ]
        .spacing(8);

        let log_card = column![
            self.section_title(s, "LOG"),
            muted(&s, "Lines shown in the Log tab. Recent history is saved to ui.log and reloaded at startup."),
            log_lines,
        ]
        .spacing(8);

        let window = column![
            row![
                self.section_title(s, "WINDOW"),
                Space::new().width(Length::Fill),
                button(t(&s, "Reset").size(12))
                    .on_press(Msg::ResetWindowSize)
                    .padding([5.0, 10.0])
                    .style(skin::btn(s, BtnKind::Ghost)),
            ]
            .align_y(Alignment::Center),
            toggler(self.remember_size)
                .label("Remember window size")
                .on_toggle(Msg::SetRememberSize)
                .text_size(12),
            muted(&s, "Reopens at the size the window had when the app last closed. Reset restores the default size now."),
        ]
        .spacing(8);

        let compact = card_box(s, compact);
        let updates = card_box(s, updates);
        let window = card_box(s, window);
        let log_card = card_box(s, log_card);
        if self.win_w >= 1000.0 {
            let right = column![updates, window, log_card].spacing(12).width(Length::Fill);
            col = col.push(row![compact, right].spacing(12));
        } else {
            col = col.push(compact).push(updates).push(window).push(log_card);
        }
        scrollable(col).height(Length::Fill).into()
    }

    fn site_editor<'a>(&'a self, s: Skin, id: SiteId, saved: Option<&Site>, d: &'a SiteDraft) -> Element<'a, Msg> {
        let labeled = |label: &'static str, input: Element<'a, Msg>| -> Element<'a, Msg> {
            column![muted(&s, label), input].spacing(2).into()
        };
        let field = |placeholder: &str,
                     value: &'a str,
                     f: fn(String) -> DraftField|
         -> Element<'a, Msg> {
            text_input(placeholder, value)
                .on_input(move |v| Msg::Draft(id, f(v)))
                .padding(5)
                .size(13)
                .style(skin::input(s))
                .into()
        };
        let required_field = |label: &'static str, value: &'a str, f: fn(String) -> DraftField| -> Element<'a, Msg> {
            let input = text_input("", value)
                .on_input(move |v| Msg::Draft(id, f(v)))
                .padding(5)
                .size(13)
                .style(skin::input_required(s, value.trim().is_empty()));
            column![muted(&s, label), input].spacing(2).into()
        };
        let cell = |e: Element<'a, Msg>, w: Length| -> Element<'a, Msg> { container(e).width(w).into() };
        let dirty = saved.is_none_or(|site| *d != SiteDraft::from_site(site));
        let valid = d.to_new_site(&self.settings.notifications.ntfy_topic, self.parsing_overridden()).is_some();

        let identity = row![
            cell(
                required_field("Site name (example: Twitch)", &d.name, DraftField::Name),
                Length::FillPortion(1)
            ),
            cell(
                labeled(
                    "Check every (seconds)",
                    field(&DEFAULT_INTERVAL_SECS.to_string(), &d.interval, DraftField::Interval)
                ),
                Length::FillPortion(1)
            ),
            cell(
                labeled("Path to yt-dlp executable", field(DEFAULT_EXE, &d.exe, DraftField::Exe)),
                Length::FillPortion(1)
            ),
        ]
        .spacing(8);

        let base = layout::base_dir(&d.output, &self.settings.default_output_dir);
        let effective = layout::preview(base, d.folder_layout, &d.name, None).to_string();
        let global = &self.settings.default_output_dir;
        let global_hint = std::path::absolute(global.trim()).map_or_else(|_| global.clone(), |p| p.display().to_string());
        let output_row = row![
            cell(labeled("Output folder", field(&global_hint, &d.output, DraftField::Output)), Length::FillPortion(1)),
            cell(
                column![
                    muted(&s, " "),
                    row![
                        button(btn_label(&s, "Browse").size(12))
                            .on_press(Msg::BrowseSiteOutput(id))
                            .padding([6.0, 12.0])
                            .style(skin::btn(s, BtnKind::Ghost)),
                        button(btn_label(&s, "Open").size(12))
                            .on_press(Msg::OpenSiteOutput(id))
                            .padding([6.0, 12.0])
                            .style(skin::btn(s, BtnKind::Ghost)),
                    ]
                    .spacing(6),
                ]
                .spacing(2)
                .into(),
                Length::Shrink
            ),
            cell(
                labeled(
                    "Subfolders",
                    pick_list(FolderLayout::ALL, Some(d.folder_layout), move |l| Msg::Draft(
                        id,
                        DraftField::FolderLayout(l)
                    ))
                    .padding(5)
                    .text_size(13)
                    .width(Length::Fill)
                    .into(),
                ),
                Length::Fixed(200.0)
            ),
            cell(
                labeled(
                    "Effective path",
                    container(muted(&s, effective)).padding([5.0, 0.0]).into()
                ),
                Length::FillPortion(1)
            ),
        ]
        .spacing(8);

        let timing = row![
            cell(
                labeled(
                    "Consider a stream ended after (minutes)",
                    field(&DEFAULT_END_AFTER_MINUTES.to_string(), &d.end_after, DraftField::EndAfter)
                ),
                Length::FillPortion(1)
            ),
            container(
                toggler(d.auto_suffix)
                    .label("Name restarted recordings _partN")
                    .on_toggle(move |v| Msg::Draft(id, DraftField::AutoSuffix(v)))
                    .text_size(12)
            )
            .width(Length::FillPortion(2)),
        ]
        .spacing(8)
        .align_y(Alignment::End);

        let split_input = text_input("", &d.split_minutes)
            .on_input(move |v| Msg::Draft(id, DraftField::SplitMinutes(v)))
            .padding(5)
            .size(13)
            .style(skin::input_required(s, d.split_invalid()));
        let splitting = row![
            container(
                toggler(d.split_enabled)
                    .label("Split recordings")
                    .on_toggle(move |v| Msg::Draft(id, DraftField::SplitEnabled(v)))
                    .text_size(12)
            )
            .width(Length::Shrink),
            cell(labeled("Split the recording after (minutes)", split_input.into()), Length::Fixed(260.0)),
        ]
        .spacing(24)
        .align_y(Alignment::End);

        let upgrade_quality = toggler(d.upgrade_quality)
            .label("Upgrade quality when available")
            .on_toggle(move |v| Msg::Draft(id, DraftField::UpgradeQuality(v)))
            .text_size(12);

        let quick_check = row![
            container(
                toggler(d.quick_check)
                    .label("Quick check ended streams")
                    .on_toggle(move |v| Msg::Draft(id, DraftField::QuickCheck(v)))
                    .text_size(12)
            )
            .width(Length::Shrink),
            cell(
                labeled(
                    "Quick check after (seconds)",
                    field(&DEFAULT_QUICK_CHECK_SECS.to_string(), &d.quick_check_secs, DraftField::QuickCheckSecs)
                ),
                Length::Fixed(260.0)
            ),
        ]
        .spacing(24)
        .align_y(Alignment::End);

        let swatch = if d.use_theme_color { s.accent2 } else { skin::parse_hex(&d.color).unwrap_or(s.muted) };
        let mut color_row: Row<'a, Msg> = row![
            muted(&s, "Site color"),
            container(Space::new().width(22).height(22)).style(skin::dot(swatch, 0.0)),
            text_input("#RRGGBB", &d.color)
                .on_input_maybe(
                    (!d.use_theme_color).then(|| move |v: String| Msg::Draft(id, DraftField::Color(v)))
                )
                .padding(5)
                .size(13)
                .width(100)
                .style(skin::input_required(s, !d.color_ok())),
            toggler(d.use_theme_color)
                .label("Use theme color")
                .on_toggle(move |v| Msg::Draft(id, DraftField::UseThemeColor(v)))
                .text_size(12),
        ]
        .spacing(10)
        .align_y(Alignment::Center);
        for hex in SITE_SWATCHES {
            let c = skin::parse_hex(hex).unwrap_or(s.muted);
            let selected = !d.use_theme_color && skin::parse_hex(&d.color) == Some(c);
            color_row = color_row.push(
                button(Space::new().width(16).height(16))
                    .on_press(Msg::Draft(id, DraftField::PickColor(hex.to_string())))
                    .padding(0)
                    .style(skin::swatch(s, c, selected)),
            );
        }
        if !d.color_ok() {
            color_row = color_row.push(muted(&s, "Use a hex color such as #9146FF."));
        }

        let templates = row![
            cell(
                required_field(
                    "URL template (example: https://twitch.tv/{username})",
                    &d.url_template,
                    DraftField::UrlTemplate
                ),
                Length::FillPortion(1)
            ),
            cell(
                labeled(
                    "Checker arguments ( {urls} = list of streamer's URL )",
                    field(DEFAULT_CHECKER_ARGS, &d.checker, DraftField::Checker)
                ),
                Length::FillPortion(1)
            ),
        ]
        .spacing(8);
        let mut templates = column![templates].spacing(2);
        if !d.checker.trim().is_empty() && CheckOutput::detect(&d.checker).is_none() {
            templates = templates.push(warn_text(&s, CHECKER_FLAG_WARNING));
        }

        let mut recorder_col = column![labeled(
            "Recorder arguments ( {url} = the streamer's URL )",
            field(DEFAULT_RECORDER_ARGS, &d.recorder, DraftField::Recorder)
        )]
        .spacing(2);
        if !d.recorder.trim().is_empty() && !has_no_part(&d.recorder) {
            recorder_col = recorder_col.push(
                iced::widget::text("Add --no-part: otherwise the recording file can't be verified, so Open and STALLED checks stay off.")
                    .font(s.font)
                    .size(12)
                    .color(s.warn),
            );
        }

        let count = self
            .settings
            .streamers
            .iter()
            .filter(|x| x.site_id == id)
            .count();
        let log = |label: &'static str, on: bool, f: fn(bool) -> DraftField| {
            toggler(on)
                .label(label)
                .on_toggle(move |v| Msg::Draft(id, f(v)))
                .text_size(12)
        };
        let mut actions: Row<'a, Msg> = row![
            log("Pause checking", d.paused, DraftField::Paused),
            Space::new().width(Length::Fill),
        ]
        .spacing(10)
        .align_y(Alignment::Center);
        if saved.is_some() {
            actions = actions
                .push(if dirty {
                    chip(s, "unsaved changes", s.warn)
                } else {
                    Space::new().into()
                })
                .push(muted(&s, format!("{count} streamer(s)")))
                .push(
                    button(btn_label(&s, "Save changes").size(13))
                        .on_press_maybe((dirty && valid).then_some(Msg::SaveSite(id)))
                        .padding([5.0, 12.0])
                        .style(skin::btn(s, BtnKind::Primary)),
                )
                .push(
                    button(t(&s, "Revert").size(13))
                        .on_press_maybe(dirty.then_some(Msg::RevertSite(id)))
                        .padding([5.0, 12.0])
                        .style(skin::btn(s, BtnKind::Ghost)),
                )
                .push(
                    button(t(&s, "Defaults").size(13))
                        .on_press(Msg::DefaultsSite(id))
                        .padding([5.0, 12.0])
                        .style(skin::btn(s, BtnKind::Ghost)),
                );
        } else {
            actions = actions
                .push(
                    button(btn_label(&s, "Add site").size(13))
                        .on_press_maybe(valid.then_some(Msg::SaveSite(id)))
                        .padding([5.0, 12.0])
                        .style(skin::btn(s, BtnKind::Primary)),
                )
                .push(
                    button(t(&s, "Discard").size(13))
                        .on_press(Msg::DiscardSite)
                        .padding([5.0, 12.0])
                        .style(skin::btn(s, BtnKind::Ghost)),
                );
        }

        let mut form = Column::new().spacing(6).push(identity).push(output_row);
        if d.folder_layout != FolderLayout::Flat && layout::has_absolute_template(&d.recorder) {
            form = form.push(warn_text(&s, ABSOLUTE_TEMPLATE_WARNING));
        }
        let form = form
            .push(timing)
            .push(splitting)
            .push(upgrade_quality)
            .push(quick_check)
            .push(color_row)
            .push(templates)
            .push(recorder_col)
            .push(actions);
        container(form)
            .padding(10)
            .width(Length::Fill)
            .style(skin::card(s, None))
            .into()
    }

    fn log_tab(&self, s: Skin) -> Element<'_, Msg> {
        let switch = |label: &'static str, which: LogTab| {
            let on = self.log_tab == which;
            let kind = if on { BtnKind::TabActive } else { BtnKind::TabIdle };
            button(t(&s, label).size(13).color(if on { s.bg } else { s.muted }))
                .on_press(Msg::SetLogTab(which))
                .padding([6.0, 16.0])
                .style(skin::btn(s, kind))
        };
        let switcher = row![switch("Engine", LogTab::Engine), switch("Output", LogTab::Output)].spacing(8);
        let body = match self.log_tab {
            LogTab::Engine => self.log_view(s),
            LogTab::Output => self.logs_output(s),
        };
        column![switcher, body].spacing(12).height(Length::Fill).into()
    }

    fn log_view(&self, s: Skin) -> Element<'_, Msg> {
        let mut col = Column::new().spacing(4);
        if self.log.is_empty() {
            col = col.push(muted(&s, "Nothing logged yet."));
        }
        for line in &self.log {
            col = col.push(
                iced::widget::text(line.as_str())
                    .font(iced::Font::MONOSPACE)
                    .size(12)
                    .color(s.text),
            );
        }
        let title = row![self.section_title(s, "ENGINE LOG")];
        container(column![title, scrollable(col).height(Length::Fill)].spacing(10))
            .padding(14)
            .width(Length::Fill)
            .height(Length::Fill)
            .style(skin::card(s, None))
            .into()
    }
}

/// Shown when the checker arguments cannot tell the app how to read yt-dlp's output.
const CHECKER_FLAG_WARNING: &str = "Add --dump-json or --simulate: otherwise no live streams can be detected.";

/// Shown when an absolute `--output` path stops the subfolder setting from applying.
const ABSOLUTE_TEMPLATE_WARNING: &str = "The recorder arguments use an absolute --output path, so subfolders are not applied.";

pub(super) fn warn_text<'a>(s: &Skin, v: impl IntoFragment<'a>) -> Text<'a> {
    iced::widget::text(v).font(s.font).size(12).color(s.warn)
}

/// One entry in a streamer's subfolder dropdown: a layout, or `None` to inherit the site's.
#[derive(Clone, PartialEq)]
struct LayoutChoice {
    value: Option<FolderLayout>,
    label: String,
}

impl std::fmt::Display for LayoutChoice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.label)
    }
}

/// One entry in a streamer's notification dropdown: on, off, or `None` to inherit the site's.
#[derive(Clone, PartialEq)]
struct NotifyChoice {
    value: Option<bool>,
    label: String,
}

impl std::fmt::Display for NotifyChoice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.label)
    }
}

fn on_off(on: bool) -> &'static str {
    if on { "On" } else { "Off" }
}

/// True when the recorder arguments contain a `--no-part` token.
fn has_no_part(args: &str) -> bool {
    args.split_whitespace().any(|t| t == "--no-part")
}

/// Wraps content in the standard settings card.
pub(super) fn card_box<'a>(s: Skin, content: impl Into<Element<'a, Msg>>) -> Element<'a, Msg> {
    container(content)
        .padding(14)
        .width(Length::Fill)
        .style(skin::card(s, None))
        .into()
}
