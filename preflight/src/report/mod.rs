//! The report of a run: the document `strata-preflight/1`, and its terminal, markdown, and JSON
//! forms.
//!
//! [`crate::html`] gives the HTML form. A script reads the exit status and the JSON; the text
//! forms can change in any release.

use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use strata::closed_set::ClosedSet;
use strata::closed_set::legend::legend;
use strata::term::color::Paint;
use strata::text::dash;

use crate::confidence::Confidence;
use crate::jq::Jq;
use crate::probe::outcome::ran_line;
use crate::probe::{Call, Outcome, Verdict};
use crate::rule::{Catalog, Evidence, Fix, Level, Rule, Severity, Status, Tier};

pub(crate) mod label;
mod markdown;
mod reproduce;
pub(crate) mod scorecard;
mod terminal;

pub use markdown::markdown;
pub use scorecard::scorecard;
pub use terminal::terminal;

use label::LabelKind;
use reproduce::auto_reproduce;
use scorecard::summarize;

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

impl Tool {
    /// This tool, at this version. Each document that this tool writes names it so.
    pub fn current() -> Tool {
        Tool {
            name: crate::TOOL_NAME.to_string(),
            version: crate::TOOL_VERSION.to_string(),
        }
    }
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

impl Subject {
    /// The name of the cluster, or `-` when the entity has none.
    pub fn name_or_dash(&self) -> &str {
        dash(&self.name)
    }

    /// The Palette release as each form of the report writes it after `Palette`: `release 4.10.3`,
    /// or `release not given`.
    pub fn release_text(&self) -> String {
        self.release.as_deref().map_or_else(
            || "release not given".to_string(),
            |x| format!("release {x}"),
        )
    }
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

impl Options {
    /// The tiers that ran, as each form of the report writes them after `tiers`: `static,
    /// palette`. `, strict` follows them when a failed probe counts as a failed check.
    pub fn tiers_text(&self) -> String {
        let tiers: Vec<&str> = self.tiers.iter().map(|t| t.as_str()).collect();
        let strict = if self.strict { ", strict" } else { "" };
        format!("{}{strict}", tiers.join(", "))
    }
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
    /// The requests of a check with `each`, one for each item, in the order of the list.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub requests: Vec<Call>,
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
        self.label_kind().word()
    }

    /// What the label of the result is. It gives the word, the color, and the CSS class.
    pub(crate) fn label_kind(&self) -> LabelKind {
        LabelKind::of(self.verdict, self.level, self.severity)
    }

    /// True when a form of the report lists the result in its section. A rule that does not
    /// apply, and a check that did not run, have no line of their own there.
    pub(crate) fn is_listed(&self) -> bool {
        !matches!(self.verdict, Verdict::Skipped | Verdict::NotRun)
    }

    /// One line for the request or the command of the check, and one for each request of a check
    /// with `each`, each with its answer: `GET /v1/x -> HTTP 200`.
    pub fn ran_lines(&self) -> Vec<String> {
        self.ran
            .iter()
            .map(|ran| ran_line(ran, self.answer.as_deref()))
            .chain(self.requests.iter().map(Call::line))
            .collect()
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
        Legend {
            verdicts: legend::<Verdict>(),
            levels: legend::<Level>(),
            tiers: legend::<Tier>(),
            exit: [
                ("0", "each counted check passed"),
                (
                    "1",
                    "a counted check failed, or the readiness is under --ready",
                ),
                ("2", "strata-preflight could not answer"),
            ]
            .into_iter()
            .map(|(code, meaning)| (code.to_string(), meaning.to_string()))
            .collect(),
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

    /// The results of one section, for a form that lists them by section. None when the form
    /// shows nothing for the section: no result to list and no check that did not run.
    pub(crate) fn section_rows(&self, section: &SectionScore) -> Option<SectionRows<'_>> {
        let rows: Vec<&CheckResult> = self
            .checks
            .iter()
            .filter(|c| c.section == section.section)
            .collect();
        let listed: Vec<&CheckResult> = rows.iter().copied().filter(|c| c.is_listed()).collect();
        (!listed.is_empty() || section.not_run > 0).then_some(SectionRows { rows, listed })
    }
}

/// The results of one section of a report.
pub(crate) struct SectionRows<'a> {
    /// Each result of the section.
    pub(crate) rows: Vec<&'a CheckResult>,
    /// The results that a form lists: see [`CheckResult::is_listed`].
    pub(crate) listed: Vec<&'a CheckResult>,
}

impl SectionRows<'_> {
    /// One line for the checks of the section that did not run, with the reason of each count.
    pub(crate) fn not_run_text(&self) -> String {
        scorecard::not_run_text(&scorecard::not_run_reasons(self.rows.iter().copied()))
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

/// True when a failed verdict is a failure that counts.
///
/// It counts when the rule has severity `fail` and has the preflight level, or when `--strict` is
/// on. It is exit `1`, and it ends a watch. A warning or a note never counts.
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
                jq.render(&rule.id, &line, a.world, None)
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
            ran: outcome.ran.text(),
            answer: outcome.ran.answer().map(str::to_string),
            requests: outcome.ran.requests().to_vec(),
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
        tool: Tool::current(),
        generated_at: a.generated_at,
        subject: a.subject,
        options: a.options,
        checks,
        sections,
        summary,
        legend: Legend::new(),
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

/// Render a report. `paint` colors the terminal form alone.
pub fn render(report: &Report, format: Format, compact: bool, paint: &Paint) -> String {
    match format {
        Format::Terminal => terminal(report, paint),
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

/// `share`, from 0 to 1, as a whole percent, such as `75%`.
pub(crate) fn percent(share: f64) -> String {
    format!("{:.0}%", share * 100.0)
}

/// A share as a whole percent, or `-` when there is none, for a cell of a table.
pub(crate) fn pct(v: Option<f64>) -> String {
    v.map_or_else(|| "-".to_string(), percent)
}

#[cfg(test)]
mod tests;
