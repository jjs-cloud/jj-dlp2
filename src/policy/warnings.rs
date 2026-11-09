//! Per-streamer INTERRUPT and ERROR counts, matched against recorder output lines.

use crate::api::{Site, SiteId, StreamerId};
use std::collections::{HashMap, HashSet};

/// A streamer's warning counts.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Warnings {
    pub interrupts: u32,
    pub errors: u32,
}

/// What a matched output line flagged.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum WarnKind {
    Interrupt,
    Error,
}

impl WarnKind {
    /// Log line shown the first time this kind is flagged in a live span.
    pub fn log_text(self) -> &'static str {
        match self {
            WarnKind::Interrupt => "recorder output matched an interrupt pattern",
            WarnKind::Error => "recorder output matched an error pattern",
        }
    }
}

/// One pattern match in a recorder output line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hit {
    pub kind: WarnKind,
    pub pattern: String,
    /// First of its kind in the live span, so worth a log line.
    pub fresh: bool,
}

/// A site's patterns as last parsed, with the field text and switches they came from.
#[derive(Default)]
struct Compiled {
    interrupt_raw: String,
    error_raw: String,
    warn_interrupts: bool,
    warn_errors: bool,
    notify_interrupts: bool,
    notify_errors: bool,
    interrupt: Vec<String>,
    error: Vec<String>,
}

impl Compiled {
    fn of(site: &Site) -> Self {
        let patterns = |on: bool, text: &str| if on { parse_patterns(text) } else { Vec::new() };
        Self {
            interrupt_raw: site.interrupt_patterns.clone(),
            error_raw: site.error_patterns.clone(),
            warn_interrupts: site.warn_interrupts,
            warn_errors: site.warn_errors,
            notify_interrupts: site.notify_interrupts,
            notify_errors: site.notify_errors,
            interrupt: patterns(site.warn_interrupts || site.notify_interrupts, &site.interrupt_patterns),
            error: patterns(site.warn_errors || site.notify_errors, &site.error_patterns),
        }
    }

    fn is_current(&self, site: &Site) -> bool {
        self.interrupt_raw == site.interrupt_patterns
            && self.error_raw == site.error_patterns
            && self.warn_interrupts == site.warn_interrupts
            && self.warn_errors == site.warn_errors
            && self.notify_interrupts == site.notify_interrupts
            && self.notify_errors == site.notify_errors
    }
}

/// Splits `|`-separated phrases into trimmed lowercase patterns, dropping empty ones.
fn parse_patterns(text: &str) -> Vec<String> {
    text.split('|').map(|p| p.trim().to_lowercase()).filter(|p| !p.is_empty()).collect()
}

/// The first of the lowercase `patterns` that the lowercase `line` contains.
fn first_match<'a>(line: &str, patterns: &'a [String]) -> Option<&'a str> {
    patterns.iter().map(String::as_str).find(|p| line.contains(p))
}

#[derive(Default)]
pub struct WarnTracker {
    counts: HashMap<StreamerId, Warnings>,
    /// Kinds already logged in each streamer's current live span.
    logged: HashSet<(StreamerId, WarnKind)>,
    compiled: HashMap<SiteId, Compiled>,
    dirty: bool,
}

impl WarnTracker {
    /// Matches `line` against the site's patterns, counting each kind whose warning is on; returns every match.
    pub fn on_line(&mut self, id: StreamerId, site: &Site, line: &str) -> Vec<Hit> {
        let c = self.compiled.entry(site.id).or_default();
        if !c.is_current(site) {
            *c = Compiled::of(site);
        }
        if c.interrupt.is_empty() && c.error.is_empty() {
            return Vec::new();
        }
        let low = line.to_lowercase();
        let hits = [
            (WarnKind::Interrupt, first_match(&low, &c.interrupt).map(str::to_string)),
            (WarnKind::Error, first_match(&low, &c.error).map(str::to_string)),
        ];
        let mut found = Vec::new();
        for (kind, hit) in hits {
            let Some(pattern) = hit else { continue };
            let counted = match kind {
                WarnKind::Interrupt => c.warn_interrupts,
                WarnKind::Error => c.warn_errors,
            };
            if counted {
                let w = self.counts.entry(id).or_default();
                match kind {
                    WarnKind::Interrupt => w.interrupts += 1,
                    WarnKind::Error => w.errors += 1,
                }
                self.dirty = true;
            }
            let fresh = self.logged.insert((id, kind));
            found.push(Hit { kind, pattern, fresh });
        }
        found
    }

