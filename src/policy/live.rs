//! Tracks live spans and each streamer's most recent live sighting, persisted in live.json.

use crate::api::{Settings, SiteId, StreamerId};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Contents of live.json.
#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(default)]
struct LiveFile {
    /// Unix seconds each current live span started.
    spans: HashMap<StreamerId, u64>,
    /// Unix seconds of each streamer's most recent live sighting.
    last_live: HashMap<StreamerId, u64>,
    /// Unix seconds of the last Twitch GQL lookup attempt.
    gql_checked_at: Option<u64>,
}

#[derive(Default)]
pub struct LiveTracker {
    spans: HashMap<StreamerId, u64>,
    last_live: HashMap<StreamerId, u64>,
    gql_checked_at: Option<u64>,
    seen_check: HashMap<StreamerId, u64>,
    site_ok: HashMap<SiteId, bool>,
    dirty: bool,
    span_changed: bool,
    last_live_changed: bool,
}

/// The streamer's site id and its end-after threshold in seconds.
pub fn lookup(settings: &Settings, id: StreamerId) -> Option<(SiteId, u64)> {
    let site_id = settings.streamers.iter().find(|s| s.id == id)?.site_id;
    let site = settings.sites.iter().find(|s| s.id == site_id)?;
    Some((site_id, site.end_after_or_default().saturating_mul(60)))
}

impl LiveTracker {
    /// Loads live.json, dropping unknown streamers and spans already past their end-after window.
    pub fn load(settings: &Settings, now: u64) -> Self {
        let LiveFile { mut spans, mut last_live, gql_checked_at } = read();
        spans.retain(|id, _| {
            lookup(settings, *id)
                .is_some_and(|(_, secs)| last_live.get(id).is_some_and(|l| now.saturating_sub(*l) < secs))
        });
        last_live.retain(|id, _| lookup(settings, *id).is_some());
        Self { spans, last_live, gql_checked_at, ..Default::default() }
    }

    /// Keeps only the streamers the predicate accepts.
    pub fn retain(&mut self, mut keep: impl FnMut(StreamerId) -> bool) {
        let (spans, last_live) = (self.spans.len(), self.last_live.len());
        self.spans.retain(|id, _| keep(*id));
        self.last_live.retain(|id, _| keep(*id));
        if self.spans.len() != spans {
            self.dirty = true;
            self.span_changed = true;
        }
        if self.last_live.len() != last_live {
            self.dirty = true;
            self.last_live_changed = true;
        }
    }

    /// Records whether a site's latest check succeeded; failed checks can't prove a stream ended.
    pub fn site_checked(&mut self, site: SiteId, ok: bool) {
        self.site_ok.insert(site, ok);
    }

    /// Applies one check's result for a streamer; repeats of the same check are ignored.
    pub fn status(&mut self, id: StreamerId, site: SiteId, checked: u64, live: bool, end_after_secs: u64) {
        if self.seen_check.insert(id, checked) == Some(checked) {
            return;
        }
        if live {
            if !self.spans.contains_key(&id) {
                self.spans.insert(id, checked);
                self.dirty = true;
                self.span_changed = true;
            }
            if self.last_live.get(&id).is_none_or(|l| checked > *l) {
                self.last_live.insert(id, checked);
                self.dirty = true;
                self.last_live_changed = true;
            }
        } else if self.site_ok.get(&site).copied().unwrap_or(true) && self.spans.contains_key(&id) {
            let last = self.last_live.get(&id).copied().unwrap_or(0);
            if checked.saturating_sub(last) >= end_after_secs {
                self.spans.remove(&id);
                self.dirty = true;
                self.span_changed = true;
            }
        }
    }

    /// Adds last-live times for streamers that have none; returns how many were added.
    pub fn backfill(&mut self, found: HashMap<StreamerId, u64>) -> usize {
        let mut added = 0;
        for (id, at) in found {
            if let std::collections::hash_map::Entry::Vacant(e) = self.last_live.entry(id) {
                e.insert(at);
                added += 1;
            }
        }
        if added > 0 {
            self.dirty = true;
            self.last_live_changed = true;
        }
        added
    }

    /// True when the streamer has a stored last-live time.
    pub fn has_last_live(&self, id: StreamerId) -> bool {
        self.last_live.contains_key(&id)
    }

    /// Unix seconds of the last Twitch GQL lookup attempt.
    pub fn gql_checked_at(&self) -> Option<u64> {
        self.gql_checked_at
    }

    /// Records a Twitch GQL lookup attempt.
    pub fn set_gql_checked_at(&mut self, at: u64) {
        self.gql_checked_at = Some(at);
        self.dirty = true;
    }

    /// Streamer id to the time it was first seen live in its current span.
    pub fn live_since(&self) -> HashMap<StreamerId, u64> {
        self.spans.clone()
    }

    /// Streamer id to the time it was most recently seen live.
    pub fn last_live(&self) -> HashMap<StreamerId, u64> {
        self.last_live.clone()
    }

    /// True once after any change that needs writing to live.json.
    pub fn take_dirty(&mut self) -> bool {
        std::mem::take(&mut self.dirty)
    }

