//! Tracks live spans and each streamer's most recent live sighting, persisted in live.json.

use crate::api::{Settings, SiteId, StreamerId};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

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
    /// Each streamer's current recording part within its live span.
    parts: HashMap<StreamerId, PartRecord>,
}

/// Where a streamer is in its `_partN` numbering during one live span.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PartRecord {
    /// Start of the live span this record belongs to.
    pub span: u64,
    /// Number of the current part.
    pub part: u32,
    /// Path of part 1 while it still lacks its `_part1` suffix.
    pub file: Option<String>,
}

#[derive(Default)]
pub struct LiveTracker {
    spans: HashMap<StreamerId, u64>,
    last_live: HashMap<StreamerId, u64>,
    gql_checked_at: Option<u64>,
    parts: HashMap<StreamerId, PartRecord>,
    seen_check: HashMap<StreamerId, u64>,
    check_failed: HashSet<StreamerId>,
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
        let LiveFile { mut spans, mut last_live, gql_checked_at, mut parts } = read();
        spans.retain(|id, _| {
            lookup(settings, *id)
                .is_some_and(|(_, secs)| last_live.get(id).is_some_and(|l| now.saturating_sub(*l) < secs))
        });
        last_live.retain(|id, _| lookup(settings, *id).is_some());
        parts.retain(|id, p| spans.get(id) == Some(&p.span));
        Self { spans, last_live, gql_checked_at, parts, ..Default::default() }
    }

    /// Keeps only the streamers the predicate accepts.
    pub fn retain(&mut self, mut keep: impl FnMut(StreamerId) -> bool) {
        let (spans, last_live) = (self.spans.len(), self.last_live.len());
        self.spans.retain(|id, _| keep(*id));
        self.last_live.retain(|id, _| keep(*id));
        self.parts.retain(|id, _| keep(*id));
        self.check_failed.retain(|id| keep(*id));
        if self.spans.len() != spans {
            self.dirty = true;
            self.span_changed = true;
        }
        if self.last_live.len() != last_live {
            self.dirty = true;
            self.last_live_changed = true;
        }
    }

    /// Records whether a streamer's latest check succeeded; failed checks can't prove a stream ended.
    pub fn streamer_checked(&mut self, id: StreamerId, ok: bool) {
        if ok {
            self.check_failed.remove(&id);
        } else {
            self.check_failed.insert(id);
        }
    }

    /// Applies one check's result for a streamer; repeats of the same check are ignored. Returns true when a live span began.
    pub fn status(&mut self, id: StreamerId, checked: u64, live: bool, end_after_secs: u64) -> bool {
        if self.seen_check.insert(id, checked) == Some(checked) {
            return false;
        }
        let mut started = false;
        if live {
            if !self.spans.contains_key(&id) {
                self.spans.insert(id, checked);
                self.dirty = true;
                self.span_changed = true;
                started = true;
            }
            if self.last_live.get(&id).is_none_or(|l| checked > *l) {
                self.last_live.insert(id, checked);
                self.dirty = true;
                self.last_live_changed = true;
            }
        } else if !self.check_failed.contains(&id) && self.spans.contains_key(&id) {
            let last = self.last_live.get(&id).copied().unwrap_or(0);
            if checked.saturating_sub(last) >= end_after_secs {
                self.spans.remove(&id);
                self.parts.remove(&id);
                self.dirty = true;
                self.span_changed = true;
            }
        }
        started
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

    /// The time the streamer was first seen live in its current span, if it has one.
    pub fn span_start(&self, id: StreamerId) -> Option<u64> {
        self.spans.get(&id).copied()
    }

    /// The streamer's part record for its current live span.
    pub fn part(&self, id: StreamerId) -> Option<&PartRecord> {
        self.parts.get(&id).filter(|p| self.spans.get(&id) == Some(&p.span))
    }

    /// Stores the streamer's part record.
    pub fn set_part(&mut self, id: StreamerId, record: PartRecord) {
        if self.parts.get(&id) != Some(&record) {
            self.parts.insert(id, record);
            self.dirty = true;
        }
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
            parts: self.parts.clone(),
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
        t.status(1, 100, true, END);
        t.status(1, 160, true, END);
        assert_eq!(t.live_since().get(&1), Some(&100));
        assert_eq!(t.last_live().get(&1), Some(&160));
    }

    #[test]
    fn repeated_check_is_ignored() {
        let mut t = LiveTracker::default();
        t.status(1, 100, true, END);
        t.status(1, 100, false, END);
        assert_eq!(t.live_since().get(&1), Some(&100));
    }

    #[test]
    fn short_offline_gap_keeps_the_span() {
        let mut t = LiveTracker::default();
        t.status(1, 100, true, END);
        t.status(1, 160, false, END);
        t.status(1, 220, true, END);
        assert_eq!(t.live_since().get(&1), Some(&100));
    }

    #[test]
    fn long_offline_gap_ends_the_span() {
        let mut t = LiveTracker::default();
        t.status(1, 100, true, END);
        t.status(1, 160, false, END);
        t.status(1, 100 + END, false, END);
        assert!(t.live_since().is_empty());
        t.status(1, 100 + END + 60, true, END);
        assert_eq!(t.live_since().get(&1), Some(&(100 + END + 60)));
    }

    #[test]
    fn failed_check_only_blocks_its_own_streamer() {
        let mut t = LiveTracker::default();
        t.status(1, 100, true, END);
        t.status(2, 100, true, END);
        t.streamer_checked(1, false);
        t.streamer_checked(2, true);
        t.status(1, 100 + END + 600, false, END);
        t.status(2, 100 + END + 600, false, END);
        assert!(t.live_since().contains_key(&1));
        assert!(!t.live_since().contains_key(&2));
    }

    #[test]
    fn failed_check_cannot_end_the_span() {
        let mut t = LiveTracker::default();
        t.status(1, 100, true, END);
        t.streamer_checked(1, false);
        t.status(1, 100 + END + 600, false, END);
        assert_eq!(t.live_since().get(&1), Some(&100));
    }

    #[test]
    fn last_live_outlives_the_span() {
        let mut t = LiveTracker::default();
        t.status(1, 100, true, END);
        t.status(1, 160, true, END);
        t.status(1, 160 + END, false, END);
        assert!(t.live_since().is_empty());
        assert_eq!(t.last_live().get(&1), Some(&160));
    }

    #[test]
    fn backfill_only_fills_missing_entries() {
        let mut t = LiveTracker::default();
        t.status(1, 500, true, END);
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
        t.status(1, 500, true, END);
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
    fn part_record_lives_and_dies_with_its_span() {
        let mut t = LiveTracker::default();
        t.status(1, 100, true, END);
        t.set_part(1, PartRecord { span: 100, part: 2, file: None });
        assert_eq!(t.part(1).map(|p| p.part), Some(2));
        t.status(1, 160, false, END);
        t.status(1, 100 + END, false, END);
        assert!(t.part(1).is_none());
    }

    #[test]
    fn part_record_from_an_older_span_is_ignored() {
        let mut t = LiveTracker::default();
        t.status(1, 500, true, END);
        t.set_part(1, PartRecord { span: 100, part: 3, file: None });
        assert!(t.part(1).is_none());
    }

    #[test]
    fn retain_drops_streamers_and_flags_changes() {
        let mut t = LiveTracker::default();
        t.status(1, 100, true, END);
        t.status(2, 100, true, END);
        t.take_dirty();
        t.take_span_changed();
        t.take_last_live_changed();
        t.retain(|id| id == 1);
        assert_eq!(t.live_since().len(), 1);
        assert_eq!(t.last_live().len(), 1);
        assert!(t.take_dirty() && t.take_span_changed() && t.take_last_live_changed());
    }
}
