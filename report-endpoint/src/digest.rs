//! The reports that the service took in the last day.
//!
//! A sender that gets no answer sends the same report again, and the merge must not count its lines
//! twice. The sender puts a key in the request, and the key is the same for a second try of the same
//! lines. [`Counted`] holds a salted hash of the key and the report, so a key alone cannot stop a
//! report of other content. It keeps the hash in memory for one day, and it writes nothing.

use std::collections::HashMap;
use std::collections::hash_map::RandomState;
use std::hash::BuildHasher;
use std::time::{Duration, Instant};

use strata::closed_set::unknown::report::Report;

/// The most hashes that [`Counted`] holds. When the table is full, the oldest hash leaves.
const MOST_KEPT: usize = 10_000;

/// The reports taken in a span of time, as salted hashes.
#[derive(Debug)]
pub struct Counted {
    span: Duration,
    salt: RandomState,
    taken: HashMap<u64, Instant>,
}

impl Counted {
    /// A table that remembers a report for the span `span`.
    pub fn new(span: Duration) -> Counted {
        Counted {
            span,
            salt: RandomState::new(),
            taken: HashMap::new(),
        }
    }

    /// The hash of the key `key` and the report `report`. `None` when the report does not
    /// serialize: it always does, so a report with no hash is never taken for a repeat.
    pub fn digest(&self, key: &str, report: &Report) -> Option<u64> {
        let text = serde_json::to_vec(report).ok()?;
        Some(self.salt.hash_one((key, text)))
    }

    /// True when the service took the report with the hash `digest` in the last span.
    pub fn seen(&mut self, digest: u64, now: Instant) -> bool {
        let span = self.span;
        match self.taken.get(&digest) {
            Some(at) if now.duration_since(*at) < span => true,
            Some(_) => {
                self.taken.remove(&digest);
                false
            }
            None => false,
        }
    }

    /// Remember that the service took the report with the hash `digest` at the time `now`.
    pub fn note(&mut self, digest: u64, now: Instant) {
        if self.taken.len() >= MOST_KEPT && !self.taken.contains_key(&digest) {
            let span = self.span;
            self.taken.retain(|_, at| now.duration_since(*at) < span);
            if self.taken.len() >= MOST_KEPT {
                let oldest = self
                    .taken
                    .iter()
                    .min_by_key(|(_, at)| **at)
                    .map(|(d, _)| *d);
                if let Some(oldest) = oldest {
                    self.taken.remove(&oldest);
                }
            }
        }
        self.taken.insert(digest, now);
    }

    /// Forget the report with the hash `digest`, so that a second try of it counts. The merge calls
    /// this when the tracker failed to write the report.
    pub fn forget(&mut self, digest: u64) {
        self.taken.remove(&digest);
    }

    /// How many hashes the table holds.
    pub fn held(&self) -> usize {
        self.taken.len()
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use strata::closed_set::unknown::report::SCHEMA;

    use super::*;

    const DAY: Duration = Duration::from_secs(24 * 3600);

    fn report(value: &str) -> Report {
        serde_json::from_value(json!({"schema": SCHEMA, "unknown": [
            {"strata": "0.26.0", "set": "ProfileType", "value": value,
             "route": "/v1/packs", "count": 1}]}))
        .unwrap()
    }

    #[test]
    fn a_report_is_a_repeat_for_a_day_and_then_it_counts_again() {
        let mut counted = Counted::new(DAY);
        let now = Instant::now();
        let digest = counted.digest("k", &report("later")).unwrap();
        assert!(!counted.seen(digest, now));
        counted.note(digest, now);
        assert!(counted.seen(digest, now + DAY - Duration::from_secs(1)));
        assert!(!counted.seen(digest, now + DAY), "the day is over");
        assert_eq!(counted.held(), 0, "the old hash left");
    }

    #[test]
    fn the_key_and_the_content_both_decide_so_a_key_alone_stops_no_other_report() {
        let counted = Counted::new(DAY);
        let same = counted.digest("k", &report("later")).unwrap();
        assert_eq!(same, counted.digest("k", &report("later")).unwrap());
        assert_ne!(same, counted.digest("other-key", &report("later")).unwrap());
        assert_ne!(same, counted.digest("k", &report("other")).unwrap());
    }

    #[test]
    fn a_forgotten_report_counts_again() {
        let mut counted = Counted::new(DAY);
        let now = Instant::now();
        let digest = counted.digest("k", &report("later")).unwrap();
        counted.note(digest, now);
        counted.forget(digest);
        assert!(!counted.seen(digest, now));
    }

    #[test]
    fn a_full_table_drops_the_oldest_hash_and_keeps_the_new_one() {
        let mut counted = Counted::new(DAY);
        let start = Instant::now();
        for n in 0..MOST_KEPT as u64 {
            counted.note(n, start + Duration::from_secs(n));
        }
        assert_eq!(counted.held(), MOST_KEPT);
        let later = start + Duration::from_secs(MOST_KEPT as u64);
        counted.note(u64::MAX, later);
        assert_eq!(counted.held(), MOST_KEPT, "the table did not grow");
        assert!(counted.seen(u64::MAX, later));
        assert!(!counted.seen(0, later), "the oldest hash left");
        assert!(counted.seen(1, later));
    }

    #[test]
    fn the_table_holds_no_text_of_the_report() {
        let mut counted = Counted::new(DAY);
        let digest = counted.digest("k", &report("hunter2")).unwrap();
        counted.note(digest, Instant::now());
        let shown = format!("{counted:?}");
        assert!(!shown.contains("hunter2"), "{shown}");
    }
}
