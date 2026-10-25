//! View functions for every screen.

use super::skin::{self, BtnKind, Skin, ThemeKind, pulse};
use super::{App, DraftField, Msg, OptionField, Preset, SiteDraft, Tab, prefs};
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
const NAME_MAX_CHARS: usize = 20;
const NAME_CHAR_W: f32 = 0.45;
const PW_SITE: u32 = 90;
const PW_STATE: u32 = 110;
const PW_UP: u32 = 80;
const PW_YT: u32 = 140;
const PW_FF: u32 = 140;

/// Sizes of one compact row, interpolated from its height; the name column fits the longest name (max 20 chars).
#[derive(Clone, Copy)]
struct RowMetrics {
    row_h: f32,
    name: f32,
    url: f32,
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
    fn new(row_h: f32, name_chars: usize) -> RowMetrics {
        let t = ((row_h - ROW_FLOOR) / (ROW_CEIL - ROW_FLOOR)).max(0.0);
        let at = |floor: f32, ceil: f32| floor + (ceil - floor) * t;
        let text = |floor: f32, ceil: f32| (at(floor, ceil) * 2.0).round() / 2.0;
        let name = text(11.0, 16.0);
        RowMetrics {
            row_h,
            name,
            url: text(10.0, 12.0),
            chip: text(9.0, 11.0),
            chip_pad_v: at(0.0, 4.0),
            chip_pad_h: at(6.0, 10.0),
            dot: at(7.0, 10.0).round(),
            toggler: at(12.0, 18.0).round(),
            site_w: at(76.0, 110.0).round(),
            name_w: (name_chars.min(NAME_MAX_CHARS) as f32 * name * NAME_CHAR_W).ceil() + 2.0,
            spacing: at(8.0, 12.0),
            pad_h: at(6.0, 12.0),
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
        chips = chips.push(mini_chip(s, m, "RECORDING", s.accent));
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
            format!("restarted {}×", status.restarts),
            s.warn,
        ));
    }
    if st.options.log_stdout || st.options.log_stderr {
        chips = chips.push(mini_chip(s, m, "logging", s.muted));
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
        container(column![self.header(s), self.tabs(s), body].spacing(14))
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
                            .on_press(Msg::SetTab(Tab::Settings))
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
        let avail = (self.win_h - COMPACT_CHROME_H - COMPACT_SLACK_H - extra_h).max(0.0);
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
        let extra_h = problems.len() as f32 * 32.0 + notices_h;
        let (cols, row_h) = self.compact_layout(entries.len(), extra_h);
        let longest = entries
            .iter()
            .map(|(_, st)| st.name.chars().count())
            .max()
            .unwrap_or(0);
        let m = RowMetrics::new(row_h, longest);
        let per_col = entries.len().div_ceil(cols);
        let mut grid = Row::new().spacing(14);
        for chunk in entries.chunks(per_col) {
            let mut c = Column::new().spacing(ROW_GAP).width(Length::Fill);
            for (i, &(site, st)) in chunk.iter().enumerate() {
                c = c.push(self.compact_row(s, m, site, st, i % 2 == 1));
            }
            grid = grid.push(c);
        }
        col.push(grid)
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
            container(mini_chip(s, m, clip(&site.name, 12), s.accent2))
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
                iced::widget::text(st.url.as_str())
                    .font(s.font)
                    .size(m.url)
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
            container(
                iced::widget::text(clip(&p.site_name, 12))
                    .font(s.font)
                    .size(12)
                    .color(s.accent2)
                    .wrapping(Wrapping::None)
            )
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
                .color(s.accent2),
        );
        if site.log_checker_stdout || site.log_checker_stderr {
            head = head.push(chip(s, "logging", s.muted));
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
            chips = chips.push(chip(s, format!("restarted {}×", status.restarts), s.warn));
        }
        if st.options.log_stdout || st.options.log_stderr {
            chips = chips.push(chip(s, "logging", s.muted));
        }
        let line = row![
            dot(s, color, glow),
            column![
                iced::widget::text(clip(&st.name, 40))
                    .font(s.font)
                    .size(15)
                    .color(if st.enabled { s.text } else { s.muted }),
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
        let mut col = Column::new().spacing(12).padding(iced::Padding {
            right: 8.0,
            ..Default::default()
        });

        let global = column![
            row![
                self.section_title(s, "GLOBAL OUTPUT FOLDER"),
                Space::new().width(Length::Fill),
                button(t(&s, "Open settings folder").size(13))
                    .on_press(Msg::OpenConfigDir)
                    .padding([8.0, 14.0])
                    .style(skin::btn(s, BtnKind::Ghost)),
            ]
            .align_y(Alignment::Center),
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
            muted(&s, "Used by any site that has no output folder of its own."),
        ]
        .spacing(8);
        col = col.push(
            container(global)
                .padding(14)
                .width(Length::Fill)
                .style(skin::card(s, None)),
        );

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
                .width(260),
                muted(&s, format!("{value:.0} px")),
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
            .width(260),
            muted(&s, format!("{:.0} px wide", self.two_col_w)),
        ]
        .spacing(12)
        .align_y(Alignment::Center);
        let compact = column![
            self.section_title(s, "COMPACT VIEW"),
            muted(&s, "Rows start at the maximum and shrink toward the minimum as streamers are added."),
            muted(&s, "Compact view shows two columns when the window is at least the column threshold wide, otherwise one."),
            row_size("Row Size Max", self.row_max, Msg::SetRowMax),
            row_size("Row Size Min", self.row_min, Msg::SetRowMin),
            two_col,
            row![
                Space::new().width(Length::Fill),
                button(t(&s, "Reset").size(12))
                    .on_press(Msg::ResetRowRange)
                    .padding([5.0, 10.0])
                    .style(skin::btn(s, BtnKind::Ghost)),
            ]
            .align_y(Alignment::Center),
        ]
        .spacing(8);
        col = col.push(
            container(compact)
                .padding(14)
                .width(Length::Fill)
                .style(skin::card(s, None)),
        );

        let help = column![
            self.section_title(s, "COMMAND PLACEHOLDERS"),
            muted(&s, "Checker: {urls} becomes every URL monitored for the site (appended if missing)."),
            muted(&s, "Recorder: {url} live stream URL · {streamer_url} the URL you added · {name} streamer name · {title} stream title · {id} video id · {output_dir} output folder."),
            muted(&s, "--simulate mode expects lines like original_url|webpage_url|id|title, e.g. from --print with --match-filter is_live."),
        ]
        .spacing(5);
        col = col.push(
            container(help)
                .padding(14)
                .width(Length::Fill)
                .style(skin::card(s, None)),
        );

        let add = row![
            iced::widget::text("ADD SITE")
                .font(s.font)
                .size(13)
                .color(s.accent2),
            button(t(&s, "Twitch").size(13))
                .on_press(Msg::AddSite(Preset::Twitch))
                .padding([7.0, 14.0])
                .style(skin::btn(s, BtnKind::Ghost)),
            button(t(&s, "Tiktok").size(13))
                .on_press(Msg::AddSite(Preset::Tiktok))
                .padding([7.0, 14.0])
                .style(skin::btn(s, BtnKind::Ghost)),
            button(t(&s, "Chaturbate").size(13))
                .on_press(Msg::AddSite(Preset::Chaturbate))
                .padding([7.0, 14.0])
                .style(skin::btn(s, BtnKind::Ghost)),
            button(t(&s, "Kick").size(13))
                .on_press(Msg::AddSite(Preset::Kick))
                .padding([7.0, 14.0])
                .style(skin::btn(s, BtnKind::Ghost)),
            button(t(&s, "Blank").size(13))
                .on_press(Msg::AddSite(Preset::Blank))
                .padding([7.0, 14.0])
                .style(skin::btn(s, BtnKind::Ghost)),
        ]
        .spacing(10)
        .align_y(Alignment::Center);
        col = col.push(add);

        for site in &self.settings.sites {
            if let Some(d) = self.drafts.get(&site.id) {
                col = col.push(self.site_editor(s, site, d));
            }
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
        let dirty = *d != SiteDraft::from_site(site);
        let valid = d.interval_secs().is_some() && !d.name.trim().is_empty();

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

        let form = column![
            top,
            labeled(
                "Path to yt-dlp executable",
                field("yt-dlp or C:\\tools\\yt-dlp.exe", &d.exe, DraftField::Exe)
            ),
            labeled(
                "URL template",
                field(
                    "https://twitch.tv/{username}",
                    &d.url_template,
                    DraftField::UrlTemplate
                )
            ),
            labeled(
                "Checker command arguments",
                field("--dump-json {urls}", &d.checker, DraftField::Checker)
            ),
            labeled(
                "Recorder command arguments",
                field(
                    "-o \"%(title)s.%(ext)s\" {url}",
                    &d.recorder,
                    DraftField::Recorder
                )
            ),
            labeled(
                "Output folder (blank = global)",
                field(
                    "Use the global output folder",
                    &d.output,
                    DraftField::Output
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
