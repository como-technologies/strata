//! The markdown form of a report: a pull request comment.

use strata::text::markdown_cell;

use super::scorecard::{bound_text, not_run_text, scorecard_legend, written_text};
use super::{CheckResult, Report, SectionScore, pct};
use crate::probe::Verdict;

/// The markdown form: a pull request comment.
pub fn markdown(r: &Report) -> String {
    let mut out = String::new();
    let s = &r.summary;
    out.push_str(&format!(
        "## strata-preflight: {} cluster `{}`\n\n",
        r.subject.cloud,
        r.subject.name_or_dash()
    ));
    out.push_str(&format!(
        "**{}** — source `{}`, Palette {}, tiers {}. {} ran: {} passed, {} failed, {} probe(s) failed; {} did not run, {} not checked; readiness **{}** ({} of {} checks passed).\n\n",
        s.verdict,
        r.subject.source,
        r.subject.release_text(),
        r.options.tiers_text(),
        s.ran, s.passed, s.failed, s.probe_failed, s.not_run, s.not_checked, pct(s.readiness), s.passed, s.ran + s.not_run
    ));
    out.push_str(
        "| result | rule | tier | what | time | confidence |\n|---|---|---|---|---|---|\n",
    );
    for c in r.checks.iter().filter(|c| c.is_listed()) {
        out.push_str(&check_row(c));
    }
    out.push_str(&reproduce_section(&r.checks));
    if s.not_run > 0 {
        out.push_str(&format!("\n{}\n", not_run_text(&s.not_run_reasons)));
    }
    out.push_str(&format!(
        "\n{}\n",
        written_text(r.options.out_dir.as_deref())
    ));
    out.push_str(&scorecard_table(&r.sections));
    out
}

/// The row of one check in the table: result, rule, tier, title, time, and confidence. A check that
/// did not pass shows its messages with the title.
fn check_row(c: &CheckResult) -> String {
    let what = if c.verdict == Verdict::Passed {
        c.title.clone()
    } else {
        let mut t = c.title.clone();
        if !c.messages.is_empty() {
            t.push_str("<br>");
            t.push_str(
                &c.messages
                    .iter()
                    .map(|m| markdown_cell(m))
                    .collect::<Vec<_>>()
                    .join("<br>"),
            );
        }
        t
    };
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
        "\n### Section scorecard\n\n| section | rows | seen | with a check | preflight | with runs | weakest bound | ran | passed | failed | did not run | not checked | readiness |\n|---|---|---|---|---|---|---|---|---|---|---|---|---|\n",
    );
    for sc in sections {
        out.push_str(&format!(
            "| {} | {} | {} ({}) | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} |\n",
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
            sc.not_run,
            sc.not_checked,
            pct(sc.readiness)
        ));
    }
    out.push_str(&format!("\n{}\n", scorecard_legend(sections)));
    out
}
