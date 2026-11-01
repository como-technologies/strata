//! The label of a check: its word, its color, and its CSS class. One decision gives the three, so
//! the terminal, markdown, and HTML forms agree on what a label is.

use strata::term::color::Paint;

use crate::probe::Verdict;
use crate::rule::{Level, Severity};

/// What a result shows as its label.
///
/// A failed check is `Warn` or `Note` by its severity. Else a probe has its own label beside the
/// check of the preflight level, because a failed probe does not change the exit code.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LabelKind {
    /// A preflight check that passed.
    Pass,
    /// A probe that passed.
    ProbePass,
    /// A preflight check that failed.
    Fail,
    /// A probe that failed.
    ProbeFail,
    /// A failed check of severity `warn`.
    Warn,
    /// A failed check of severity `note`.
    Note,
    /// A rule with no check that strata-preflight can run here.
    NotChecked,
    /// A check that applies here and did not run.
    NotRun,
    /// A rule that does not apply to this run.
    Skip,
    /// A check that could not answer.
    Error,
}

impl LabelKind {
    /// The label of a result with `verdict`, from the `level` and the `severity` of its rule.
    pub(crate) fn of(verdict: Verdict, level: Level, severity: Severity) -> LabelKind {
        match (verdict, level, severity) {
            (Verdict::Failed, _, Severity::Note) => LabelKind::Note,
            (Verdict::Failed, _, Severity::Warn) => LabelKind::Warn,
            (Verdict::Passed, Level::Probe, _) => LabelKind::ProbePass,
            (Verdict::Passed, Level::Preflight, _) => LabelKind::Pass,
            (Verdict::Failed, Level::Probe, _) => LabelKind::ProbeFail,
            (Verdict::Failed, Level::Preflight, _) => LabelKind::Fail,
            (Verdict::NotChecked, _, _) => LabelKind::NotChecked,
            (Verdict::NotRun, _, _) => LabelKind::NotRun,
            (Verdict::Skipped, _, _) => LabelKind::Skip,
            (Verdict::Error, _, _) => LabelKind::Error,
        }
    }

    /// The verdict whose word the label carries.
    fn verdict(self) -> Verdict {
        match self {
            LabelKind::Pass | LabelKind::ProbePass => Verdict::Passed,
            LabelKind::Fail | LabelKind::ProbeFail | LabelKind::Warn | LabelKind::Note => {
                Verdict::Failed
            }
            LabelKind::NotChecked => Verdict::NotChecked,
            LabelKind::NotRun => Verdict::NotRun,
            LabelKind::Skip => Verdict::Skipped,
            LabelKind::Error => Verdict::Error,
        }
    }

    /// The word of the label: `PASS`, `FAIL`, `PROBE PASS`, `PROBE FAIL`, `WARN`, `NOTE`,
    /// `NOT CHECKED`, `NOT RUN`, `SKIP`, or `ERROR`.
    pub(crate) fn word(self) -> String {
        match self {
            LabelKind::ProbePass | LabelKind::ProbeFail => {
                format!("PROBE {}", self.verdict().name())
            }
            LabelKind::Warn => "WARN".to_string(),
            LabelKind::Note => "NOTE".to_string(),
            _ => self.verdict().name().to_string(),
        }
    }

    /// `text` in the color of the label: green for a pass, red for a counted failure, magenta
    /// for a failed probe, yellow for a warning, cyan for a note, dim for what did not run.
    pub(crate) fn paint(self, paint: &Paint, text: &str) -> String {
        match self {
            LabelKind::Pass | LabelKind::ProbePass => paint.green(text),
            LabelKind::Fail | LabelKind::Error => paint.red(text),
            LabelKind::ProbeFail => paint.magenta(text),
            LabelKind::Warn | LabelKind::NotChecked => paint.yellow(text),
            LabelKind::Note => paint.cyan(text),
            LabelKind::NotRun | LabelKind::Skip => paint.dim(text),
        }
    }

