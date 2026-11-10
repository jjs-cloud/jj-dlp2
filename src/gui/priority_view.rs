//! View of the Priority settings tab.

use super::priority::{PriorityMsg, ROW_GAP, ROW_H, scroll_id};
use super::skin::{self, BtnKind, Skin};
use super::view::{btn_label, card_box, chip, muted, t, warn_text};
use super::{App, Msg};
use iced::widget::{Column, Space, button, column, container, row, scrollable, text_input, toggler};
use iced::{Alignment, Element, Length};

fn send(m: PriorityMsg) -> Msg {
    Msg::Priority(m)
}

impl App {
    pub(super) fn priority_settings(&self, s: Skin) -> Element<'_, Msg> {
        let st = &self.priority;
        let d = &st.draft;
        let dirty = d.dirty(&self.settings);
        let pos = st.selected.and_then(|id| d.order.iter().position(|x| *x == id));

        let notice: Element<'_, Msg> = if dirty {
            warn_text(&s, "⚠ Unsaved changes. Press Save changes to apply them.").size(13).width(Length::Fill).into()
        } else {
            Space::new().width(Length::Fill).into()
        };

        let limit = column![
            self.section_title(s, "RECORDING LIMIT"),
            muted(&s, "Most streamers recorded at once; leave empty for unlimited. Changes apply when saved and can stop running recordings."),
            row![
                text_input("Unlimited", &d.max)
                    .on_input(|v| send(PriorityMsg::MaxEdit(v)))
                    .on_submit_maybe(dirty.then_some(send(PriorityMsg::Save)))
                    .padding(8)
                    .width(140)
                    .style(skin::input(s)),
                notice,
                button(btn_label(&s, "Revert").size(13))
                    .on_press_maybe(dirty.then_some(send(PriorityMsg::Revert)))
                    .padding([8.0, 16.0])
                    .style(skin::btn(s, BtnKind::Ghost)),
                button(btn_label(&s, "Save changes").size(13))
                    .on_press_maybe(dirty.then_some(send(PriorityMsg::Save)))
                    .padding([8.0, 16.0])
                    .style(skin::btn(s, BtnKind::Primary)),
            ]
            .spacing(8)
            .align_y(Alignment::Center),
        ]
        .spacing(8);

        let mover = |label: &'static str, ok: bool, msg: PriorityMsg| {
            button(t(&s, label).size(12))
                .on_press_maybe(ok.then_some(send(msg)))
                .padding([5.0, 12.0])
                .style(skin::btn(s, BtnKind::Ghost))
        };
        let controls = row![
            self.section_title(s, "PRIORITY"),
            Space::new().width(Length::Fill),
            mover("Move up", pos.is_some_and(|i| i > 0), PriorityMsg::MoveUp),
            mover("Move down", pos.is_some_and(|i| i + 1 < d.order.len()), PriorityMsg::MoveDown),
        ]
        .spacing(8)
        .align_y(Alignment::Center);

        let mut list: Column<'_, Msg> = Column::new().spacing(ROW_GAP).padding(iced::Padding { right: 8.0, ..Default::default() });
        if d.order.is_empty() {
            list = list.push(muted(&s, "No streamers yet."));
        }
        for (i, id) in d.order.iter().copied().enumerate() {
            let Some(streamer) = self.settings.streamers.iter().find(|x| x.id == id) else { continue };
            let site = self.settings.sites.iter().find(|x| x.id == streamer.site_id);
            let color = site.map_or(s.accent2, |x| s.site_color(x.color.as_deref()));
            let selected = st.selected == Some(id);
            let kind = if selected { BtnKind::TabActive } else { BtnKind::Ghost };
            let label = row![
                muted(&s, format!("{}", i + 1)).width(30),
                chip(s, site.map_or("?", |x| x.name.as_str()).to_string(), color),
                t(&s, streamer.name.clone()).size(13),
            ]
            .spacing(10)
            .align_y(Alignment::Center);
            list = list.push(
                row![
                    button(label).on_press(send(PriorityMsg::Select(id))).width(Length::Fill).height(Length::Fill).padding([2.0, 10.0]).style(skin::btn(s, kind)),
                    toggler(d.always.contains(&id))
                        .label("Always record")
                        .on_toggle(move |on| send(PriorityMsg::ToggleAlways(id, on)))
                        .text_size(12)
                        .width(150),
                ]
                .spacing(10)
                .height(ROW_H)
                .align_y(Alignment::Center),
            );
        }
        let list_card = column![
            controls,
            muted(&s, "Top is the highest priority. Select a streamer, then move it. Always record streamers move to the top and are never stopped for others."),
            scrollable(list).id(scroll_id()).on_scroll(|v| send(PriorityMsg::Scrolled(v))).height(Length::Fill),
        ]
        .spacing(8);
        let list_card = container(list_card).padding(14).width(Length::Fill).height(Length::Fill).style(skin::card(s, None));

        Column::new()
            .spacing(12)
            .padding(iced::Padding { right: 8.0, ..Default::default() })
            .push(card_box(s, limit))
            .push(list_card)
            .height(Length::Fill)
            .into()
    }
}
