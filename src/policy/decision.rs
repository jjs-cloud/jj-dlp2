//! Decides whether a live streamer should be recording right now.

use super::notify::NotRecordingReason;
use crate::api::{Streamer, StreamerOptions};

/// What the policy layer wants for a streamer's recording.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decision {
    Record,
    NotRecording(NotRecordingReason),
}

/// Unix seconds the intro delay ends, or `None` when the streamer has no intro delay.
pub fn intro_delay_end(options: &StreamerOptions, live_since: u64) -> Option<u64> {
    (options.intro_delay_enabled && options.intro_delay_minutes > 0)
        .then(|| live_since.saturating_add(options.intro_delay_minutes.saturating_mul(60)))
}

/// Decides whether the streamer should be recording at `now`; `live_since` starts its current live span.
pub fn decide(streamer: &Streamer, live_since: Option<u64>, now: u64) -> Decision {
    if !streamer.enabled {
        return Decision::NotRecording(NotRecordingReason::Disabled);
    }
    if streamer.options.intro_split_minutes().is_some() {
        return Decision::Record;
    }
    let delay_end = intro_delay_end(&streamer.options, live_since.unwrap_or(now));
    if delay_end.is_some_and(|end| now < end) {
        return Decision::NotRecording(NotRecordingReason::IntroDelay);
    }
    Decision::Record
}

#[cfg(test)]
mod tests {
    use super::*;

    fn streamer(enabled: bool, delay: Option<u64>) -> Streamer {
        let options = StreamerOptions {
            intro_delay_enabled: delay.is_some(),
            intro_delay_minutes: delay.unwrap_or(0),
            ..Default::default()
        };
        Streamer { id: 1, site_id: 1, name: "n".into(), url: "u".into(), enabled, options, schedule_runs: Default::default() }
    }

    #[test]
    fn disabled_streamer_is_not_recorded() {
        let d = decide(&streamer(false, None), Some(0), 100);
        assert_eq!(d, Decision::NotRecording(NotRecordingReason::Disabled));
    }

    #[test]
    fn no_delay_records_at_once() {
        assert_eq!(decide(&streamer(true, None), Some(100), 100), Decision::Record);
    }

    #[test]
    fn zero_minutes_or_switch_off_means_no_delay() {
        assert_eq!(decide(&streamer(true, Some(0)), Some(100), 100), Decision::Record);
        let mut s = streamer(true, Some(5));
        s.options.intro_delay_enabled = false;
        assert_eq!(decide(&s, Some(100), 100), Decision::Record);
    }

    #[test]
    fn delay_holds_until_it_ends() {
        let s = streamer(true, Some(5));
        let held = Decision::NotRecording(NotRecordingReason::IntroDelay);
        assert_eq!(decide(&s, Some(1000), 1000), held);
        assert_eq!(decide(&s, Some(1000), 1299), held);
        assert_eq!(decide(&s, Some(1000), 1300), Decision::Record);
    }

    #[test]
    fn delay_already_over_records_at_once() {
        assert_eq!(decide(&streamer(true, Some(5)), Some(0), 10_000), Decision::Record);
    }

    #[test]
    fn unknown_span_counts_the_delay_from_now() {
        let held = Decision::NotRecording(NotRecordingReason::IntroDelay);
        assert_eq!(decide(&streamer(true, Some(5)), None, 1000), held);
    }

    #[test]
    fn disabled_wins_over_delay() {
        let d = decide(&streamer(false, Some(5)), Some(0), 100);
        assert_eq!(d, Decision::NotRecording(NotRecordingReason::Disabled));
    }

    #[test]
    fn delay_end_is_none_without_a_delay() {
        assert_eq!(intro_delay_end(&StreamerOptions::default(), 100), None);
        let opts = StreamerOptions { intro_delay_enabled: true, intro_delay_minutes: 2, ..Default::default() };
        assert_eq!(intro_delay_end(&opts, 100), Some(220));
    }

    #[test]
    fn intro_split_records_straight_away() {
        let options = StreamerOptions {
            intro_delay_enabled: true,
            intro_delay_minutes: 5,
            intro_delay_split: Some(true),
            ..Default::default()
        };
        let st = Streamer { id: 1, site_id: 1, name: "n".into(), url: "u".into(), enabled: true, options, schedule_runs: Default::default() };
        assert_eq!(decide(&st, Some(0), 10), Decision::Record);
    }
}