    /// Lets each kind be logged again for the streamer; called when its live span ends.
    pub fn end_span(&mut self, id: StreamerId) {
        self.logged.retain(|(sid, _)| *sid != id);
    }

    /// Resets every count.
    pub fn clear(&mut self) {
        if !self.counts.is_empty() {
            self.dirty = true;
        }
        self.counts.clear();
    }

    /// Resets one kind of count for the streamer.
    pub fn clear_kind(&mut self, id: StreamerId, kind: WarnKind) {
        let Some(w) = self.counts.get_mut(&id) else { return };
        let n = match kind {
            WarnKind::Interrupt => &mut w.interrupts,
            WarnKind::Error => &mut w.errors,
        };
        if *n > 0 {
            *n = 0;
            self.dirty = true;
        }
        if *w == Warnings::default() {
            self.counts.remove(&id);
        }
    }

    /// Keeps only the streamers and sites the predicates accept.
    pub fn retain(&mut self, mut keep_streamer: impl FnMut(StreamerId) -> bool, mut keep_site: impl FnMut(SiteId) -> bool) {
        let before = self.counts.len();
        self.counts.retain(|id, _| keep_streamer(*id));
        self.logged.retain(|(id, _)| keep_streamer(*id));
        self.compiled.retain(|id, _| keep_site(*id));
        if self.counts.len() != before {
            self.dirty = true;
        }
    }

    /// Streamer id to its counts, leaving out streamers with none.
    pub fn counts(&self) -> HashMap<StreamerId, Warnings> {
        self.counts.clone()
    }

