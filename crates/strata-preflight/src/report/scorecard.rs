//! The scorecard of a report: one line for each section, and the counts of the run. It also holds
//! the texts about them that the forms of the report share.

use std::collections::BTreeMap;

use super::{CheckResult, Options, SectionScore, Summary, is_counted, percent};
use crate::confidence::{Confidence, NO_RUNS, runs_to_reach};
use crate::probe::Verdict;
use crate::rule::{Catalog, DEFAULT_BAR, Level, Severity, Status};
use strata::render::count_of;
use strata::text::plural;

/// What the results add to the counts of the scorecard and of the summary. One function counts
/// both, so the counts of the sections add to the counts of the summary.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(super) struct Tally {
    pub(super) passed: u32,
    /// The failures that count: exit `1`.
    pub(super) failed: u32,
    /// The probes that failed and do not count.
    pub(super) probe_failed: u32,
    /// The failures of severity `warn`. A warning is not a failure, and it never counts.
    pub(super) warned: u32,
    /// The failures of severity `note`.
    pub(super) noted: u32,
    pub(super) not_run: u32,
}

impl Tally {
    /// The counts of `checks`. A note is outside the checks that ran and outside the readiness,
    /// whether it ran or not: a failed note is a note, and another note adds nothing.
    pub(super) fn of<'a>(checks: impl IntoIterator<Item = &'a CheckResult>) -> Tally {
        let mut t = Tally::default();
        for c in checks {
            let count = match (c.verdict, c.severity) {
                (Verdict::Failed, Severity::Note) => &mut t.noted,
                (_, Severity::Note) => continue,
                (Verdict::Passed, _) => &mut t.passed,
                (Verdict::Failed, _) if c.counts_as_failure => &mut t.failed,
                (Verdict::Failed, Severity::Warn) => &mut t.warned,
                (Verdict::Failed, _) => &mut t.probe_failed,
                (Verdict::NotRun, _) => &mut t.not_run,
                _ => continue,
            };
            *count += 1;
        }
        t
    }

    /// The checks that ran: each one that passed or failed, at each level and severity but
    /// `note`.
    pub(super) fn ran(&self) -> u32 {
        self.passed + self.failed + self.probe_failed + self.warned
    }

    /// The readiness of the counts: see [`readiness_of`].
    fn readiness(&self) -> Option<f64> {
        readiness_of(self.passed, self.ran(), self.not_run)
    }
}

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
        let tally = Tally::of(rows.iter().copied());
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
            ran: tally.ran(),
            passed: tally.passed,
            failed: tally.failed,
            probe_failed: tally.probe_failed,
            warned: tally.warned,
            not_run: tally.not_run,
            not_checked: rows
                .iter()
                .filter(|c| c.verdict == Verdict::NotChecked)
                .count() as u32,
            readiness: tally.readiness(),
        });
    }
    out
}

/// The checks that apply here: the ones that ran and the ones that did not run. The readiness
/// counts them, and so does the summary line of each form of the report.
pub(super) fn applies(ran: u32, not_run: u32) -> u32 {
    ran + not_run
}

