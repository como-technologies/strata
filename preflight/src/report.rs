//! The report of a run: the document `strata-preflight/1`, and its terminal, markdown, and JSON
//! forms. [`crate::html`] gives the HTML form. A script reads the exit status and the JSON; the
//! text forms can change in any release.

use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::confidence::Confidence;
use crate::jq::Jq;
use crate::probe::{Outcome, Verdict};
use crate::rule::{Catalog, Evidence, Fix, Level, Rule, Severity, Status, Tier};

/// The schema id of the report.
pub const REPORT_SCHEMA: &str = "strata-preflight/1";

/// Which tool wrote the report.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Tool {
    /// `strata-preflight`.
    pub name: String,
    /// The crate version.
    pub version: String,
}

/// What the run checked.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Subject {
    /// The engine.
    pub cloud: String,
    /// The Palette release, when given.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub release: Option<String>,
    /// Where the entity came from: a file, `cluster:<uid>`, or both with a change.
    pub source: String,
    /// The name of the cluster, from the entity.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub name: String,
    /// The cluster uid, for an existing cluster.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cluster_uid: Option<String>,
}

/// The options of the run that change what the report holds.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Options {
    /// The tiers that ran.
    pub tiers: Vec<Tier>,
    /// True when a failed probe counts as a failed check.
    pub strict: bool,
    /// The readiness the run needs, as a share, when `--ready` was given.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ready: Option<f64>,
    /// The directory `--out` wrote the reports and the world into, when it was given.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub out_dir: Option<String>,
}

/// One rule in the report.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct CheckResult {
    /// The rule id.
    pub id: String,
    /// The title of the rule.
    pub title: String,
    /// Its section.
    pub section: String,
    /// The tier that ran, or the tier of the row when nothing ran.
    pub tier: Tier,
    /// Probe or preflight check.
    pub level: Level,
    /// What a failure means.
    pub severity: Severity,
    /// Where the knowledge comes from.
    pub status: Status,
    /// What the check answered.
    pub verdict: Verdict,
    /// One line for each failure, or the reason for another verdict.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub messages: Vec<String>,
    /// The time to the result, in milliseconds.
    pub elapsed_ms: u64,
    /// The request or the command the check ran.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ran: Option<String>,
    /// The answer of the call: `HTTP 200`, or `exit 0`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub answer: Option<String>,
    /// What a person runs by hand to see the same thing.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub reproduce: Vec<String>,
    /// The fix for good.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fix: Option<Fix>,
    /// The evidence of the rule.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub evidence: Vec<Evidence>,
    /// The confidence of the check on this engine and release, from the ledger.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub confidence: Option<Confidence>,
    /// True when this result makes the exit code `1`: a failed preflight check of severity
    /// `fail`, or a failed probe with `--strict`.
    pub counts_as_failure: bool,
}

impl CheckResult {
    /// `PASS`, `FAIL`, `PROBE PASS`, `PROBE FAIL`, `WARN`, `NOTE`, `NOT CHECKED`, `SKIP`, `ERROR`.
    pub fn label(&self) -> String {
        label_for(self.verdict, self.level, self.severity)
    }
}

/// The word of a result: `PASS`, `FAIL`, `PROBE PASS`, `PROBE FAIL`, `WARN`, `NOTE`,
/// `NOT CHECKED`, `NOT RUN`, `SKIP`, `ERROR`.
pub fn label_for(verdict: Verdict, level: Level, severity: Severity) -> String {
    match (verdict, level, severity) {
        (Verdict::Failed, _, Severity::Note) => "NOTE".to_string(),
        (Verdict::Failed, _, Severity::Warn) => "WARN".to_string(),
        (Verdict::Passed | Verdict::Failed, Level::Probe, _) => {
            format!("PROBE {}", verdict.name())
        }
        _ => verdict.name().to_string(),
    }
}

/// One line of the section scorecard.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SectionScore {
    /// The section id.
    pub section: String,
    /// Its title.
    pub title: String,
    /// The rows of the section that apply to this engine.
    pub rows: u32,
    /// The rows a live run shows.
    pub seen: u32,
    /// The rows with a check in code.
    pub with_check: u32,
    /// The rows promoted to preflight checks.
    pub preflight: u32,
    /// The rows with a run in the ledger on this engine, of either kind.
    pub with_runs: u32,
    /// The lowest 95% lower bound among the rows of the section with runs on this engine, or
    /// none when no row has a run.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub weakest_bound: Option<f64>,
    /// The checks that ran: passed or failed.
    pub ran: u32,
    /// The checks that passed.
    pub passed: u32,
    /// The checks that failed, at any level and severity but `note`.
    pub failed: u32,
    /// The checks that apply here and did not run: a tier not asked for, no key, or `--no-exec`.
    pub not_run: u32,
    /// The rows strata-preflight could not check here.
    pub not_checked: u32,
    /// `passed / (ran + not run)`, or none when no check applies. A check that did not run
    /// counts against it: nothing shows that it passes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub readiness: Option<f64>,
}