    /// True once after any change to which streamers have a live span.
    pub fn take_span_changed(&mut self) -> bool {
        std::mem::take(&mut self.span_changed)
    }

    /// True once after any change to the last-live times.
    pub fn take_last_live_changed(&mut self) -> bool {
        std::mem::take(&mut self.last_live_changed)
    }

    /// Writes live.json atomically via a temp file.
    pub fn save(&self) -> std::io::Result<()> {
        let dir = crate::settings::config_dir();
        std::fs::create_dir_all(&dir)?;
        let tmp = dir.join("live.json.tmp");
        let file = LiveFile {
            spans: self.spans.clone(),
            last_live: self.last_live.clone(),
            gql_checked_at: self.gql_checked_at,
        };
        let json = serde_json::to_string_pretty(&file).map_err(std::io::Error::other)?;
        std::fs::write(&tmp, json)?;
        std::fs::rename(&tmp, path())
    }
}

fn path() -> std::path::PathBuf {
    crate::settings::config_dir().join("live.json")
}

/// Reads live.json, returning defaults when it is missing or invalid.
fn read() -> LiveFile {
    std::fs::read_to_string(path()).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    const END: u64 = 1800;

    #[test]
    fn first_live_sighting_starts_the_span() {
        let mut t = LiveTracker::default();
        t.status(1, 1, 100, true, END);
        t.status(1, 1, 160, true, END);
        assert_eq!(t.live_since().get(&1), Some(&100));
        assert_eq!(t.last_live().get(&1), Some(&160));
    }

    #[test]
    fn repeated_check_is_ignored() {
        let mut t = LiveTracker::default();
        t.status(1, 1, 100, true, END);
        t.status(1, 1, 100, false, END);
        assert_eq!(t.live_since().get(&1), Some(&100));
    }

    #[test]
    fn short_offline_gap_keeps_the_span() {
        let mut t = LiveTracker::default();
        t.status(1, 1, 100, true, END);
        t.status(1, 1, 160, false, END);
        t.status(1, 1, 220, true, END);
        assert_eq!(t.live_since().get(&1), Some(&100));
    }

    #[test]
    fn long_offline_gap_ends_the_span() {
        let mut t = LiveTracker::default();
        t.status(1, 1, 100, true, END);
        t.status(1, 1, 160, false, END);
        t.status(1, 1, 100 + END, false, END);
        assert!(t.live_since().is_empty());
        t.status(1, 1, 100 + END + 60, true, END);
        assert_eq!(t.live_since().get(&1), Some(&(100 + END + 60)));
    }

    #[test]
    fn failed_check_cannot_end_the_span() {
        let mut t = LiveTracker::default();
        t.status(1, 1, 100, true, END);
        t.site_checked(1, false);
        t.status(1, 1, 100 + END + 600, false, END);
        assert_eq!(t.live_since().get(&1), Some(&100));
    }

    #[test]
    fn last_live_outlives_the_span() {
        let mut t = LiveTracker::default();
        t.status(1, 1, 100, true, END);
        t.status(1, 1, 160, true, END);
        t.status(1, 1, 160 + END, false, END);
        assert!(t.live_since().is_empty());
        assert_eq!(t.last_live().get(&1), Some(&160));
    }

    #[test]
    fn backfill_only_fills_missing_entries() {
        let mut t = LiveTracker::default();
        t.status(1, 1, 500, true, END);
        t.take_dirty();
        t.take_last_live_changed();
        let added = t.backfill(HashMap::from([(1, 100), (2, 200)]));
        assert_eq!(added, 1);
        assert_eq!(t.last_live().get(&1), Some(&500));
        assert_eq!(t.last_live().get(&2), Some(&200));
        assert!(t.take_dirty() && t.take_last_live_changed());
        assert_eq!(t.backfill(HashMap::from([(2, 300)])), 0);
        assert!(!t.take_dirty());
    }

    #[test]
    fn live_sighting_overrides_a_backfilled_time() {
        let mut t = LiveTracker::default();
        t.backfill(HashMap::from([(1, 100)]));
        t.status(1, 1, 500, true, END);
        assert_eq!(t.last_live().get(&1), Some(&500));
    }

    #[test]
    fn gql_attempt_is_stored_and_marks_dirty() {
        let mut t = LiveTracker::default();
        assert_eq!(t.gql_checked_at(), None);
        t.set_gql_checked_at(42);
        assert_eq!(t.gql_checked_at(), Some(42));
        assert!(t.take_dirty());
    }

    #[test]
    fn retain_drops_streamers_and_flags_changes() {
        let mut t = LiveTracker::default();
        t.status(1, 1, 100, true, END);
        t.status(2, 1, 100, true, END);
        t.take_dirty();
        t.take_span_changed();
        t.take_last_live_changed();
        t.retain(|id| id == 1);
        assert_eq!(t.live_since().len(), 1);
        assert_eq!(t.last_live().len(), 1);
        assert!(t.take_dirty() && t.take_span_changed() && t.take_last_live_changed());
    }
}
