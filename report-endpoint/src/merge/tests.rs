//! The tests of the merge, with the issues in memory.

use serde_json::json;
use strata::closed_set::unknown::report::{Report, SCHEMA};

use super::memory::Memory;
use super::*;

fn report(entries: &[serde_json::Value]) -> Report {
    serde_json::from_value(json!({"schema": SCHEMA, "unknown": entries})).unwrap()
}

fn entry(set: &str, value: &str, strata: &str, count: u64) -> serde_json::Value {
    json!({"strata": strata, "set": set, "value": value,
        "route": "/v1/clusterprofiles/{uid}", "count": count})
}

#[test]
fn two_reports_of_one_value_are_one_issue_with_the_sum_of_their_lines() {
    let mut tracker = Memory::default();
    let first = merge(
        &mut tracker,
        &report(&[entry("ProfileType", "later", "0.26.0", 2)]),
    )
    .unwrap();
    assert_eq!(
        first,
        Outcome {
            opened: 1,
            updated: 0
        }
    );
    let second = merge(
        &mut tracker,
        &report(&[entry("ProfileType", "later", "0.27.0", 3)]),
    )
    .unwrap();
    assert_eq!(
        second,
        Outcome {
            opened: 0,
            updated: 1
        }
    );
    assert_eq!(tracker.issues.len(), 1, "one issue for a set and a value");
    let (title, text, open) = &tracker.issues[0];
    assert!(open);
    assert_eq!(
        title,
        "A user of strata reports the `ProfileType` value `later` that strata does not know"
    );
    assert!(text.contains("| Lines | 5 |"), "{text}");
    assert!(text.contains("| Reports | 2 |"), "{text}");
    assert!(
        text.contains("| Releases of strata | `0.26.0`, `0.27.0` |"),
        "{text}"
    );
    assert!(
        text.contains("| Routes | `/v1/clusterprofiles/{uid}` |"),
        "{text}"
    );
    for row in ["Releases of Palette", "Refusals"] {
        assert!(
            !text.contains(row),
            "no release of strata writes this: {text}"
        );
    }
}

#[test]
fn a_second_value_or_a_second_set_is_another_issue() {
    let mut tracker = Memory::default();
    let both = report(&[
        entry("ProfileType", "later", "0.26.0", 1),
        entry("ProfileType", "other", "0.26.0", 1),
        entry("PackState", "later", "0.26.0", 1),
    ]);
    assert_eq!(
        merge(&mut tracker, &both).unwrap(),
        Outcome {
            opened: 3,
            updated: 0
        }
    );
    assert_eq!(
        merge(&mut tracker, &both).unwrap(),
        Outcome {
            opened: 0,
            updated: 3
        }
    );
    assert_eq!(tracker.issues.len(), 3);
}

#[test]
fn entries_of_one_value_in_one_report_count_one_report_and_add_their_lines() {
    let mut tracker = Memory::default();
    let mut second = entry("ProfileType", "later", "0.27.0", 4);
    second["route"] = json!("/v1/clusterprofiles/metadata");
    let outcome = merge(
        &mut tracker,
        &report(&[entry("ProfileType", "later", "0.26.0", 1), second]),
    )
    .unwrap();
    assert_eq!(
        outcome,
        Outcome {
            opened: 1,
            updated: 0
        }
    );
    let text = &tracker.issues[0].1;
    assert!(text.contains("| Lines | 5 |"), "{text}");
    assert!(text.contains("| Reports | 1 |"), "{text}");
    assert!(
        text.contains("| Routes | `/v1/clusterprofiles/metadata`, `/v1/clusterprofiles/{uid}` |"),
        "{text}"
    );
}

#[test]
fn the_marker_is_the_first_line_and_the_state_is_the_second() {
    let mut tracker = Memory::default();
    merge(
        &mut tracker,
        &report(&[entry("PackState", "gone", "0.26.0", 1)]),
    )
    .unwrap();
    let text = &tracker.issues[0].1;
    let mut lines = text.lines();
    assert_eq!(
        lines.next(),
        Some("<!-- strata-report: values PackState gone -->")
    );
    assert!(
        lines
            .next()
            .unwrap()
            .starts_with("<!-- strata-report-state: ")
    );
}