/// The readiness: the checks that passed over the checks that apply, or none when none applies.
pub(super) fn readiness_of(passed: u32, ran: u32, not_run: u32) -> Option<f64> {
    let apply = applies(ran, not_run);
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

/// `N checks did not run: reason (count); reason (count)`.
pub(crate) fn not_run_text(reasons: &BTreeMap<String, u32>) -> String {
    let n: u32 = reasons.values().sum();
    let why: Vec<String> = reasons.iter().map(|(r, c)| format!("{r} ({c})")).collect();
    format!(
        "{n} check{} did not run: {}",
        plural(n as usize, "", "s"),
        why.join("; ")
    )
}

/// What `--out` wrote, or what it writes. The line names the two files that hold the values of the
/// variables. When the run did not write them, the line says how to write them.
pub(crate) fn written_text(options: &Options) -> String {
    const REPORTS: &str = "report.html, report.md, report.json";
    match (options.out_dir.as_deref(), options.out_world) {
        (Some(dir), true) => format!(
            "Written to {dir}: {REPORTS}, world.json, entity.json. The last two hold the values of the variables."
        ),
        (Some(dir), false) => format!(
            "Written to {dir}: {REPORTS}. The reproduce lines read world.json and entity.json there ($OUT): run again with --world to write them. They hold the values of the variables."
        ),
        (None, _) => "No file written; --out DIR writes report.html, report.md, and report.json, and --world adds world.json and entity.json, which the reproduce lines read there ($OUT).".to_string(),
    }
}

/// The counts of the run, and its exit code and verdict. `cloud` is the engine: the verdict of a
/// run with no counted check names it.
pub(super) fn summarize(
    checks: &[CheckResult],
    cloud: &str,
    options: &Options,
    cluster_gone: bool,
) -> Summary {
    let count = |f: &dyn Fn(&CheckResult) -> bool| checks.iter().filter(|c| f(c)).count() as u32;
    let tally = Tally::of(checks);
    let summary = Summary {
        rules: checks.len() as u32,
        ran: tally.ran(),
        counted: count(&|c| {
            matches!(c.verdict, Verdict::Passed | Verdict::Failed)
                && is_counted(c.level, c.severity, options.strict)
        }),
        passed: tally.passed,
        failed: tally.failed,
        probe_failed: tally.probe_failed,
        warned: tally.warned,
        noted: tally.noted,
        not_run: tally.not_run,
        not_run_reasons: not_run_reasons(checks.iter()),
        not_checked: count(&|c| c.verdict == Verdict::NotChecked),
        skipped: count(&|c| c.verdict == Verdict::Skipped),
        errors: count(&|c| c.verdict == Verdict::Error),
        readiness: tally.readiness(),
        exit_code: 0,
        verdict: String::new(),
    };
    let (exit_code, verdict) = exit_and_verdict(checks, &summary, cloud, options, cluster_gone);
    Summary {
        exit_code,
        verdict,
        ..summary
    }
}

/// The exit code and the verdict line of a run, from its counts. `cloud` is the engine, and
/// `options` give the readiness that the run needs. `cluster_gone` is true when a watch ended
/// because Palette deleted the cluster: a verdict with no failure then says so.
fn exit_and_verdict(
    checks: &[CheckResult],
    summary: &Summary,
    cloud: &str,
    options: &Options,
    cluster_gone: bool,
) -> (u8, String) {
    let Summary {
        failed,
        errors,
        readiness,
        ..
    } = *summary;
    let under_ready = match (options.ready, readiness) {
        (Some(want), Some(have)) => have < want,
        (Some(_), None) => true,
        _ => false,
    };
    let ids = |f: &dyn Fn(&CheckResult) -> bool| {
        checks
            .iter()
            .filter(|c| f(c))
            .map(|c| c.id.as_str())
            .collect::<Vec<_>>()
            .join(", ")
    };
    if failed > 0 {
        (
            1,
            format!(
                "FAIL: {} failed: {}",
                count_of(failed as usize, "check"),
                ids(&|c| c.counts_as_failure)
            ),
        )
    } else if under_ready {
        (
            1,
            format!(
                "NOT READY: readiness {} is under {}",
                options.readiness_text(&readiness.map_or_else(|| "none".to_string(), percent)),
                percent(options.ready.unwrap_or(0.0))
            ),
        )
    } else if errors > 0 {
        (
            2,
            format!(
                "ERROR: {} could not answer: {}",
                count_of(errors as usize, "check"),
                ids(&|c| c.verdict == Verdict::Error)
            ),
        )
    } else {
        (0, no_failure_verdict(summary, cloud, cluster_gone))
    }
}

/// The verdict of a run in which no counted check failed and no check gave an error, with the
/// failed probes. A run with no counted check is `NOT COUNTED`, and not `OK`: no check of the run
/// can fail it. `cloud` is the engine, which the words name.
fn no_failure_verdict(summary: &Summary, cloud: &str, cluster_gone: bool) -> String {
    let gone = if cluster_gone {
        "the cluster is gone; "
    } else {
        ""
    };
    let probes = if summary.probe_failed > 0 {
        format!(
            "; {} failed, not counted",
            count_of(summary.probe_failed as usize, "probe")
        )
    } else {
        String::new()
    };
    if summary.ran == 0 {
        format!("NOT COUNTED: {gone}no check ran on {cloud}")
    } else if summary.counted == 0 {
        format!("NOT COUNTED: {gone}no check counted on {cloud}{probes}")
    } else if cluster_gone {
        format!("OK: {gone}no counted check failed{probes}")
    } else {
        format!("OK: each counted check passed{probes}")
    }
}

/// What the evidence columns of the scorecard mean, with the count of promoted rules here.
pub(crate) fn scorecard_legend(sections: &[SectionScore]) -> String {
    let promoted: u32 = sections.iter().map(|s| s.preflight).sum();
    let checks: u32 = sections.iter().map(|s| s.with_check).sum();
    let noun = plural(checks as usize, "check", "checks");
    let (bar, runs) = (percent(DEFAULT_BAR), runs_to_reach(DEFAULT_BAR));
    format!(
        "runs: the rules of the section with a run in the ledger on this engine; weakest: the lowest 95% lower bound among them, and no runs when none has one; preflight: the rules promoted to preflight checks, whose failure is exit 1. A rule is promoted at a bound of {bar}, which is {runs} correct runs of {runs} on the engine; every other rule is a probe. Promoted here: {promoted} of {checks} {noun}."
    )
}