/// The counts of the run.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Summary {
    /// The rules that apply to this engine and release.
    pub rules: u32,
    /// The checks that ran.
    pub ran: u32,
    /// Passed.
    pub passed: u32,
    /// Failed, and counted: exit `1`.
    pub failed: u32,
    /// Failed probes, not counted without `--strict`.
    pub probe_failed: u32,
    /// Warnings.
    pub warned: u32,
    /// Notes.
    pub noted: u32,
    /// `NOT RUN`: the checks that apply here and did not run.
    pub not_run: u32,
    /// Why the checks did not run, with the count of each reason.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub not_run_reasons: BTreeMap<String, u32>,
    /// `NOT CHECKED`.
    pub not_checked: u32,
    /// `SKIP`: the rules that do not apply to this run.
    pub skipped: u32,
    /// `ERROR`.
    pub errors: u32,
    /// `passed / (ran + not run)`, or none when no check applies.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub readiness: Option<f64>,
    /// The exit code of the run.
    pub exit_code: u8,
    /// One line that says the outcome.
    pub verdict: String,
}

/// What the words of the document mean, for a reader of the JSON.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Legend {
    /// Each verdict.
    pub verdicts: BTreeMap<String, String>,
    /// Each level.
    pub levels: BTreeMap<String, String>,
    /// Each tier.
    pub tiers: BTreeMap<String, String>,
    /// The exit codes.
    pub exit: BTreeMap<String, String>,
}

impl Legend {
    fn new() -> Legend {
        let m = |pairs: &[(&str, &str)]| {
            pairs
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect()
        };
        Legend {
            verdicts: m(&[
                ("passed", "the check ran and found no fault"),
                (
                    "failed",
                    "the check ran and found the fault; the messages say what",
                ),
                (
                    "not-checked",
                    "strata-preflight has no check it can run here; the reproduce lines say what to run by hand",
                ),
                (
                    "not-run",
                    "the check applies here and did not run: a tier not asked for, no key, or --no-exec; it counts against the readiness",
                ),
                ("skipped", "the rule does not apply to this run"),
                ("error", "the check could not answer"),
            ]),
            levels: m(&[
                (
                    "probe",
                    "the runs do not yet prove the check; a failure does not change the exit code without --strict",
                ),
                (
                    "preflight",
                    "the runs prove the check on this engine; a failure is exit 1",
                ),
            ]),
            tiers: m(&[
                (
                    "static",
                    "the entity and the records alone; no key, no network",
                ),
                ("palette", "a call to Palette"),
                ("cloud", "a call to the cloud with its tool"),
                (
                    "watch",
                    "the conditions and the events of the cluster during the build",
                ),
                (
                    "race",
                    "no check before the build finds it; the watch finds its effect",
                ),
                (
                    "tool",
                    "a rule of the tool that makes the entity, out of scope here",
                ),
            ]),
            exit: m(&[
                ("0", "each counted check passed"),
                (
                    "1",
                    "a counted check failed, or the readiness is under --ready",
                ),
                ("2", "strata-preflight could not answer"),
            ]),
        }
    }
}

/// The report.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Report {
    /// The schema id.
    #[schemars(extend("const" = REPORT_SCHEMA))]
    pub schema: String,
    /// Which tool wrote it.
    pub tool: Tool,
    /// When, UTC.
    pub generated_at: String,
    /// What was checked.
    pub subject: Subject,
    /// The options of the run.
    pub options: Options,
    /// One entry for each rule that applies, in the order of the catalog.
    pub checks: Vec<CheckResult>,
    /// The section scorecard.
    pub sections: Vec<SectionScore>,
    /// The counts.
    pub summary: Summary,
    /// What the words mean.
    pub legend: Legend,
}

impl Report {
    /// The exit code of the run.
    pub fn exit_code(&self) -> u8 {
        self.summary.exit_code
    }

    /// The failures that count, by id.
    pub fn counted_failures(&self) -> Vec<&CheckResult> {
        self.checks.iter().filter(|c| c.counts_as_failure).collect()
    }
}

/// The inputs of [`assemble`]: each rule with its outcome, and what the report needs beside them.
pub struct Assembly<'a> {
    /// The catalog the run read.
    pub catalog: &'a Catalog,
    /// Each rule that applies, with what it answered.
    pub outcomes: Vec<(&'a Rule, Outcome)>,
    /// The confidence of each rule from the ledger, by id.
    pub confidence: BTreeMap<String, Confidence>,
    /// The world, for the reproduce templates.
    pub world: &'a Value,
    /// The subject.
    pub subject: Subject,
    /// The options.
    pub options: Options,
    /// The time.
    pub generated_at: String,
}

/// A failure that counts: a failed rule of severity `fail` at the preflight level, or any failed
/// probe of that severity with `--strict`. It is exit `1`, and it ends a watch. A warning or a
/// note never counts.
pub fn counts_as_failure(rule: &Rule, verdict: Verdict, strict: bool) -> bool {
    verdict == Verdict::Failed
        && rule.severity == Severity::Fail
        && (rule.level == Level::Preflight || strict)
}