#[test]
fn a_closed_issue_gets_the_new_count_and_stays_closed() {
    let mut tracker = Memory::default();
    merge(
        &mut tracker,
        &report(&[entry("ProfileType", "later", "0.26.0", 1)]),
    )
    .unwrap();
    tracker.issues[0].2 = false;
    let outcome = merge(
        &mut tracker,
        &report(&[entry("ProfileType", "later", "0.26.0", 1)]),
    )
    .unwrap();
    assert_eq!(
        outcome,
        Outcome {
            opened: 0,
            updated: 1
        }
    );
    assert_eq!(tracker.issues.len(), 1);
    assert!(
        !tracker.issues[0].2,
        "the merge does not open a closed issue"
    );
    assert!(tracker.issues[0].1.contains("| Lines | 2 |"));
}

#[test]
fn an_issue_of_another_marker_or_with_no_marker_is_left_alone() {
    let mut tracker = Memory::default();
    tracker.issues.push((
        "Palette drift".into(),
        "<!-- strata-contract: values fact ProfileType -->\nbody".into(),
        true,
    ));
    tracker
        .issues
        .push(("A person".into(), "ProfileType later".into(), true));
    merge(
        &mut tracker,
        &report(&[entry("ProfileType", "later", "0.26.0", 1)]),
    )
    .unwrap();
    assert_eq!(tracker.issues.len(), 3);
    assert_eq!(
        tracker.issues[0].1,
        "<!-- strata-contract: values fact ProfileType -->\nbody"
    );
    assert_eq!(tracker.issues[1].1, "ProfileType later");
}

#[test]
fn a_state_that_does_not_read_starts_the_count_again() {
    let mut tracker = Memory::default();
    let mut edited = body("ProfileType", "later", &State::default());
    edited = edited.replace(STATE, "<!-- strata-report-state: zz");
    tracker.issues.push(("t".into(), edited, true));
    merge(
        &mut tracker,
        &report(&[entry("ProfileType", "later", "0.26.0", 3)]),
    )
    .unwrap();
    assert!(tracker.issues[0].1.contains("| Lines | 3 |"));
    assert_eq!(read_state("one line"), State::default());
}

#[test]
fn a_value_cannot_end_the_marker_or_a_code_span() {
    let mut tracker = Memory::default();
    let value = "--> `x` | @user #1";
    merge(
        &mut tracker,
        &report(&[entry("ProfileType", value, "0.26.0", 1)]),
    )
    .unwrap();
    let (title, text, _) = &tracker.issues[0];
    let first = text.lines().next().unwrap();
    assert_eq!(first.matches("-->").count(), 1, "{first}");
    assert!(first.ends_with(" -->"), "{first}");
    assert!(title.contains("``--> `x` | @user #1``"), "{title}");
    assert!(text.contains("``--> `x` | @user #1``"), "{text}");
    // the second report finds the same issue
    merge(
        &mut tracker,
        &report(&[entry("ProfileType", value, "0.26.0", 1)]),
    )
    .unwrap();
    assert_eq!(tracker.issues.len(), 1);
}

#[test]
fn the_title_and_the_sentence_of_a_value_with_a_bar_hold_the_value_without_an_escape() {
    let mut tracker = Memory::default();
    merge(
        &mut tracker,
        &report(&[entry("ProfileType", "a|b", "0.26.0", 1)]),
    )
    .unwrap();
    let (title, text, _) = &tracker.issues[0];
    assert_eq!(
        title,
        "A user of strata reports the `ProfileType` value `a|b` that strata does not know"
    );
    assert!(
        text.contains(
            "Users of strata sent 1 report that Palette gave the `ProfileType` value `a|b`."
        ),
        "{text}"
    );
    assert!(!text.contains("\\|"), "{text}");
}

#[test]
fn a_bar_in_a_cell_of_the_table_of_the_issue_is_escaped() {
    // the check of the report refuses a bar in a version or a route: this is the second guard
    let mut tracker = Memory::default();
    let mut bad = entry("ProfileType", "later", "0.26.0", 1);
    bad["route"] = json!("/v1/a|b");
    merge(&mut tracker, &report(&[bad])).unwrap();
    let (_, text, _) = &tracker.issues[0];
    assert!(text.contains("| Routes | `/v1/a\\|b` |"), "{text}");
}

#[test]
fn a_line_break_in_a_field_cannot_end_the_row_of_the_table() {
    // the check of the report refuses these fields: this is the second guard
    let mut tracker = Memory::default();
    let mut bad = entry("ProfileType", "later", "0.26.0", 1);
    bad["route"] = json!("/v1/x\n\n@someone [a](http://example.invalid)");
    merge(&mut tracker, &report(&[bad])).unwrap();
    let (_, text, _) = &tracker.issues[0];
    for line in text.lines() {
        assert!(!line.starts_with('@'), "{text}");
    }
    assert!(
        text.contains("| Routes | `/v1/x  @someone [a](http://example.invalid)` |"),
        "{text}"
    );
}

