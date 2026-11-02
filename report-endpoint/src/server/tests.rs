//! The tests of the rules of the service, with the issues in memory.

use std::sync::{Arc, Mutex, mpsc};
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use strata::closed_set::unknown::report::SCHEMA;

use super::*;
use crate::limit::MOST_SENDERS;
use crate::merge::memory::Memory;
use crate::tracker::{Issue, TrackerError};

fn entry(value: &str) -> Value {
    json!({"strata": "0.26.0", "set": "ProfileType", "value": value,
        "route": "/v1/clusterprofiles/{uid}", "count": 1})
}

fn body(entries: &[Value]) -> Vec<u8> {
    json!({"schema": SCHEMA, "unknown": entries})
        .to_string()
        .into_bytes()
}

fn post<'a>(sender: &'a str, body: &'a [u8]) -> Request<'a> {
    Request {
        method: "POST",
        path: "/",
        content_type: Some("application/json"),
        key: None,
        sender,
        body,
    }
}

/// A request with the key `key`, as the send of strata makes it.
fn keyed<'a>(sender: &'a str, key: &'a str, body: &'a [u8]) -> Request<'a> {
    Request {
        key: Some(key),
        ..post(sender, body)
    }
}

/// A key of the form that the check takes: 64 characters of hex.
fn key(n: u8) -> String {
    format!("{n:02x}").repeat(32)
}

/// The values `0` to `count - 1`, as entries.
fn values(count: usize) -> Vec<Value> {
    (0..count).map(|n| entry(&format!("v{n:03}"))).collect()
}

fn issues(service: &Service<Memory>) -> usize {
    service.tracker.lock().unwrap().issues.len()
}

/// The limits where only the one in `change` differs from "no limit that these tests reach".
fn wide() -> Limits {
    Limits {
        per_sender: usize::MAX,
        new_issues: usize::MAX,
        writes: usize::MAX,
    }
}

#[test]
fn a_report_opens_one_issue_and_the_second_report_of_the_value_changes_it() {
    let service = Service::new(Memory::default());
    let now = Instant::now();
    let report = body(&[entry("later")]);
    let first = service.handle(&post("a", &report), now);
    assert_eq!(
        (first.status, first.body),
        (202, json!({"ok": true, "opens": 1, "updates": 0}))
    );
    let second = service.handle(&post("b", &report), now);
    assert_eq!(
        (second.status, second.body),
        (202, json!({"ok": true, "opens": 0, "updates": 1}))
    );
    assert_eq!(issues(&service), 1);
}

#[test]
fn a_report_with_another_field_changes_nothing_and_gets_422_with_no_name_of_the_field() {
    let service = Service::new(Memory::default());
    let mut with = entry("later");
    with["uid"] = json!("64f0a1b2c3d4e5f6a7b8c9d0");
    let answer = service.handle(&post("a", &body(&[with])), Instant::now());
    assert_eq!(answer.status, 422);
    let message = answer.body["error"].as_str().unwrap();
    assert!(message.contains("is not in the list"), "{message}");
    assert!(!message.contains("uid"), "{message}");
    assert_eq!(issues(&service), 0);
}

#[test]
fn only_post_at_the_root_is_taken() {
    let service = Service::new(Memory::default());
    let report = body(&[entry("later")]);
    let now = Instant::now();
    let mut get = post("a", &report);
    get.method = "GET";
    assert_eq!(service.handle(&get, now).status, 405);
    let mut elsewhere = post("a", &report);
    elsewhere.path = "/v1/report";
    assert_eq!(service.handle(&elsewhere, now).status, 404);
    assert_eq!(issues(&service), 0);
}

#[test]
fn a_sender_that_makes_too_many_requests_gets_429_with_the_time_to_wait() {
    let limits = Limits {
        per_sender: 2,
        ..wide()
    };
    let service = Service::with_limits(Memory::default(), limits);
    let report = body(&[entry("later")]);
    let now = Instant::now();
    for _ in 0..2 {
        assert_eq!(service.handle(&post("a", &report), now).status, 202);
    }
    let refused = service.handle(&post("a", &report), now + Duration::from_secs(60));
    assert_eq!(refused.status, 429);
    assert_eq!(refused.retry_after, Some(3540));
    // another sender is not held, and the sender is free after the hour
    assert_eq!(service.handle(&post("b", &report), now).status, 202);
    assert_eq!(
        service
            .handle(&post("a", &report), now + Duration::from_secs(3600))
            .status,
        202
    );
}