/// Build the report from the outcomes.
pub fn assemble(a: Assembly<'_>, jq: &Jq) -> Report {
    let mut checks = Vec::with_capacity(a.outcomes.len());
    for (rule, outcome) in &a.outcomes {
        let counts_as_failure = counts_as_failure(rule, outcome.verdict, a.options.strict);
        let mut reproduce: Vec<String> = Vec::new();
        for line in rule.reproduce.for_engine(&a.subject.cloud) {
            reproduce.push(
                jq.render(&rule.id, &line, a.world)
                    .unwrap_or_else(|_| line.clone()),
            );
        }
        if matches!(
            outcome.verdict,
            Verdict::Failed | Verdict::Passed | Verdict::Error
        ) {
            reproduce.extend(auto_reproduce(
                rule,
                outcome,
                &a.subject,
                a.options.out_dir.as_deref(),
            ));
        }
        checks.push(CheckResult {
            id: rule.id.clone(),
            title: rule.title.clone(),
            section: rule.section.clone(),
            tier: rule.effective_tier(),
            level: rule.level,
            severity: rule.severity,
            status: rule.status,
            verdict: outcome.verdict,
            messages: outcome.messages.clone(),
            elapsed_ms: outcome.elapsed.as_millis() as u64,
            ran: outcome.ran.clone(),
            answer: outcome.answer.clone(),
            reproduce,
            fix: rule.fix.clone(),
            evidence: rule.evidence.clone(),
            confidence: a.confidence.get(&rule.id).cloned(),
            counts_as_failure,
        });
    }
    let sections = scorecard(a.catalog, &checks);
    let summary = summarize(&checks, &sections, &a.options);
    Report {
        schema: REPORT_SCHEMA.to_string(),
        tool: Tool {
            name: crate::TOOL_NAME.to_string(),
            version: crate::TOOL_VERSION.to_string(),
        },
        generated_at: a.generated_at,
        subject: a.subject,
        options: a.options,
        checks,
        sections,
        summary,
        legend: Legend::new(),
    }
}

/// The line a person runs to see the same thing the check saw: the `jq` over the world file, the
/// `curl` to Palette, or the command.
fn auto_reproduce(
    rule: &Rule,
    outcome: &Outcome,
    subject: &Subject,
    out_dir: Option<&str>,
) -> Vec<String> {
    use crate::rule::Check;
    // with no --out, the two files are not on disk: $OUT names where --out would put them
    let dir = out_dir.unwrap_or("$OUT");
    let mut lines = Vec::new();
    match &rule.check {
        Some(Check::Static { expr }) => {
            lines.push(format!("jq '{}' {dir}/world.json", one_line(expr)));
        }
        Some(Check::Palette {
            method, expr, body, ..
        }) => {
            let path = outcome
                .ran
                .as_deref()
                .and_then(|r| r.split_once(' '))
                .map(|(_, p)| p.to_string())
                .unwrap_or_default();
            let data = match (method, body) {
                // a rule with its own body program sends what the program gives, not the entity
                (crate::rule::Method::Post, Some(program)) => {
                    lines.push(format!(
                        "jq '{}' {dir}/world.json > request.json",
                        one_line(program)
                    ));
                    " -X POST -H 'Content-Type: application/json' --data @request.json".to_string()
                }
                (crate::rule::Method::Post, None) => format!(
                    " -X POST -H 'Content-Type: application/json' --data @{dir}/entity.json"
                ),
                (crate::rule::Method::Get, _) => String::new(),
            };
            lines.push(format!(
                "code=$(curl -s -o body.json -w '%{{http_code}}' -H \"ApiKey: $PALETTE_API_KEY\" -H \"ProjectUid: $PALETTE_PROJECT_UID\"{data} \"${{PALETTE_API:-https://api.spectrocloud.com}}{path}\")"
            ));
            lines.push(format!(
                "jq --arg code \"$code\" --slurpfile body body.json '.response = {{ok: (($code | tonumber) < 300), status: ($code | tonumber), body: ($body[0] // null)}} | {}' {dir}/world.json",
                one_line(expr)
            ));
        }
        Some(Check::Exec { expr, .. }) => {
            if let Some(ran) = &outcome.ran {
                lines.push(ran.clone());
                lines.push(format!(
                    "jq --argjson out \"$(<the output>)\" '.exec = {{exit: 0, stdout: $out, stderr: \"\"}} | {}' {dir}/world.json",
                    one_line(expr)
                ));
            }
        }
        Some(Check::Watch { .. }) => {
            if let Some(uid) = &subject.cluster_uid {
                lines.push(format!(
                    "curl -s -H \"ApiKey: $PALETTE_API_KEY\" -H \"ProjectUid: $PALETTE_PROJECT_UID\" \"${{PALETTE_API:-https://api.spectrocloud.com}}/v1/events/components/spectrocluster/{uid}?limit=50\""
                ));
            }
        }
        None => {}
    }
    lines
}

fn one_line(expr: &str) -> String {
    expr.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .replace('\'', "'\\''")
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
fn readiness_of(passed: u32, apply: u32) -> Option<f64> {
    (apply > 0).then(|| f64::from(passed) / f64::from(apply))
}

/// Why the checks did not run, with the count of each reason.
pub(crate) fn not_run_reasons<'a>(
    checks: impl Iterator<Item = &'a CheckResult>,
) -> BTreeMap<String, u32> {
    let mut out = BTreeMap::new();
    for c in checks.filter(|c| c.verdict == Verdict::NotRun) {
        let why = c.messages.first().cloned().unwrap_or_default();
        *out.entry(why).or_insert(0) += 1;
    }
    out
}

