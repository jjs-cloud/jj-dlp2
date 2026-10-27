//! Tracks when each streamer was first seen live and when the checker last saw them live.

use crate::api::{Settings, SiteId, StreamerId};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Unix seconds of the first and the most recent live sighting of one stream.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct LiveRecord {
    pub live_since: u64,
    pub last_live: u64,
}

#[derive(Default)]
pub struct LiveTracker {
    records: HashMap<StreamerId, LiveRecord>,
    seen_check: HashMap<StreamerId, u64>,
    site_ok: HashMap<SiteId, bool>,
    dirty: bool,
    span_changed: bool,
}

/// The streamer's site id and its end-after threshold in seconds.
pub fn lookup(settings: &Settings, id: StreamerId) -> Option<(SiteId, u64)> {
    let site_id = settings.streamers.iter().find(|s| s.id == id)?.site_id;
    let site = settings.sites.iter().find(|s| s.id == site_id)?;
    Some((site_id, site.end_after_minutes.saturating_mul(60)))
}

impl LiveTracker {
    /// Loads live.json, dropping unknown streamers and streams already past their end-after window.
    pub fn load(settings: &Settings, now: u64) -> Self {
        let mut t = Self { records: read(), ..Default::default() };
        t.retain(|id, r| lookup(settings, id).is_some_and(|(_, secs)| now.saturating_sub(r.last_live) < secs));
        t.dirty = false;
        t.span_changed = false;
        t
    }

    /// Keeps only the records the predicate accepts.
    pub fn retain(&mut self, mut keep: impl FnMut(StreamerId, &LiveRecord) -> bool) {
        let before = self.records.len();
        self.records.retain(|id, r| keep(*id, r));
        if self.records.len() != before {
            self.dirty = true;
            self.span_changed = true;
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
            match self.records.get_mut(&id) {
                Some(r) => {
                    if checked > r.last_live {
                        r.last_live = checked;
                        self.dirty = true;
                    }
                }
                None => {
                    self.records.insert(id, LiveRecord { live_since: checked, last_live: checked });
                    self.dirty = true;
                    self.span_changed = true;
                }
            }
        } else if self.site_ok.get(&site).copied().unwrap_or(true) {
            let ended = self.records.get(&id).is_some_and(|r| checked.saturating_sub(r.last_live) >= end_after_secs);
            if ended {
                self.records.remove(&id);
                self.dirty = true;
                self.span_changed = true;
            }
        }
    }

    /// Streamer id to the time it was first seen live.
    pub fn live_since(&self) -> HashMap<StreamerId, u64> {
        self.records.iter().map(|(id, r)| (*id, r.live_since)).collect()
    }

    /// True once after any change that needs writing to live.json.
    pub fn take_dirty(&mut self) -> bool {
        std::mem::take(&mut self.dirty)
    }

    /// True once after any change to which streamers have a live_since.
    pub fn take_span_changed(&mut self) -> bool {
        std::mem::take(&mut self.span_changed)
    }

    /// Writes live.json atomically via a temp file.
    pub fn save(&self) -> std::io::Result<()> {
        let dir = crate::settings::config_dir();
        std::fs::create_dir_all(&dir)?;
        let tmp = dir.join("live.json.tmp");
        let json = serde_json::to_string_pretty(&self.records).map_err(std::io::Error::other)?;
        std::fs::write(&tmp, json)?;
        std::fs::rename(&tmp, path())
    }
}

fn path() -> std::path::PathBuf {
    crate::settings::config_dir().join("live.json")
}

/// Reads live.json, returning nothing when it is missing or invalid.
fn read() -> HashMap<StreamerId, LiveRecord> {
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
        assert_eq!(t.records[&1].last_live, 160);
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
    fn retain_drops_records_and_flags_changes() {
        let mut t = LiveTracker::default();
        t.status(1, 1, 100, true, END);
        t.status(2, 1, 100, true, END);
        t.take_dirty();
        t.take_span_changed();
        t.retain(|id, _| id == 1);
        assert_eq!(t.live_since().len(), 1);
        assert!(t.take_dirty() && t.take_span_changed());
    }
}