#[test]
fn a_request_that_fails_a_check_costs_the_sender_nothing() {
    let limits = Limits {
        per_sender: 1,
        ..wide()
    };
    let service = Service::with_limits(Memory::default(), limits);
    let now = Instant::now();
    for _ in 0..5 {
        assert_eq!(service.handle(&post("a", b"junk"), now).status, 400);
    }
    assert_eq!(
        service
            .handle(&post("a", &body(&[entry("later")])), now)
            .status,
        202,
        "the sender made one valid request"
    );
    assert_eq!(
        service
            .handle(&post("a", &body(&[entry("later")])), now)
            .status,
        429
    );
}

#[test]
fn the_new_issues_that_fit_the_limit_open_and_the_entries_of_the_others_wait() {
    let limits = Limits {
        new_issues: 3,
        ..wide()
    };
    let service = Service::with_limits(Memory::default(), limits);
    let now = Instant::now();
    let two = body(&[entry("a"), entry("b")]);
    assert_eq!(service.handle(&post("a", &two), now).status, 202);
    // two more new values: one issue is left, so one opens and the other waits
    let more = body(&[entry("c"), entry("d")]);
    let partly = service.handle(&post("b", &more), now);
    assert_eq!(partly.status, 202);
    assert_eq!(
        partly.body,
        json!({"ok": true, "opens": 1, "updates": 0, "waiting": [2]})
    );
    assert_eq!(issues(&service), 3);
    // a report of values that have an issue opens none, so the limit does not hold it
    let known = service.handle(&post("c", &two), now);
    assert_eq!(known.body["updates"], 2, "{}", known.body);
    // only new values, and no issue left: the report waits as a whole, and writes nothing
    let refused = service.handle(&post("d", &body(&[entry("e")])), now);
    assert_eq!(refused.status, 429);
    assert!(refused.retry_after.is_some());
    assert_eq!(issues(&service), 3, "the refused report wrote nothing");
}

#[test]
fn a_report_of_21_new_values_opens_20_and_names_the_entry_that_waits() {
    let service = Service::new(Memory::default());
    let now = Instant::now();
    let report = body(&values(21));
    let answer = service.handle(&post("a", &report), now);
    assert_eq!(answer.status, 202, "{}", answer.body);
    assert_eq!(
        answer.body,
        json!({"ok": true, "opens": 20, "updates": 0, "waiting": [21]})
    );
    assert_eq!(issues(&service), 20);
    // the wait is over in an hour: the value that waited opens then
    let last = body(&[entry("v020")]);
    assert_eq!(service.handle(&post("b", &last), now).status, 429);
    let later = now + Duration::from_secs(3600);
    let opened = service.handle(&post("b", &last), later);
    assert_eq!(opened.status, 202);
    assert_eq!(issues(&service), 21);
}

#[test]
fn a_report_of_the_most_values_of_a_report_of_strata_fits_an_hour_that_no_one_used() {
    const { assert!(NEW_ISSUES_PER_HOUR >= MOST_VALUES) };
    let service = Service::new(Memory::default());
    let answer = service.handle(&post("a", &body(&values(MOST_VALUES))), Instant::now());
    assert_eq!(answer.status, 202);
    assert_eq!(answer.body.get("waiting"), None, "{}", answer.body);
    assert_eq!(issues(&service), MOST_VALUES);
}

#[test]
fn the_writes_have_a_limit_and_the_entries_of_the_steps_over_it_wait() {
    let limits = Limits {
        writes: 3,
        ..wide()
    };
    let service = Service::with_limits(Memory::default(), limits);
    let now = Instant::now();
    let five = body(&values(5));
    let answer = service.handle(&post("a", &five), now);
    assert_eq!(
        answer.body,
        json!({"ok": true, "opens": 3, "updates": 0, "waiting": [4, 5]})
    );
    // the three issues exist, and a change is a write too
    let updates = service.handle(&post("b", &body(&values(3))), now);
    assert_eq!(
        updates.status, 429,
        "the writes are spent: {}",
        updates.body
    );
    assert!(
        updates.body["error"].as_str().unwrap().contains("writes"),
        "{}",
        updates.body
    );
    assert_eq!(issues(&service), 3);
    let after_the_hour = now + Duration::from_secs(3600);
    assert_eq!(
        service
            .handle(&post("b", &body(&values(3))), after_the_hour)
            .body["updates"],
        3
    );
}

#[test]
fn the_updates_of_issues_cost_the_limit_of_the_writes_for_a_sender_of_any_address() {
    let limits = Limits {
        writes: 20,
        ..wide()
    };
    let service = Service::with_limits(Memory::default(), limits);
    let now = Instant::now();
    let report = body(&[entry("later")]);
    let statuses: Vec<u16> = (0..30)
        .map(|n| {
            let sender = format!("2001:db8:{n:x}::1");
            service.handle(&post(&sender, &report), now).status
        })
        .collect();
    assert_eq!(statuses.iter().filter(|s| **s == 202).count(), 20);
    assert_eq!(statuses.iter().filter(|s| **s == 429).count(), 10);
    let issue = service.tracker.lock().unwrap().issues[0].1.clone();
    assert!(issue.contains("| Reports | 20 |"), "{issue}");
}

