//! View functions for every screen.

use super::skin::{self, BtnKind, Skin, ThemeKind, pulse};
use super::{
    App, DraftField, Msg, OptionField, Preset, SITE_SWATCHES, SettingsTab, SiteDraft, Tab, UpdateStage, prefs,
};
use crate::api::*;
use iced::widget::text::{IntoFragment, Wrapping};
use iced::widget::{
    Column, Row, Space, Text, button, column, container, pick_list, row, scrollable, slider,
    text_input, toggler,
};
use iced::{Alignment, Color, Element, Length};

fn t<'a>(s: &Skin, v: impl IntoFragment<'a>) -> Text<'a> {
    iced::widget::text(v).font(s.font).color(s.text)
}

/// Button label that inherits the button style's text colour.
fn btn_label<'a>(s: &Skin, v: impl IntoFragment<'a>) -> Text<'a> {
    iced::widget::text(v).font(s.font)
}

fn muted<'a>(s: &Skin, v: impl IntoFragment<'a>) -> Text<'a> {
    iced::widget::text(v).font(s.font).color(s.muted).size(12)
}

fn chip<'a>(s: Skin, label: impl IntoFragment<'a>, color: Color) -> Element<'a, Msg> {
    container(iced::widget::text(label).font(s.font).size(11).color(color))
        .padding([3.0, 10.0])
        .style(skin::chip(s, color))
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

/// Hours and minutes only, for compact rows.
fn fmt_dur_short(secs: u64) -> String {
    let (h, m) = (secs / 3600, (secs / 60) % 60);
    if h > 0 {
        format!("{h}h {m:02}m")
    } else if m > 0 {
        format!("{m}m")
    } else {
        "<1m".to_string()
    }
}

fn clip(s: &str, max: usize) -> String {
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
const COLUMN_FRAME: f32 = 1.0;
const COLUMN_GAP: f32 = 14.0;
const NAME_MAX_CHARS: usize = 20;
const NAME_CHAR_W: f32 = 0.6;
const SITE_CLIP_CHARS: usize = 12;
const SITE_CHAR_W: f32 = 0.6;
const PW_SITE: u32 = 104;
const PW_STATE: u32 = 110;
const PW_UP: u32 = 80;
const PW_YT: u32 = 140;
const PW_FF: u32 = 140;

/// Sizes of one compact row, interpolated from its height; name and site columns fit the longest name and site chip.
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
    name_w: f32,
    spacing: f32,
    pad_h: f32,
}