    pub fn take_dirty(&mut self) -> bool {
        std::mem::take(&mut self.dirty)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::policy::{new_site, Preset};

    fn twitch() -> Site {
        new_site(Preset::Twitch).into_site(1)
    }

    /// The matches that were new to their live span, as kind and pattern.
    fn fresh(hits: Vec<Hit>) -> Vec<(WarnKind, String)> {
        hits.into_iter().filter(|h| h.fresh).map(|h| (h.kind, h.pattern)).collect()
    }

    const PACKET: &str = "[mpegts @ 000002b60c2f6b80] Packet corrupt (stream = 1, dts = 835343848).";
    const DISCONT: &str = "[aist#0:0/aac @ 000002b60c3045c0] timestamp discontinuity (stream id=0): 24000001";
    const AD_TAG: &str = "[in#0/hls @ 0x1] Skip ('#EXT-X-DISCONTINUITY')";
    const SEGMENT: &str = "[https @ 0x2] Opening 'https://x.hls.ttvnw.net/v1/segment/abc.ts' for reading";

    #[test]
    fn patterns_split_trim_and_skip_blanks() {
        assert_eq!(parse_patterns(" Packet corrupt | timestamp discontinuity ||"), ["packet corrupt", "timestamp discontinuity"]);
        assert!(parse_patterns("  ").is_empty());
        assert_eq!(first_match("a packet corrupt b", &["x".to_string(), "packet corrupt".to_string()]), Some("packet corrupt"));
        assert_eq!(first_match("anything", &[]), None);
    }

    #[test]
    fn counts_matches_per_kind_ignoring_case() {
        let (mut t, site) = (WarnTracker::default(), twitch());
        t.on_line(1, &site, PACKET);
        t.on_line(1, &site, DISCONT);
        t.on_line(1, &site, AD_TAG);
        t.on_line(1, &site, SEGMENT);
        t.on_line(2, &site, &PACKET.to_uppercase());
        assert_eq!(t.counts().get(&1), Some(&Warnings { interrupts: 1, errors: 2 }));
        assert_eq!(t.counts().get(&2), Some(&Warnings { interrupts: 0, errors: 1 }));
        assert!(t.take_dirty() && !t.take_dirty());
    }

    #[test]
    fn clear_kind_resets_only_that_kind() {
        let (mut t, site) = (WarnTracker::default(), twitch());
        t.on_line(1, &site, "Packet corrupt in /ad/ segment");
        t.take_dirty();
        t.clear_kind(1, WarnKind::Error);
        assert_eq!(t.counts().get(&1), Some(&Warnings { interrupts: 1, errors: 0 }));
        assert!(t.take_dirty());
        t.clear_kind(1, WarnKind::Interrupt);
        assert!(t.counts().is_empty());
    }

    #[test]
    fn a_line_matching_both_kinds_counts_both() {
        let (mut t, site) = (WarnTracker::default(), twitch());
        t.on_line(1, &site, "Packet corrupt in /ad/ segment");
        assert_eq!(t.counts().get(&1), Some(&Warnings { interrupts: 1, errors: 1 }));
    }

    #[test]
    fn blank_patterns_turn_a_kind_off() {
        let (mut t, mut site) = (WarnTracker::default(), twitch());
        site.interrupt_patterns.clear();
        t.on_line(1, &site, AD_TAG);
        assert!(t.counts().is_empty());
        site.error_patterns.clear();
        t.on_line(1, &site, PACKET);
        assert!(t.counts().is_empty() && !t.take_dirty());
    }

    #[test]
    fn warn_toggles_turn_a_kind_off() {
        let (mut t, mut site) = (WarnTracker::default(), twitch());
        site.warn_interrupts = false;
        t.on_line(1, &site, AD_TAG);
        assert!(t.counts().is_empty());
        site.warn_errors = false;
        t.on_line(1, &site, PACKET);
        assert!(t.counts().is_empty() && !t.take_dirty());
    }

    #[test]
    fn notify_alone_matches_without_counting() {
        let (mut t, mut site) = (WarnTracker::default(), twitch());
        site.warn_errors = false;
        site.notify_errors = true;
        let hits = t.on_line(1, &site, PACKET);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].kind, WarnKind::Error);
        assert!(t.counts().is_empty() && !t.take_dirty());
    }

    #[test]
    fn edited_patterns_apply_to_the_next_line() {
        let (mut t, mut site) = (WarnTracker::default(), twitch());
        t.on_line(1, &site, "custom marker");
        assert!(t.counts().is_empty());
        site.error_patterns = "Custom Marker".into();
        t.on_line(1, &site, "custom marker");
        assert_eq!(t.counts().get(&1), Some(&Warnings { interrupts: 0, errors: 1 }));
    }

    #[test]
    fn each_kind_is_logged_once_per_live_span() {
        let (mut t, site) = (WarnTracker::default(), twitch());
        assert_eq!(fresh(t.on_line(1, &site, PACKET)), [(WarnKind::Error, "packet corrupt".to_string())]);
        assert!(fresh(t.on_line(1, &site, DISCONT)).is_empty());
        assert_eq!(fresh(t.on_line(1, &site, AD_TAG)), [(WarnKind::Interrupt, "ext-x-discontinuity".to_string())]);
        assert_eq!(fresh(t.on_line(2, &site, PACKET)), [(WarnKind::Error, "packet corrupt".to_string())]);
        t.end_span(1);
        assert_eq!(fresh(t.on_line(1, &site, PACKET)), [(WarnKind::Error, "packet corrupt".to_string())]);
        assert!(fresh(t.on_line(2, &site, PACKET)).is_empty());
    }

    #[test]
    fn clear_and_retain_reset_counts() {
        let (mut t, site) = (WarnTracker::default(), twitch());
        t.on_line(1, &site, PACKET);
        t.on_line(2, &site, PACKET);
        t.take_dirty();
        t.retain(|id| id == 1, |_| true);
        assert!(t.take_dirty() && t.counts().len() == 1);
        t.clear();
        assert!(t.take_dirty() && t.counts().is_empty());
        t.clear();
        assert!(!t.take_dirty());
    }
}
