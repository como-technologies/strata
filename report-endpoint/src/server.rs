//! The service: the checks of a request in their order, and the loop that answers requests.
//!
//! [`Service::admit`] holds the rules, and it knows nothing of HTTP. [`serve`] reads each request
//! from a `tiny_http` server, calls it, and writes the answer. The service makes no log of a
//! request: it keeps no address, and the report is not for a log.
//!
//! The rules run under two locks. The `rules` lock guards the limits and the reports taken. It
//! guards memory only. No call to the tracker runs under it, so a request that a check refuses
//! never waits for a merge. The `tracker` lock serves one merge at a time. A merge reads the
//! issues, answers the sender, and then writes. So the sender gets its answer before the slow part,
//! and it does not leave and send the same report again.

use std::io::Read;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use strata::closed_set::unknown::report::MOST_VALUES;
use strata::closed_set::unknown::report::Report;
use strata::closed_set::unknown::send::KEY_HEADER;

use crate::accept::{self, MOST_BYTES, Refusal};
use crate::digest::Counted;
use crate::limit::{Denied, Limiter, network_of};
use crate::merge::{self, Plan};
use crate::tracker::{Tracker, TrackerError};

/// The requests that one sender makes in an hour, at most.
pub const REQUESTS_PER_HOUR: usize = 10;

/// The issues that the service opens in an hour, at most, for all senders.
///
/// A flood of made-up values opens this many issues. A report of strata has [`MOST_VALUES`] values
/// or fewer, so an hour that no one used takes a whole report.
pub const NEW_ISSUES_PER_HOUR: usize = MOST_VALUES;

/// The writes to the tracker in an hour, at most, for all senders: new issues and changes.
///
/// GitHub limits the writes of one token, and a flood of reports of known values stays under that
/// limit with this one.
pub const WRITES_PER_HOUR: usize = 200;

/// The requests that the service handles at once, at most. A merge goes on after its answer, so
/// the limit of Cloud Run does not bound the threads: this limit does. A request over it gets 503.
pub const MOST_AT_ONCE: usize = 32;

/// The span of the three limits.
const HOUR: Duration = Duration::from_secs(3600);

/// The time that the service remembers a report: see [`Counted`].
const DAY: Duration = Duration::from_secs(24 * 3600);

/// The time between two sweeps of the expired hashes. A hash stays one minute after its span at most,
/// while requests come: the first request after the sweep is due drops it.
const SWEEP_EVERY: Duration = Duration::from_secs(60);

/// The seconds that a sender waits after a 503.
const BUSY_WAIT: u64 = 60;

/// The key of the limits for all senders.
const EVERYONE: &str = "all";

/// A request, as the rules of the service read it.
#[derive(Debug, Clone, Copy)]
pub struct Request<'a> {
    /// The method, such as `POST`.
    pub method: &'a str,
    /// The path, without a query.
    pub path: &'a str,
    /// The content type of the body, when the request has one.
    pub content_type: Option<&'a str>,
    /// The value of the header `Idempotency-Key`, when the request has one: see [`accept::key`].
    pub key: Option<&'a str>,
    /// The address of the sender, as text. The service hashes it and keeps no text. An IPv6 address
    /// counts by its /64 network: see [`network_of`].
    pub sender: &'a str,
    /// The body, read up to one byte over [`MOST_BYTES`].
    pub body: &'a [u8],
}

/// The answer of the service.
#[derive(Debug, Clone, PartialEq)]
pub struct Answer {
    /// The HTTP status.
    pub status: u16,
    /// The JSON body: `{"ok":true,...}` or `{"error":"..."}`.
    pub body: Value,
    /// The seconds to wait before the next request, for a 429 or a 503.
    pub retry_after: Option<u64>,
}

impl Answer {
    fn error(status: u16, message: &str) -> Answer {
        Answer {
            status,
            body: json!({"error": message}),
            retry_after: None,
        }
    }

