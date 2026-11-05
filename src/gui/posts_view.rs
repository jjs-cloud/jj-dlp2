//! Views of the Posts monitor and its settings.

use super::posts::{PostsMsg, SiteDraft, SiteField, NEW_SITE};
use super::skin::{self, BtnKind, Skin};
use super::view::{btn_label, card_box, chip, clip, fmt_ago, muted, t, warn_text};
use super::{App, Msg};
use crate::posts::{self, Post, PostSite, PostState, Preset, Section};
use iced::widget::text::Wrapping;
use iced::widget::text_editor::{self, Action};
use iced::widget::{Column, Row, Space, button, column, container, pick_list, row, scrollable, slider, stack, text_editor as editor, text_input, toggler};
use iced::{Alignment, Background, Border, Color, ContentFit, Element, Length};

const SIDEBAR_W: f32 = 210.0;
/// Seconds the "has been added!" message stays up.
const SITE_ADDED_SECS: u64 = 15;
const GAP: f32 = 10.0;
const CARD_PAD: f32 = 8.0;
/// Two 13px lines at the default 1.3 line height plus 8px padding top and bottom.
const ARGS_BOX_H: f32 = 2.0 * 13.0 * 1.3 + 16.0;
/// Window width taken by the page padding, the sidebar gap and the scrollbar.
const CHROME_W: f32 = 36.0 + 12.0 + 26.0;

fn send(m: PostsMsg) -> Msg {
    Msg::Posts(m)
}

/// `m:ss` or `h:mm:ss` for a clip length.
fn fmt_clock(secs: u64) -> String {
    let (h, m, s) = (secs / 3600, (secs / 60) % 60, secs % 60);
    if h > 0 { format!("{h}:{m:02}:{s:02}") } else { format!("{m}:{s:02}") }
}

/// A small dark label that stays readable on top of a thumbnail.
fn badge<'a>(label: String, color: Color) -> Element<'a, Msg> {
    container(iced::widget::text(label).size(10).color(color))
        .padding([2.0, 6.0])
        .style(|_| container::Style {
            background: Some(Background::Color(Color { a: 0.72, ..Color::BLACK })),
            border: Border { radius: 6.0.into(), ..Default::default() },
            ..Default::default()
        })
        .into()
}

/// One entry of the sidebar lists.
fn nav_item<'a>(s: Skin, label: String, count: usize, selected: bool, msg: PostsMsg) -> Element<'a, Msg> {
    let count_color = if count > 0 && !selected { s.accent2 } else { s.muted };
    button(
        row![
            t(&s, label).size(13).width(Length::Fill).wrapping(Wrapping::None),
            iced::widget::text(count.to_string()).font(s.font).size(12).color(count_color),
        ]
        .spacing(6)
        .align_y(Alignment::Center),
    )
    .on_press(send(msg))
    .padding([7.0, 10.0])
    .width(Length::Fill)
    .style(skin::tile(s, s.muted, selected))
    .into()
}

fn card_btn<'a>(s: Skin, label: &'static str, msg: PostsMsg, kind: BtnKind) -> Element<'a, Msg> {
    button(btn_label(&s, label).size(11).width(Length::Fill).align_x(Alignment::Center))
        .on_press(send(msg))
        .padding([4.0, 2.0])
        .width(Length::Fill)
        .style(skin::btn(s, kind))
        .into()
}

/// One entry of the site selector.
#[derive(Debug, Clone, PartialEq)]
struct SiteChoice {
    id: posts::SiteId,
    name: String,
}

impl std::fmt::Display for SiteChoice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.name)
    }
}

fn flag<'a>(label: &'static str, on: bool, make: fn(bool) -> PostsMsg) -> Element<'a, Msg> {
    toggler(on).label(label).on_toggle(move |v| send(make(v))).text_size(12).into()
}