/// `N check(s) did not run: reason (count); reason (count)`.
pub(crate) fn not_run_text(reasons: &BTreeMap<String, u32>) -> String {
    let n: u32 = reasons.values().sum();
    let why: Vec<String> = reasons.iter().map(|(r, c)| format!("{r} ({c})")).collect();
    format!(
        "{n} check{} did not run: {}",
        if n == 1 { "" } else { "s" },
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

fn summarize(checks: &[CheckResult], sections: &[SectionScore], options: &Options) -> Summary {
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
    // a note is outside the readiness whether it ran or not, as `ran` leaves it out
    let not_run = count(&|c| c.verdict == Verdict::NotRun && c.severity != Severity::Note);
    let not_run_reasons = not_run_reasons(checks.iter().filter(|c| c.severity != Severity::Note));
    let skipped = count(&|c| c.verdict == Verdict::Skipped);
    let errors = count(&|c| c.verdict == Verdict::Error);
    let readiness = readiness_of(passed, ran + not_run);
    let under_ready = match (options.ready, readiness) {
        (Some(want), Some(have)) => have < want,
        (Some(_), None) => true,
        _ => false,
    };
    let (exit_code, verdict) = if failed > 0 {
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
                "NOT READY: readiness {} is under {:.0}%",
                readiness.map_or("none".to_string(), |r| format!("{:.0}%", r * 100.0)),
                options.ready.unwrap_or(0.0) * 100.0
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
    };
    Summary {
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
        exit_code,
        verdict,
    }
}

/// The report formats.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    /// Text for a terminal.
    Terminal,
    /// A pull request comment.
    Markdown,
    /// The document.
    Json,
    /// A page with the scorecard.
    Html,
}

/// Render a report.
pub fn render(report: &Report, format: Format, compact: bool, color: bool) -> String {
    match format {
        Format::Terminal => terminal(report, color),
        Format::Markdown => markdown(report),
        Format::Json => {
            if compact {
                serde_json::to_string(report).unwrap_or_default()
            } else {
                serde_json::to_string_pretty(report).unwrap_or_default() + "\n"
            }
        }
        Format::Html => crate::html::page(report),
    }
}

fn pct(v: Option<f64>) -> String {
    v.map_or("-".to_string(), |x| format!("{:.0}%", x * 100.0))
}

/// `text` in the ANSI color `code` when `color` is on, else as it is.
pub(crate) fn paint(color: bool, code: &str, text: &str) -> String {
    if color {
        format!("\x1b[{code}m{text}\x1b[0m")
    } else {
        text.to_string()
    }
}

fn label_color(c: &CheckResult) -> &'static str {
    label_color_for(c.verdict, c.severity, c.level)
}

/// The ANSI color of a label: green for a pass, red for a counted failure, magenta for a failed
/// probe, yellow for a warning, cyan for a note, dim for what did not run.
pub(crate) fn label_color_for(verdict: Verdict, severity: Severity, level: Level) -> &'static str {
    match (verdict, severity, level) {
        (Verdict::Passed, _, _) => "32",
        (Verdict::Failed, Severity::Note, _) => "36",
        (Verdict::Failed, Severity::Warn, _) => "33",
        (Verdict::Failed, _, Level::Preflight) => "31",
        (Verdict::Failed, _, Level::Probe) => "35",
        (Verdict::NotChecked, _, _) => "33",
        (Verdict::NotRun, _, _) => "2",
        (Verdict::Skipped, _, _) => "2",
        (Verdict::Error, _, _) => "31",
    }
}