    fn wait(wait: Duration, message: &str) -> Answer {
        Answer {
            retry_after: Some(wait.as_secs().max(1)),
            ..Answer::error(429, message)
        }
    }

    /// The answer for a limit that refused: a wait, or a limit that no wait can serve.
    fn denied(denied: Denied, message: &str) -> Answer {
        match denied {
            Denied::Wait(wait) => Answer::wait(wait, message),
            Denied::Over => Answer::error(503, "the endpoint takes no request now"),
        }
    }

    fn busy() -> Answer {
        Answer {
            retry_after: Some(BUSY_WAIT),
            ..Answer::error(503, "the endpoint is busy: try again later")
        }
    }

    fn duplicate() -> Answer {
        Answer {
            status: 200,
            body: json!({"ok": true, "duplicate": true}),
            retry_after: None,
        }
    }

    /// The answer for a report that the service takes: the merge goes on after this answer.
    /// `waiting` holds the numbers of the entries whose values the limits hold back.
    fn taken(plan: &Plan, waiting: &[usize]) -> Answer {
        let mut body = json!({"ok": true, "opens": plan.new_issues(), "updates": plan.updates()});
        if !waiting.is_empty() {
            body["waiting"] = json!(waiting);
        }
        Answer {
            status: 202,
            body,
            retry_after: None,
        }
    }
}

/// The limits of the service, and the reports that it took.
#[derive(Debug)]
struct Rules {
    senders: Limiter,
    new_issues: Limiter,
    writes: Limiter,
    counted: Counted,
    /// The time of the last sweep of the expired hashes.
    swept: Option<Instant>,
}

impl Rules {
    /// Drop each hash of a sender and of a report whose span is over, at most one time in each
    /// [`SWEEP_EVERY`]. So a hash that no request reads leaves the table, and a request does not
    /// read the whole table.
    fn sweep(&mut self, now: Instant) {
        if self
            .swept
            .is_some_and(|at| now.saturating_duration_since(at) < SWEEP_EVERY)
        {
            return;
        }
        self.senders.forget_expired(now);
        self.counted.forget_expired(now);
        self.swept = Some(now);
    }
}

/// The limits of a service, in requests or writes for each hour.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limits {
    /// The requests of one sender: [`REQUESTS_PER_HOUR`].
    pub per_sender: usize,
    /// The new issues, for all senders: [`NEW_ISSUES_PER_HOUR`].
    pub new_issues: usize,
    /// The writes, for all senders: [`WRITES_PER_HOUR`].
    pub writes: usize,
}

impl Default for Limits {
    fn default() -> Limits {
        Limits {
            per_sender: REQUESTS_PER_HOUR,
            new_issues: NEW_ISSUES_PER_HOUR,
            writes: WRITES_PER_HOUR,
        }
    }
}

/// The rules of the service, and the tracker that they write to.
#[derive(Debug)]
pub struct Service<T: Tracker> {
    rules: Mutex<Rules>,
    tracker: Mutex<T>,
}

/// A request that passed the checks that need no tracker.
struct Checked {
    report: Report,
    /// The hash of the key and the report, when the request has a key.
    digest: Option<u64>,
}

/// The answer to write now, and the merge that goes on after it.
#[derive(Debug)]
pub struct Admitted<'a, T: Tracker> {
    /// The answer to write to the sender.
    pub answer: Answer,
    later: Option<Later<'a, T>>,
}

/// The write of a report that the service took. It holds the `tracker` lock until it ends.
#[derive(Debug)]
struct Later<'a, T: Tracker> {
    service: &'a Service<T>,
    tracker: MutexGuard<'a, T>,
    plan: Plan,
    digest: Option<u64>,
}

