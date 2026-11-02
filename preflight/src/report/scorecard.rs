//! The scorecard of a report: one line for each section, and the counts of the run. It also holds
//! the texts about them that the forms of the report share.

use std::collections::BTreeMap;

use super::{CheckResult, Options, SectionScore, Summary, percent};
use crate::confidence::{Confidence, NO_RUNS, runs_to_reach};
use crate::probe::Verdict;
use crate::rule::{Catalog, DEFAULT_BAR, Level, Severity, Status};
use strata::text::plural;

/// The scorecard: one line for each section that has a row for this engine.
pub fn scorecard(catalog: &Catalog, checks: &[CheckResult]) -> Vec<SectionScore> {
    let mut out = Vec::new();
    for section in &catalog.sections {
        let rows: Vec<&CheckResult> = checks
            .iter()
            .filter(|c| c.section == section.section)
            .collect();
        if rows.is_empty() {
            continue;
        }
        let ran: Vec<&&CheckResult> = rows
            .iter()
            .filter(|c| {
                matches!(c.verdict, Verdict::Passed | Verdict::Failed)
                    && c.severity != Severity::Note
            })
            .collect();
        let passed = ran.iter().filter(|c| c.verdict == Verdict::Passed).count() as u32;
        let failed = ran.len() as u32 - passed;
        let not_run = rows
            .iter()
            .filter(|c| c.verdict == Verdict::NotRun && c.severity != Severity::Note)
            .count() as u32;
        let with_runs = rows
            .iter()
            .filter(|c| c.confidence.as_ref().is_some_and(Confidence::has_runs))
            .count() as u32;
        let weakest = rows
            .iter()
            .filter_map(|c| c.confidence.as_ref().and_then(Confidence::weakest_bound))
            .fold(None, |acc: Option<f64>, b| {
                Some(acc.map_or(b, |a| a.min(b)))
            });
        out.push(SectionScore {
            section: section.section.clone(),
            title: section.title.clone(),
            rows: rows.len() as u32,
            seen: rows.iter().filter(|c| c.status == Status::Seen).count() as u32,
            with_check: rows
                .iter()
                .filter(|c| catalog.rule(&c.id).is_some_and(|r| r.check.is_some()))
                .count() as u32,
            preflight: rows.iter().filter(|c| c.level == Level::Preflight).count() as u32,
            with_runs,
            weakest_bound: weakest,
            ran: ran.len() as u32,
            passed,
            failed,
            not_run,
            not_checked: rows
                .iter()
                .filter(|c| c.verdict == Verdict::NotChecked)
                .count() as u32,
            readiness: readiness_of(passed, ran.len() as u32 + not_run),
        });
    }
    out
}

/// The readiness: the checks that passed over the checks that apply, or none when none applies.
pub(super) fn readiness_of(passed: u32, apply: u32) -> Option<f64> {
    (apply > 0).then(|| f64::from(passed) / f64::from(apply))
}

/// Why the checks did not run, with the count of each reason. A note is outside the count, as it
/// is outside the readiness.
pub(crate) fn not_run_reasons<'a>(
    checks: impl Iterator<Item = &'a CheckResult>,
) -> BTreeMap<String, u32> {
    let mut out = BTreeMap::new();
    for c in checks.filter(|c| c.verdict == Verdict::NotRun && c.severity != Severity::Note) {
        let why = c.messages.first().cloned().unwrap_or_default();
        *out.entry(why).or_insert(0) += 1;
    }
    out
}

/// The lowest bound of a section as the forms of the report show it. It is a whole percent, or
/// `no runs` when no row of the section has a run.
pub(crate) fn bound_text(bound: Option<f64>) -> String {
    bound.map_or_else(|| NO_RUNS.to_string(), percent)
}

/// `N check(s) did not run: reason (count); reason (count)`.
pub(crate) fn not_run_text(reasons: &BTreeMap<String, u32>) -> String {
    let n: u32 = reasons.values().sum();
    let why: Vec<String> = reasons.iter().map(|(r, c)| format!("{r} ({c})")).collect();
    format!(
        "{n} check{} did not run: {}",
        plural(n as usize, "", "s"),
        why.join("; ")
    )
}

/// What `--out` wrote, or what it would write.
pub(crate) fn written_text(out_dir: Option<&str>) -> String {
    match out_dir {
        Some(dir) => format!("Written to {dir}: report.html, report.md, report.json, world.json, entity.json."),
        None => "No file written; --out DIR writes report.html, report.md, report.json, world.json, and entity.json, and the reproduce lines read world.json and entity.json there ($OUT).".to_string(),
    }
}