impl RowMetrics {
    fn new(row_h: f32, name_chars: usize, site_chars: usize, pad_trim: f32) -> RowMetrics {
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
            name_w: (name_chars.min(NAME_MAX_CHARS) as f32 * name * NAME_CHAR_W).ceil() + 2.0,
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
) -> Row<'a, Msg> {
    let mut chips: Row<'a, Msg> = Row::new()
        .spacing(m.spacing / 2.0)
        .align_y(Alignment::Center);
    if status.recording {
        chips = chips.push(mini_chip(s, m, "REC", s.accent));
    }
    if status.live {
        chips = chips.push(mini_chip(s, m, "LIVE", s.live));
    }
    if !status.live && !status.recording {
        chips = chips.push(mini_chip(s, m, "OFFLINE", s.muted));
    }
    if !st.enabled {
        chips = chips.push(mini_chip(s, m, "PAUSED", s.warn));
    }
    if status.restarts > 0 {
        chips = chips.push(mini_chip(
            s,
            m,
            format!("RESTARTED {}×", status.restarts),
            s.warn,
        ));
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
            Tab::Monitor => self.monitor(s),
            Tab::Processes => self.processes(s),
            Tab::Settings => self.settings_tab(s),
            Tab::Log => self.log_view(s),
        };
        let mut page = Column::new().spacing(14).push(self.header(s));
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
        let p = pulse(self.phase);
        let title = row![
            dot(s, s.accent, 0.4 + 0.6 * p),
            iced::widget::text("jj-dlp")
                .font(s.font)
                .size(22)
                .color(s.accent2),
        ]
        .spacing(12)
        .align_y(Alignment::Center);
        let chips = row![
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
        let right = row![pick_list(ThemeKind::ALL, Some(self.kind), Msg::ThemeChosen).padding(8),]
            .spacing(10)
            .align_y(Alignment::Center);
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
        let tab = |label: &'static str, which: Tab| {
            let kind = if self.tab == which {
                BtnKind::TabActive
            } else {
                BtnKind::TabIdle
            };
            button(
                t(&s, label)
                    .size(14)
                    .color(if self.tab == which { s.bg } else { s.muted }),
            )
            .on_press(Msg::SetTab(which))
            .padding([7.0, 18.0])
            .style(skin::btn(s, kind))
        };
        row![
            tab("Monitor", Tab::Monitor),
            tab("Processes", Tab::Processes),
            tab("Settings", Tab::Settings),
            tab("Log", Tab::Log)
        ]
        .spacing(8)
        .into()
    }

    fn section_title<'a>(&self, s: Skin, label: &'a str) -> Element<'a, Msg> {
        iced::widget::text(label)
            .font(s.font)
            .size(13)
            .color(s.accent2)
            .into()
    }

    fn monitor(&self, s: Skin) -> Element<'_, Msg> {
        let title = row![
            self.section_title(s, "STREAMERS"),
            Space::new().width(Length::Fill),
            toggler(self.compact)
                .label("Compact view")
                .on_toggle(Msg::SetCompact)
                .text_size(12),
        ]
        .padding(iced::Padding {
            right: 8.0,
            ..Default::default()
        })
        .align_y(Alignment::Center);
        let mut col = Column::new().spacing(12).padding(iced::Padding {
            right: 8.0,
            ..Default::default()
        });
        if !self.notices.is_empty() {
            col = col.push(self.setup_notices(s));
        }
        if self.settings.sites.is_empty() {
            col = col.push(
                container(
                    column![
                        t(&s, "No sites configured yet."),
                        muted(&s, "A site holds the yt-dlp path plus the checker and recorder commands for a group of streamers."),
                        button(btn_label(&s, "Add a site").size(13))
                            .on_press(Msg::OpenAddSite)
                            .padding([8.0, 14.0])
                            .style(skin::btn(s, BtnKind::Primary)),
                    ]
                    .spacing(10),
                )
                .padding(16)
                .width(Length::Fill)
                .style(skin::card(s, None)),
            );
        }
        if self.compact {
            col = self.compact_list(s, col);
        } else {
            for site in &self.settings.sites {
                col = col.push(self.site_block(s, site));
            }
        }
        column![title, scrollable(col).height(Length::Fill)]
            .spacing(12)
            .into()
    }

    /// Picks two columns when the window is at least the column threshold wide, else one; sizes rows to fit.
    fn compact_layout(&self, rows: usize, extra_h: f32) -> (usize, f32) {
        let avail = (self.win_h - COMPACT_CHROME_H - COMPACT_SLACK_H - extra_h - 2.0 * COLUMN_FRAME).max(0.0);
        let cols = if self.win_w >= self.two_col_w { 2 } else { 1 };
        let per_col = rows.div_ceil(cols).max(1);
        let fit = ((avail + ROW_GAP) / per_col as f32 - ROW_GAP).floor();
        (cols, fit.clamp(self.row_min, self.row_max))
    }

    fn compact_list<'a>(&'a self, s: Skin, mut col: Column<'a, Msg>) -> Column<'a, Msg> {
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
        for p in &problems {
            col = col.push(self.warning_line(s, p));
        }
        let mut entries: Vec<(&'a Site, &'a Streamer)> = Vec::new();
        for site in &self.settings.sites {
            for st in self
                .settings
                .streamers
                .iter()
                .filter(|x| x.site_id == site.id)
            {
                entries.push((site, st));
            }
        }
        if entries.is_empty() {
            if !self.settings.sites.is_empty() {
                col = col.push(muted(
                    &s,
                    "No streamers yet. Turn off Compact view to add one.",
                ));
            }
            return col;
        }
        let notices_h = if self.notices.is_empty() {
            0.0
        } else {
            48.0 + 80.0 * self.notices.len() as f32
        };
        let banner_h = if self.update.is_some() { 70.0 } else { 0.0 };
        let extra_h = problems.len() as f32 * 32.0 + notices_h + banner_h;
        let (cols, row_h) = self.compact_layout(entries.len(), extra_h);
        let longest = entries
            .iter()
            .map(|(_, st)| st.name.chars().count())
            .max()
            .unwrap_or(0);
        let longest_site = entries
            .iter()
            .map(|(site, _)| clip(&site.name, SITE_CLIP_CHARS).chars().count())
            .max()
            .unwrap_or(0);
        let frame = COLUMN_FRAME;
        let pad_trim = if cols == 1 { frame } else { 0.0 };
        let m = RowMetrics::new(row_h, longest, longest_site, pad_trim);
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

    /// Seconds the streamer has been live; only while the checker reports it live.
    fn live_for(&self, id: StreamerId, status: StreamerStatus) -> Option<u64> {
        if !status.live {
            return None;
        }
        self.live_since.get(&id).map(|t| self.now.saturating_sub(*t))
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
            container(
                iced::widget::text(st.name.as_str())
                    .font(s.font)
                    .size(m.name)
                    .color(name_color)
                    .wrapping(Wrapping::None)
            )
            .width(m.name_w)
            .clip(true),
            container(
                iced::widget::text(
                    self.live_for(id, status).map(|d| format!("live {}", fmt_dur_short(d))).unwrap_or_default()
                )
                .font(s.font)
                .size(m.info)
                .color(s.muted)
                .wrapping(Wrapping::None)
            )
            .width(Length::Fill)
            .clip(true),
            compact_chips(s, m, st, status),
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
            .style(skin::stripe(s, shaded))
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
            RecorderState::Running => ("RECORDING".to_string(), s.live),
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
            dot(s, state_color, glow),
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
            container(chip(s, state_label, state_color)).width(PW_STATE),
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
        button(line)
            .on_press(Msg::ToggleProc(p.id))
            .padding([6.0, 12.0])
            .width(Length::Fill)
            .style(skin::tile(
                s,
                if matches!(p.state, RecorderState::Exited(_)) {
                    s.muted
                } else {
                    state_color
                },
                open,
            ))
            .into()
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
        if !p.last_line.is_empty() {
            body = body.push(
                container(
                    iced::widget::text(p.last_line.as_str())
                        .font(s.font)
                        .size(12)
                        .color(s.text),
                )
                .padding([6.0, 10.0])
                .width(Length::Fill)
                .style(skin::inset(s)),
            );
        }
        body = body.push(muted(&s, p.cmdline.as_str()));
        container(body)
            .padding([8.0, 14.0])
            .width(Length::Fill)
            .style(skin::inset(s))
            .into()
    }

    fn site_block<'a>(&'a self, s: Skin, site: &'a Site) -> Element<'a, Msg> {
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
        let head = head.push(Space::new().width(Length::Fill)).push(check);

        let mut col = Column::new().spacing(8).push(head);
        if let Some(p) = self.checks.get(&site.id).and_then(|c| c.problem.as_deref()) {
            col = col.push(self.warning_line(s, p));
        }
        let streamers: Vec<&Streamer> = self
            .settings
            .streamers
            .iter()
            .filter(|x| x.site_id == site.id)
            .collect();
        if streamers.is_empty() {
            col = col.push(muted(&s, "No streamers yet; add one below."));
        }
        for st in streamers {
            col = col.push(self.streamer_row(s, st));
        }
        let (name, username) = self.new_streamer.get(&site.id).cloned().unwrap_or_default();
        let id = site.id;
        let add = row![
            text_input("Display Name (optional)", &name)
                .on_input(move |v| Msg::NewStreamerName(id, v))
                .on_submit(Msg::AddStreamer(id))
                .padding(8)
                .width(230)
                .style(skin::input(s)),
            text_input("Streamer username", &username)
                .on_input(move |v| Msg::NewStreamerUsername(id, v))
                .on_submit(Msg::AddStreamer(id))
                .padding(8)
                .width(Length::Fill)
                .style(skin::input(s)),
            button(btn_label(&s, "Add streamer").size(13))
                .on_press(Msg::AddStreamer(id))
                .padding([8.0, 14.0])
                .style(skin::btn(s, BtnKind::Primary)),
        ]
        .spacing(8);
        col = col.push(add);
        container(col)
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
        if status.recording {
            chips = chips.push(chip(s, "RECORDING", s.accent));
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
        if status.restarts > 0 {
            chips = chips.push(chip(s, format!("RESTARTED {}×", status.restarts), s.warn));
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
                    if let Some(d) = self.live_for(id, status) {
                        name = name.push(muted(&s, format!("live for {}", fmt_dur(d))));
                    }
                    name
                },
                muted(&s, clip(&st.url, 70)),
            ]
            .spacing(2)
            .width(Length::Fill),
            chips,
            toggler(st.enabled)
                .label("Record")
                .on_toggle(move |v| Msg::ToggleStreamer(id, v))
                .text_size(12),
            button(t(&s, "Settings").size(12))
                .on_press(Msg::ToggleStreamerSettings(id))
                .padding([5.0, 10.0])
                .style(skin::btn(s, BtnKind::Ghost)),
            button(t(&s, "Open").size(12))
                .on_press(Msg::OpenOutputDir(st.site_id))
                .padding([5.0, 10.0])
                .style(skin::btn(s, BtnKind::Ghost)),
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
            .style(skin::inset(s))
            .into()
    }

    fn streamer_settings_panel<'a>(&'a self, s: Skin, st: &'a Streamer) -> Element<'a, Msg> {
        let id = st.id;
        let log = |label: &'static str, on: bool, f: fn(bool) -> OptionField| {
            toggler(on)
                .label(label)
                .on_toggle(move |v| Msg::StreamerOption(id, f(v)))
                .text_size(12)
        };
        column![
            muted(
                &s,
                "Output is appended to the logs folder next to settings.json."
            ),
            row![
                log(
                    "Log recorder stdout",
                    st.options.log_stdout,
                    OptionField::LogStdout
                ),
                log(
                    "Log recorder stderr",
                    st.options.log_stderr,
                    OptionField::LogStderr
                ),
            ]
            .spacing(24),
        ]
        .spacing(8)
        .into()
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
            switch("Sites", SettingsTab::Sites),
            switch("General", SettingsTab::General)
        ]
        .spacing(8);
        let body = match self.settings_tab {
            SettingsTab::Sites => self.settings_sites(s),
            SettingsTab::General => self.settings_general(s),
        };
        column![switcher, body].spacing(12).height(Length::Fill).into()
    }

    fn settings_sites(&self, s: Skin) -> Element<'_, Msg> {
        let mut col = Column::new().spacing(12).padding(iced::Padding {
            right: 8.0,
            ..Default::default()
        });
        let presets = [
            ("Twitch", Preset::Twitch),
            ("Tiktok", Preset::Tiktok),
            ("Chaturbate", Preset::Chaturbate),
            ("Kick", Preset::Kick),
            ("Blank", Preset::Blank),
        ];
        let mut add = row![self.section_title(s, "ADD SITE")]
            .spacing(10)
            .align_y(Alignment::Center);
        for (label, preset) in presets {
            add = add.push(
                button(t(&s, label).size(13))
                    .on_press(Msg::AddSite(preset))
                    .padding([7.0, 14.0])
                    .style(skin::btn(s, BtnKind::Ghost)),
            );
        }
        col = col.push(add);
        if self.settings.sites.is_empty() {
            col = col.push(muted(&s, "Pick a preset to start; every field stays editable afterwards."));
        }
        for site in &self.settings.sites {
            if let Some(d) = self.drafts.get(&site.id) {
                col = col.push(self.site_editor(s, site, d));
            }
        }
        scrollable(col).height(Length::Fill).into()
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
                text_input("Recordings folder", &self.global_out)
                    .on_input(Msg::GlobalOutEdit)
                    .on_submit(Msg::GlobalOutSave)
                    .padding(8)
                    .style(skin::input(s)),
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

        let row_size = |label: &'static str, value: f32, on_change: fn(f32) -> Msg| {
            row![
                t(&s, label).size(13).width(110),
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
            t(&s, "Column threshold").size(13).width(110),
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
        let compact = column![
            row![
                self.section_title(s, "COMPACT VIEW"),
                Space::new().width(Length::Fill),
                button(t(&s, "Reset").size(12))
                    .on_press(Msg::ResetRowRange)
                    .padding([5.0, 10.0])
                    .style(skin::btn(s, BtnKind::Ghost)),
            ]
            .align_y(Alignment::Center),
            muted(&s, "Rows start at the maximum and shrink toward the minimum as streamers are added."),
            muted(&s, "Two columns are used when the window is at least the column threshold wide."),
            row_size("Row Size Max", self.row_max, Msg::SetRowMax),
            row_size("Row Size Min", self.row_min, Msg::SetRowMin),
            two_col,
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

        let compact = card_box(s, compact);
        let updates = card_box(s, updates);
        if self.win_w >= 1000.0 {
            col = col.push(row![compact, updates].spacing(12));
        } else {
            col = col.push(compact).push(updates);
        }
        scrollable(col).height(Length::Fill).into()
    }

    fn site_editor<'a>(&'a self, s: Skin, site: &'a Site, d: &'a SiteDraft) -> Element<'a, Msg> {
        let id = site.id;
        let labeled = |label: &'static str, input: Element<'a, Msg>| -> Element<'a, Msg> {
            column![muted(&s, label), input].spacing(4).into()
        };
        let field = |placeholder: &'static str,
                     value: &'a str,
                     f: fn(String) -> DraftField|
         -> Element<'a, Msg> {
            text_input(placeholder, value)
                .on_input(move |v| Msg::Draft(id, f(v)))
                .padding(8)
                .style(skin::input(s))
                .into()
        };
        let hinted = |input: Element<'a, Msg>, hint: &'static str| -> Element<'a, Msg> {
            column![input, muted(&s, hint)].spacing(4).into()
        };
        let dirty = *d != SiteDraft::from_site(site);
        let valid =
            d.interval_secs().is_some() && d.end_after_minutes().is_some() && d.color_ok() && !d.name.trim().is_empty();

        let top = row![
            container(labeled(
                "Site name",
                field("Site name", &d.name, DraftField::Name)
            ))
            .width(Length::Fill),
            container(labeled(
                "Check every (seconds, min 5)",
                field("60", &d.interval, DraftField::Interval)
            ))
            .width(210),
            container(labeled(
                "Checker mode",
                pick_list(CheckMode::ALL, Some(d.mode), move |m| Msg::Draft(
                    id,
                    DraftField::Mode(m)
                ))
                .padding(8)
                .width(Length::Fill)
                .into(),
            ))
            .width(170),
        ]
        .spacing(10);

        let count = self
            .settings
            .streamers
            .iter()
            .filter(|x| x.site_id == id)
            .count();
        let actions = row![
            button(btn_label(&s, "Save changes").size(13))
                .on_press_maybe((dirty && valid).then_some(Msg::SaveSite(id)))
                .padding([8.0, 16.0])
                .style(skin::btn(s, BtnKind::Primary)),
            button(t(&s, "Revert").size(13))
                .on_press_maybe(dirty.then_some(Msg::RevertSite(id)))
                .padding([8.0, 16.0])
                .style(skin::btn(s, BtnKind::Ghost)),
            if dirty {
                chip(s, "unsaved changes", s.warn)
            } else {
                Space::new().into()
            },
            Space::new().width(Length::Fill),
            muted(&s, format!("{count} streamer(s)")),
            button(t(&s, "Remove site").size(13))
                .on_press(Msg::RemoveSite(id))
                .padding([8.0, 16.0])
                .style(skin::btn(s, BtnKind::Danger)),
        ]
        .spacing(10)
        .align_y(Alignment::Center);

        let log = |label: &'static str, on: bool, f: fn(bool) -> DraftField| {
            toggler(on)
                .label(label)
                .on_toggle(move |v| Msg::Draft(id, f(v)))
                .text_size(12)
        };
        let logging = row![
            log("Log checker stdout", d.log_stdout, DraftField::LogStdout),
            log("Log checker stderr", d.log_stderr, DraftField::LogStderr),
        ]
        .spacing(24);

        let paths = row![
            container(labeled(
                "Path to yt-dlp executable",
                field("yt-dlp or C:\\tools\\yt-dlp.exe", &d.exe, DraftField::Exe)
            ))
            .width(Length::FillPortion(1)),
            container(labeled(
                "Output folder (blank = global)",
                field(
                    "Use the global output folder",
                    &d.output,
                    DraftField::Output
                )
            ))
            .width(Length::FillPortion(1)),
        ]
        .spacing(10);

        let swatch = if d.use_theme_color { s.accent2 } else { skin::parse_hex(&d.color).unwrap_or(s.muted) };
        let color_row = row![
            container(Space::new().width(26).height(26)).style(skin::dot(swatch, 0.0)),
            text_input("#RRGGBB", &d.color)
                .on_input_maybe(
                    (!d.use_theme_color).then(|| move |v: String| Msg::Draft(id, DraftField::Color(v)))
                )
                .padding(8)
                .width(120)
                .style(skin::input(s)),
            toggler(d.use_theme_color)
                .label("Use theme color")
                .on_toggle(move |v| Msg::Draft(id, DraftField::UseThemeColor(v)))
                .text_size(12),
        ]
        .spacing(10)
        .align_y(Alignment::Center);
        let mut swatches: Row<'a, Msg> = Row::new().spacing(6).align_y(Alignment::Center);
        for hex in SITE_SWATCHES {
            let c = skin::parse_hex(hex).unwrap_or(s.muted);
            let selected = !d.use_theme_color && skin::parse_hex(&d.color) == Some(c);
            swatches = swatches.push(
                button(Space::new().width(18).height(18))
                    .on_press(Msg::Draft(id, DraftField::PickColor(hex.to_string())))
                    .padding(0)
                    .style(skin::swatch(s, c, selected)),
            );
        }
        let mut color_col = Column::new()
            .spacing(4)
            .push(muted(&s, "Site color"))
            .push(color_row)
            .push(swatches);
        if !d.color_ok() {
            color_col = color_col.push(muted(&s, "Use a hex color such as #9146FF."));
        }
        let timing_color = row![
            container(labeled(
                "Consider a stream ended after x minutes:",
                field("30", &d.end_after, DraftField::EndAfter)
            ))
            .width(320),
            color_col,
        ]
        .spacing(20)
        .align_y(Alignment::Start);

        let form = column![
            top,
            timing_color,
            paths,
            labeled(
                "URL template",
                hinted(
                    field(
                        "https://twitch.tv/{username}",
                        &d.url_template,
                        DraftField::UrlTemplate
                    ),
                    "{username} is replaced with the name entered when adding a streamer."
                )
            ),
            labeled(
                "Checker command arguments",
                hinted(
                    field("--dump-json {urls}", &d.checker, DraftField::Checker),
                    "{urls} expands to every monitored URL (appended if missing)."
                )
            ),
            labeled(
                "Recorder command arguments",
                hinted(
                    field(
                        "-o \"%(title)s.%(ext)s\" {url}",
                        &d.recorder,
                        DraftField::Recorder
                    ),
                    "{url} is the live stream URL (appended if missing)."
                )
            ),
            logging,
            actions,
        ]
        .spacing(10);
        container(form)
            .padding(14)
            .width(Length::Fill)
            .style(skin::card(s, None))
            .into()
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
        let title = row![self.section_title(s, "ENGINE LOG (UTC)")];
        container(column![title, scrollable(col).height(Length::Fill)].spacing(10))
            .padding(14)
            .width(Length::Fill)
            .height(Length::Fill)
            .style(skin::card(s, None))
            .into()
    }
}

/// Wraps content in the standard settings card.
fn card_box<'a>(s: Skin, content: impl Into<Element<'a, Msg>>) -> Element<'a, Msg> {
    container(content)
        .padding(14)
        .width(Length::Fill)
        .style(skin::card(s, None))
        .into()
}