/// The terminal form.
pub fn terminal(r: &Report, color: bool) -> String {
    let mut out = String::new();
    let release = r
        .subject
        .release
        .as_deref()
        .map_or("release not given".to_string(), |x| format!("release {x}"));
    let tiers: Vec<&str> = r.options.tiers.iter().map(|t| t.name()).collect();
    out.push_str(&format!(
        "{}: {} cluster {} ({}), Palette {release}, tiers {}{}\n",
        crate::TOOL_NAME,
        r.subject.cloud,
        if r.subject.name.is_empty() {
            "-"
        } else {
            &r.subject.name
        },
        r.subject.source,
        tiers.join(", "),
        if r.options.strict { ", strict" } else { "" }
    ));
    for sc in &r.sections {
        let rows: Vec<&CheckResult> = r
            .checks
            .iter()
            .filter(|c| c.section == sc.section)
            .collect();
        let visible: Vec<&CheckResult> = rows
            .iter()
            .copied()
            .filter(|c| !matches!(c.verdict, Verdict::Skipped | Verdict::NotRun))
            .collect();
        if visible.is_empty() && sc.not_run == 0 {
            continue;
        }
        out.push_str(&format!("\n{}\n", paint(color, "1", &sc.title)));
        if sc.not_run > 0 {
            // one line for the checks of the section that did not run, not one line each
            out.push_str(&format!(
                "  {}  {}\n",
                paint(color, "2", &format!("{:<12}", "NOT RUN")),
                not_run_text(&not_run_reasons(
                    rows.iter()
                        .copied()
                        .filter(|c| c.severity != Severity::Note)
                ))
            ));
        }
        for c in visible {
            let label = format!("{:<12}", c.label());
            let conf = c
                .confidence
                .as_ref()
                .map(|k| format!("  catch {} clean {}", k.catch.text(), k.clean.text()))
                .unwrap_or_default();
            out.push_str(&format!(
                "  {}  {:<28} {:<60} {:>5} ms{conf}\n",
                paint(color, label_color(c), &label),
                c.id,
                truncate(&c.title, 60),
                c.elapsed_ms
            ));
            if c.verdict != Verdict::Passed {
                for m in &c.messages {
                    out.push_str(&format!("{:<16}{m}\n", ""));
                }
                if let Some(ran) = &c.ran {
                    out.push_str(&format!(
                        "{:<16}{} {ran}{}\n",
                        "",
                        paint(color, "2", "ran:"),
                        c.answer
                            .as_ref()
                            .map(|a| format!(" -> {a}"))
                            .unwrap_or_default()
                    ));
                }
            }
            if matches!(
                c.verdict,
                Verdict::Failed | Verdict::NotChecked | Verdict::Error
            ) {
                for line in &c.reproduce {
                    out.push_str(&format!(
                        "{:<16}{} {line}\n",
                        "",
                        paint(color, "2", "reproduce:")
                    ));
                }
                if c.verdict == Verdict::Failed
                    && let Some(fix) = &c.fix
                    && !fix.text.is_empty()
                {
                    out.push_str(&format!(
                        "{:<16}{} {} ({})\n",
                        "",
                        paint(color, "2", "fix:"),
                        fix.text,
                        fix.owner
                    ));
                }
            }
        }
    }
    out.push_str(&format!("\n{}\n", paint(color, "1", "Section scorecard")));
    out.push_str(&format!(
        "  {:<28} {:>4} {:>4} {:>5} {:>9} {:>4} {:>7} {:>4} {:>4} {:>4} {:>7} {:>11} {:>5}\n",
        "section",
        "rows",
        "seen",
        "check",
        "preflight",
        "runs",
        "weakest",
        "ran",
        "pass",
        "fail",
        "not run",
        "not checked",
        "ready"
    ));
    for s in &r.sections {
        out.push_str(&format!(
            "  {:<28} {:>4} {:>4} {:>5} {:>9} {:>4} {:>7} {:>4} {:>4} {:>4} {:>7} {:>11} {:>5}\n",
            truncate(&s.title, 28),
            s.rows,
            s.seen,
            s.with_check,
            s.preflight,
            s.with_runs,
            s.weakest_bound
                .map_or("no runs".to_string(), |b| format!("{:.0}%", b * 100.0)),
            s.ran,
            s.passed,
            s.failed,
            s.not_run,
            s.not_checked,
            pct(s.readiness)
        ));
    }
    out.push_str(&format!(
        "{}\n",
        paint(color, "2", &scorecard_legend(&r.sections))
    ));
    let s = &r.summary;
    out.push_str(&format!(
        "\n{} rules apply; {} ran: {} passed, {} failed, {} probe(s) failed, {} warning(s), {} note(s); {} did not run, {} not checked, {} do not apply, {} error(s); readiness {}: {} of {} checks passed\n",
        s.rules, s.ran, s.passed, s.failed, s.probe_failed, s.warned, s.noted, s.not_run, s.not_checked, s.skipped, s.errors, pct(s.readiness), s.passed, s.ran + s.not_run
    ));
    if s.not_run > 0 {
        out.push_str(&format!(
            "{}\n",
            paint(color, "2", &not_run_text(&s.not_run_reasons))
        ));
    }
    out.push_str(&format!(
        "{}\n",
        paint(color, "2", &written_text(r.options.out_dir.as_deref()))
    ));
    let code = if s.exit_code == 0 { "32" } else { "31" };
    out.push_str(&format!("{}\n", paint(color, code, &s.verdict)));
    out
}

fn truncate(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        s.to_string()
    } else {
        let cut: String = s.chars().take(n - 1).collect();
        format!("{cut}…")
    }
}

