//! Decides when a streamer's schedule switches it on or off.

use crate::api::{Schedule, ScheduleMode, ScheduleRuns};
use chrono::{Datelike, Days, NaiveDateTime, NaiveTime};

/// What a schedule wants done to its streamer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Edge {
    Enable,
    Disable,
}

/// The edge to apply and which run stamps it settles.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Due {
    pub edge: Edge,
    pub stamp_enable: bool,
    pub stamp_disable: bool,
}

/// The most recent start and end that have already passed at `now`.
fn passed_edges(s: &Schedule, now: NaiveDateTime) -> (Option<NaiveDateTime>, Option<NaiveDateTime>) {
    match s.mode {
        ScheduleMode::OneOff => {
            let (Some(start), Some(end)) = (s.one_off_start, s.one_off_end) else {
                return (None, None);
            };
            ((now >= start).then_some(start), (now >= end).then_some(end))
        }
        ScheduleMode::Recurring => {
            let (Some(start), Some(end)) = (s.start_time, s.end_time) else {
                return (None, None);
            };
            if !s.days.contains(&true) {
                return (None, None);
            }
            let overnight = end < start;
            (latest(now, &s.days, start, false), latest(now, &s.days, end, overnight))
        }
    }
}

/// Latest occurrence of `at` not after `now` in the past 14 days; an overnight end belongs to the day after its selected weekday.
fn latest(now: NaiveDateTime, days: &[bool; 7], at: NaiveTime, next_day: bool) -> Option<NaiveDateTime> {
    (0..14u64).find_map(|back| {
        let date = now.date().checked_sub_days(Days::new(back))?;
        let weekday_of = if next_day { date.pred_opt()? } else { date };
        if !days[weekday_of.weekday().num_days_from_monday() as usize] {
            return None;
        }
        let candidate = date.and_time(at);
        (candidate <= now).then_some(candidate)
    })
}