impl<T: Tracker> Admitted<'_, T> {
    fn answered(answer: Answer) -> Self {
        Admitted {
            answer,
            later: None,
        }
    }

    /// Write the report to the tracker. Call it after the answer went to the sender. A request that
    /// the service refused has nothing to write.
    ///
    /// When the tracker fails, the report is lost to the maintainers. The service forgets that it
    /// took the report, so a second try of it counts.
    pub fn finish(self) {
        let Some(Later {
            service,
            mut tracker,
            plan,
            digest,
        }) = self.later
        else {
            return;
        };
        if let Err(error) = merge::apply(&mut *tracker, plan) {
            log_failure(&error);
            if let Some(digest) = digest {
                service.rules().counted.forget(digest);
            }
        }
    }
}

impl<T: Tracker> Service<T> {
    /// A service with the limits [`REQUESTS_PER_HOUR`], [`NEW_ISSUES_PER_HOUR`], and
    /// [`WRITES_PER_HOUR`].
    pub fn new(tracker: T) -> Service<T> {
        Service::with_limits(tracker, Limits::default())
    }

    /// A service with the limits `limits`.
    pub fn with_limits(tracker: T, limits: Limits) -> Service<T> {
        Service {
            rules: Mutex::new(Rules {
                senders: Limiter::new(limits.per_sender, HOUR),
                new_issues: Limiter::new(limits.new_issues, HOUR),
                writes: Limiter::new(limits.writes, HOUR),
                counted: Counted::new(DAY),
                swept: None,
            }),
            tracker: Mutex::new(tracker),
        }
    }

