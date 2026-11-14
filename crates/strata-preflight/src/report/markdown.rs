//! The markdown form of a report: a pull request comment.

use strata::text::{markdown_cell, plural};

use super::scorecard::{bound_text, not_run_text, scorecard_legend, written_text};
use super::{CheckResult, FAILED_REQUESTS, FailedRequest, Report, SectionScore, pct};
use crate::probe::Verdict;
use strata::render::count_of;

/// The markdown form: a pull request comment.
pub fn markdown(r: &Report) -> String {
    let mut out = String::new();
    let s = &r.summary;
    let probes = count_of(s.probe_failed as usize, "probe");
    let warnings = count_of(s.warned as usize, "warning");
    let checks = plural(s.applies() as usize, "check", "checks");
    out.push_str(&format!(
        "## strata-preflight: {} cluster `{}`\n\n",
        r.subject.cloud,
        r.subject.name_or_dash()
    ));
    out.push_str(&format!(
        "**{}** — source `{}`, Palette {}, tiers {}. {} ran: {} passed, {} failed, {probes} failed, {warnings}; {} did not run, {} not checked; readiness {}: {} of {} {checks} passed.\n\n",
        s.verdict,
        r.subject.source,
        r.subject.release_text(),
        r.options.tiers_text(),
        s.ran, s.passed, s.failed, s.not_run, s.not_checked, r.options.readiness_text(&format!("**{}**", pct(s.readiness))), s.passed, s.applies()
    ));
    out.push_str(
        "| result | rule | tier | what | time | confidence |\n|---|---|---|---|---|---|\n",
    );
    for c in r.checks.iter().filter(|c| c.is_listed()) {
        out.push_str(&check_row(c));
    }
    out.push_str(&failed_request_section(&r.failed_requests()));
    out.push_str(&reproduce_section(&r.checks));
    if s.not_run > 0 {
        out.push_str(&format!(
            "\n{}\n",
            markdown_cell(&not_run_text(&s.not_run_reasons))
        ));
    }
    out.push_str(&format!("\n{}\n", written_text(&r.options)));
    out.push_str(&scorecard_table(&r.sections));
    out
}

/// The row of one check in the table: result, rule, tier, title, time, and confidence. A check that
/// did not pass shows its messages with the title. A check that a request stopped shows one line in
/// their place, because [`failed_request_section`] shows the error once.
fn check_row(c: &CheckResult) -> String {
    let stopped = c.stopped_text();
    let lines: Vec<&str> = match &stopped {
        Some(stopped) => vec![stopped],
        None if c.verdict == Verdict::Passed => Vec::new(),
        None => c.messages.iter().map(String::as_str).collect(),
    };
    let mut what = c.title.clone();
    for line in lines {
        what.push_str("<br>");
        what.push_str(&markdown_cell(line));
    }
    format!(
        "| {} | `{}` | {} | {} | {} ms | {} |\n",
        c.label(),
        c.id,
        c.tier.letter(),
        what,
        c.elapsed_ms,
        c.confidence_cell()
    )
}

/// Each request that failed, once: the request with the checks that it stopped, the error, and
/// the lines to run by hand. The text is empty when no request failed.
fn failed_request_section(failed: &[FailedRequest<'_>]) -> String {
    if failed.is_empty() {
        return String::new();
    }
    let mut out = format!("\n### {FAILED_REQUESTS}\n\n");
    for f in failed {
        out.push_str(&format!(
            "`{}` {}.\n\n```text\n{}\n```\n\n",
            f.request,
            f.stopped_text(),
            f.messages.join("\n")
        ));
        let reproduce = f.reproduce();
        if !reproduce.is_empty() {
            out.push_str(&format!("```bash\n{}\n```\n\n", reproduce.join("\n")));
        }
    }
    out
}

/// The lines to run by hand and the fix of each check that shows them: see
/// [`CheckResult::shown_reproduce`] and [`CheckResult::shown_fix`]. The text is empty when no check
/// shows one.
fn reproduce_section(checks: &[CheckResult]) -> String {
    let shown: Vec<&CheckResult> = checks
        .iter()
        .filter(|c| !c.shown_reproduce().is_empty() || c.shown_fix().is_some())
        .collect();
    if shown.is_empty() {
        return String::new();
    }
    let mut out = String::from("\n<details><summary>Reproduce by hand and fix</summary>\n\n");
    for c in shown {
        out.push_str(&format!("**`{}`** — {}\n\n", c.id, c.title));
        if !c.shown_reproduce().is_empty() {
            out.push_str(&format!(
                "```bash\n{}\n```\n\n",
                c.shown_reproduce().join("\n")
            ));
        }
        if let Some(fix) = c.shown_fix() {
            out.push_str(&format!("Fix: {}\n\n", fix.line()));
        }
    }
    out.push_str("</details>\n");
    out
}

/// The section scorecard: a table with one row for each section, then its legend.
fn scorecard_table(sections: &[SectionScore]) -> String {
    let mut out = String::from(
        "\n### Section scorecard\n\n| section | rows | seen | with a check | preflight | with runs | weakest bound | ran | passed | failed | probes failed | warnings | did not run | not checked | readiness |\n|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|\n",
    );
    for sc in sections {
        out.push_str(&format!(
            "| {} | {} | {} ({}) | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} |\n",
            sc.title,
            sc.rows,
            sc.seen,
            pct((sc.rows > 0).then(|| f64::from(sc.seen) / f64::from(sc.rows))),
            sc.with_check,
            sc.preflight,
            sc.with_runs,
            bound_text(sc.weakest_bound),
            sc.ran,
            sc.passed,
            sc.failed,
            sc.probe_failed,
            sc.warned,
            sc.not_run,
            sc.not_checked,
            pct(sc.readiness)
        ));
    }
    out.push_str(&format!("\n{}\n", scorecard_legend(sections)));
    out
}