    /// The CSS class of the label in the HTML form.
    pub(crate) fn class(self) -> &'static str {
        match self {
            LabelKind::Pass | LabelKind::ProbePass => "pass",
            LabelKind::Fail => "fail",
            LabelKind::ProbeFail => "probe-fail",
            LabelKind::Warn => "warn",
            LabelKind::Note => "note",
            LabelKind::NotChecked => "not-checked",
            LabelKind::NotRun | LabelKind::Skip => "skip",
            LabelKind::Error => "error",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use strata::term::color::{ColorMode, Stream};

    const VERDICTS: [Verdict; 6] = [
        Verdict::Passed,
        Verdict::Failed,
        Verdict::NotChecked,
        Verdict::NotRun,
        Verdict::Skipped,
        Verdict::Error,
    ];

    #[test]
    fn a_failed_check_is_a_warning_or_a_note_by_its_severity_and_a_probe_by_its_level() {
        let word = |verdict, level, severity| LabelKind::of(verdict, level, severity).word();
        assert_eq!(
            word(Verdict::Failed, Level::Preflight, Severity::Fail),
            "FAIL"
        );
        assert_eq!(
            word(Verdict::Failed, Level::Probe, Severity::Fail),
            "PROBE FAIL"
        );
        assert_eq!(
            word(Verdict::Failed, Level::Preflight, Severity::Warn),
            "WARN"
        );
        assert_eq!(word(Verdict::Failed, Level::Probe, Severity::Note), "NOTE");
        assert_eq!(
            word(Verdict::Passed, Level::Preflight, Severity::Fail),
            "PASS"
        );
        assert_eq!(
            word(Verdict::Passed, Level::Probe, Severity::Warn),
            "PROBE PASS"
        );
    }

    #[test]
    fn a_result_that_did_not_fail_has_the_word_of_its_verdict_at_each_level() {
        for verdict in [
            Verdict::NotChecked,
            Verdict::NotRun,
            Verdict::Skipped,
            Verdict::Error,
        ] {
            for level in [Level::Probe, Level::Preflight] {
                for severity in [Severity::Fail, Severity::Warn, Severity::Note] {
                    assert_eq!(
                        LabelKind::of(verdict, level, severity).word(),
                        verdict.name()
                    );
                }
            }
        }
    }

    #[test]
    fn each_label_has_one_color_and_one_class_for_each_verdict_level_and_severity() {
        let on = Paint::resolve(ColorMode::Always, Stream::Stdout);
        let mut seen = Vec::new();
        for verdict in VERDICTS {
            for level in [Level::Probe, Level::Preflight] {
                for severity in [Severity::Fail, Severity::Warn, Severity::Note] {
                    let kind = LabelKind::of(verdict, level, severity);
                    assert!(kind.paint(&on, "x").contains("\x1b["), "{kind:?}");
                    assert!(!kind.class().is_empty(), "{kind:?}");
                    seen.push(kind);
                }
            }
        }
        seen.sort_by_key(|k| *k as u8);
        seen.dedup();
        assert_eq!(seen.len(), 10, "each label is reachable");
    }

    #[test]
    fn the_colors_are_the_ones_of_the_book() {
        let on = Paint::resolve(ColorMode::Always, Stream::Stdout);
        assert_eq!(LabelKind::Pass.paint(&on, "x"), on.green("x"));
        assert_eq!(LabelKind::ProbePass.paint(&on, "x"), on.green("x"));
        assert_eq!(LabelKind::Fail.paint(&on, "x"), on.red("x"));
        assert_eq!(LabelKind::ProbeFail.paint(&on, "x"), on.magenta("x"));
        assert_eq!(LabelKind::Warn.paint(&on, "x"), on.yellow("x"));
        assert_eq!(LabelKind::NotChecked.paint(&on, "x"), on.yellow("x"));
        assert_eq!(LabelKind::Note.paint(&on, "x"), on.cyan("x"));
        assert_eq!(LabelKind::NotRun.paint(&on, "x"), on.dim("x"));
        assert_eq!(LabelKind::Error.paint(&on, "x"), on.red("x"));
    }
}