    /// The `rules` lock. A thread that panicked cannot hurt the next request: the rules hold no
    /// half state.
    fn rules(&self) -> MutexGuard<'_, Rules> {
        self.rules.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Answer the request `request` at the time `now`, and hold the write of the report for
    /// [`Admitted::finish`].
    ///
    /// The checks run in this order:
    ///
    /// 1. The path and the method.
    /// 2. The key and the body.
    /// 3. The limit of the sender.
    /// 4. A report that the service took before.
    /// 5. The limits of the new issues and of the writes.
    ///
    /// The reads of the tracker come before the last two checks, and the writes come after the
    /// answer. So a request that fails a check changes nothing.
    pub fn admit(&self, request: &Request, now: Instant) -> Admitted<'_, T> {
        let checked = match self.check(request, now) {
            Ok(checked) => checked,
            Err(answer) => return Admitted::answered(answer),
        };
        // a second try of a send that a merge still writes gets its answer at once
        if self.took(checked.digest, now) {
            return Admitted::answered(Answer::duplicate());
        }
        let mut tracker = self.tracker.lock().unwrap_or_else(PoisonError::into_inner);
        // a merge that ran while this request waited for the lock can have taken the report
        if self.took(checked.digest, now) {
            return Admitted::answered(Answer::duplicate());
        }
        let mut plan = match merge::plan(&mut *tracker, &checked.report) {
            Ok(plan) => plan,
            Err(error) => return Admitted::answered(Self::tracker_failed(&error)),
        };
        let waiting = match self.share_out(&mut plan, now) {
            Ok(waiting) => waiting,
            Err(answer) => return Admitted::answered(answer),
        };
        if let Some(digest) = checked.digest {
            self.rules().counted.note(digest, now);
        }
        Admitted {
            answer: Answer::taken(&plan, &waiting),
            later: Some(Later {
                service: self,
                tracker,
                plan,
                digest: checked.digest,
            }),
        }
    }

    /// True when the service took the report with the hash `digest` in the last day.
    fn took(&self, digest: Option<u64>, now: Instant) -> bool {
        digest.is_some_and(|digest| self.rules().counted.seen(digest, now))
    }

    /// Wait until no merge goes on. A test calls it after an answer of 202, to read the tracker.
    pub fn wait_for_merge(&self) {
        drop(self.tracker.lock().unwrap_or_else(PoisonError::into_inner));
    }

    /// Answer the request `request` at the time `now`, with the write done. The tests use it. The
    /// server answers first, and writes after: see [`Service::admit`].
    pub fn handle(&self, request: &Request, now: Instant) -> Answer {
        let admitted = self.admit(request, now);
        let answer = admitted.answer.clone();
        admitted.finish();
        answer
    }

    /// The checks that need no tracker: the path, the method, the key, the body, and the limit of
    /// the sender. They run under the `rules` lock alone, and for a short time.
    ///
    /// The sweep of the expired hashes runs first, so a request that a check refuses also drops
    /// them.
    fn check(&self, request: &Request, now: Instant) -> Result<Checked, Answer> {
        self.rules().sweep(now);
        if request.path != "/" {
            return Err(Answer::error(404, "the endpoint takes a report at /"));
        }
        if request.method != "POST" {
            return Err(Answer::error(405, "the endpoint takes POST"));
        }
        let refused = |refusal: Refusal| Answer::error(refusal.status, &refusal.message);
        let key = accept::key(request.key).map_err(refused)?;
        let report = accept::parse(request.content_type, request.body).map_err(refused)?;
        let mut rules = self.rules();
        // the limit comes after the check of the body: a request that fails a check spends nothing
        rules
            .senders
            .allow(&network_of(request.sender), 1, now)
            .map_err(|denied| {
                Answer::denied(
                    denied,
                    "this sender made too many requests: try again later",
                )
            })?;
        let digest = key.and_then(|key| rules.counted.digest(key, &report));
        Ok(Checked { report, digest })
    }

    /// Keep the steps of `plan` that the limits of the new issues and of the writes take, and spend
    /// them. Gives the numbers of the entries that wait.
    ///
    /// # Errors
    ///
    /// The answer 429 when the limits take no step: it names the time until one step fits.
    fn share_out(&self, plan: &mut Plan, now: Instant) -> Result<Vec<usize>, Answer> {
        let mut rules = self.rules();
        let mut new_room = rules.new_issues.room(EVERYONE, now);
        let write_room_at_start = rules.writes.room(EVERYONE, now);
        let mut write_room = write_room_at_start;
        let waiting = plan.keep(|opens| {
            if write_room == 0 || (opens && new_room == 0) {
                return false;
            }
            write_room -= 1;
            if opens {
                new_room -= 1;
            }
            true
        });
        if plan.is_empty() {
            return Err(if write_room_at_start == 0 {
                let wait = rules.writes.wait(EVERYONE, 1, now);
                Answer::wait(wait, "the endpoint writes no more now: try again later")
            } else {
                let wait = rules.new_issues.wait(EVERYONE, 1, now);
                Answer::wait(
                    wait,
                    "the endpoint opens no more issues now: try again later",
                )
            });
        }
        rules.new_issues.spend(EVERYONE, plan.new_issues(), now);
        rules
            .writes
            .spend(EVERYONE, plan.new_issues() + plan.updates(), now);
        Ok(waiting)
    }

    /// The answer for a tracker that failed.
    fn tracker_failed(error: &TrackerError) -> Answer {
        log_failure(error);
        Answer::error(
            502,
            "the endpoint could not write the report: try again later",
        )
    }
}

/// Write the message of a failed tracker to standard error, with no address, so an operator can
/// read it. The sender gets no detail.
fn log_failure(error: &TrackerError) {
    eprintln!("strata-report-endpoint: {error}");
}

/// The address of the sender of `request`. With `trust_forwarded`, it is the last entry of
/// `X-Forwarded-For`, which the front end of Cloud Run adds. Otherwise it is the address of the
/// connection.
///
/// A client can send `X-Forwarded-For` to a service that no proxy fronts, and then each request
/// has a sender of its own. So a service reads the header only when its settings say that a proxy
/// sets it.
fn sender_of(request: &tiny_http::Request, trust_forwarded: bool) -> String {
    let forwarded = request
        .headers()
        .iter()
        .find(|header| trust_forwarded && header.field.equiv("X-Forwarded-For"))
        .and_then(|header| header.value.as_str().rsplit(',').next())
        .map(|last| last.trim().to_string())
        .filter(|last| !last.is_empty());
    forwarded.unwrap_or_else(|| {
        request
            .remote_addr()
            .map(|address| address.ip().to_string())
            .unwrap_or_default()
    })
}

