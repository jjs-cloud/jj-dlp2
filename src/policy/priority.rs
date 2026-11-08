//! Decides which live streamers get a recording slot, from their priority order and the recording limit.

use crate::api::StreamerId;
use std::collections::HashSet;

/// A streamer in priority order, highest first, with its always-record flag.
pub type Ranked = (StreamerId, bool);

/// The streamers in `wanting` that may record: always-record ones, then the rest by priority up to `max` slots minus the always-record count.
pub fn admitted(order: &[Ranked], wanting: &HashSet<StreamerId>, max: Option<u32>) -> HashSet<StreamerId> {
    let always = order.iter().filter(|(id, always)| *always && wanting.contains(id)).count();
    let mut free = max.map(|m| (m as usize).saturating_sub(always));
    let mut out = HashSet::new();
    for (id, always) in order {
        if !wanting.contains(id) {
            continue;
        }
        if *always {
            out.insert(*id);
        } else if free.is_none_or(|n| n > 0) {
            out.insert(*id);
            if let Some(n) = free.as_mut() {
                *n -= 1;
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn set(ids: &[StreamerId]) -> HashSet<StreamerId> {
        ids.iter().copied().collect()
    }

    fn order(ids: &[(StreamerId, bool)]) -> Vec<Ranked> {
        ids.to_vec()
    }

    #[test]
    fn unlimited_admits_everyone() {
        let o = order(&[(1, false), (2, false), (3, false)]);
        assert_eq!(admitted(&o, &set(&[1, 2, 3]), None), set(&[1, 2, 3]));
    }

    #[test]
    fn limit_keeps_the_highest_priority() {
        let o = order(&[(1, false), (2, false), (3, false)]);
        assert_eq!(admitted(&o, &set(&[1, 2, 3]), Some(2)), set(&[1, 2]));
        assert_eq!(admitted(&o, &set(&[2, 3]), Some(2)), set(&[2, 3]));
        assert_eq!(admitted(&o, &set(&[3]), Some(1)), set(&[3]));
    }

    #[test]
    fn higher_priority_pushes_out_the_lowest() {
        let o = order(&[(1, false), (2, false), (3, false)]);
        assert_eq!(admitted(&o, &set(&[1, 3]), Some(1)), set(&[1]));
    }

    #[test]
    fn always_record_is_never_blocked_and_uses_slots() {
        let o = order(&[(1, true), (2, true), (3, false), (4, false)]);
        assert_eq!(admitted(&o, &set(&[1, 2, 3, 4]), Some(1)), set(&[1, 2]));
        assert_eq!(admitted(&o, &set(&[1, 3, 4]), Some(2)), set(&[1, 3]));
    }

    #[test]
    fn always_record_below_others_still_takes_its_slot_first() {
        let o = order(&[(1, false), (2, true)]);
        assert_eq!(admitted(&o, &set(&[1, 2]), Some(1)), set(&[2]));
    }

    #[test]
    fn zero_limit_admits_only_always_record() {
        let o = order(&[(1, false), (2, true)]);
        assert_eq!(admitted(&o, &set(&[1, 2]), Some(0)), set(&[2]));
    }
}
