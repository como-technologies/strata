//! The limit of the rate for each sender, and for the whole service.
//!
//! The endpoint keeps no address. A [`Limiter`] holds a hash of the address, with a salt that the
//! process makes by itself when it starts. The hash is in memory only: it is never written or
//! logged, and a new process cannot read the hashes of the old one. So the limiter can tell that
//! one sender comes again, and nobody can read who the sender is.

use std::collections::hash_map::RandomState;
use std::collections::{HashMap, VecDeque};
use std::hash::BuildHasher;
use std::net::IpAddr;
use std::time::{Duration, Instant};

/// The most senders that a limiter holds. A new sender of a full table takes the place of the oldest
/// sender. So a flood from many addresses cannot lock out a new sender.
pub(crate) const MOST_SENDERS: usize = 10_000;

/// Why a [`Limiter`] refused to spend.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Denied {
    /// The sender spent the limit in the last span. A try after this time can work.
    Wait(Duration),
    /// The cost is above the limit itself, so no wait helps.
    Over,
}

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
    /// [`Denied::Wait`] with the time until the sender can spend `cost`, when it has spent too much
    /// in the last span. [`Denied::Over`] when `cost` is above the limit, because no wait helps
    /// then. Nothing is spent in either case.
    pub fn allow(&mut self, sender: &str, cost: usize, now: Instant) -> Result<(), Denied> {
        if cost > self.limit {
            return Err(Denied::Over);
        }
        let wait = self.wait(sender, cost, now);
        if !wait.is_zero() {
            return Err(Denied::Wait(wait));
        }
        self.spend(sender, cost, now);
        Ok(())
    }

    /// How much the sender `sender` can spend at the time `now`.
    pub fn room(&mut self, sender: &str, now: Instant) -> usize {
        let key = self.salt.hash_one(sender);
        let span = self.span;
        let limit = self.limit;
        self.seen.get_mut(&key).map_or(limit, |spent| {
            expire(spent, span, now);
            limit.saturating_sub(spent.len())
        })
    }

    /// The time until the sender `sender` can spend `cost`, or zero when it can now. A `cost`
    /// above the limit gives the whole span: it can never be spent.
    pub fn wait(&mut self, sender: &str, cost: usize, now: Instant) -> Duration {
        let key = self.salt.hash_one(sender);
        let (limit, span) = (self.limit, self.span);
        let Some(spent) = self.seen.get_mut(&key) else {
            return if cost > limit { span } else { Duration::ZERO };
        };
        expire(spent, span, now);
        if spent.len() + cost <= limit {
            return Duration::ZERO;
        }
        // the sender can spend again when enough old requests have expired
        let free_at = spent
            .get((spent.len() + cost).saturating_sub(limit + 1))
            .copied()
            .unwrap_or(now);
        (free_at + span).saturating_duration_since(now)
    }

    /// Spend `cost` for the sender `sender` at the time `now`, with no check of the limit. The
    /// caller made sure that [`room`](Limiter::room) holds `cost`. A new sender of a full table
    /// takes the place of the sender that spent last the longest time ago.
    pub fn spend(&mut self, sender: &str, cost: usize, now: Instant) {
        if cost == 0 {
            return;
        }
        let key = self.salt.hash_one(sender);
        if self.seen.len() >= MOST_SENDERS && !self.seen.contains_key(&key) {
            self.forget_expired(now);
            if self.seen.len() >= MOST_SENDERS {
                self.forget_oldest();
            }
        }
        self.seen
            .entry(key)
            .or_default()
            .extend(std::iter::repeat_n(now, cost));
    }

    /// Drop each sender whose requests are all older than the span at the time `now`. The service
    /// calls it one time in a minute at most, so a sender that does not come again leaves the table.
    pub fn forget_expired(&mut self, now: Instant) {
        let span = self.span;
        self.seen.retain(|_, spent| {
            spent
                .back()
                .is_some_and(|at| now.duration_since(*at) < span)
        });
    }

    /// Drop the sender whose last request is the oldest.
    fn forget_oldest(&mut self) {
        let oldest = self
            .seen
            .iter()
            .min_by_key(|(_, spent)| spent.back().copied())
            .map(|(key, _)| *key);
        if let Some(key) = oldest {
            self.seen.remove(&key);
        }
    }

    /// How many senders the limiter holds.
    pub fn senders(&self) -> usize {
        self.seen.len()
    }
}

/// Drop the requests in `spent` that are `span` old or older at the time `now`.
fn expire(spent: &mut VecDeque<Instant>, span: Duration, now: Instant) {
    while spent
        .front()
        .is_some_and(|at| now.duration_since(*at) >= span)
    {
        spent.pop_front();
    }
}

/// The sender that the limit counts for the address `address`, as text.
///
/// A machine with an IPv6 address holds at least the 64-bit network of the address, and so it can
/// send from 2^64 addresses. So the limit counts an IPv6 address by its /64 network. An IPv4
/// address, an IPv4 address in the form of IPv6, and a text that is no address stay as they are.
pub fn network_of(address: &str) -> String {
    match address.parse::<IpAddr>() {
        Ok(IpAddr::V6(v6)) => match v6.to_ipv4_mapped() {
            Some(v4) => v4.to_string(),
            None => {
                let [a, b, c, d, ..] = v6.segments();
                format!("{a:x}:{b:x}:{c:x}:{d:x}::/64")
            }
        },
        _ => address.to_string(),
    }
}

#[cfg(test)]
mod tests;
