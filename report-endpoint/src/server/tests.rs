//! The tests of the rules of the service, with the issues in memory.

use std::time::{Duration, Instant};

use serde_json::{Value, json};

use super::*;
use crate::merge::memory::Memory;

fn entry(value: &str) -> Value {
    json!({"strata": "0.26.0", "set": "ProfileType", "value": value,
        "route": "/v1/clusterprofiles/{uid}", "count": 1})
}

fn body(entries: &[Value]) -> Vec<u8> {
    json!({"schema": "strata-report/1", "unknown": entries})
        .to_string()
        .into_bytes()
}

fn post<'a>(sender: &'a str, body: &'a [u8]) -> Request<'a> {
    Request {
        method: "POST",
        path: "/",
        content_type: Some("application/json"),
        sender,
        body,
    }
}

#[test]
fn a_report_opens_one_issue_and_the_second_report_of_the_value_changes_it() {
    let mut service = Service::new(Memory::default());
    let now = Instant::now();
    let report = body(&[entry("later")]);
    let first = service.handle(&post("a", &report), now);
    assert_eq!(
        (first.status, first.body),
        (200, json!({"ok": true, "opened": 1, "updated": 0}))
    );
    let second = service.handle(&post("b", &report), now);
    assert_eq!(
        (second.status, second.body),
        (200, json!({"ok": true, "opened": 0, "updated": 1}))
    );
    assert_eq!(service.tracker.issues.len(), 1);
}

#[test]
fn a_report_with_another_field_changes_nothing_and_gets_422() {
    let mut service = Service::new(Memory::default());
    let mut with = entry("later");
    with["uid"] = json!("64f0a1b2c3d4e5f6a7b8c9d0");
    let answer = service.handle(&post("a", &body(&[with])), Instant::now());
    assert_eq!(answer.status, 422);
    assert!(
        answer.body["error"]
            .as_str()
            .unwrap()
            .contains("unknown field `uid`")
    );
    assert!(service.tracker.issues.is_empty());
}

#[test]
fn only_post_at_the_root_is_taken() {
    let mut service = Service::new(Memory::default());
    let report = body(&[entry("later")]);
    let now = Instant::now();
    let mut get = post("a", &report);
    get.method = "GET";
    assert_eq!(service.handle(&get, now).status, 405);
    let mut elsewhere = post("a", &report);
    elsewhere.path = "/v1/report";
    assert_eq!(service.handle(&elsewhere, now).status, 404);
    assert!(service.tracker.issues.is_empty());
}

#[test]
fn a_sender_that_makes_too_many_requests_gets_429_with_the_time_to_wait() {
    let mut service = Service::with_limits(Memory::default(), 2, 100);
    let report = body(&[entry("later")]);
    let now = Instant::now();
    for _ in 0..2 {
        assert_eq!(service.handle(&post("a", &report), now).status, 200);
    }
    let refused = service.handle(&post("a", &report), now + Duration::from_secs(60));
    assert_eq!(refused.status, 429);
    assert_eq!(refused.retry_after, Some(3540));
    // another sender is not held, and the sender is free after the hour
    assert_eq!(service.handle(&post("b", &report), now).status, 200);
    assert_eq!(
        service
            .handle(&post("a", &report), now + Duration::from_secs(3600))
            .status,
        200
    );
}

#[test]
fn a_refused_request_costs_the_sender_a_request() {
    let mut service = Service::with_limits(Memory::default(), 1, 100);
    let now = Instant::now();
    assert_eq!(service.handle(&post("a", b"junk"), now).status, 400);
    assert_eq!(
        service
            .handle(&post("a", &body(&[entry("later")])), now)
            .status,
        429
    );
}

#[test]
fn a_flood_of_made_up_values_opens_only_the_most_issues_and_writes_nothing_for_the_refused_report()
{
    let mut service = Service::with_limits(Memory::default(), 100, 3);
    let now = Instant::now();
    let two = body(&[entry("a"), entry("b")]);
    assert_eq!(service.handle(&post("a", &two), now).status, 200);
    // two more new values: only one issue is left, so the report is refused as a whole
    let more = body(&[entry("c"), entry("d")]);
    let refused = service.handle(&post("b", &more), now);
    assert_eq!(refused.status, 429);
    assert!(refused.retry_after.is_some());
    assert_eq!(
        service.tracker.issues.len(),
        2,
        "the refused report wrote nothing"
    );
    // a report of values that have an issue opens none, so the limit does not hold it
    assert_eq!(service.handle(&post("c", &two), now).status, 200);
    assert_eq!(
        service.handle(&post("d", &body(&[entry("c")])), now).status,
        200
    );
}

#[test]
fn a_tracker_that_fails_gives_502_and_the_answer_names_no_detail() {
    #[derive(Debug)]
    struct Down;
    impl Tracker for Down {
        fn issues(&mut self) -> Result<Vec<crate::tracker::Issue>, crate::tracker::TrackerError> {
            Err(crate::tracker::TrackerError {
                message: "list the issues: HTTP 401 token ghp_x".into(),
            })
        }
        fn open(&mut self, _: &str, _: &str) -> Result<u64, crate::tracker::TrackerError> {
            unreachable!()
        }
        fn update(&mut self, _: u64, _: &str) -> Result<(), crate::tracker::TrackerError> {
            unreachable!()
        }
    }
    let mut service = Service::new(Down);
    let answer = service.handle(&post("a", &body(&[entry("later")])), Instant::now());
    assert_eq!(answer.status, 502);
    assert!(
        !answer.body.to_string().contains("ghp_x"),
        "{}",
        answer.body
    );
}
