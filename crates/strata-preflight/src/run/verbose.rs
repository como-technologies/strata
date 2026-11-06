//! The lines of `--verbose`: what each check ran, and what each reading of a watch showed.

use std::collections::{BTreeMap, BTreeSet};

use serde_json::Value;
use strata::api::{cluster_events_path, cluster_summary_path, subject_path};
use strata::closed_set::ClosedSet;
use strata::json::{arr, str_at, text_or};
use strata::render::{count_of, paint_state};
use strata::snapshot::SubjectKind;
use strata::term::color::Paint;
use strata::text::{cut, plural};

use crate::probe::{self, Outcome, Verdict};
use crate::report::label::LabelKind;
use crate::rule::{Check, Rule};

/// What a line of `--verbose` shows for a field that the reading does not have.
pub(super) const UNKNOWN: &str = "?";

/// The most events that one reading shows.
const EVENTS_SHOWN: usize = 15;

/// The conditions and the events that the lines showed at the readings before, so that each
/// reading prints what changed.
#[derive(Default)]
pub(super) struct Seen {
    conditions: BTreeMap<String, String>,
    events: BTreeSet<String>,
}

/// The lines of `--verbose` for one check: one line for each request of a check with `each`, then
/// the line of the check.
pub(super) fn verbose_lines(rule: &Rule, o: &Outcome, paint: &Paint) -> Vec<String> {
    let tier = rule.effective_tier().as_str();
    let mut lines: Vec<String> = o
        .ran
        .requests()
        .iter()
        .map(|c| format!("{tier}: {}: {}", rule.id, paint.dim(&c.line())))
        .collect();
    lines.push(verbose_line(rule, o, paint));
    lines
}

/// What a check ran, for its line of `--verbose`. This is its request or command, the count of the
/// requests of a check with `each`, or the program alone.
fn what_ran(o: &Outcome) -> String {
    match (o.ran.text(), o.ran.requests().len()) {
        (Some(ran), _) => ran,
        (None, 0) => "jq over the world".to_string(),
        (None, 1) => "1 request".to_string(),
        (None, n) => format!("{n} requests"),
    }
}

/// One line of `--verbose`: what the check ran, its answer, the time, and the verdict, with the
/// first message when it did not pass.
fn verbose_line(rule: &Rule, o: &Outcome, paint: &Paint) -> String {
    let mut line = format!(
        "{}: {}: {} {}",
        rule.effective_tier().as_str(),
        rule.id,
        paint.dim(&format!(
            "{} -> {}{} ms,",
            what_ran(o),
            o.ran.answer().map(|a| format!("{a}, ")).unwrap_or_default(),
            o.elapsed.as_millis()
        )),
        label(rule, o, paint)
    );
    if o.verdict != Verdict::Passed
        && let Some(m) = o.messages.first()
    {
        line.push_str(&format!(": {m}"));
    }
    line
}

/// The lines of `--verbose` that name the program of each watch rule, before the first reading.
pub(super) fn program_lines(rules: &[&Rule], paint: &Paint) -> Vec<String> {
    rules
        .iter()
        .filter_map(|rule| match &rule.check {
            Some(Check::Watch { expr }) => Some(paint.dim(&format!(
                "watch: {} reads the cluster, its summary, and its events with: {}",
                rule.id,
                cut(&probe::one_line(expr.clone()), 160)
            ))),
            _ => None,
        })
        .collect()
}

/// The lines of `--verbose` for one reading: the three requests it made and what they gave. Then
/// each condition that changed since the reading before. Then each event new since it, the newest
/// 15 at most.
pub(super) fn reading_lines(
    uid: &str,
    read: &Value,
    seen: &mut Seen,
    paint: &Paint,
) -> Vec<String> {
    let mut out = vec![reading_header(uid, read, paint)];
    out.extend(condition_lines(read, &mut seen.conditions, paint));
    out.extend(event_lines(read, &mut seen.events, paint));
    out
}

/// The first line of a reading: its three requests, the state, the health, and the count of the
/// events.
fn reading_header(uid: &str, read: &Value, paint: &Paint) -> String {
    let state = text_or(read, "/cluster/status/state", UNKNOWN);
    format!(
        "{} state {}, health {}, {}",
        paint.dim(&format!(
            "watch: read GET {}, GET {}, GET {}:",
            subject_path(SubjectKind::Cluster, uid),
            cluster_summary_path(uid),
            cluster_events_path(uid)
        )),
        paint_state(paint, state),
        text_or(read, "/summary/status/health/state", UNKNOWN),
        count_of(arr(read, "/events").len(), "event")
    )
}

