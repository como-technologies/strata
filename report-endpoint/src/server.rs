//! The service: the checks of a request in their order, and the loop that answers requests.
//!
//! [`Service::handle`] holds the rules, and it knows nothing of HTTP. [`serve`] reads each request
//! from a `tiny_http` server, calls it, and writes the answer. The service makes no log of a
//! request: it keeps no address, and the report is not for a log.

use std::io::Read;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde_json::{Value, json};

use crate::accept::{self, MOST_BYTES};
use crate::limit::Limiter;
use crate::merge;
use crate::tracker::Tracker;

/// The requests that one sender makes in an hour, at most.
pub const REQUESTS_PER_HOUR: usize = 10;

/// The issues that the service opens in an hour, at most, for all senders. A flood of made-up
/// values opens this many issues.
pub const NEW_ISSUES_PER_HOUR: usize = 20;

/// The span of the two limits.
const HOUR: Duration = Duration::from_secs(3600);

/// The key of the limit for all senders.
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
    /// The address of the sender, as text. The service hashes it and keeps no text.
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
    /// The seconds to wait before the next request, for a 429.
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
}

/// The rules of the service, and the tracker that they write to.
#[derive(Debug)]
pub struct Service<T: Tracker> {
    tracker: T,
    senders: Limiter,
    new_issues: Limiter,
}

impl<T: Tracker> Service<T> {
    /// A service with the limits [`REQUESTS_PER_HOUR`] and [`NEW_ISSUES_PER_HOUR`].
    pub fn new(tracker: T) -> Service<T> {
        Service::with_limits(tracker, REQUESTS_PER_HOUR, NEW_ISSUES_PER_HOUR)
    }

    /// A service where a sender makes `per_sender` requests and the service opens `new_issues`
    /// issues in an hour.
    pub fn with_limits(tracker: T, per_sender: usize, new_issues: usize) -> Service<T> {
        Service {
            tracker,
            senders: Limiter::new(per_sender, HOUR),
            new_issues: Limiter::new(new_issues, HOUR),
        }
    }

    /// Answer the request `request` at the time `now`. The checks run in this order: the path and
    /// the method, the limit of the sender, the report, and the limit of the new issues. The
    /// merge comes last, so a request that fails a check changes nothing.
    pub fn handle(&mut self, request: &Request, now: Instant) -> Answer {
        if request.path != "/" {
            return Answer::error(404, "the endpoint takes a report at /");
        }
        if request.method != "POST" {
            return Answer::error(405, "the endpoint takes POST");
        }
        if let Err(wait) = self.senders.allow(request.sender, 1, now) {
            return Answer::wait(wait, "this sender made too many requests: try again later");
        }
        let report = match accept::parse(request.content_type, request.body) {
            Ok(report) => report,
            Err(refusal) => return Answer::error(refusal.status, &refusal.message),
        };
        let plan = match merge::plan(&mut self.tracker, &report) {
            Ok(plan) => plan,
            Err(error) => return Self::tracker_failed(&error),
        };
        let opening = plan.new_issues();
        if opening > 0
            && let Err(wait) = self.new_issues.allow(EVERYONE, opening, now)
        {
            return Answer::wait(
                wait,
                "the endpoint opens no more issues now: try again later",
            );
        }
        match merge::apply(&mut self.tracker, plan) {
            Ok(outcome) => Answer {
                status: 200,
                body: json!({"ok": true, "opened": outcome.opened, "updated": outcome.updated}),
                retry_after: None,
            },
            Err(error) => Self::tracker_failed(&error),
        }
    }

    /// The answer for a tracker that failed. The message of the error goes to standard error, with
    /// no address, so an operator can read it. The sender gets no detail.
    fn tracker_failed(error: &crate::tracker::TrackerError) -> Answer {
        eprintln!("strata-report-endpoint: {error}");
        Answer::error(
            502,
            "the endpoint could not write the report: try again later",
        )
    }
}

/// The address of the sender of `request`: the last entry of `X-Forwarded-For`, which the front
/// end of Cloud Run adds, or the address of the connection.
fn sender_of(request: &tiny_http::Request) -> String {
    let forwarded = request
        .headers()
        .iter()
        .find(|header| header.field.equiv("X-Forwarded-For"))
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

/// Read one request, ask the service, and write the answer.
fn respond<T: Tracker>(mut request: tiny_http::Request, service: &Mutex<Service<T>>) {
    let sender = sender_of(&request);
    let content_type = header_of(&request, "Content-Type");
    let method = request.method().as_str().to_string();
    let path = request.url().split('?').next().unwrap_or("/").to_string();
    let mut body = Vec::new();
    // one byte over the most, so that the rules can tell a body that is too long
    let read = request
        .as_reader()
        .take(MOST_BYTES as u64 + 1)
        .read_to_end(&mut body);
    let answer = match read {
        Ok(_) => {
            let asked = Request {
                method: &method,
                path: &path,
                content_type: content_type.as_deref(),
                sender: &sender,
                body: &body,
            };
            // a thread that panicked cannot hurt the next request: the rules hold no half state
            let mut service = service
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            service.handle(&asked, Instant::now())
        }
        Err(_) => Answer::error(400, "the endpoint could not read the body"),
    };
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

/// Answer the requests of `server` until `server` is unblocked.
///
/// Each request runs in a thread of its own, so a sender that reads slowly holds no one else. The
/// rules run one at a time, under the lock of `service`.
pub fn serve<T: Tracker + Send + 'static>(
    server: &Arc<tiny_http::Server>,
    service: &Arc<Mutex<Service<T>>>,
) {
    for request in server.incoming_requests() {
        let service = Arc::clone(service);
        std::thread::spawn(move || respond(request, &service));
    }
}

#[cfg(test)]
mod tests;