#[test]
fn an_ipv6_network_is_one_sender_so_its_hosts_share_one_limit() {
    let service = Service::new(Memory::default());
    let report = body(&[entry("later")]);
    let now = Instant::now();
    for n in 0..REQUESTS_PER_HOUR {
        let host = format!("2001:db8:1:2::{n:x}");
        assert_eq!(service.handle(&post(&host, &report), now).status, 202);
    }
    let eleventh = service.handle(&post("2001:db8:1:2:ffff:ffff:ffff:ffff", &report), now);
    assert_eq!(eleventh.status, 429, "the 11th host of one /64");
    assert_eq!(
        service
            .handle(&post("2001:db8:1:3::1", &report), now)
            .status,
        202,
        "another network is another sender"
    );
}

#[test]
fn a_flood_of_senders_does_not_lock_out_a_new_sender() {
    let limits = Limits {
        per_sender: 1,
        ..wide()
    };
    let service = Service::with_limits(Memory::default(), limits);
    let report = body(&[entry("later")]);
    let now = Instant::now();
    // more networks than the table of senders holds
    for n in 0..MOST_SENDERS + 100 {
        let sender = format!("2001:db8:{:x}:{:x}::1", n / 65_536, n % 65_536);
        assert_eq!(service.handle(&post(&sender, &report), now).status, 202);
    }
    let real = service.handle(&post("203.0.113.9", &report), now);
    assert_eq!(real.status, 202, "{}", real.body);
}

#[test]
fn a_count_above_the_most_is_refused_and_changes_nothing() {
    let service = Service::new(Memory::default());
    let mut huge = entry("later");
    huge["count"] = json!(u64::MAX);
    let answer = service.handle(&post("a", &body(&[huge])), Instant::now());
    assert_eq!(answer.status, 422);
    assert!(
        answer.body["error"]
            .as_str()
            .unwrap()
            .contains("the count is more than"),
        "{}",
        answer.body
    );
    assert_eq!(issues(&service), 0);
}

#[test]
fn a_second_try_with_the_key_of_the_first_counts_once_and_a_new_key_counts_again() {
    let service = Service::new(Memory::default());
    let now = Instant::now();
    let report = body(&[entry("later")]);
    let first = service.handle(&keyed("a", &key(1), &report), now);
    assert_eq!(first.status, 202);
    let again = service.handle(&keyed("a", &key(1), &report), now);
    assert_eq!(
        (again.status, again.body),
        (200, json!({"ok": true, "duplicate": true}))
    );
    let issue = service.tracker.lock().unwrap().issues[0].1.clone();
    assert!(issue.contains("| Reports | 1 |"), "{issue}");
    // the run after the first one sends other lines, so it has another key
    assert_eq!(
        service.handle(&keyed("a", &key(2), &report), now).status,
        202
    );
    let issue = service.tracker.lock().unwrap().issues[0].1.clone();
    assert!(issue.contains("| Reports | 2 |"), "{issue}");
    // a request with no key is never a repeat
    assert_eq!(service.handle(&post("a", &report), now).status, 202);
    assert_eq!(service.handle(&post("a", &report), now).status, 202);
}

#[test]
fn a_key_stops_the_report_that_it_belongs_to_and_no_other_report() {
    let service = Service::new(Memory::default());
    let now = Instant::now();
    let mine = body(&[entry("later")]);
    let other = body(&[entry("other")]);
    // an attacker that knows the key of the true report sends another report with it first
    assert_eq!(
        service.handle(&keyed("x", &key(1), &other), now).status,
        202
    );
    let taken = service.handle(&keyed("a", &key(1), &mine), now);
    assert_eq!(taken.status, 202, "{}", taken.body);
    assert_eq!(issues(&service), 2);
}

#[test]
fn a_report_taken_a_day_ago_counts_again() {
    let service = Service::new(Memory::default());
    let now = Instant::now();
    let report = body(&[entry("later")]);
    assert_eq!(
        service.handle(&keyed("a", &key(1), &report), now).status,
        202
    );
    let next_day = now + DAY;
    let answer = service.handle(&keyed("b", &key(1), &report), next_day);
    assert_eq!(answer.status, 202, "{}", answer.body);
}

