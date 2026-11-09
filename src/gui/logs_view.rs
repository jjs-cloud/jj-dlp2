//! View of the Log tab's Output sub-tab.

use super::logs::{LogKind, LogsMsg};
use super::skin::{self, BtnKind, Skin};
use super::view::{btn_label, clip, muted, t};
use super::{App, Msg};
use crate::api::Site;
use crate::policy::logtail;
use crate::policy::posts::PostSite;
use iced::widget::text::Wrapping;
use iced::alignment::Horizontal;
use iced::widget::{Column, Space, button, column, container, row, scrollable, text_editor, toggler};
use iced::{Alignment, Element, Font, Length, Padding};
use std::path::PathBuf;

const LIST_W: f32 = 290.0;
const NAME_W: f32 = 84.0;
const COL_W: f32 = 80.0;
const GAP: f32 = 6.0;

/// One row of the file list: a label, then the stdout and stderr cells.
fn log_line<'a>(label: Element<'a, Msg>, out: Element<'a, Msg>, err: Element<'a, Msg>) -> Element<'a, Msg> {
    row![container(label).width(NAME_W), out, err].spacing(GAP).align_y(Alignment::Center).into()
}

impl App {
    pub(super) fn logs_output(&self, s: Skin) -> Element<'_, Msg> {
        row![self.logs_list(s), self.logs_viewer(s)].spacing(12).height(Length::Fill).into()
    }

    fn logs_list(&self, s: Skin) -> Element<'_, Msg> {
        let mut col = Column::new().spacing(12).padding(Padding { right: 10.0, ..Padding::ZERO });
        if self.settings.sites.is_empty() && self.posts.cfg.sites.is_empty() {
            col = col.push(muted(&s, "Add a streamer on the Livestreams tab, or an account on the Posts tab, to see their logs."));
        }
        for site in &self.settings.sites {
            col = col.push(self.logs_site(s, site));
        }
        if !self.posts.cfg.sites.is_empty() {
            col = col.push(self.section_title(s, "POSTS"));
        }
        for site in &self.posts.cfg.sites {
            col = col.push(self.logs_post_site(s, site));
        }
        let heads = row![
            Space::new().width(NAME_W),
            container(muted(&s, "stdout")).width(COL_W).align_x(Horizontal::Center),
            container(muted(&s, "stderr")).width(COL_W).align_x(Horizontal::Center),
        ]
        .spacing(GAP);
        container(column![self.section_title(s, "LOG FILES"), heads, scrollable(col).height(Length::Fill)].spacing(8))
            .padding(10)
            .width(LIST_W)
            .height(Length::Fill)
            .style(skin::card(s, None))
            .into()
    }

    fn logs_site<'a>(&'a self, s: Skin, site: &'a Site) -> Element<'a, Msg> {
        let id = site.id;
        let name = iced::widget::text(clip(&site.name, 28)).font(s.font).size(14).color(s.site_color(site.color.as_deref()));
        let checker = |kind: LogKind, on: bool| {
            let file = logtail::checker_file(&site.name, kind.file_label());
            self.log_cell(s, on, move |v| LogsMsg::ToggleChecker(id, kind, v), file)
        };
        let mut col = Column::new().spacing(6).push(name).push(log_line(
            muted(&s, "Checker").into(),
            checker(LogKind::Stdout, site.log_checker_stdout),
            checker(LogKind::Stderr, site.log_checker_stderr),
        ));
        for st in self.settings.streamers.iter().filter(|x| x.site_id == site.id) {
            let sid = st.id;
            let cell = |kind: LogKind, on: bool| {
                let file = logtail::stream_file(&site.name, &st.name, kind.file_label());
                self.log_cell(s, on, move |v| LogsMsg::ToggleStreamer(sid, kind, v), file)
            };
            col = col.push(log_line(
                t(&s, clip(&st.name, 12)).size(12).into(),
                cell(LogKind::Stdout, st.options.log_stdout),
                cell(LogKind::Stderr, st.options.log_stderr),
            ));
        }
        col.into()
    }

    /// A Posts site's downloader row, then one checker row per account.
    fn logs_post_site<'a>(&'a self, s: Skin, site: &'a PostSite) -> Element<'a, Msg> {
        let id = site.id;
        let downloader = |kind: LogKind, on: bool| {
            let file = logtail::posts_download_file(&site.name, kind.file_label());
            self.log_cell(s, on, move |v| LogsMsg::TogglePostDownloader(id, kind, v), file)
        };
        let mut col = Column::new().spacing(6).push(t(&s, clip(&site.name, 28)).size(14)).push(log_line(
            muted(&s, "Downloader").into(),
            downloader(LogKind::Stdout, site.log_download_stdout),
            downloader(LogKind::Stderr, site.log_download_stderr),
        ));
        if !site.accounts.is_empty() {
            col = col.push(muted(&s, "Checker per account"));
        }
        for acct in &site.accounts {
            let cell = |kind: LogKind, on: bool| {
                let name = acct.name.clone();
                let file = logtail::posts_checker_file(&site.name, &name, kind.file_label());
                self.log_cell(s, on, move |v| LogsMsg::TogglePostChecker(id, name.clone(), kind, v), file)
            };
            col = col.push(log_line(
                t(&s, clip(&format!("@{}", acct.name), 12)).size(12).into(),
                cell(LogKind::Stdout, acct.log_stdout),
                cell(LogKind::Stderr, acct.log_stderr),
            ));
        }
        col.into()
    }

    /// A log switch with the button that opens its file beside it.
    fn log_cell<'a>(&self, s: Skin, on: bool, make: impl Fn(bool) -> LogsMsg + 'a, path: PathBuf) -> Element<'a, Msg> {
        let switch = toggler(on).on_toggle(move |v| Msg::Logs(make(v)));
        container(row![switch, self.view_button(s, path)].spacing(4).align_y(Alignment::Center)).width(COL_W).into()
    }

    /// Button that opens a log file in the viewer; disabled until the file exists.
    fn view_button<'a>(&self, s: Skin, path: PathBuf) -> Element<'a, Msg> {
        let shown = self.logs.open.as_ref().is_some_and(|(p, _)| *p == path);
        let kind = if shown { BtnKind::Primary } else { BtnKind::Ghost };
        let open = self.logs.has_file(&path).then(|| {
            let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            Msg::Logs(LogsMsg::Open(path, name))
        });
        button(btn_label(&s, "View").size(11)).on_press_maybe(open).padding([3.0, 7.0]).style(skin::btn(s, kind)).into()
    }

    fn logs_viewer(&self, s: Skin) -> Element<'_, Msg> {
        let l = &self.logs;
        let title = l.open.as_ref().map_or("No file selected", |(_, name)| name.as_str());
        let follow_label = if l.pending.is_empty() { "Follow".to_string() } else { format!("Follow ({} new)", l.pending.len()) };
        let header = row![
            iced::widget::text(title).font(s.font).size(13).color(s.accent2),
            Space::new().width(Length::Fill),
            toggler(l.follow).label(follow_label).on_toggle(|v| Msg::Logs(LogsMsg::SetFollow(v))).text_size(12),
            toggler(l.wrap).label("Wrap").on_toggle(|v| Msg::Logs(LogsMsg::SetWrap(v))).text_size(12),
            button(btn_label(&s, "Copy").size(12))
                .on_press_maybe(l.open.is_some().then_some(Msg::Logs(LogsMsg::Copy)))
                .padding([3.0, 10.0])
                .style(skin::btn(s, BtnKind::Ghost)),
        ]
        .spacing(12)
        .align_y(Alignment::Center);
        let mut col = Column::new().spacing(8).push(header);
        if l.open.is_none() {
            col = col.push(muted(&s, "Choose a log file with View. The newest lines are shown and updated as the file grows."));
        } else {
            if l.missing {
                col = col.push(muted(&s, "This file does not exist yet; waiting for it to be created."));
            }
            col = col.push(
                text_editor(&l.content)
                    .on_action(|a| Msg::Logs(LogsMsg::Action(a)))
                    .font(Font::MONOSPACE)
                    .size(12)
                    .wrapping(if l.wrap { Wrapping::WordOrGlyph } else { Wrapping::None })
                    .height(Length::Fill)
                    .style(skin::editor(s)),
            );
        }
        container(col).padding(14).width(Length::Fill).height(Length::Fill).style(skin::card(s, None)).into()
    }
}