/// The markdown form: a pull request comment.
pub fn markdown(r: &Report) -> String {
    let mut out = String::new();
    let s = &r.summary;
    out.push_str(&format!(
        "## strata-preflight: {} cluster `{}`\n\n",
        r.subject.cloud,
        if r.subject.name.is_empty() {
            "-"
        } else {
            &r.subject.name
        }
    ));
    out.push_str(&format!(
        "**{}** — source `{}`, Palette {}, tiers {}{}. {} ran: {} passed, {} failed, {} probe(s) failed; {} did not run, {} not checked; readiness **{}** ({} of {} checks passed).\n\n",
        s.verdict,
        r.subject.source,
        r.subject.release.as_deref().unwrap_or("release not given"),
        r.options.tiers.iter().map(|t| t.name()).collect::<Vec<_>>().join(", "),
        if r.options.strict { ", strict" } else { "" },
        s.ran, s.passed, s.failed, s.probe_failed, s.not_run, s.not_checked, pct(s.readiness), s.passed, s.ran + s.not_run
    ));
    out.push_str(
        "| result | rule | tier | what | time | confidence |\n|---|---|---|---|---|---|\n",
    );
    for c in r
        .checks
        .iter()
        .filter(|c| !matches!(c.verdict, Verdict::Skipped | Verdict::NotRun))
    {
        let what = if c.verdict == Verdict::Passed {
            c.title.clone()
        } else {
            let mut t = c.title.clone();
            if !c.messages.is_empty() {
                t.push_str("<br>");
                t.push_str(
                    &c.messages
                        .iter()
                        .map(|m| md_escape(m))
                        .collect::<Vec<_>>()
                        .join("<br>"),
                );
            }
            t
        };
        let conf = c
            .confidence
            .as_ref()
            .map(|k| format!("catch {}, clean {}", k.catch.text(), k.clean.text()))
            .unwrap_or_else(|| "no runs".to_string());
        out.push_str(&format!(
            "| {} | `{}` | {} | {} | {} ms | {} |\n",
            c.label(),
            c.id,
            c.tier.letter(),
            what,
            c.elapsed_ms,
            conf
        ));
    }
    let repro: Vec<&CheckResult> = r
        .checks
        .iter()
        .filter(|c| {
            matches!(
                c.verdict,
                Verdict::Failed | Verdict::NotChecked | Verdict::Error
            ) && !c.reproduce.is_empty()
        })
        .collect();
    if !repro.is_empty() {
        out.push_str("\n<details><summary>Reproduce by hand</summary>\n\n");
        for c in repro {
            out.push_str(&format!(
                "**`{}`** — {}\n\n```bash\n{}\n```\n\n",
                c.id,
                c.title,
                c.reproduce.join("\n")
            ));
            if let Some(fix) = &c.fix
                && !fix.text.is_empty()
            {
                out.push_str(&format!("Fix ({}): {}\n\n", fix.owner, fix.text));
            }
        }
        out.push_str("</details>\n");
    }
    if s.not_run > 0 {
        out.push_str(&format!("\n{}\n", not_run_text(&s.not_run_reasons)));
    }
    out.push_str(&format!(
        "\n{}\n",
        written_text(r.options.out_dir.as_deref())
    ));
    out.push_str("\n### Section scorecard\n\n| section | rows | seen | with a check | preflight | with runs | weakest bound | ran | passed | failed | did not run | not checked | readiness |\n|---|---|---|---|---|---|---|---|---|---|---|---|---|\n");
    for sc in &r.sections {
        out.push_str(&format!(
            "| {} | {} | {} ({}) | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} |\n",
            sc.title,
            sc.rows,
            sc.seen,
            pct((sc.rows > 0).then(|| f64::from(sc.seen) / f64::from(sc.rows))),
            sc.with_check,
            sc.preflight,
            sc.with_runs,
            sc.weakest_bound
                .map_or("no runs".to_string(), |b| format!("{:.0}%", b * 100.0)),
            sc.ran,
            sc.passed,
            sc.failed,
            sc.not_run,
            sc.not_checked,
            pct(sc.readiness)
        ));
    }
    out.push_str(&format!("\n{}\n", scorecard_legend(&r.sections)));
    out
}

/// What the evidence columns of the scorecard mean, with the count of promoted rules here.
pub(crate) fn scorecard_legend(sections: &[SectionScore]) -> String {
    let promoted: u32 = sections.iter().map(|s| s.preflight).sum();
    let checks: u32 = sections.iter().map(|s| s.with_check).sum();
    format!(
        "runs: the rules of the section with a run in the ledger on this engine; weakest: the lowest 95% lower bound among them, and no runs when none has one; preflight: the rules promoted to preflight checks, whose failure is exit 1. A rule is promoted at a bound of 80%, which is 14 correct runs of 14 on the engine; every other rule is a probe. Promoted here: {promoted} of {checks} checks."
    )
}

