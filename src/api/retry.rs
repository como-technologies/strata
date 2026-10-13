//! Another attempt at a GET that failed for a reason that passes.

use std::time::Duration;

use serde_json::Value;

use super::PaletteApi;
use crate::config::Config;
use crate::error::{Error, FailureClass, Result};
use crate::term::progress::Progress;

/// How many times to retry a failed GET and how long to wait between attempts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RetryPolicy {
    /// Attempts after the first. Zero means one attempt.
    pub retries: u32,
    /// Wait before the first retry; each later wait doubles.
    pub base_delay: Duration,
    /// The longest computed wait. A `Retry-After` header is honored as sent, up to
    /// `max_retry_after`.
    pub max_delay: Duration,
    /// The longest `Retry-After` honored. A longer one ends the request at once with
    /// [`Error::RetryAfterTooLong`]: the server said when to come back, and that is later than a
    /// job should sit waiting.
    pub max_retry_after: Duration,
}

impl RetryPolicy {
    /// `retries` retries, starting at one second and capped at thirty, honoring a `Retry-After` of
    /// up to five minutes.
    pub fn new(retries: u32) -> Self {
        RetryPolicy {
            retries,
            base_delay: Duration::from_secs(1),
            max_delay: Duration::from_secs(30),
            max_retry_after: Duration::from_secs(300),
        }
    }

    /// The retries and the `Retry-After` ceiling a [`Config`] asks for.
    pub fn from_config(config: &Config) -> Self {
        RetryPolicy {
            max_retry_after: config.max_retry_after,
            ..RetryPolicy::new(config.retries)
        }
    }

    /// The wait before retry number `attempt` (zero-based), or the server's `Retry-After`.
    pub fn delay(&self, attempt: u32, retry_after: Option<u64>) -> Duration {
        match retry_after {
            Some(secs) => Duration::from_secs(secs),
            None => self
                .base_delay
                .saturating_mul(2u32.saturating_pow(attempt))
                .min(self.max_delay),
        }
    }
}

/// Retries transient failures of any [`PaletteApi`]: HTTP 5xx, 429, and transport errors. A 5xx
/// that is a refusal of Palette ([`super::codes::REFUSAL_CODES`]) and other errors return at
/// once. [`Progress`] announces each retry.
pub struct Retry<A> {
    inner: A,
    policy: RetryPolicy,
    progress: Progress,
    sleep: Box<dyn Fn(Duration)>,
}

impl<A: PaletteApi> Retry<A> {
    /// Wrap `inner`, sleeping on the current thread between attempts.
    pub fn new(inner: A, policy: RetryPolicy, progress: Progress) -> Self {
        Self::with_sleep(inner, policy, progress, std::thread::sleep)
    }

    /// Wrap `inner` with a caller-supplied wait, so tests can record delays instead of sleeping.
    pub fn with_sleep(
        inner: A,
        policy: RetryPolicy,
        progress: Progress,
        sleep: impl Fn(Duration) + 'static,
    ) -> Self {
        Retry {
            inner,
            policy,
            progress,
            sleep: Box::new(sleep),
        }
    }

    /// The wrapped API.
    pub fn inner(&self) -> &A {
        &self.inner
    }
}

/// What `Retry` tries again. The rule is [`Error::class`]. `RetryAfterTooLong` is transient too,
/// and it is how `Retry` itself says "not now", so `Retry` does not try that one again.
fn is_transient(e: &Error) -> bool {
    e.class() == FailureClass::Transient && !matches!(e, Error::RetryAfterTooLong { .. })
}

impl<A: PaletteApi> Retry<A> {
    /// Run `call` until it succeeds, fails for a reason that does not pass, or runs out of
    /// retries. A GET and a POST that changes nothing retry the same way.
    fn attempts(&self, path: &str, call: impl Fn() -> Result<Value>) -> Result<Value> {
        let mut attempt = 0;
        loop {
            match call() {
                Ok(v) => return Ok(v),
                Err(e) if attempt < self.policy.retries && is_transient(&e) => {
                    let retry_after = match &e {
                        Error::Api { retry_after, .. } => *retry_after,
                        _ => None,
                    };
                    let limit = self.policy.max_retry_after.as_secs();
                    if let (Some(secs), Error::Api { method, status, .. }) = (retry_after, &e)
                        && secs > limit
                    {
                        return Err(Error::RetryAfterTooLong {
                            method: *method,
                            path: path.to_string(),
                            status: *status,
                            retry_after: secs,
                            limit,
                        });
                    }
                    let delay = self.policy.delay(attempt, retry_after);
                    attempt += 1;
                    self.progress.note(&format!(
                        "{e}; retry {attempt} of {} in {delay:?}",
                        self.policy.retries
                    ));
                    (self.sleep)(delay);
                }
                Err(e) => return Err(e),
            }
        }
    }
}

impl<A: PaletteApi> PaletteApi for Retry<A> {
    fn get_json(&self, path: &str) -> Result<Value> {
        self.attempts(path, || self.inner.get_json(path))
    }

    fn post_json(&self, path: &str, body: &Value) -> Result<Value> {
        self.attempts(path, || self.inner.post_json(path, body))
    }

    fn project_uid(&self) -> Option<&str> {
        self.inner.project_uid()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retry_policy_backs_off_exponentially_up_to_the_maximum() {
        let p = RetryPolicy::new(5);
        assert_eq!(p.delay(0, None), Duration::from_secs(1));
        assert_eq!(p.delay(1, None), Duration::from_secs(2));
        assert_eq!(p.delay(2, None), Duration::from_secs(4));
        assert_eq!(p.delay(9, None), p.max_delay);
        assert_eq!(p.delay(0, Some(7)), Duration::from_secs(7));
    }
}
