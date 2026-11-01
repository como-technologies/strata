//! The terminal form of a report.

use strata::term::color::Paint;
use strata::text::cut;

use super::scorecard::{bound_text, not_run_text, scorecard_legend, written_text};
use super::{CheckResult, Report, pct};
use crate::probe::Verdict;

/// The terminal form. `paint` gives the color, or none.
pub fn terminal(r: &Report, paint: &Paint) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "{}: {} cluster {} ({}), Palette {}, tiers {}\n",
        crate::TOOL_NAME,
        r.subject.cloud,
        r.subject.name_or_dash(),
        r.subject.source,
        r.subject.release_text(),
        r.options.tiers_text()
    ));
    for sc in &r.sections {
        let Some(section) = r.section_rows(sc) else {
            continue;
        };
        out.push_str(&format!("\n{}\n", paint.bold(&sc.title)));
        if sc.not_run > 0 {
            // one line for the checks of the section that did not run, not one line each
            out.push_str(&format!(
                "  {}  {}\n",
                paint.dim(&format!("{:<12}", Verdict::NotRun.name())),
                section.not_run_text()
            ));
        }
        for c in section.listed {
            out.push_str(&result_lines(c, paint));
        }
    }
    out.push_str(&format!("\n{}\n", paint.bold("Section scorecard")));
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
            cut(&s.title, 28),
            s.rows,
            s.seen,
            s.with_check,
            s.preflight,
            s.with_runs,
            bound_text(s.weakest_bound),
            s.ran,
            s.passed,
            s.failed,
            s.not_run,
            s.not_checked,
            pct(s.readiness)
        ));
    }
    out.push_str(&format!("{}\n", paint.dim(&scorecard_legend(&r.sections))));
    let s = &r.summary;
    out.push_str(&format!(
        "\n{} rules apply; {} ran: {} passed, {} failed, {} probe(s) failed, {} warning(s), {} note(s); {} did not run, {} not checked, {} do not apply, {} error(s); readiness {}: {} of {} checks passed\n",
        s.rules, s.ran, s.passed, s.failed, s.probe_failed, s.warned, s.noted, s.not_run, s.not_checked, s.skipped, s.errors, pct(s.readiness), s.passed, s.ran + s.not_run
    ));
    if s.not_run > 0 {
        out.push_str(&format!(
            "{}\n",
            paint.dim(&not_run_text(&s.not_run_reasons))
        ));
    }
    out.push_str(&format!(
        "{}\n",
        paint.dim(&written_text(r.options.out_dir.as_deref()))
    ));
    let verdict = if s.exit_code == 0 {
        paint.green(&s.verdict)
    } else {
        paint.red(&s.verdict)
    };
    out.push_str(&format!("{verdict}\n"));
    out
}

/// The lines of one result: its row, then its messages, what it ran, its reproduce lines, and its
/// fix. The verdict decides which of them show.
fn result_lines(c: &CheckResult, paint: &Paint) -> String {
    let mut out = String::new();
    let label = format!("{:<12}", c.label());
    let conf = c
        .confidence
        .as_ref()
        .map(|k| format!("  catch {} clean {}", k.catch.text(), k.clean.text()))
        .unwrap_or_default();
    out.push_str(&format!(
        "  {}  {:<28} {:<60} {:>5} ms{conf}\n",
        c.label_kind().paint(paint, &label),
        c.id,
        cut(&c.title, 60),
        c.elapsed_ms
    ));
    if c.verdict != Verdict::Passed {
        for m in &c.messages {
            out.push_str(&format!("{:<16}{m}\n", ""));
        }
        for line in c.ran_lines() {
            out.push_str(&format!("{:<16}{} {line}\n", "", paint.dim("ran:")));
        }
    }
    if matches!(
        c.verdict,
        Verdict::Failed | Verdict::NotChecked | Verdict::Error
    ) {
        for line in &c.reproduce {
            out.push_str(&format!("{:<16}{} {line}\n", "", paint.dim("reproduce:")));
        }
        if c.verdict == Verdict::Failed
            && let Some(fix) = &c.fix
            && !fix.text.is_empty()
        {
            out.push_str(&format!(
                "{:<16}{} {} ({})\n",
                "",
                paint.dim("fix:"),
                fix.text,
                fix.owner
            ));
        }
    }
    out
}