#[test]
fn a_list_of_releases_keeps_the_newest_at_the_most_and_the_count_does_not_stop() {
    let mut tracker = Memory::default();
    let send = |tracker: &mut Memory, version: &str| {
        let report = report(&[entry("ProfileType", "later", version, 1)]);
        merge(tracker, &report).unwrap();
    };
    for n in 0..30 {
        send(&mut tracker, &format!("0.{n}.0"));
    }
    let row_of = |tracker: &Memory| -> String {
        tracker.issues[0]
            .1
            .lines()
            .find(|l| l.starts_with("| Releases of strata"))
            .unwrap()
            .to_string()
    };
    assert!(tracker.issues[0].1.contains("| Lines | 30 |"));
    let row = row_of(&tracker);
    assert_eq!(row.matches('`').count() / 2, MOST_LISTED, "{row}");
    assert!(
        row.contains("`0.29.0`") && row.contains("`0.10.0`"),
        "{row}"
    );
    assert!(
        !row.contains("`0.9.0`") && !row.contains("`0.0.0`"),
        "the oldest left: {row}"
    );
    // a release older than the whole list does not enter, and the same release twice counts once
    send(&mut tracker, "0.1.0");
    send(&mut tracker, "0.29.0");
    let row = row_of(&tracker);
    assert!(!row.contains("`0.1.0`"), "{row}");
    assert_eq!(row.matches('`').count() / 2, MOST_LISTED, "{row}");
}

#[test]
fn the_plan_reads_and_writes_nothing_and_counts_the_new_issues() {
    let mut tracker = Memory::default();
    merge(
        &mut tracker,
        &report(&[entry("ProfileType", "later", "0.26.0", 1)]),
    )
    .unwrap();
    let before = tracker.issues.clone().len();
    let found = plan(
        &mut tracker,
        &report(&[
            entry("ProfileType", "later", "0.26.0", 1),
            entry("ProfileType", "new-a", "0.26.0", 1),
            entry("PackState", "new-b", "0.26.0", 1),
        ]),
    )
    .unwrap();
    assert_eq!(found.new_issues(), 2);
    assert_eq!(tracker.issues.len(), before);
}

#[test]
fn versions_are_listed_in_the_order_of_versions_and_not_of_text() {
    let mut tracker = Memory::default();
    for version in ["0.9.0", "0.10.0", "0.2.0"] {
        merge(&mut tracker, &report(&[entry("S", "v", version, 1)])).unwrap();
    }
    let text = &tracker.issues[0].1;
    assert!(
        text.contains("| Releases of strata | `0.2.0`, `0.9.0`, `0.10.0` |"),
        "{text}"
    );
}

#[test]
fn a_state_that_an_older_service_wrote_with_the_palette_and_the_refusals_still_reads() {
    // release 0.27.0 wrote the lists `palette` and `refusals`, and no release writes them now
    let old = json!({"lines": 4, "reports": 2, "strata": ["0.26.0"], "palette": ["4.7.1"],
        "routes": ["/v1/packs"], "refusals": ["409 Gone"]});
    let text = format!(
        "<!-- strata-report: values S v -->\n{STATE}{} -->\nbody",
        hex::encode(old.to_string())
    );
    let state = read_state(&text);
    assert_eq!((state.lines, state.reports), (4, 2));
    let mut tracker = Memory::default();
    tracker.issues.push(("t".into(), text, true));
    merge(&mut tracker, &report(&[entry("S", "v", "0.27.0", 1)])).unwrap();
    let body = &tracker.issues[0].1;
    assert!(body.contains("| Lines | 5 |"), "the count goes on: {body}");
    assert!(body.contains("| Reports | 3 |"), "{body}");
    assert!(!body.contains("4.7.1") && !body.contains("409"), "{body}");
}

#[test]
fn the_endpoint_writes_a_value_in_a_code_span_as_the_library_does() {
    let value = "late`r";
    let mut tracker = Memory::default();
    merge(
        &mut tracker,
        &report(&[entry("ProfileType", value, "0.26.0", 1)]),
    )
    .unwrap();
    let (title, text, _) = &tracker.issues[0];
    let span = strata::text::code_span(value);
    assert_eq!(span, "``late`r``");
    assert!(title.contains(&span), "{title}");
    assert!(text.contains(&span), "{text}");
}
