//! The limit of the rate for each sender, and for the whole service.
//!
//! The endpoint keeps no address. A [`Limiter`] holds a hash of the address, with a salt that the
//! process makes by itself when it starts. The hash is in memory only: it is never written or
//! logged, and a new process cannot read the hashes of the old one. So the limiter can tell that
//! one sender comes again, and nobody can read who the sender is.

use std::collections::hash_map::RandomState;
use std::collections::{HashMap, VecDeque};
use std::hash::BuildHasher;
use std::time::{Duration, Instant};

/// The most senders that a limiter holds. A flood from more addresses than this is refused until
/// the old requests expire.
const MOST_SENDERS: usize = 10_000;

/// A limit of the requests in a span of time, for each sender.
#[derive(Debug)]
pub struct Limiter {
    limit: usize,
    span: Duration,
    salt: RandomState,
    seen: HashMap<u64, VecDeque<Instant>>,
}

impl Limiter {
    /// A limiter that lets a sender spend `limit` in each span of time `span`.
    pub fn new(limit: usize, span: Duration) -> Limiter {
        Limiter {
            limit,
            span,
            salt: RandomState::new(),
            seen: HashMap::new(),
        }
    }

    /// Spend `cost` for the sender `sender` at the time `now`.
    ///
    /// # Errors
    ///
    /// The time to wait until the sender can spend `cost`, when it has spent too much in the last
    /// span. Nothing is spent then.
    pub fn allow(&mut self, sender: &str, cost: usize, now: Instant) -> Result<(), Duration> {
        let key = self.salt.hash_one(sender);
        if self.seen.len() >= MOST_SENDERS && !self.seen.contains_key(&key) {
            self.forget_expired(now);
            if self.seen.len() >= MOST_SENDERS {
                return Err(self.span);
            }
        }
        let span = self.span;
        if cost > self.limit {
            return Err(span);
        }
        let spent = self.seen.entry(key).or_default();
        while spent
            .front()
            .is_some_and(|at| now.duration_since(*at) >= span)
        {
            spent.pop_front();
        }
        if spent.len() + cost > self.limit {
            // the sender can spend again when enough old requests have expired
            let free_at = spent
                .get(spent.len() + cost - self.limit - 1)
                .copied()
                .unwrap_or(now);
            return Err((free_at + span).saturating_duration_since(now));
        }
        spent.extend(std::iter::repeat_n(now, cost));
        Ok(())
    }

    /// Drop each sender whose requests are all older than the span.
    fn forget_expired(&mut self, now: Instant) {
        let span = self.span;
        self.seen.retain(|_, spent| {
            spent
                .back()
                .is_some_and(|at| now.duration_since(*at) < span)
        });
    }

    /// How many senders the limiter holds.
    pub fn senders(&self) -> usize {
        self.seen.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const HOUR: Duration = Duration::from_secs(3600);

    #[test]
    fn a_sender_spends_its_limit_and_then_waits_until_the_oldest_request_expires() {
        let mut limiter = Limiter::new(2, HOUR);
        let start = Instant::now();
        assert!(limiter.allow("a", 1, start).is_ok());
        assert!(
            limiter
                .allow("a", 1, start + Duration::from_secs(60))
                .is_ok()
        );
        let wait = limiter
            .allow("a", 1, start + Duration::from_secs(120))
            .unwrap_err();
        assert_eq!(wait, HOUR - Duration::from_secs(120));
        assert!(
            limiter.allow("a", 1, start + HOUR).is_ok(),
            "the first request expired"
        );
    }

    #[test]
    fn senders_do_not_share_a_limit_and_a_refused_request_spends_nothing() {
        let mut limiter = Limiter::new(1, HOUR);
        let now = Instant::now();
        assert!(limiter.allow("a", 1, now).is_ok());
        assert!(limiter.allow("b", 1, now).is_ok());
        assert!(limiter.allow("a", 1, now).is_err());
        assert!(limiter.allow("a", 1, now + HOUR).is_ok());
    }

    #[test]
    fn a_cost_above_the_limit_is_refused_and_a_cost_spends_that_many_requests() {
        let mut limiter = Limiter::new(3, HOUR);
        let now = Instant::now();
        assert!(limiter.allow("all", 4, now).is_err());
        assert!(limiter.allow("all", 2, now).is_ok());
        assert!(
            limiter.allow("all", 2, now).is_err(),
            "only one request is left"
        );
        assert!(limiter.allow("all", 1, now).is_ok());
    }

    #[test]
    fn the_limiter_holds_no_address_and_a_new_limiter_hashes_it_in_another_way() {
        let mut one = Limiter::new(1, HOUR);
        let mut two = Limiter::new(1, HOUR);
        let now = Instant::now();
        one.allow("203.0.113.7", 1, now).unwrap();
        two.allow("203.0.113.7", 1, now).unwrap();
        let held = |l: &Limiter| *l.seen.keys().next().unwrap();
        assert_ne!(held(&one), held(&two), "the salt differs for each process");
        let shown = format!("{one:?}");
        assert!(!shown.contains("203.0.113.7"), "{shown}");
    }

    #[test]
    fn a_flood_of_senders_is_refused_when_none_has_expired_and_accepted_when_the_old_ones_expire() {
        let mut limiter = Limiter::new(1, HOUR);
        let now = Instant::now();
        for n in 0..MOST_SENDERS {
            limiter.allow(&format!("sender-{n}"), 1, now).unwrap();
        }
        assert_eq!(limiter.senders(), MOST_SENDERS);
        assert!(limiter.allow("one-more", 1, now).is_err());
        assert!(limiter.allow("one-more", 1, now + HOUR).is_ok());
        assert_eq!(limiter.senders(), 1, "the expired senders are gone");
    }
}
