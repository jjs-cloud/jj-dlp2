//! View functions for every screen.

use super::skin::{self, pulse, BtnKind, Skin, ThemeKind};
use super::{App, DraftField, Msg, Preset, SiteDraft, Tab};
use crate::api::*;
use iced::widget::text::{IntoFragment, Wrapping};
use iced::widget::{
    button, column, container, pick_list, row, scrollable, text_input, toggler, Column, Space, Text,
};
use iced::{Alignment, Color, Element, Length};

fn t<'a>(s: &Skin, v: impl IntoFragment<'a>) -> Text<'a> {
    iced::widget::text(v).font(s.font).color(s.text)
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
    container(Space::new().width(10).height(10)).style(skin::dot(color, glow)).into()
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

impl App {
    pub(super) fn view(&self) -> Element<'_, Msg> {
        let s = self.skin();
        let body: Element<'_, Msg> = match self.tab {
            Tab::Monitor => self.monitor(s),
            Tab::Processes => self.processes(s),
            Tab::Sites => self.sites(s),
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
            iced::widget::text("jj-dlp").font(s.font).size(22).color(s.accent2),
        ]
        .spacing(12)
        .align_y(Alignment::Center);
        let chips = row![
            chip(s, format!("{live} live"), if live > 0 { s.live } else { s.muted }),
            chip(s, format!("{rec} recording"), if rec > 0 { s.accent } else { s.muted }),
        ]
        .spacing(8);
        let right = row![
            pick_list(ThemeKind::ALL, Some(self.kind), Msg::ThemeChosen).padding(8),
            button(t(&s, "Open settings folder").size(13))
                .on_press(Msg::OpenConfigDir)
                .padding([8.0, 14.0])
                .style(skin::btn(s, BtnKind::Ghost)),
        ]
        .spacing(10)
        .align_y(Alignment::Center);
        container(row![title, chips, Space::new().width(Length::Fill), right].spacing(18).align_y(Alignment::Center))
            .padding([12.0, 18.0])
            .width(Length::Fill)
            .style(skin::header(s))
            .into()
    }

    fn tabs(&self, s: Skin) -> Element<'_, Msg> {
        let tab = |label: &'static str, which: Tab| {
            let kind = if self.tab == which { BtnKind::TabActive } else { BtnKind::TabIdle };
            button(t(&s, label).size(14).color(if self.tab == which { s.bg } else { s.muted }))
                .on_press(Msg::SetTab(which))
                .padding([7.0, 18.0])
                .style(skin::btn(s, kind))
        };
        row![
            tab("Monitor", Tab::Monitor),
            tab("Processes", Tab::Processes),
            tab("Sites", Tab::Sites),
            tab("Log", Tab::Log)
        ]
        .spacing(8)
        .into()
    }

    fn section_title<'a>(&self, s: Skin, label: &'a str) -> Element<'a, Msg> {
        iced::widget::text(label).font(s.font).size(13).color(s.accent2).into()
    }

    fn monitor(&self, s: Skin) -> Element<'_, Msg> {
        let mut col = Column::new().spacing(12).padding(iced::Padding { right: 8.0, ..Default::default() });
        col = col.push(self.section_title(s, "STREAMERS"));
        if self.settings.sites.is_empty() {
            col = col.push(
                container(
                    column![
                        t(&s, "No sites configured yet."),
                        muted(&s, "A site holds the yt-dlp path plus the checker and downloader commands for a group of streamers."),
                        button(t(&s, "Add a site").size(13))
                            .on_press(Msg::SetTab(Tab::Sites))
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
        for site in &self.settings.sites {
            col = col.push(self.site_block(s, site));
        }
        scrollable(col).height(Length::Fill).into()
    }

    fn processes(&self, s: Skin) -> Element<'_, Msg> {
        let mut col = Column::new().spacing(12).padding(iced::Padding { right: 8.0, ..Default::default() });
        col = col.push(self.section_title(s, "PROCESSES"));
        if self.processes.is_empty() {
            col = col.push(
                container(muted(&s, "No active recordings. Streams are recorded automatically when they go live."))
                    .padding(16)
                    .width(Length::Fill)
                    .style(skin::card(s, None)),
            );
        }
        for p in self.processes.values() {
            col = col.push(self.process_card(s, p));
        }
        scrollable(col).height(Length::Fill).into()
    }

    fn proc_row<'a>(&self, s: Skin, label: &'a str, color: Color, p: &ProcSample) -> Element<'a, Msg> {
        row![
            container(iced::widget::text(label).font(s.font).size(12).color(color)).width(88),
            iced::widget::text(format!("PID {}", p.pid)).font(s.font).size(12).color(s.text).width(96),
            iced::widget::text(format!("CPU {:.1}%", p.cpu_percent)).font(s.font).size(12).color(s.text).width(96),
            iced::widget::text(format!("MEM {}", fmt_mem(p.mem_bytes))).font(s.font).size(12).color(s.text),
        ]
        .spacing(8)
        .into()
    }

    fn process_card<'a>(&'a self, s: Skin, p: &'a ProcessInfo) -> Element<'a, Msg> {
        let ph = pulse(self.phase);
        let (state_label, state_color) = match p.state {
            RecorderState::Starting => ("STARTING".to_string(), s.warn),
            RecorderState::Running => ("RECORDING".to_string(), s.live),
            RecorderState::Stopping => ("STOPPING".to_string(), s.warn),
            RecorderState::Exited(code) => (
                format!("EXITED {} · CONFIRMING", code.map_or("?".into(), |c| c.to_string())),
                s.muted,
            ),
        };
        let glow = match p.state {
            RecorderState::Running => Some((s.accent, ph)),
            RecorderState::Starting | RecorderState::Stopping => Some((s.warn, ph)),
            RecorderState::Exited(_) => None,
        };
        let top = row![
            dot(s, state_color, if glow.is_some() { 0.4 + 0.6 * ph } else { 0.0 }),
            iced::widget::text(clip(&p.streamer_name, 40)).font(s.font).size(18).color(s.text),
            chip(s, p.site_name.clone(), s.accent2),
            chip(s, state_label, state_color),
            Space::new().width(Length::Fill),
            muted(&s, format!("up {}", fmt_dur(p.uptime_secs))),
        ]
        .spacing(10)
        .align_y(Alignment::Center);

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

        let mut card = Column::new().spacing(10).push(top).push(body);
        if !p.last_line.is_empty() {
            card = card.push(
                container(
                    iced::widget::text(clip(&p.last_line, 180))
                        .font(s.font)
                        .size(12)
                        .color(s.text)
                        .wrapping(Wrapping::None),
                )
                .clip(true)
                .padding([6.0, 10.0])
                .width(Length::Fill)
                    .style(skin::inset(s)),
            );
        }
        card = card.push(muted(&s, clip(&p.cmdline, 200)));
        container(card).padding(14).width(Length::Fill).style(skin::card(s, glow)).into()
    }

    fn site_block<'a>(&'a self, s: Skin, site: &'a Site) -> Element<'a, Msg> {
        let check = match self.checks.get(&site.id) {
            Some(c) => {
                let color = if c.ok { s.live } else { s.danger };
                row![
                    iced::widget::text(if c.ok { "●" } else { "✕" }).font(s.font).size(12).color(color),
                    muted(&s, format!("{}s ago · {}", self.now.saturating_sub(c.at), clip(&c.message, 110))),
                ]
                .spacing(6)
                .align_y(Alignment::Center)
            }
            None => row![muted(&s, "waiting for first check")],
        };
        let head = row![
            iced::widget::text(clip(&site.name, 30)).font(s.font).size(18).color(s.accent2),
            chip(s, format!("every {}s", site.interval_secs), s.muted),
            chip(s, site.check_mode.to_string(), s.muted),
            Space::new().width(Length::Fill),
            check,
        ]
        .spacing(10)
        .align_y(Alignment::Center);

        let mut col = Column::new().spacing(8).push(head);
        let streamers: Vec<&Streamer> = self.settings.streamers.iter().filter(|x| x.site_id == site.id).collect();
        if streamers.is_empty() {
            col = col.push(muted(&s, "No streamers yet; add one below."));
        }
        for st in streamers {
            col = col.push(self.streamer_row(s, st));
        }
        let (name, url) = self.new_streamer.get(&site.id).cloned().unwrap_or_default();
        let id = site.id;
        let add = row![
            text_input("Name (optional)", &name)
                .on_input(move |v| Msg::NewStreamerName(id, v))
                .on_submit(Msg::AddStreamer(id))
                .padding(8)
                .width(190)
                .style(skin::input(s)),
            text_input("Channel or stream URL", &url)
                .on_input(move |v| Msg::NewStreamerUrl(id, v))
                .on_submit(Msg::AddStreamer(id))
                .padding(8)
                .width(Length::Fill)
                .style(skin::input(s)),
            button(t(&s, "Add streamer").size(13))
                .on_press(Msg::AddStreamer(id))
                .padding([8.0, 14.0])
                .style(skin::btn(s, BtnKind::Primary)),
        ]
        .spacing(8);
        col = col.push(add);
        container(col).padding(14).width(Length::Fill).style(skin::card(s, None)).into()
    }

    fn streamer_row<'a>(&'a self, s: Skin, st: &'a Streamer) -> Element<'a, Msg> {
        let status = self.statuses.get(&st.id).copied().unwrap_or_default();
        let ph = pulse(self.phase);
        let (label, color) = if status.recording {
            ("RECORDING", s.accent)
        } else if status.live {
            ("LIVE", s.live)
        } else {
            ("OFFLINE", s.muted)
        };
        let glow = if status.live || status.recording { 0.4 + 0.6 * ph } else { 0.0 };
        let id = st.id;
        let mut chips = row![chip(s, label, color)].spacing(6);
        if !st.enabled {
            chips = chips.push(chip(s, "PAUSED", s.warn));
        }
        let line = row![
            dot(s, color, glow),
            column![
                iced::widget::text(clip(&st.name, 40)).font(s.font).size(15).color(if st.enabled { s.text } else { s.muted }),
                muted(&s, clip(&st.url, 70)),
            ]
            .spacing(2)
            .width(Length::Fill),
            chips,
            toggler(st.enabled).label("Record").on_toggle(move |v| Msg::ToggleStreamer(id, v)).text_size(12),
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
        container(line).padding([8.0, 12.0]).width(Length::Fill).style(skin::inset(s)).into()
    }

    fn sites(&self, s: Skin) -> Element<'_, Msg> {
        let mut col = Column::new().spacing(12).padding(iced::Padding { right: 8.0, ..Default::default() });

        let global = column![
            self.section_title(s, "GLOBAL OUTPUT FOLDER"),
            row![
                text_input("Recordings folder", &self.global_out)
                    .on_input(Msg::GlobalOutEdit)
                    .on_submit(Msg::GlobalOutSave)
                    .padding(8)
                    .style(skin::input(s)),
                button(t(&s, "Save").size(13))
                    .on_press_maybe((self.global_out.trim() != self.settings.default_output_dir).then_some(Msg::GlobalOutSave))
                    .padding([8.0, 16.0])
                    .style(skin::btn(s, BtnKind::Primary)),
            ]
            .spacing(8),
            muted(&s, "Used by any site that has no output folder of its own."),
        ]
        .spacing(8);
        col = col.push(container(global).padding(14).width(Length::Fill).style(skin::card(s, None)));

        let help = column![
            self.section_title(s, "COMMAND PLACEHOLDERS"),
            muted(&s, "Checker: {urls} becomes every URL monitored for the site (appended if missing)."),
            muted(&s, "Downloader: {url} live stream URL · {streamer_url} the URL you added · {name} streamer name · {title} stream title · {id} video id · {output_dir} output folder."),
            muted(&s, "--simulate mode expects lines like original_url|webpage_url|id|title, e.g. from --print with --match-filter is_live."),
        ]
        .spacing(5);
        col = col.push(container(help).padding(14).width(Length::Fill).style(skin::card(s, None)));

        let add = row![
            iced::widget::text("ADD SITE").font(s.font).size(13).color(s.accent2),
            button(t(&s, "YouTube").size(13))
                .on_press(Msg::AddSite(Preset::YouTube))
                .padding([7.0, 14.0])
                .style(skin::btn(s, BtnKind::Ghost)),
            button(t(&s, "Twitch").size(13))
                .on_press(Msg::AddSite(Preset::Twitch))
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
        let field = |placeholder: &'static str, value: &'a str, f: fn(String) -> DraftField| -> Element<'a, Msg> {
            text_input(placeholder, value)
                .on_input(move |v| Msg::Draft(id, f(v)))
                .padding(8)
                .style(skin::input(s))
                .into()
        };
        let dirty = *d != SiteDraft::from_site(site);
        let valid = d.interval_secs().is_some() && !d.name.trim().is_empty();

        let top = row![
            container(labeled("Site name", field("Site name", &d.name, DraftField::Name))).width(Length::Fill),
            container(labeled("Check every (seconds, min 5)", field("60", &d.interval, DraftField::Interval))).width(210),
            container(labeled(
                "Checker mode",
                pick_list(CheckMode::ALL, Some(d.mode), move |m| Msg::Draft(id, DraftField::Mode(m)))
                    .padding(8)
                    .width(Length::Fill)
                    .into(),
            ))
            .width(170),
        ]
        .spacing(10);

        let count = self.settings.streamers.iter().filter(|x| x.site_id == id).count();
        let actions = row![
            button(t(&s, "Save changes").size(13))
                .on_press_maybe((dirty && valid).then_some(Msg::SaveSite(id)))
                .padding([8.0, 16.0])
                .style(skin::btn(s, BtnKind::Primary)),
            button(t(&s, "Revert").size(13))
                .on_press_maybe(dirty.then_some(Msg::RevertSite(id)))
                .padding([8.0, 16.0])
                .style(skin::btn(s, BtnKind::Ghost)),
            if dirty { chip(s, "unsaved changes", s.warn) } else { Space::new().into() },
            Space::new().width(Length::Fill),
            muted(&s, format!("{count} streamer(s)")),
            button(t(&s, "Remove site").size(13))
                .on_press(Msg::RemoveSite(id))
                .padding([8.0, 16.0])
                .style(skin::btn(s, BtnKind::Danger)),
        ]
        .spacing(10)
        .align_y(Alignment::Center);

        let form = column![
            top,
            labeled("Path to yt-dlp executable", field("yt-dlp or C:\\tools\\yt-dlp.exe", &d.exe, DraftField::Exe)),
            labeled("Checker command arguments", field("--dump-json {urls}", &d.checker, DraftField::Checker)),
            labeled("Downloader command arguments", field("-o \"%(title)s.%(ext)s\" {url}", &d.downloader, DraftField::Downloader)),
            labeled("Output folder (blank = global)", field("Use the global output folder", &d.output, DraftField::Output)),
            actions,
        ]
        .spacing(10);
        container(form).padding(14).width(Length::Fill).style(skin::card(s, None)).into()
    }

    fn log_view(&self, s: Skin) -> Element<'_, Msg> {
        let mut col = Column::new().spacing(4);
        if self.log.is_empty() {
            col = col.push(muted(&s, "Nothing logged yet."));
        }
        for line in &self.log {
            col = col.push(iced::widget::text(line.as_str()).font(iced::Font::MONOSPACE).size(12).color(s.text));
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