pub(super) fn summarize(
    checks: &[CheckResult],
    sections: &[SectionScore],
    options: &Options,
) -> Summary {
    let count = |f: &dyn Fn(&CheckResult) -> bool| checks.iter().filter(|c| f(c)).count() as u32;
    let ran = sections.iter().map(|s| s.ran).sum();
    let passed = sections.iter().map(|s| s.passed).sum();
    let failed = count(&|c| c.counts_as_failure);
    let probe_failed = count(&|c| {
        c.verdict == Verdict::Failed
            && c.level == Level::Probe
            && c.severity == Severity::Fail
            && !c.counts_as_failure
    });
    let warned = count(&|c| c.verdict == Verdict::Failed && c.severity == Severity::Warn);
    let noted = count(&|c| c.verdict == Verdict::Failed && c.severity == Severity::Note);
    let not_checked = count(&|c| c.verdict == Verdict::NotChecked);
    // a note is outside the readiness whether it ran or not, as `ran` omits it
    let not_run = count(&|c| c.verdict == Verdict::NotRun && c.severity != Severity::Note);
    let not_run_reasons = not_run_reasons(checks.iter());
    let skipped = count(&|c| c.verdict == Verdict::Skipped);
    let errors = count(&|c| c.verdict == Verdict::Error);
    let readiness = readiness_of(passed, ran + not_run);
    let counted = Summary {
        rules: checks.len() as u32,
        ran,
        passed,
        failed,
        probe_failed,
        warned,
        noted,
        not_run,
        not_run_reasons,
        not_checked,
        skipped,
        errors,
        readiness,
        exit_code: 0,
        verdict: String::new(),
    };
    let (exit_code, verdict) = exit_and_verdict(checks, &counted, options.ready);
    Summary {
        exit_code,
        verdict,
        ..counted
    }
}

/// The exit code and the verdict line of a run, from its counts. `ready` is the readiness that the
/// run needs.
fn exit_and_verdict(checks: &[CheckResult], summary: &Summary, ready: Option<f64>) -> (u8, String) {
    let Summary {
        ran,
        failed,
        probe_failed,
        errors,
        readiness,
        ..
    } = *summary;
    let under_ready = match (ready, readiness) {
        (Some(want), Some(have)) => have < want,
        (Some(_), None) => true,
        _ => false,
    };
    if failed > 0 {
        let ids: Vec<&str> = checks
            .iter()
            .filter(|c| c.counts_as_failure)
            .map(|c| c.id.as_str())
            .collect();
        (
            1,
            format!("FAIL: {failed} check(s) failed: {}", ids.join(", ")),
        )
    } else if under_ready {
        (
            1,
            format!(
                "NOT READY: readiness {} is under {}",
                readiness.map_or_else(|| "none".to_string(), percent),
                percent(ready.unwrap_or(0.0))
            ),
        )
    } else if errors > 0 {
        let ids: Vec<&str> = checks
            .iter()
            .filter(|c| c.verdict == Verdict::Error)
            .map(|c| c.id.as_str())
            .collect();
        (
            2,
            format!(
                "ERROR: {errors} check(s) could not answer: {}",
                ids.join(", ")
            ),
        )
    } else if ran == 0 {
        (0, "OK: no check ran".to_string())
    } else {
        let probes = if probe_failed > 0 {
            format!("; {probe_failed} probe(s) failed, not counted")
        } else {
            String::new()
        };
        (0, format!("OK: each counted check passed{probes}"))
    }
}

/// What the evidence columns of the scorecard mean, with the count of promoted rules here.
pub(crate) fn scorecard_legend(sections: &[SectionScore]) -> String {
    let promoted: u32 = sections.iter().map(|s| s.preflight).sum();
    let checks: u32 = sections.iter().map(|s| s.with_check).sum();
    let (bar, runs) = (percent(DEFAULT_BAR), runs_to_reach(DEFAULT_BAR));
    format!(
        "runs: the rules of the section with a run in the ledger on this engine; weakest: the lowest 95% lower bound among them, and no runs when none has one; preflight: the rules promoted to preflight checks, whose failure is exit 1. A rule is promoted at a bound of {bar}, which is {runs} correct runs of {runs} on the engine; every other rule is a probe. Promoted here: {promoted} of {checks} checks."
    )
}