impl App {
    pub(super) fn posts_view(&self, s: Skin) -> Element<'_, Msg> {
        row![self.posts_sidebar(s), self.posts_main(s)].spacing(12).height(Length::Fill).into()
    }

    fn posts_sidebar(&self, s: Skin) -> Element<'_, Msg> {
        let p = &self.posts;
        let acct = p.account.as_ref().map(|(sid, n)| (*sid, n.as_str()));
        let mut nav = Column::new().spacing(4).push(self.section_title(s, "LIBRARY"));
        for sec in [Section::Unviewed, Section::Viewed, Section::Starred] {
            nav = nav.push(nav_item(s, sec.label().to_string(), p.count(sec, acct), p.section == sec, PostsMsg::SetSection(sec)));
        }
        let failed = p.count(Section::Failed, acct);
        if failed > 0 {
            nav = nav.push(nav_item(s, "Failed".into(), failed, p.section == Section::Failed, PostsMsg::SetSection(Section::Failed)));
        }
        nav = nav.push(Space::new().height(8)).push(self.section_title(s, "ACCOUNTS"));
        nav = nav.push(nav_item(s, "All accounts".into(), p.count(Section::Unviewed, None), acct.is_none(), PostsMsg::SetAccount(None)));
        let multi = p.cfg.sites.iter().filter(|x| !x.accounts.is_empty()).count() > 1;
        for site in &p.cfg.sites {
            if multi && !site.accounts.is_empty() {
                nav = nav.push(muted(&s, clip(&site.name, 24)).size(11));
            }
            for a in &site.accounts {
                let mark = if p.errors.contains_key(&(site.id, a.name.clone())) { "⚠ " } else { "" };
                let off = if a.enabled { "" } else { " (off)" };
                let label = format!("{mark}@{}{off}", clip(&a.name, 16));
                let selected = p.account.as_ref().is_some_and(|(sid, n)| *sid == site.id && n.eq_ignore_ascii_case(&a.name));
                let unviewed = p.count(Section::Unviewed, Some((site.id, a.name.as_str())));
                nav = nav.push(nav_item(s, label, unviewed, selected, PostsMsg::SetAccount(Some((site.id, a.name.clone())))));
            }
        }
        let status = if p.checking {
            "Checking…".to_string()
        } else if p.all_paused() {
            "Paused".to_string()
        } else {
            p.secs_to_next().map_or("No accounts enabled".to_string(), |n| format!("Next check in {n}s"))
        };
        let last = p.last_check.map(|at| format!("Last check {}", fmt_ago(self.now.saturating_sub(at))));
        let controls = column![
            muted(&s, status),
            muted(&s, last.unwrap_or_default()),
            row![
                card_btn(s, "Check now", PostsMsg::CheckNow, BtnKind::Ghost),
                card_btn(s, if p.all_paused() { "Resume" } else { "Pause" }, PostsMsg::Paused(!p.all_paused()), BtnKind::Ghost),
            ]
            .spacing(6),
        ]
        .spacing(4);
        let mut bottom = Column::new().spacing(10);
        if let Some((sid, name)) = &p.account {
            bottom = bottom.push(
                button(btn_label(&s, format!("Remove @{}", clip(name, 14))).size(12).width(Length::Fill).align_x(Alignment::Center))
                    .on_press(send(PostsMsg::RemoveAccount(*sid, name.clone())))
                    .padding([6.0, 10.0])
                    .width(Length::Fill)
                    .style(skin::btn(s, BtnKind::Danger)),
            );
        }
        container(column![scrollable(container(nav).padding(iced::Padding { right: 10.0, ..Default::default() })).height(Length::Fill), bottom.push(controls)].spacing(10))
            .padding(12)
            .width(SIDEBAR_W)
            .height(Length::Fill)
            .style(skin::card(s, None))
            .into()
    }

    fn posts_main(&self, s: Skin) -> Element<'_, Msg> {
        let p = &self.posts;
        let visible = p.visible();
        let mut bar = Row::new().spacing(10).align_y(Alignment::Center);
        bar = bar.push(iced::widget::text(format!("{} · {}", p.section.label().to_uppercase(), visible.len())).font(s.font).size(13).color(s.accent2));
        bar = bar.push(container(muted(&s, p.status_text(self.now).unwrap_or(""))).width(Length::Fill).clip(true));
        bar = bar.push(
            text_input("Search", &p.search).on_input(|v| send(PostsMsg::Search(v))).padding([5.0, 8.0]).width(160).style(skin::input(s)),
        );
        bar = bar.push(flag("Newest first", p.cfg.newest_first, PostsMsg::NewestFirst));
        bar = bar.push(
            slider(posts::CARD_W_MIN..=posts::CARD_W_MAX, p.cfg.card_w, |v| send(PostsMsg::CardWidth(v)))
                .step(10.0_f32)
                .on_release(send(PostsMsg::CardWidthSave))
                .width(90),
        );
        if p.section == Section::Unviewed && !visible.is_empty() {
            bar = bar.push(
                button(btn_label(&s, "Mark all viewed").size(12))
                    .on_press(send(PostsMsg::MarkAllViewed))
                    .padding([4.0, 10.0])
                    .style(skin::btn(s, BtnKind::Ghost)),
            );
        }
        let mut col = Column::new().spacing(10).push(bar);
        let mut errors: Vec<(&(posts::SiteId, String), &String)> = p.errors.iter().collect();
        errors.sort();
        for ((_, name), e) in errors {
            col = col.push(warn_text(&s, format!("⚠ @{name}: {}", clip(e, 200))));
        }
        if visible.is_empty() {
            let empty = container(self.posts_empty(s)).height(Length::Fill);
            return col.push(empty).push(self.posts_add_bar(s)).height(Length::Fill).into();
        }
        let w = p.cfg.card_w;
        let avail = (self.win_w - SIDEBAR_W - CHROME_W).max(w);
        let cols = (((avail + GAP) / (w + GAP)).floor() as usize).max(1);
        let shown: Vec<&Post> = visible.iter().take(p.shown).copied().collect();
        let mut grid = Column::new().spacing(GAP);
        for chunk in shown.chunks(cols) {
            let mut r = Row::new().spacing(GAP);
            for post in chunk {
                r = r.push(self.post_card(s, *post, w));
            }
            grid = grid.push(r);
        }
        if visible.len() > shown.len() {
            let more = format!("Show more ({} remaining)", visible.len() - shown.len());
            grid = grid.push(
                button(btn_label(&s, more).size(12))
                    .on_press(send(PostsMsg::ShowMore))
                    .padding([6.0, 14.0])
                    .style(skin::btn(s, BtnKind::Ghost)),
            );
        }
        col.push(scrollable(container(grid).padding(iced::Padding { right: 10.0, ..Default::default() })).height(Length::Fill))
            .push(self.posts_add_bar(s))
            .height(Length::Fill)
            .into()
    }

    /// Site selector, username box and Add button under the posts.
    fn posts_add_bar(&self, s: Skin) -> Element<'_, Msg> {
        let p = &self.posts;
        let choices: Vec<SiteChoice> = p.cfg.sites.iter().map(|x| SiteChoice { id: x.id, name: x.name.clone() }).collect();
        let selected = p.add_target().and_then(|id| choices.iter().find(|c| c.id == id).cloned());
        let can_add = selected.is_some() && !p.add_name.trim().is_empty();
        let pulsing = selected.is_some() && p.cfg.sites.iter().all(|x| x.accounts.is_empty());
        let (calm, pulse) = (skin::input(s), skin::input_pulse(s, skin::pulse(self.phase)));
        row![
            pick_list(choices, selected, |c: SiteChoice| send(PostsMsg::PickAddSite(c.id))).placeholder("Site").padding(6).text_size(13).width(150),
            text_input("Username", &p.add_name)
                .on_input(|v| send(PostsMsg::AddName(v)))
                .on_submit_maybe(can_add.then(|| send(PostsMsg::AddAccount)))
                .padding(6)
                .size(13)
                .width(Length::Fill)
                .style(move |th, st| if pulsing { pulse(th, st) } else { calm(th, st) }),
            button(btn_label(&s, "Add").size(13))
                .on_press_maybe(can_add.then(|| send(PostsMsg::AddAccount)))
                .padding([6.0, 16.0])
                .style(skin::btn(s, BtnKind::Primary)),
        ]
        .spacing(8)
        .align_y(Alignment::Center)
        .into()
    }

    fn posts_empty(&self, s: Skin) -> Element<'_, Msg> {
        let p = &self.posts;
        if p.cfg.sites.is_empty() {
            return card_box(
                s,
                column![
                    t(&s, "No sites configured yet."),
                    button(btn_label(&s, "Add a site").size(13))
                        .on_press(send(PostsMsg::OpenSettings))
                        .padding([8.0, 14.0])
                        .style(skin::btn(s, BtnKind::Primary)),
                ]
                .spacing(10),
            );
        }
        if p.cfg.sites.iter().all(|x| x.accounts.is_empty()) {
            return muted(&s, "No accounts yet. Add a username below.").into();
        }
        let text = match p.section {
            Section::Unviewed => "You're all caught up.",
            Section::Viewed => "Posts you have viewed will appear here.",
            Section::Starred => "Star a post to keep it here.",
            Section::Failed => "No failed downloads.",
        };
        muted(&s, text).into()
    }

    fn post_card<'a>(&'a self, s: Skin, post: &'a Post, w: f32) -> Element<'a, Msg> {
        let iw = w - 2.0 * CARD_PAD;
        let ih = (iw * 1.45).round();
        let preview: Element<'a, Msg> = if post.has_thumb {
            iced::widget::image::<iced::widget::image::Handle>(iced::widget::image::Handle::from_path(post.thumb_path()))
                .width(iw)
                .height(ih)
                .content_fit(ContentFit::Cover)
                .opacity(if post.viewed { 0.6_f32 } else { 1.0_f32 })
                .border_radius(6.0)
                .into()
        } else {
            container(muted(&s, "No preview"))
                .center_x(iw)
                .center_y(ih)
                .style(skin::inset(s))
                .into()
        };
        let state_badge = match post.state {
            PostState::Downloading => Some(("Downloading…", s.accent2)),
            PostState::Queued => Some(("Queued", s.muted)),
            PostState::Failed => Some(("Failed", s.danger)),
            PostState::Remote => Some(("Not saved", s.warn)),
            PostState::Ready | PostState::Removed => None,
        };
        let top = state_badge.map_or_else(|| Space::new().into(), |(l, c)| badge(l.to_string(), c));
        let bottom: Element<'a, Msg> = match post.duration {
            Some(d) => badge(fmt_clock(d), Color::WHITE),
            None => Space::new().into(),
        };
        let overlay = container(
            column![top, Space::new().height(Length::Fill), row![Space::new().width(Length::Fill), bottom]],
        )
        .padding(6)
        .width(iw)
        .height(ih);
        let thumb = button(stack([preview, overlay.into()]))
            .on_press(send(PostsMsg::Play(post.id.clone())))
            .padding(0)
            .style(|_, _| button::Style::default());

        let ago = fmt_ago(self.now.saturating_sub(post.sort_time()));
        let meta = row![
            iced::widget::text(format!("@{}", clip(&post.account, 14))).font(s.font).size(12).color(s.accent2),
            muted(&s, ago).size(11),
        ]
        .spacing(6);
        let caption = if post.title.is_empty() { "(no caption)".to_string() } else { clip(&post.title, 70) };
        let title = container(t(&s, caption).size(12)).height(32).clip(true);

        let id = &post.id;
        let play = if post.state == PostState::Ready { "Play" } else { "Open" };
        let (seen_label, seen_kind) = if post.viewed { ("Unview", BtnKind::Ghost) } else { ("Viewed", BtnKind::Ghost) };
        let row1 = row![
            card_btn(s, play, PostsMsg::Play(id.clone()), BtnKind::Primary),
            card_btn(s, seen_label, PostsMsg::ToggleViewed(id.clone()), seen_kind),
        ]
        .spacing(6);
        let star = card_btn(
            s,
            if post.starred { "★" } else { "☆" },
            PostsMsg::ToggleStar(id.clone()),
            if post.starred { BtnKind::Primary } else { BtnKind::Ghost },
        );
        let page = card_btn(s, "Page", PostsMsg::OpenPage(id.clone()), BtnKind::Ghost);
        let row2 = match post.state {
            PostState::Ready => row![
                star,
                card_btn(s, "Folder", PostsMsg::Reveal(id.clone()), BtnKind::Ghost),
                card_btn(s, "Delete", PostsMsg::Delete(id.clone()), BtnKind::Danger),
            ],
            PostState::Remote => row![star, card_btn(s, "Save", PostsMsg::Download(id.clone()), BtnKind::Ghost), page],
            PostState::Failed => row![star, card_btn(s, "Retry", PostsMsg::Download(id.clone()), BtnKind::Ghost), page],
            _ => row![star, page],
        }
        .spacing(6);

        let mut body = Column::new().spacing(6).push(thumb).push(meta).push(title);
        if post.state == PostState::Failed && !post.error.is_empty() {
            body = body.push(warn_text(&s, clip(&post.error, 60)).size(10));
        }
        container(body.push(row1).push(row2)).padding(CARD_PAD).width(w).style(skin::card(s, None)).into()
    }

    pub(super) fn posts_settings(&self, s: Skin) -> Element<'_, Msg> {
        let p = &self.posts;
        let mut add = row![self.section_title(s, "ADD SITE")].spacing(10).align_y(Alignment::Center);
        for (label, preset) in [("TikTok", Preset::Tiktok), ("Youtube Shorts", Preset::YoutubeShorts), ("Blank", Preset::Blank)] {
            add = add.push(
                button(t(&s, label).size(13))
                    .on_press(send(PostsMsg::AddSite(preset)))
                    .padding([7.0, 14.0])
                    .style(skin::btn(s, BtnKind::Ghost)),
            );
        }
        if let Some((name, at)) = &p.added {
            if self.now.saturating_sub(*at) < SITE_ADDED_SECS {
                add = add.push(t(&s, format!("{name} has been added! Switch to the Monitor tab to add a username.")).size(13).color(s.live));
            }
        }
        add = add.push(muted(&s, p.status_text(self.now).unwrap_or("")));

        let mut col = Column::new().spacing(12).padding(iced::Padding { right: 8.0, ..Default::default() });
        let new_draft = p.drafts.get(&NEW_SITE);
        if p.cfg.sites.is_empty() && new_draft.is_none() {
            col = col.push(muted(&s, "Pick a preset to start; every field stays editable afterwards."));
        }
        if let Some(d) = new_draft {
            col = col.push(self.post_site_editor(s, NEW_SITE, None, d));
        }
        for site in &p.cfg.sites {
            if let Some(d) = p.drafts.get(&site.id) {
                col = col.push(self.post_site_editor(s, site.id, Some(site), d));
            }
        }
        column![add, scrollable(col).id(super::posts_scroll_id()).height(Length::Fill)].spacing(12).height(Length::Fill).into()
    }

    /// The settings card of one site; `saved` is `None` for the unsaved blank site.
    fn post_site_editor<'a>(&'a self, s: Skin, id: posts::SiteId, saved: Option<&'a PostSite>, d: &'a SiteDraft) -> Element<'a, Msg> {
        let edit = move |f: fn(String) -> SiteField| move |v: String| send(PostsMsg::Site(id, f(v)));
        let input = |label: &'static str, hint: &str, value: &str, f: fn(String) -> SiteField, invalid: bool| -> Element<'a, Msg> {
            column![
                muted(&s, label),
                text_input(hint, value).on_input(edit(f)).padding(5).size(13).style(skin::input_required(s, invalid)),
            ]
            .spacing(2)
            .into()
        };
        let cell = |e: Element<'a, Msg>, w: Length| -> Element<'a, Msg> { container(e).width(w).into() };
        let toggle = |label: &'static str, on: bool, f: fn(bool) -> SiteField| -> Element<'a, Msg> {
            toggler(on).label(label).on_toggle(move |v| send(PostsMsg::Site(id, f(v)))).text_size(12).into()
        };
        let bad_number = |text: &str, min: u64| !text.trim().is_empty() && !text.trim().parse::<u64>().is_ok_and(|n| n >= min);
        let topic_bad = d.notify_ntfy && !crate::policy::notify::topic_valid(&d.ntfy_topic);
        let base = saved.cloned().unwrap_or_default();
        let dirty = saved.is_none() || d.differs_from(&base);
        let valid = d.valid();

        let identity = row![
            cell(input("Site name (example: TikTok)", "", &d.name, SiteField::Name, d.name.trim().is_empty()), Length::FillPortion(1)),
            cell(
                input("Check every (seconds)", &posts::DEFAULT_INTERVAL_SECS.to_string(), &d.interval, SiteField::Interval, bad_number(&d.interval, posts::MIN_INTERVAL_SECS)),
                Length::FillPortion(1),
            ),
            cell(input("Path to yt-dlp executable", crate::api::DEFAULT_EXE, &d.exe, SiteField::Exe, false), Length::FillPortion(1)),
        ]
        .spacing(8);

        let out_dir = PostSite { output_dir: d.output_dir.clone(), ..PostSite::default() }.output_folder(&self.settings.default_output_dir);
        let folder = row![
            cell(input("Profile URL template", "", &d.url_template, SiteField::UrlTemplate, d.url_template.trim().is_empty()), Length::FillPortion(1)),
            cell(
                input("Download folder (blank = global/posts)", &out_dir.to_string_lossy(), &d.output_dir, SiteField::OutDir, false),
                Length::FillPortion(1),
            ),
            column![
                muted(&s, " "),
                row![
                    button(btn_label(&s, "Browse").size(12)).on_press(send(PostsMsg::BrowseOutput(id))).padding([6.0, 12.0]).style(skin::btn(s, BtnKind::Ghost)),
                    button(btn_label(&s, "Open").size(12)).on_press(send(PostsMsg::OpenOutputDir(id))).padding([6.0, 12.0]).style(skin::btn(s, BtnKind::Ghost)),
                ]
                .spacing(6),
            ]
            .spacing(2),
        ]
        .spacing(8);

        let cleanup = row![
            cell(
                input("Delete unstarred posts after (days, blank = never)", "Never", &d.auto_delete, SiteField::AutoDelete, bad_number(&d.auto_delete, 1)),
                Length::FillPortion(1),
            ),
            cell(column![muted(&s, " "), toggle("Mobile (ntfy)", d.notify_ntfy, SiteField::NotifyNtfy)].spacing(2).into(), Length::FillPortion(1)),
            cell(input("NTFY topic (example: j73hd92xl)", "", &d.ntfy_topic, SiteField::NtfyTopic, topic_bad), Length::FillPortion(1)),
        ]
        .spacing(8);

        let toggles = column![
            toggle("Pause checking", d.paused, SiteField::Paused),
            toggle("Download new posts automatically", d.auto_download, SiteField::AutoDownload),
            toggle("Download existing posts when an account is first checked", d.backfill, SiteField::Backfill),
            toggle("Mark a post viewed when it is opened", d.mark_on_open, SiteField::MarkOnOpen),
            toggle("Desktop notification", d.notify_desktop, SiteField::NotifyDesktop),
        ]
        .spacing(6);

        let args = |label: &'static str, hint: &'static str, content: &'a text_editor::Content, f: fn(Action) -> SiteField| -> Element<'a, Msg> {
            column![
                muted(&s, label),
                editor(content)
                    .placeholder(hint)
                    .on_action(move |a| send(PostsMsg::Site(id, f(a))))
                    .font(s.font)
                    .size(13)
                    .padding(8)
                    .height(ARGS_BOX_H)
                    .style(skin::editor(s)),
            ]
            .spacing(2)
            .into()
        };

        let action = |label: &'static str, msg: Option<PostsMsg>, kind: BtnKind| -> Element<'a, Msg> {
            button(btn_label(&s, label).size(13)).on_press_maybe(msg.map(send)).padding([5.0, 12.0]).style(skin::btn(s, kind)).into()
        };
        let mut actions: Row<'a, Msg> = row![Space::new().width(Length::Fill)].spacing(10).align_y(Alignment::Center);
        if saved.is_some() {
            if dirty {
                actions = actions.push(chip(s, "unsaved changes", s.warn));
            }
            actions = actions
                .push(action("Save changes", (dirty && valid).then_some(PostsMsg::SaveSite(id)), BtnKind::Primary))
                .push(action("Revert", dirty.then_some(PostsMsg::RevertSite(id)), BtnKind::Ghost))
                .push(action("Defaults", Some(PostsMsg::DefaultsSite(id)), BtnKind::Ghost));
            if self.posts.confirm_remove == Some(id) {
                actions = actions
                    .push(warn_text(&s, "Remove site and its posts?"))
                    .push(action("Confirm", Some(PostsMsg::ConfirmRemoveSite(id)), BtnKind::Danger))
                    .push(action("Cancel", Some(PostsMsg::CancelRemoveSite), BtnKind::Ghost));
            } else {
                actions = actions.push(action("Remove site", Some(PostsMsg::RemoveSite(id)), BtnKind::Danger));
            }
        } else {
            actions = actions
                .push(action("Add site", valid.then_some(PostsMsg::SaveSite(id)), BtnKind::Primary))
                .push(action("Discard", Some(PostsMsg::RemoveSite(id)), BtnKind::Ghost));
        }

        card_box(
            s,
            column![
                identity,
                folder,
                cleanup,
                toggles,
                args("Checker arguments ({url} = profile URL)", posts::DEFAULT_CHECKER_ARGS, &d.checker, SiteField::Checker),
                args("Downloader arguments ({url} = post URL, {dir} = download folder)", posts::DEFAULT_DOWNLOAD_ARGS, &d.download, SiteField::Download),
                actions,
            ]
            .spacing(10),
        )
    }
}
