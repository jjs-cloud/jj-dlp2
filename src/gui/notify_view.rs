//! View of the Settings > Notifications tab.

use super::posts::{PostsMsg, SiteField};
use super::skin::{self, BtnKind, Skin};
use super::view::{btn_label, card_box, chip, muted, t};
use super::{App, DraftField, Msg, NotifyTab, SiteDraft};
use crate::api::*;
use crate::policy::notify;
use crate::policy::posts::PostSite;
use iced::alignment::Horizontal;
use iced::widget::{Column, Space, button, column, container, pick_list, row, scrollable, text_input, toggler};
use iced::{Alignment, Element, Length, Padding};

const NAME_W: f32 = 100.0;
const COL_W: f32 = 80.0;
const GAP: f32 = 6.0;
const PICKER_W: f32 = 240.0;
/// Example shown in every ntfy topic label.
const TOPIC_LABEL: &str = "ntfy topic";

/// One entry in a site picker.
#[derive(Clone, PartialEq)]
pub(super) struct SiteChoice {
    pub id: u64,
    pub name: String,
}

impl std::fmt::Display for SiteChoice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.name)
    }
}

impl App {
    pub(super) fn notifications_settings(&self, s: Skin) -> Element<'_, Msg> {
        let switch = |label: &'static str, which: NotifyTab| {
            let on = self.notify_tab == which;
            let kind = if on { BtnKind::TabActive } else { BtnKind::TabIdle };
            button(t(&s, label).size(12).color(if on { s.bg } else { s.muted }))
                .on_press(Msg::SetNotifyTab(which))
                .padding([5.0, 14.0])
                .style(skin::btn(s, kind))
        };
        let switcher = row![
            switch("Livestreams", NotifyTab::Livestreams),
            switch("Posts", NotifyTab::Posts),
            switch("General", NotifyTab::General)
        ]
        .spacing(8);
        let body = match self.notify_tab {
            NotifyTab::Livestreams => self.notify_livestreams(s),
            NotifyTab::Posts => self.notify_posts(s),
            NotifyTab::General => self.notify_general(s),
        };
        column![switcher, body].spacing(12).height(Length::Fill).into()
    }

    /// The text shown when a global switch overrides the per-site ones.
    fn global_off_note<'a>(&self, s: Skin) -> Option<Element<'a, Msg>> {
        let g = &self.settings.notifications;
        let text = match (g.disable_notifications, g.disable_warnings) {
            (true, true) => "All notifications and warnings are disabled globally (General sub-tab).",
            (true, false) => "All notifications are disabled globally (General sub-tab).",
            (false, true) => "All warnings are disabled globally (General sub-tab).",
            (false, false) => return None,
        };
        Some(iced::widget::text(text).font(s.font).size(12).color(s.warn).into())
    }

    fn notify_livestreams(&self, s: Skin) -> Element<'_, Msg> {
        let sites = &self.settings.sites;
        if sites.is_empty() {
            return muted(&s, "Add a site in Settings > Livestreams to edit its notifications and warnings.").into();
        }
        let site = self.notify_site.and_then(|id| sites.iter().find(|x| x.id == id)).unwrap_or(&sites[0]);
        let choices: Vec<SiteChoice> = sites.iter().map(|x| SiteChoice { id: x.id, name: x.name.clone() }).collect();
        let selected = Some(SiteChoice { id: site.id, name: site.name.clone() });
        let picker = pick_list(choices, selected, |c: SiteChoice| Msg::PickNotifySite(c.id))
            .padding(8)
            .text_size(13)
            .width(PICKER_W);
        let mut col = Column::new().spacing(12).padding(Padding { right: 8.0, ..Padding::ZERO }).push(picker);
        if let Some(note) = self.global_off_note(s) {
            col = col.push(note);
        }
        if let Some(d) = self.drafts.get(&site.id) {
            col = col.push(self.notify_site_editor(s, site, d));
        }
        scrollable(col).height(Length::Fill).into()
    }

    fn notify_site_editor<'a>(&'a self, s: Skin, site: &'a Site, d: &'a SiteDraft) -> Element<'a, Msg> {
        let id = site.id;
        let global_topic = self.settings.notifications.ntfy_topic.as_str();
        let no_notify = self.settings.notifications.disable_notifications;
        let no_warn = self.settings.notifications.disable_warnings;
        let labeled = |label: &'static str, input: Element<'a, Msg>| -> Element<'a, Msg> {
            column![muted(&s, label), input].spacing(2).into()
        };
        let cell = |e: Element<'a, Msg>, w: Length| -> Element<'a, Msg> { container(e).width(w).into() };
        let field = |placeholder: &str, value: &'a str, f: fn(String) -> DraftField, enabled: bool| -> Element<'a, Msg> {
            text_input(placeholder, value)
                .on_input_maybe(enabled.then(|| move |v: String| Msg::Draft(id, f(v))))
                .padding(5)
                .size(13)
                .style(skin::input(s))
                .into()
        };
        let toggle = |label: &'static str, on: bool, enabled: bool, f: fn(bool) -> DraftField| -> Element<'a, Msg> {
            toggler(on && enabled)
                .label(label)
                .on_toggle_maybe(enabled.then(|| move |v: bool| Msg::Draft(id, f(v))))
                .text_size(12)
                .into()
        };
        let dirty = *d != SiteDraft::from_site(site);
        let parsing_off = self.parsing_overridden();
        let valid = d.to_new_site(global_topic, parsing_off).is_some();
        let can_test = d.ntfy_ok(global_topic) && !d.test_targets(global_topic).is_empty();
        let notify_on = d.notify && !no_notify;
        let channels_on = d.notify_desktop || d.notify_ntfy;
        let topic_on = notify_on && d.notify_ntfy;

        let topic_input = text_input(if topic_on { global_topic } else { "" }, &d.ntfy_topic)
            .on_input_maybe(topic_on.then(|| move |v: String| Msg::Draft(id, DraftField::NtfyTopic(v))))
            .padding(5)
            .size(13)
            .style(skin::input_required(s, topic_on && !d.ntfy_ok(global_topic)));
        let topic_row = row![
            topic_input,
            button(btn_label(&s, "Randomize").size(12))
                .on_press_maybe(topic_on.then(|| Msg::Draft(id, DraftField::NtfyTopic(notify::random_topic()))))
                .padding([5.0, 10.0])
                .style(skin::btn(s, BtnKind::Ghost)),
        ]
        .spacing(8)
        .align_y(Alignment::Center);

        let channels = row![
            toggler(d.notify && !no_notify)
                .label(format!("Enable Notifications for {} Livestreams", site.name))
                .on_toggle_maybe((!no_notify).then(|| move |v: bool| Msg::Draft(id, DraftField::Notify(v))))
                .text_size(12),
            toggler(d.warnings_enabled && !no_warn)
                .label(format!("Enable Warnings for {} Livestreams", site.name))
                .on_toggle_maybe((!no_warn).then(|| move |v: bool| Msg::Draft(id, DraftField::WarningsEnabled(v))))
                .text_size(12),
            toggle("Flash warnings", d.flash_warnings, !no_warn && d.warnings_enabled, DraftField::FlashWarnings),
            toggle("Desktop Notifications", d.notify_desktop, notify_on, DraftField::NotifyDesktop),
            toggle("Mobile Notifications (ntfy)", d.notify_ntfy, notify_on, DraftField::NotifyNtfy),
            Space::new().width(Length::Fill),
            button(t(&s, "Send test").size(12))
                .on_press_maybe(can_test.then_some(Msg::TestNotify(id)))
                .padding([5.0, 10.0])
                .style(skin::btn(s, BtnKind::Ghost)),
        ]
        .spacing(10)
        .align_y(Alignment::Center);

        let delivery = row![
            cell(
                labeled(
                    "Notification Cooldown (minutes)",
                    field(&DEFAULT_NOTIFY_COOLDOWN_MINUTES.to_string(), &d.notify_cooldown, DraftField::NotifyCooldown, !no_notify)
                ),
                Length::FillPortion(1)
            ),
            cell(
                labeled(
                    "Notification Timeout (seconds)",
                    field(&DEFAULT_NOTIFY_TIMEOUT_SECS.to_string(), &d.notify_timeout, DraftField::NotifyTimeout, !no_notify)
                ),
                Length::FillPortion(1)
            ),
            cell(labeled(TOPIC_LABEL, topic_row.into()), Length::FillPortion(2)),
        ]
        .spacing(8);

        let live_toggles = row![
            toggle("Wait for confirmed growth", d.notify_confirmed, !no_notify, DraftField::NotifyConfirmed),
            toggle("Notify when already live at launch", d.notify_on_launch, !no_notify, DraftField::NotifyOnLaunch),
        ]
        .spacing(24);

        let output_on = d.forward_output && !parsing_off;
        let matrix_cell = |on: bool, enabled: bool, f: fn(bool) -> DraftField| -> Element<'a, Msg> {
            container(toggler(on && enabled).on_toggle_maybe(enabled.then(|| move |v: bool| Msg::Draft(id, f(v)))))
                .width(COL_W)
                .align_x(Horizontal::Center)
                .into()
        };
        let matrix_line = |label: &'static str, notify_cell: Element<'a, Msg>, warn_cell: Element<'a, Msg>| -> Element<'a, Msg> {
            row![container(t(&s, label).size(12)).width(NAME_W), notify_cell, warn_cell].spacing(GAP).align_y(Alignment::Center).into()
        };
        let heads = row![
            Space::new().width(NAME_W),
            container(muted(&s, "Notify")).width(COL_W).align_x(Horizontal::Center),
            container(muted(&s, "Warn")).width(COL_W).align_x(Horizontal::Center),
        ]
        .spacing(GAP);
        let notify_cells = notify_on && channels_on;
        let warn_cells = !no_warn && d.warnings_enabled;
        let matrix = column![
            self.section_title(s, "NOTIFICATIONS AND WARNINGS"),
            heads,
            matrix_line(
                "Live",
                matrix_cell(d.notify_live, notify_cells, DraftField::NotifyLive),
                Space::new().width(COL_W).into(),
            ),
            matrix_line(
                "Stalled",
                matrix_cell(d.notify_stalled, notify_cells, DraftField::NotifyStalled),
                matrix_cell(d.warn_stalled, warn_cells, DraftField::WarnStalled),
            ),
            matrix_line(
                "Restarts",
                matrix_cell(d.notify_restarted, notify_cells, DraftField::NotifyRestarted),
                matrix_cell(d.warn_restarts, warn_cells, DraftField::WarnRestarts),
            ),
            matrix_line(
                "Interrupts",
                matrix_cell(d.notify_interrupts, notify_cells && output_on, DraftField::NotifyInterrupts),
                matrix_cell(d.warn_interrupts, warn_cells && output_on, DraftField::WarnInterrupts),
            ),
            matrix_line(
                "Errors",
                matrix_cell(d.notify_errors, notify_cells && output_on, DraftField::NotifyErrors),
                matrix_cell(d.warn_errors, warn_cells && output_on, DraftField::WarnErrors),
            ),
        ]
        .spacing(6);

        let restarts = column![
            row![
                cell(
                    labeled(
                        "Clear warnings after (minutes, 0 = never)",
                        field(&DEFAULT_WARNING_CLEAR_MINUTES.to_string(), &d.warning_clear, DraftField::WarningClear, true)
                    ),
                    Length::FillPortion(1)
                ),
                cell(
                    labeled(
                        "Warning threshold (restarts)",
                        field(&DEFAULT_RESTART_THRESHOLD.to_string(), &d.restart_threshold, DraftField::RestartThreshold, true)
                    ),
                    Length::FillPortion(1)
                ),
                Space::new().width(Length::FillPortion(2)),
            ]
            .spacing(8),
            muted(&s, "Restarts are counted based on \"Consider a stream ended after (minutes)\" in Settings > Livestreams."),
        ]
        .spacing(2);

        let file_checks = row![
            cell(
                labeled(
                    "Warn if file doesn't exist after (seconds)",
                    field(&DEFAULT_FILE_EXISTS_GRACE_SECS.to_string(), &d.file_exists_grace, DraftField::FileExistsGrace, true)
                ),
                Length::FillPortion(1)
            ),
            cell(
                labeled(
                    "Warn if file doesn't start growing after (seconds)",
                    field(&DEFAULT_GROWTH_GRACE_SECS.to_string(), &d.growth_grace, DraftField::GrowthGrace, true)
                ),
                Length::FillPortion(1)
            ),
            cell(
                labeled(
                    "Warn if file stops growing for (seconds)",
                    field(&DEFAULT_STALL_WINDOW_SECS.to_string(), &d.stall_window, DraftField::StallWindow, true)
                ),
                Length::FillPortion(1)
            ),
            cell(
                labeled(
                    "Check for growth every (seconds)",
                    field(&DEFAULT_FILE_CHECK_INTERVAL_SECS.to_string(), &d.file_check_interval, DraftField::FileCheckInterval, true)
                ),
                Length::FillPortion(1)
            ),
        ]
        .spacing(8);

        let interrupt_on = d.interrupts_active(parsing_off);
        let error_on = d.errors_active(parsing_off);
        let pattern_field = |label: &'static str, value: &'a str, on: bool, f: fn(String) -> DraftField| -> Element<'a, Msg> {
            let input = text_input("", if on { value } else { "" })
                .on_input_maybe(on.then(|| move |v: String| Msg::Draft(id, f(v))))
                .padding(5)
                .size(13)
                .style(skin::input_required(s, on && value.trim().is_empty()));
            labeled(label, input.into())
        };
        let patterns = row![
            container(toggle("Enable command output parsing", d.forward_output, !parsing_off, DraftField::ForwardOutput)).width(Length::Shrink),
            cell(
                pattern_field("Interrupt patterns (separate with |)", &d.interrupt_patterns, interrupt_on, DraftField::InterruptPatterns),
                Length::FillPortion(1)
            ),
            cell(
                pattern_field("Error patterns (separate with |)", &d.error_patterns, error_on, DraftField::ErrorPatterns),
                Length::FillPortion(1)
            ),
        ]
        .spacing(16)
        .align_y(Alignment::End);

        let actions = row![
            Space::new().width(Length::Fill),
            if dirty { chip(s, "unsaved changes", s.warn) } else { Space::new().into() },
            button(btn_label(&s, "Save changes").size(13))
                .on_press_maybe((dirty && valid).then_some(Msg::SaveSite(id)))
                .padding([5.0, 12.0])
                .style(skin::btn(s, BtnKind::Primary)),
            button(t(&s, "Revert").size(13))
                .on_press_maybe(dirty.then_some(Msg::RevertSite(id)))
                .padding([5.0, 12.0])
                .style(skin::btn(s, BtnKind::Ghost)),
            button(t(&s, "Defaults").size(13))
                .on_press(Msg::DefaultsNotify(id))
                .padding([5.0, 12.0])
                .style(skin::btn(s, BtnKind::Ghost)),
        ]
        .spacing(10)
        .align_y(Alignment::Center);

        card_box(
            s,
            column![channels, delivery, live_toggles, matrix, restarts, file_checks, patterns, actions].spacing(10),
        )
    }

    fn notify_posts(&self, s: Skin) -> Element<'_, Msg> {
        let p = &self.posts;
        if p.cfg.sites.is_empty() {
            return muted(&s, "Add a site in Settings > Posts to edit its notifications.").into();
        }
        let site = self.notify_post_site.and_then(|id| p.cfg.site(id)).unwrap_or(&p.cfg.sites[0]);
        let choices: Vec<SiteChoice> = p.cfg.sites.iter().map(|x| SiteChoice { id: x.id, name: x.name.clone() }).collect();
        let selected = Some(SiteChoice { id: site.id, name: site.name.clone() });
        let picker = pick_list(choices, selected, |c: SiteChoice| Msg::PickNotifyPostSite(c.id))
            .padding(8)
            .text_size(13)
            .width(PICKER_W);
        let mut col = Column::new().spacing(12).padding(Padding { right: 8.0, ..Padding::ZERO }).push(picker);
        if let Some(note) = self.global_off_note(s) {
            col = col.push(note);
        }
        if let Some(d) = p.drafts.get(&site.id) {
            col = col.push(self.notify_post_editor(s, site, d));
        }
        scrollable(col).height(Length::Fill).into()
    }

    fn notify_post_editor<'a>(&'a self, s: Skin, site: &'a PostSite, d: &'a super::posts::SiteDraft) -> Element<'a, Msg> {
        let id = site.id;
        let global_topic = self.settings.notifications.ntfy_topic.as_str();
        let no_notify = self.settings.notifications.disable_notifications;
        let notify_on = d.notify && !no_notify;
        let send = move |f: SiteField| Msg::Posts(PostsMsg::Site(id, f));
        let toggle = |label: &'static str, on: bool, enabled: bool, f: fn(bool) -> SiteField| -> Element<'a, Msg> {
            toggler(on && enabled)
                .label(label)
                .on_toggle_maybe(enabled.then(|| move |v: bool| send(f(v))))
                .text_size(12)
                .into()
        };
        let dirty = d.differs_from(site);
        let valid = d.valid(global_topic);
        let topic_on = notify_on && d.notify_ntfy;
        let topic_bad = topic_on && !d.ntfy_ok(global_topic);
        let topic_input = text_input(if topic_on { global_topic } else { "" }, &d.ntfy_topic)
            .on_input_maybe(topic_on.then(|| move |v: String| send(SiteField::NtfyTopic(v))))
            .padding(5)
            .size(13)
            .style(skin::input_required(s, topic_bad));
        let topic_row = row![
            topic_input,
            button(btn_label(&s, "Randomize").size(12))
                .on_press_maybe(topic_on.then(|| send(SiteField::NtfyTopic(notify::random_topic()))))
                .padding([5.0, 10.0])
                .style(skin::btn(s, BtnKind::Ghost)),
        ]
        .spacing(8)
        .align_y(Alignment::Center);
        let channels = row![
            toggler(d.notify && !no_notify)
                .label(format!("Enable Notifications for {} Posts", site.name))
                .on_toggle_maybe((!no_notify).then(|| move |v: bool| send(SiteField::Notify(v))))
                .text_size(12),
            toggle("Desktop Notifications", d.notify_desktop, notify_on, SiteField::NotifyDesktop),
            toggle("Mobile Notifications (ntfy)", d.notify_ntfy, notify_on, SiteField::NotifyNtfy),
        ]
        .spacing(24);
        let actions = row![
            Space::new().width(Length::Fill),
            if dirty { chip(s, "unsaved changes", s.warn) } else { Space::new().into() },
            button(btn_label(&s, "Save changes").size(13))
                .on_press_maybe((dirty && valid).then_some(Msg::Posts(PostsMsg::SaveSite(id))))
                .padding([5.0, 12.0])
                .style(skin::btn(s, BtnKind::Primary)),
            button(t(&s, "Revert").size(13))
                .on_press_maybe(dirty.then_some(Msg::Posts(PostsMsg::RevertSite(id))))
                .padding([5.0, 12.0])
                .style(skin::btn(s, BtnKind::Ghost)),
        ]
        .spacing(10)
        .align_y(Alignment::Center);
        card_box(
            s,
            column![
                self.section_title(s, "NOTIFICATIONS"),
                channels,
                column![muted(&s, TOPIC_LABEL), topic_row].spacing(2),
                actions
            ]
            .spacing(10),
        )
    }

    fn notify_general(&self, s: Skin) -> Element<'_, Msg> {
        let g = &self.settings.notifications;
        let typed = self.global_ntfy.trim();
        let dirty = typed != g.ntfy_topic;
        let topic_ok = typed.is_empty() || notify::topic_valid(typed);
        let switches = column![
            self.section_title(s, "GLOBAL"),
            toggler(g.disable_notifications)
                .label("Disable all notifications globally")
                .on_toggle(Msg::SetDisableNotifications)
                .text_size(12),
            toggler(g.disable_warnings)
                .label("Disable all warnings globally")
                .on_toggle(Msg::SetDisableWarnings)
                .text_size(12),
            muted(&s, "These override every site's own settings without changing them."),
        ]
        .spacing(8);
        let topic = column![
            muted(&s, "Global ntfy topic"),
            row![
                text_input("", &self.global_ntfy)
                    .on_input(Msg::GlobalNtfyEdit)
                    .on_submit(Msg::GlobalNtfySave)
                    .padding(8)
                    .style(skin::input_required(s, !topic_ok)),
                button(btn_label(&s, "Randomize").size(13))
                    .on_press(Msg::GlobalNtfyRandomize)
                    .padding([8.0, 16.0])
                    .style(skin::btn(s, BtnKind::Ghost)),
                button(btn_label(&s, "Revert").size(13))
                    .on_press_maybe(dirty.then_some(Msg::GlobalNtfyRevert))
                    .padding([8.0, 16.0])
                    .style(skin::btn(s, BtnKind::Ghost)),
                button(btn_label(&s, "Save").size(13))
                    .on_press_maybe((dirty && topic_ok).then_some(Msg::GlobalNtfySave))
                    .padding([8.0, 16.0])
                    .style(skin::btn(s, BtnKind::Primary)),
            ]
            .spacing(8),
            muted(&s, "Used by every site whose own ntfy topic is blank."),
        ]
        .spacing(6);
        let col = column![card_box(s, switches), card_box(s, topic)].spacing(12).padding(Padding { right: 8.0, ..Padding::ZERO });
        scrollable(col).height(Length::Fill).into()
    }
}
