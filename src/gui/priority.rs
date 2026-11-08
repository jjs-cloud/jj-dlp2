//! Priority tab state: an unsaved copy of the priority order, always-record flags and recording limit.

use super::App;
use crate::api::{Settings, StreamerId};
use crate::policy::PolicyCommand;
use std::collections::HashSet;

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
}

impl App {
    pub(super) fn priority_update(&mut self, m: PriorityMsg) {
        let st = &mut self.priority;
        match m {
            PriorityMsg::Select(id) => st.selected = Some(id),
            PriorityMsg::MoveUp => {
                if let Some(id) = st.selected {
                    st.draft.move_by(id, -1);
                }
            }
            PriorityMsg::MoveDown => {
                if let Some(id) = st.selected {
                    st.draft.move_by(id, 1);
                }
            }
            PriorityMsg::ToggleAlways(id, on) => st.draft.set_always(id, on),
            PriorityMsg::MaxEdit(v) => st.draft.max = v.chars().filter(char::is_ascii_digit).collect(),
            PriorityMsg::Save => {
                let d = &st.draft;
                self.client.send(PolicyCommand::SetPriority { order: d.order.clone(), always: d.always.clone(), max: d.max_value() });
            }
            PriorityMsg::Revert => st.draft = PriorityDraft::from_settings(&self.settings),
        }
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
