//! The terminal form of a report.

use strata::term::color::Paint;
use strata::text::{cut, plural};

use super::scorecard::{bound_text, not_run_text, scorecard_legend, written_text};
use super::{CheckResult, FAILED_REQUESTS, FailedRequest, Report, pct};
use crate::probe::Verdict;
use strata::render::count_of;

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
    out.push_str(&failed_request_lines(&r.failed_requests(), paint));
    out.push_str(&scorecard_lines(r, paint));
    out.push_str(&summary_lines(r, paint));
    out
}

/// Each request that failed, once: the request, the error, the checks that it stopped, and the
/// lines to run by hand. Empty when no request failed.
fn failed_request_lines(failed: &[FailedRequest<'_>], paint: &Paint) -> String {
    if failed.is_empty() {
        return String::new();
    }
    let mut out = format!("\n{}\n", paint.bold(FAILED_REQUESTS));
    for f in failed {
        out.push_str(&format!(
            "  {}  {}\n",
            paint.red(&format!("{:<12}", Verdict::Error.name())),
            f.request
        ));
        for m in f.messages {
            out.push_str(&format!("{:<16}{m}\n", ""));
        }
        out.push_str(&format!("{:<16}{}\n", "", f.stopped_text()));
        for line in f.reproduce() {
            out.push_str(&format!("{:<16}{} {line}\n", "", paint.dim("reproduce:")));
        }
    }
    out
}

/// The table of the scorecard: one row for each section, then its legend.
fn scorecard_lines(r: &Report, paint: &Paint) -> String {
    let mut out = String::new();
    out.push_str(&format!("\n{}\n", paint.bold("Section scorecard")));
    out.push_str(&format!(
        "  {:<28} {:>4} {:>4} {:>5} {:>9} {:>4} {:>7} {:>4} {:>4} {:>4} {:>10} {:>4} {:>7} {:>11} {:>5}\n",
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
        "probe fail",
        "warn",
        "not run",
        "not checked",
        "ready"
    ));
    for s in &r.sections {
        out.push_str(&format!(
            "  {:<28} {:>4} {:>4} {:>5} {:>9} {:>4} {:>7} {:>4} {:>4} {:>4} {:>10} {:>4} {:>7} {:>11} {:>5}\n",
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
            s.probe_failed,
            s.warned,
            s.not_run,
            s.not_checked,
            pct(s.readiness)
        ));
    }
    out.push_str(&format!("{}\n", paint.dim(&scorecard_legend(&r.sections))));
    out
}

/// The totals of the run, the reason for the checks that did not run, where the files went, and
/// the verdict.
fn summary_lines(r: &Report, paint: &Paint) -> String {
    let mut out = String::new();
    let s = &r.summary;
    let probes = count_of(s.probe_failed as usize, "probe");
    let warnings = count_of(s.warned as usize, "warning");
    let notes = count_of(s.noted as usize, "note");
    let errors = count_of(s.errors as usize, "error");
    let rules = count_of(s.rules as usize, "rule");
    let apply = plural(s.rules as usize, "applies", "apply");
    let checks = plural(s.applies() as usize, "check", "checks");
    out.push_str(&format!(
        "\n{rules} {apply}; {} ran: {} passed, {} failed, {probes} failed, {warnings}, {notes}; {} did not run, {} not checked, {} do not apply, {errors}; readiness {}: {} of {} {checks} passed\n",
        s.ran, s.passed, s.failed, s.not_run, s.not_checked, s.skipped, r.options.readiness_text(&pct(s.readiness)), s.passed, s.applies()
    ));
    if s.not_run > 0 {
        out.push_str(&format!(
            "{}\n",
            paint.dim(&not_run_text(&s.not_run_reasons))
        ));
    }
    out.push_str(&format!("{}\n", paint.dim(&written_text(&r.options))));
    let verdict = if s.exit_code != 0 {
        paint.red(&s.verdict)
    } else if s.not_counted() {
        paint.yellow(&s.verdict)
    } else {
        paint.green(&s.verdict)
    };
    out.push_str(&format!("{verdict}\n"));
    out
}

/// The lines of one result: its row, then its messages, what it ran, its reproduce lines, and its
/// fix. The verdict decides which of them show. A check that a request stopped shows one line in
/// place of the error, because [`failed_request_lines`] shows the error once.
fn result_lines(c: &CheckResult, paint: &Paint) -> String {
    let mut out = String::new();
    let label = format!("{:<12}", c.label());
    let conf = c
        .confidence_text()
        .map(|text| format!("  {text}"))
        .unwrap_or_default();
    out.push_str(&format!(
        "  {}  {:<28} {:<60} {:>5} ms{conf}\n",
        c.label_kind().paint(paint, &label),
        c.id,
        cut(&c.title, 60),
        c.elapsed_ms
    ));
    if let Some(stopped) = c.stopped_text() {
        out.push_str(&format!("{:<16}{stopped}\n", ""));
    } else if c.verdict != Verdict::Passed {
        for m in &c.messages {
            out.push_str(&format!("{:<16}{m}\n", ""));
        }
        for line in c.ran_lines() {
            out.push_str(&format!("{:<16}{} {line}\n", "", paint.dim("ran:")));
        }
    }
    for line in c.shown_reproduce() {
        out.push_str(&format!("{:<16}{} {line}\n", "", paint.dim("reproduce:")));
    }
    if let Some(fix) = c.shown_fix() {
        out.push_str(&format!("{:<16}{} {}\n", "", paint.dim("fix:"), fix.line()));
    }
    out
}
