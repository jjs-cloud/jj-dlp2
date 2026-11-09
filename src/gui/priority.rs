//! Priority tab state: an unsaved copy of the priority order, always-record flags and recording limit.

use super::{App, Msg};
use crate::api::{Settings, StreamerId};
use crate::policy::PolicyCommand;
use iced::Task;
use iced::widget::scrollable::{AbsoluteOffset, Viewport};
use std::collections::HashSet;

/// Height of one priority row and the gap between rows.
pub const ROW_H: f32 = 36.0;
pub const ROW_GAP: f32 = 6.0;

/// Scrollable id of the priority list.
pub fn scroll_id() -> iced::widget::Id {
    iced::widget::Id::new("priority-scroll")
}

/// The unsaved priority edits.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PriorityDraft {
    pub order: Vec<StreamerId>,
    pub always: HashSet<StreamerId>,
    pub max: String,
}

impl PriorityDraft {
    pub fn from_settings(s: &Settings) -> Self {
        Self {
            order: s.streamers.iter().map(|st| st.id).collect(),
            always: s.streamers.iter().filter(|st| st.options.always_record).map(|st| st.id).collect(),
            max: s.max_recordings.map(|m| m.to_string()).unwrap_or_default(),
        }
    }

    /// The recording limit typed in; empty, zero or invalid means unlimited.
    pub fn max_value(&self) -> Option<u32> {
        self.max.trim().parse().ok().filter(|m| *m > 0)
    }

    /// True when the draft differs from the saved settings.
    pub fn dirty(&self, saved: &Settings) -> bool {
        let base = Self::from_settings(saved);
        self.order != base.order || self.always != base.always || self.max_value() != saved.max_recordings
    }

    /// Keeps these edits but drops removed streamers and appends new ones at the bottom.
    fn merged(&self, saved: &Settings) -> Self {
        let base = Self::from_settings(saved);
        let mut order: Vec<StreamerId> = self.order.iter().copied().filter(|id| base.order.contains(id)).collect();
        let added: Vec<StreamerId> = base.order.iter().copied().filter(|id| !order.contains(id)).collect();
        let always = self.always.iter().copied().filter(|id| order.contains(id)).chain(added.iter().copied().filter(|id| base.always.contains(id))).collect();
        order.extend(added);
        Self { order, always, max: self.max.clone() }
    }

    fn move_by(&mut self, id: StreamerId, delta: isize) {
        let Some(i) = self.order.iter().position(|x| *x == id) else { return };
        let Some(j) = i.checked_add_signed(delta).filter(|j| *j < self.order.len()) else { return };
        self.order.swap(i, j);
    }

    fn set_always(&mut self, id: StreamerId, on: bool) {
        if !on {
            self.always.remove(&id);
            return;
        }
        self.always.insert(id);
        if let Some(i) = self.order.iter().position(|x| *x == id) {
            self.order.remove(i);
            self.order.insert(0, id);
        }
    }
}

#[derive(Debug, Default)]
pub struct PriorityState {
    pub draft: PriorityDraft,
    pub selected: Option<StreamerId>,
    /// Scroll offset and height of the list as last reported.
    scroll_y: f32,
    view_h: f32,
}

#[derive(Debug, Clone)]
pub enum PriorityMsg {
    Select(StreamerId),
    MoveUp,
    MoveDown,
    ToggleAlways(StreamerId, bool),
    MaxEdit(String),
    Save,
    Revert,
    Scrolled(Viewport),
}

impl App {
    pub(super) fn priority_update(&mut self, m: PriorityMsg) -> Task<Msg> {
        let st = &mut self.priority;
        match m {
            PriorityMsg::Select(id) => st.selected = Some(id),
            PriorityMsg::MoveUp => {
                if let Some(id) = st.selected {
                    st.draft.move_by(id, -1);
                }
                return self.priority_reveal();
            }
            PriorityMsg::MoveDown => {
                if let Some(id) = st.selected {
                    st.draft.move_by(id, 1);
                }
                return self.priority_reveal();
            }
            PriorityMsg::ToggleAlways(id, on) => st.draft.set_always(id, on),
            PriorityMsg::MaxEdit(v) => st.draft.max = v.chars().filter(char::is_ascii_digit).collect(),
            PriorityMsg::Save => {
                let d = &st.draft;
                self.client.send(PolicyCommand::SetPriority { order: d.order.clone(), always: d.always.clone(), max: d.max_value() });
            }
            PriorityMsg::Revert => st.draft = PriorityDraft::from_settings(&self.settings),
            PriorityMsg::Scrolled(v) => {
                st.scroll_y = v.absolute_offset().y;
                st.view_h = v.bounds().height;
            }
        }
        Task::none()
    }

    /// Scrolls the list so the selected row is inside the visible part.
    fn priority_reveal(&mut self) -> Task<Msg> {
        let st = &mut self.priority;
        let Some(i) = st.selected.and_then(|id| st.draft.order.iter().position(|x| *x == id)) else { return Task::none() };
        let h = if st.view_h > 0.0 { st.view_h } else { (self.win_h - 420.0).max(ROW_H * 4.0) };
        let (top, off) = (i as f32 * (ROW_H + ROW_GAP), st.scroll_y);
        let y = if top < off {
            top
        } else if top + ROW_H > off + h {
            top + ROW_H - h
        } else {
            return Task::none();
        };
        st.scroll_y = y;
        iced::widget::operation::scroll_to(scroll_id(), AbsoluteOffset { x: 0.0, y })
    }

    /// Refreshes the draft for `new` settings, keeping unsaved edits; call before `self.settings` is replaced.
    pub(super) fn priority_on_settings(&mut self, new: &Settings) {
        let st = &mut self.priority;
        st.draft = if st.draft.dirty(&self.settings) { st.draft.merged(new) } else { PriorityDraft::from_settings(new) };
        if st.selected.is_some_and(|id| !st.draft.order.contains(&id)) {
            st.selected = None;
        }
    }
}