/// The action a schedule calls for at `now`; when a start and an end are both pending only the later one applies.
pub fn due(s: &Schedule, runs: &ScheduleRuns, now: NaiveDateTime) -> Option<Due> {
    if !s.enabled {
        return None;
    }
    let (start, end) = passed_edges(s, now);
    let start = start.filter(|t| runs.last_enable.is_none_or(|last| last < *t));
    let end = end.filter(|t| runs.last_disable.is_none_or(|last| last < *t));
    let edge = match (start, end) {
        (None, None) => return None,
        (Some(_), None) => Edge::Enable,
        (None, Some(_)) => Edge::Disable,
        (Some(st), Some(en)) => {
            if st > en {
                Edge::Enable
            } else {
                Edge::Disable
            }
        }
    };
    Some(Due { edge, stamp_enable: start.is_some(), stamp_disable: end.is_some() })
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    fn dt(y: i32, mo: u32, d: u32, h: u32, mi: u32) -> NaiveDateTime {
        NaiveDate::from_ymd_opt(y, mo, d).unwrap().and_hms_opt(h, mi, 0).unwrap()
    }

    fn tm(h: u32, mi: u32) -> NaiveTime {
        NaiveTime::from_hms_opt(h, mi, 0).unwrap()
    }

    fn one_off(start: NaiveDateTime, end: NaiveDateTime) -> Schedule {
        Schedule { enabled: true, one_off_start: Some(start), one_off_end: Some(end), ..Default::default() }
    }

    fn recurring(days: &[usize], start: NaiveTime, end: NaiveTime) -> Schedule {
        let mut flags = [false; 7];
        for d in days {
            flags[*d] = true;
        }
        Schedule {
            enabled: true,
            mode: ScheduleMode::Recurring,
            days: flags,
            start_time: Some(start),
            end_time: Some(end),
            ..Default::default()
        }
    }

    fn enable(stamp_enable: bool, stamp_disable: bool) -> Option<Due> {
        Some(Due { edge: Edge::Enable, stamp_enable, stamp_disable })
    }

    fn disable(stamp_enable: bool, stamp_disable: bool) -> Option<Due> {
        Some(Due { edge: Edge::Disable, stamp_enable, stamp_disable })
    }

    #[test]
    fn off_schedule_does_nothing() {
        let mut s = one_off(dt(2026, 10, 28, 20, 0), dt(2026, 10, 28, 23, 0));
        s.enabled = false;
        assert_eq!(due(&s, &ScheduleRuns::default(), dt(2026, 10, 28, 21, 0)), None);
    }

    #[test]
    fn one_off_waits_for_start() {
        let s = one_off(dt(2026, 10, 28, 20, 0), dt(2026, 10, 28, 23, 0));
        assert_eq!(due(&s, &ScheduleRuns::default(), dt(2026, 10, 28, 19, 59)), None);
    }

    #[test]
    fn one_off_enables_once_inside_the_window() {
        let s = one_off(dt(2026, 10, 28, 20, 0), dt(2026, 10, 28, 23, 0));
        let now = dt(2026, 10, 28, 21, 0);
        assert_eq!(due(&s, &ScheduleRuns::default(), now), enable(true, false));
        let runs = ScheduleRuns { last_enable: Some(now), last_disable: None };
        assert_eq!(due(&s, &runs, now), None);
    }

    #[test]
    fn one_off_disables_after_the_end() {
        let s = one_off(dt(2026, 10, 28, 20, 0), dt(2026, 10, 28, 23, 0));
        let runs = ScheduleRuns { last_enable: Some(dt(2026, 10, 28, 20, 1)), last_disable: None };
        assert_eq!(due(&s, &runs, dt(2026, 10, 28, 23, 0)), disable(false, true));
    }

    #[test]
    fn one_off_already_over_only_disables() {
        let s = one_off(dt(2026, 10, 28, 20, 0), dt(2026, 10, 28, 23, 0));
        assert_eq!(due(&s, &ScheduleRuns::default(), dt(2026, 10, 29, 9, 0)), disable(true, true));
    }

    #[test]
    fn one_off_needs_both_times() {
        let mut s = one_off(dt(2026, 10, 28, 20, 0), dt(2026, 10, 28, 23, 0));
        s.one_off_end = None;
        assert_eq!(due(&s, &ScheduleRuns::default(), dt(2026, 10, 29, 9, 0)), None);
    }

    #[test]
    fn recurring_first_run_inside_window_enables_only() {
        // 2026-10-28 is a Wednesday.
        let s = recurring(&[0, 1, 2, 3, 4], tm(20, 0), tm(23, 0));
        assert_eq!(due(&s, &ScheduleRuns::default(), dt(2026, 10, 28, 21, 0)), enable(true, true));
    }

    #[test]
    fn recurring_first_run_outside_window_disables_only() {
        let s = recurring(&[0, 1, 2, 3, 4], tm(20, 0), tm(23, 0));
        assert_eq!(due(&s, &ScheduleRuns::default(), dt(2026, 10, 28, 23, 30)), disable(true, true));
    }

    #[test]
    fn recurring_disables_when_the_window_closes() {
        let s = recurring(&[2], tm(20, 0), tm(23, 0));
        let runs = ScheduleRuns { last_enable: Some(dt(2026, 10, 28, 20, 5)), last_disable: Some(dt(2026, 10, 21, 23, 1)) };
        assert_eq!(due(&s, &runs, dt(2026, 10, 28, 22, 59)), None);
        assert_eq!(due(&s, &runs, dt(2026, 10, 28, 23, 0)), disable(false, true));
    }

    #[test]
    fn recurring_enables_on_the_next_selected_day() {
        let s = recurring(&[2], tm(20, 0), tm(23, 0));
        let runs = ScheduleRuns { last_enable: Some(dt(2026, 10, 28, 20, 5)), last_disable: Some(dt(2026, 10, 28, 23, 1)) };
        assert_eq!(due(&s, &runs, dt(2026, 10, 29, 12, 0)), None);
        assert_eq!(due(&s, &runs, dt(2026, 11, 4, 20, 0)), enable(true, false));
    }

    #[test]
    fn overnight_window_ends_on_the_day_after() {
        // Friday 22:00 to Saturday 02:00; 2026-10-30 is a Friday.
        let s = recurring(&[4], tm(22, 0), tm(2, 0));
        assert_eq!(due(&s, &ScheduleRuns::default(), dt(2026, 10, 31, 1, 0)), enable(true, true));
        let runs = ScheduleRuns { last_enable: Some(dt(2026, 10, 30, 22, 5)), last_disable: Some(dt(2026, 10, 24, 2, 1)) };
        assert_eq!(due(&s, &runs, dt(2026, 10, 31, 1, 59)), None);
        assert_eq!(due(&s, &runs, dt(2026, 10, 31, 2, 0)), disable(false, true));
    }

    #[test]
    fn recurring_needs_days_and_times() {
        let mut s = recurring(&[], tm(20, 0), tm(23, 0));
        assert_eq!(due(&s, &ScheduleRuns::default(), dt(2026, 10, 28, 21, 0)), None);
        s = recurring(&[2], tm(20, 0), tm(23, 0));
        s.end_time = None;
        assert_eq!(due(&s, &ScheduleRuns::default(), dt(2026, 10, 28, 21, 0)), None);
    }
}