/// The value of the header `name` of `request`.
fn header_of(request: &tiny_http::Request, name: &'static str) -> Option<String> {
    request
        .headers()
        .iter()
        .find(|header| header.field.equiv(name))
        .map(|header| header.value.as_str().to_string())
}

/// Write `answer` to the sender of `request`.
fn write_answer(request: tiny_http::Request, answer: &Answer) {
    let mut response =
        tiny_http::Response::from_string(answer.body.to_string()).with_status_code(answer.status);
    for (name, value) in [
        ("Content-Type", "application/json".to_string()),
        (
            "Retry-After",
            answer
                .retry_after
                .map(|s| s.to_string())
                .unwrap_or_default(),
        ),
    ] {
        if value.is_empty() {
            continue;
        }
        if let Ok(header) = tiny_http::Header::from_bytes(name, value) {
            response = response.with_header(header);
        }
    }
    // a sender that left has nobody to tell
    let _ = request.respond(response);
}

/// Read one request, ask the service, write the answer, and then write the report.
fn respond<T: Tracker>(
    mut request: tiny_http::Request,
    service: &Service<T>,
    trust_forwarded: bool,
) {
    let sender = sender_of(&request, trust_forwarded);
    let content_type = header_of(&request, "Content-Type");
    let key = header_of(&request, KEY_HEADER);
    let method = request.method().as_str().to_string();
    let path = request.url().split('?').next().unwrap_or("/").to_string();
    let mut body = Vec::new();
    // one byte over the most, so that the rules can tell a body that is too long
    let read = request
        .as_reader()
        .take(MOST_BYTES as u64 + 1)
        .read_to_end(&mut body);
    let admitted = match read {
        Ok(_) => service.admit(
            &Request {
                method: &method,
                path: &path,
                content_type: content_type.as_deref(),
                key: key.as_deref(),
                sender: &sender,
                body: &body,
            },
            Instant::now(),
        ),
        Err(_) => Admitted::answered(Answer::error(400, "the endpoint could not read the body")),
    };
    write_answer(request, &admitted.answer);
    admitted.finish();
}

/// A thread that handles a request. The count of [`serve`] drops when the thread ends.
struct Busy(Arc<AtomicUsize>);

impl Drop for Busy {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::SeqCst);
    }
}

/// Answer the requests of `server` until `server` is unblocked.
///
/// Each request runs in a thread of its own, so a sender that reads slowly holds no one else. A
/// thread lives until its report is written, which is after its answer, so Cloud Run does not count
/// it. The service handles `most_at_once` requests at a time, and it answers 503 to a request over
/// that number: see [`MOST_AT_ONCE`]. The rules of the checks never wait for a merge: see the page
/// of this module. `trust_forwarded` is the setting of the same name: see
/// [`crate::settings::Settings`].
pub fn serve<T: Tracker + Send + 'static>(
    server: &Arc<tiny_http::Server>,
    service: &Arc<Service<T>>,
    trust_forwarded: bool,
    most_at_once: usize,
) {
    let busy = Arc::new(AtomicUsize::new(0));
    for request in server.incoming_requests() {
        if busy.load(Ordering::SeqCst) >= most_at_once {
            write_answer(request, &Answer::busy());
            continue;
        }
        busy.fetch_add(1, Ordering::SeqCst);
        let held = Busy(Arc::clone(&busy));
        let service = Arc::clone(service);
        std::thread::spawn(move || {
            let _held = held;
            respond(request, &service, trust_forwarded);
        });
    }
}

#[cfg(test)]
mod tests;