fn md_escape(s: &str) -> String {
    s.replace('|', "\\|").replace('\n', " ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn check(
        id: &str,
        verdict: Verdict,
        level: Level,
        severity: Severity,
        counts: bool,
    ) -> CheckResult {
        CheckResult {
            id: id.into(),
            title: format!("title of {id}"),
            section: "compute".into(),
            tier: Tier::Static,
            level,
            severity,
            status: Status::Seen,
            verdict,
            messages: if verdict == Verdict::Failed {
                vec!["why".into()]
            } else {
                vec![]
            },
            elapsed_ms: 1,
            ran: None,
            answer: None,
            reproduce: vec![],
            fix: None,
            evidence: vec![],
            confidence: None,
            counts_as_failure: counts,
        }
    }

    fn options() -> Options {
        Options {
            tiers: vec![Tier::Static],
            strict: false,
            ready: None,
            out_dir: None,
        }
    }

    /// One section that holds every check, for the tests of the summary.
    fn sections(checks: &[CheckResult]) -> Vec<SectionScore> {
        vec![SectionScore {
            section: "compute".into(),
            title: "Compute".into(),
            rows: checks.len() as u32,
            seen: 0,
            with_check: 0,
            preflight: 0,
            with_runs: 0,
            weakest_bound: None,
            ran: checks
                .iter()
                .filter(|c| matches!(c.verdict, Verdict::Passed | Verdict::Failed))
                .count() as u32,
            passed: checks
                .iter()
                .filter(|c| c.verdict == Verdict::Passed)
                .count() as u32,
            failed: checks
                .iter()
                .filter(|c| c.verdict == Verdict::Failed)
                .count() as u32,
            not_run: checks
                .iter()
                .filter(|c| c.verdict == Verdict::NotRun)
                .count() as u32,
            not_checked: 0,
            readiness: None,
        }]
    }

    #[test]
    fn a_check_that_did_not_run_lowers_the_readiness_and_a_rule_that_does_not_apply_does_not() {
        let mut not_run = check("b", Verdict::NotRun, Level::Probe, Severity::Fail, false);
        not_run.messages = vec!["tier palette was not asked for".into()];
        let checks = vec![
            check("a", Verdict::Passed, Level::Probe, Severity::Fail, false),
            not_run,
            check("c", Verdict::Skipped, Level::Probe, Severity::Fail, false),
            check(
                "d",
                Verdict::NotChecked,
                Level::Probe,
                Severity::Fail,
                false,
            ),
        ];
        let s = summarize(&checks, &sections(&checks), &options());
        assert_eq!((s.ran, s.not_run, s.skipped, s.not_checked), (1, 1, 1, 1));
        assert_eq!(
            s.readiness,
            Some(0.5),
            "one of two checks that apply passed"
        );
        assert_eq!(
            s.not_run_reasons.get("tier palette was not asked for"),
            Some(&1)
        );
        let none = vec![check(
            "c",
            Verdict::Skipped,
            Level::Probe,
            Severity::Fail,
            false,
        )];
        assert_eq!(
            summarize(&none, &sections(&none), &options()).readiness,
            None
        );
    }

    #[test]
    fn a_note_that_did_not_run_is_outside_the_readiness_as_a_note_that_ran_is() {
        let mut note = check("n", Verdict::NotRun, Level::Probe, Severity::Note, false);
        note.messages = vec!["tier palette was not asked for".into()];
        let checks = vec![
            check("a", Verdict::Passed, Level::Probe, Severity::Fail, false),
            note,
        ];
        let s = summarize(&checks, &sections(&checks), &options());
        assert_eq!((s.ran, s.not_run), (1, 0));
        assert_eq!(s.readiness, Some(1.0), "the note does not count against it");
        assert!(s.not_run_reasons.is_empty());
    }

    #[test]
    fn the_reproduce_line_of_a_post_sends_the_body_the_check_sent() {
        let section = r#"
section: identity
title: I
rules:
  - id: sts-account-not-assumable
    title: t
    engines: [aws]
    tier: palette
    status: seen
    what: w
    check: {kind: palette, method: POST, path: "/v1/clouds/aws/account/validate", body: ".account.spec", expr: "true"}
  - id: palette-validate
    title: t
    engines: [aws]
    tier: palette
    status: seen
    what: w
    check: {kind: palette, method: POST, path: "/v1/spectroclusters/aws/validate", expr: "true"}
"#;
        let mut c = crate::rule::Catalog::default();
        c.push(
            crate::rule::Catalog::parse_section(section, std::path::Path::new("i.yaml")).unwrap(),
            std::path::Path::new("i.yaml"),
        )
        .unwrap();
        let subject = Subject {
            cloud: "aws".into(),
            release: None,
            source: "c.json".into(),
            name: "fred".into(),
            cluster_uid: None,
        };
        let outcome = |path: &str| Outcome {
            verdict: Verdict::Failed,
            messages: vec![],
            elapsed: Duration::from_millis(1),
            ran: Some(format!("POST {path}")),
            answer: None,
        };
        let with_body = c.rule("sts-account-not-assumable").unwrap();
        let lines = auto_reproduce(
            with_body,
            &outcome("/v1/clouds/aws/account/validate"),
            &subject,
            Some("out"),
        );
        assert_eq!(lines[0], "jq '.account.spec' out/world.json > request.json");
        assert!(lines[1].contains("--data @request.json"), "{}", lines[1]);
        assert!(
            lines[1].contains("/v1/clouds/aws/account/validate"),
            "{}",
            lines[1]
        );
        let entity = c.rule("palette-validate").unwrap();
        let lines = auto_reproduce(
            entity,
            &outcome("/v1/spectroclusters/aws/validate"),
            &subject,
            Some("out"),
        );
        assert!(lines[0].contains("--data @out/entity.json"), "{}", lines[0]);
    }

    #[test]
    fn labels_say_the_level_and_the_severity() {
        assert_eq!(
            check("a", Verdict::Passed, Level::Probe, Severity::Fail, false).label(),
            "PROBE PASS"
        );
        assert_eq!(
            check("a", Verdict::Failed, Level::Probe, Severity::Fail, false).label(),
            "PROBE FAIL"
        );
        assert_eq!(
            check(
                "a",
                Verdict::Passed,
                Level::Preflight,
                Severity::Fail,
                false
            )
            .label(),
            "PASS"
        );
        assert_eq!(
            check("a", Verdict::Failed, Level::Preflight, Severity::Fail, true).label(),
            "FAIL"
        );
        assert_eq!(
            check(
                "a",
                Verdict::Failed,
                Level::Preflight,
                Severity::Warn,
                false
            )
            .label(),
            "WARN"
        );
        assert_eq!(
            check("a", Verdict::Failed, Level::Probe, Severity::Note, false).label(),
            "NOTE"
        );
        assert_eq!(
            check(
                "a",
                Verdict::NotChecked,
                Level::Probe,
                Severity::Fail,
                false
            )
            .label(),
            "NOT CHECKED"
        );
    }

    #[test]
    fn the_exit_code_follows_the_counted_failures_then_the_errors() {
        let probe_fail = vec![
            check(
                "a",
                Verdict::Passed,
                Level::Preflight,
                Severity::Fail,
                false,
            ),
            check("b", Verdict::Failed, Level::Probe, Severity::Fail, false),
        ];
        let s = summarize(&probe_fail, &sections(&probe_fail), &options());
        assert_eq!((s.exit_code, s.probe_failed, s.failed), (0, 1, 0));
        assert!(s.verdict.starts_with("OK"), "{}", s.verdict);

        let counted = vec![check(
            "b",
            Verdict::Failed,
            Level::Preflight,
            Severity::Fail,
            true,
        )];
        let s = summarize(&counted, &sections(&counted), &options());
        assert_eq!(s.exit_code, 1);
        assert!(s.verdict.contains("b"), "{}", s.verdict);

        let error = vec![check(
            "c",
            Verdict::Error,
            Level::Probe,
            Severity::Fail,
            false,
        )];
        assert_eq!(
            summarize(&error, &sections(&error), &options()).exit_code,
            2
        );

        let mut ready = options();
        ready.ready = Some(0.9);
        let half = vec![
            check("a", Verdict::Passed, Level::Probe, Severity::Fail, false),
            check("b", Verdict::Failed, Level::Probe, Severity::Fail, false),
        ];
        let s = summarize(&half, &sections(&half), &ready);
        assert_eq!(s.exit_code, 1);
        assert!(
            s.verdict.starts_with("NOT READY: readiness 50%"),
            "{}",
            s.verdict
        );
    }

    #[test]
    fn the_terminal_and_the_markdown_carry_each_result() {
        let outcome = |v: Verdict| Outcome {
            verdict: v,
            messages: vec!["pool worker has size 4".into()],
            elapsed: Duration::from_millis(3),
            ran: None,
            answer: None,
        };
        let section = r#"
section: compute
title: Compute and pools
rules:
  - id: gke-pool-multiple-3
    title: A GKE worker pool size is a multiple of 3
    engines: [gke]
    tier: static
    status: seen
    what: w
    check: {kind: static, expr: "true"}
    reproduce: ["echo {{ .cloud }}"]
  - id: gke-no-gpu
    title: GPU types are not supported on GKE
    engines: [gke]
    tier: static
    status: source
    what: w
"#;
        let mut catalog = Catalog::default();
        catalog
            .push(
                Catalog::parse_section(section, std::path::Path::new("c.yaml")).unwrap(),
                std::path::Path::new("c.yaml"),
            )
            .unwrap();
        let rules: Vec<&Rule> = catalog.rules().collect();
        let world = serde_json::json!({"cloud": "gke"});
        let report = assemble(
            Assembly {
                catalog: &catalog,
                outcomes: vec![
                    (rules[0], outcome(Verdict::Failed)),
                    (rules[1], Outcome::not_run(Verdict::NotChecked, "no check")),
                ],
                confidence: BTreeMap::new(),
                world: &world,
                subject: Subject {
                    cloud: "gke".into(),
                    release: Some("4.10".into()),
                    source: "c.json".into(),
                    name: "fred".into(),
                    cluster_uid: None,
                },
                options: Options {
                    out_dir: Some("runs/x".into()),
                    ..options()
                },
                generated_at: "2026-09-30T00:00:00Z".into(),
            },
            &Jq::new(),
        );
        assert_eq!(
            report.checks[0].reproduce[0], "echo gke",
            "the template is filled"
        );
        assert!(
            report.checks[0].reproduce[1].starts_with("jq 'true' runs/x/world.json"),
            "{:?}",
            report.checks[0].reproduce
        );
        assert_eq!(report.summary.exit_code, 0, "a probe");
        assert_eq!(report.sections[0].not_checked, 1);
        let text = terminal(&report, false);
        assert!(text.contains("PROBE FAIL    gke-pool-multiple-3"), "{text}");
        assert!(text.contains("pool worker has size 4"), "{text}");
        assert!(text.contains("NOT CHECKED   gke-no-gpu"), "{text}");
        assert!(text.contains("Section scorecard"), "{text}");
        let md = markdown(&report);
        assert!(
            md.contains("| PROBE FAIL | `gke-pool-multiple-3` | S |"),
            "{md}"
        );
        assert!(md.contains("Reproduce by hand"), "{md}");
        let json: Value =
            serde_json::from_str(&render(&report, Format::Json, true, false)).unwrap();
        assert_eq!(json["schema"], REPORT_SCHEMA);
        assert_eq!(json["checks"][0]["verdict"], "failed");
    }
}