#[test]
fn a_key_of_another_form_is_refused_with_400_and_costs_the_sender_nothing() {
    let service = Service::new(Memory::default());
    let report = body(&[entry("later")]);
    let answer = service.handle(&keyed("a", "short", &report), Instant::now());
    assert_eq!(answer.status, 400);
    assert!(
        !answer.body.to_string().contains("short\""),
        "{}",
        answer.body
    );
    assert_eq!(issues(&service), 0);
}

/// A tracker that writes to a log that the test shares, so a test can read it while the service
/// holds the tracker.
#[derive(Debug, Clone, Default)]
struct Logged {
    events: Arc<Mutex<Vec<String>>>,
    fail_writes: bool,
}

impl Logged {
    fn events(&self) -> Vec<String> {
        self.events.lock().unwrap().clone()
    }
}

impl Tracker for Logged {
    fn issues(&mut self) -> Result<Vec<Issue>, TrackerError> {
        self.events.lock().unwrap().push("list".into());
        Ok(Vec::new())
    }

    fn open(&mut self, title: &str, _: &str) -> Result<u64, TrackerError> {
        if self.fail_writes {
            return Err(TrackerError {
                message: "open an issue: HTTP 500".into(),
            });
        }
        self.events.lock().unwrap().push(format!("open {title}"));
        Ok(1)
    }

    fn update(&mut self, _: u64, _: &str) -> Result<(), TrackerError> {
        unreachable!("the stand-in has no issue to change")
    }
}

#[test]
fn the_answer_comes_after_the_reads_and_before_the_writes() {
    let tracker = Logged::default();
    let service = Service::new(tracker.clone());
    let admitted = service.admit(&post("a", &body(&values(2))), Instant::now());
    assert_eq!(admitted.answer.status, 202);
    assert_eq!(tracker.events(), ["list"], "nothing is written yet");
    admitted.finish();
    assert_eq!(tracker.events().len(), 3, "{:?}", tracker.events());
}

#[test]
fn a_request_that_a_check_refuses_does_not_wait_for_a_merge_that_is_going_on() {
    let service = Service::new(Logged::default());
    let report = body(&[entry("later")]);
    let now = Instant::now();
    // the first report is answered, and its write has not run: it holds the lock of the tracker
    let slow = service.admit(&post("a", &report), now);
    assert_eq!(slow.answer.status, 202);
    let (sender, received) = mpsc::channel();
    std::thread::scope(|scope| {
        scope.spawn(|| {
            let mut get = post("b", &report);
            get.method = "GET";
            let mut statuses = vec![service.handle(&get, now).status];
            statuses.push(service.handle(&post("b", b"junk"), now).status);
            let mut wrong = entry("later");
            wrong["uid"] = json!("x");
            statuses.push(service.handle(&post("b", &body(&[wrong])), now).status);
            sender.send(statuses).unwrap();
        });
        let statuses = received.recv_timeout(Duration::from_secs(10));
        assert_eq!(statuses, Ok(vec![405, 400, 422]));
        slow.finish();
    });
}

#[test]
fn a_tracker_that_fails_to_write_loses_the_report_and_the_second_try_counts() {
    let tracker = Logged {
        fail_writes: true,
        ..Logged::default()
    };
    let service = Service::new(tracker.clone());
    let report = body(&[entry("later")]);
    let now = Instant::now();
    let first = service.handle(&keyed("a", &key(1), &report), now);
    assert_eq!(first.status, 202, "the answer came before the write");
    // the write failed, so the service does not call the second try a repeat
    service.tracker.lock().unwrap().fail_writes = false;
    let second = service.handle(&keyed("a", &key(1), &report), now);
    assert_eq!(second.status, 202, "{}", second.body);
    assert_eq!(tracker.events().len(), 3, "{:?}", tracker.events());
}

#[test]
fn a_tracker_that_fails_to_read_gives_502_and_the_answer_names_no_detail() {
    #[derive(Debug)]
    struct Down;
    impl Tracker for Down {
        fn issues(&mut self) -> Result<Vec<Issue>, TrackerError> {
            Err(TrackerError {
                message: "list the issues: HTTP 401 token ghp_x".into(),
            })
        }
        fn open(&mut self, _: &str, _: &str) -> Result<u64, TrackerError> {
            unreachable!()
        }
        fn update(&mut self, _: u64, _: &str) -> Result<(), TrackerError> {
            unreachable!()
        }
    }
    let service = Service::new(Down);
    let answer = service.handle(&post("a", &body(&[entry("later")])), Instant::now());
    assert_eq!(answer.status, 502);
    assert!(
        !answer.body.to_string().contains("ghp_x"),
        "{}",
        answer.body
    );
}