/// One line for each condition of the cluster whose status or reason changed since `seen`.
fn condition_lines(
    read: &Value,
    seen: &mut BTreeMap<String, String>,
    paint: &Paint,
) -> Vec<String> {
    let mut out = Vec::new();
    for c in arr(read, "/cluster/status/conditions") {
        let kind = text_or(c, "/type", UNKNOWN);
        let status = text_or(c, "/status", UNKNOWN);
        let reason = text_or(c, "/reason", UNKNOWN);
        let now = format!("{status} {reason}");
        if seen.insert(kind.to_string(), now.clone()).as_deref() == Some(now.as_str()) {
            continue;
        }
        let message = text_or(c, "/message", UNKNOWN);
        let status_text = match status {
            "True" => paint.green(status),
            "False" => paint.yellow(status),
            _ => status.to_string(),
        };
        out.push(format!(
            "watch: condition {kind}={status_text} {reason}{}",
            if message == UNKNOWN || message.is_empty() {
                String::new()
            } else {
                paint.dim(&format!(": {}", cut(message, 160)))
            }
        ));
    }
    out
}

/// One line for each event not in `seen`, the newest 15 at most, with a line first that counts
/// the older ones.
fn event_lines(read: &Value, seen: &mut BTreeSet<String>, paint: &Paint) -> Vec<String> {
    let mut fresh: Vec<(String, String)> = arr(read, "/events")
        .iter()
        .map(|e| {
            let at = text_or(e, "/metadata/creationTimestamp", UNKNOWN);
            let reason = str_at(e, "/reason");
            let message = text_or(e, "/message", UNKNOWN);
            let key = format!("{at} {reason} {message}");
            let line = format!(
                "watch: event {}{}: {}",
                paint.cyan(at),
                if reason.is_empty() {
                    String::new()
                } else {
                    format!(" {reason}")
                },
                cut(&probe::one_line(message.to_string()), 200)
            );
            (key, line)
        })
        .filter(|(key, _)| seen.insert(key.clone()))
        .collect();
    fresh.sort();
    let mut out = Vec::new();
    if fresh.len() > EVENTS_SHOWN {
        let older = fresh.len() - EVENTS_SHOWN;
        fresh.drain(..older);
        out.push(paint.dim(&format!(
            "watch: {older} older {} not shown; the report holds them",
            plural(older, "event", "events")
        )));
    }
    out.extend(fresh.into_iter().map(|(_, line)| line));
    out
}

/// One line of `--verbose` at a reading of the watch: the rule and its verdict, with the first
/// message when it did not pass. The time of a watch rule is the time of the reading, which the
/// line above it names, so no time here.
pub(super) fn watch_line(rule: &Rule, o: &Outcome, paint: &Paint) -> String {
    let mut line = format!("watch: {}: {}", rule.id, label(rule, o, paint));
    if o.verdict != Verdict::Passed
        && let Some(m) = o.messages.first()
    {
        line.push_str(&format!(": {m}"));
    }
    line
}

/// The label of an outcome, in its color.
fn label(rule: &Rule, o: &Outcome, paint: &Paint) -> String {
    let kind = LabelKind::of(o.verdict, rule.level, rule.severity);
    kind.paint(paint, &kind.word())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// A reading with `n` events, one second apart.
    fn reading_with_events(n: usize) -> Value {
        let events: Vec<Value> = (0..n)
            .map(|i| {
                json!({
                    "metadata": {"creationTimestamp": format!("2026-01-01T00:00:{i:02}Z")},
                    "reason": "Reconcile",
                    "message": format!("event {i}"),
                })
            })
            .collect();
        json!({"events": events})
    }

    #[test]
    fn the_line_of_the_older_events_agrees_its_noun_with_the_count() {
        for (extra, line) in [
            (1, "watch: 1 older event not shown; the report holds them"),
            (2, "watch: 2 older events not shown; the report holds them"),
        ] {
            let read = reading_with_events(EVENTS_SHOWN + extra);
            let lines = event_lines(&read, &mut BTreeSet::new(), &Paint::off());
            assert_eq!(lines[0], line);
            assert_eq!(
                lines.len(),
                EVENTS_SHOWN + 1,
                "the newest events and the count"
            );
        }
    }

    #[test]
    fn the_first_line_of_a_reading_agrees_its_noun_with_the_count_of_events() {
        for (n, tail) in [(0, ", no events"), (1, ", 1 event"), (2, ", 2 events")] {
            let line = reading_header("u", &reading_with_events(n), &Paint::off());
            assert!(line.ends_with(tail), "{line}");
        }
    }
}
