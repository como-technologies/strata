//! The lines of `snapshot --validate` on standard error, beside the record: the answer of Palette.

use super::palette_line;
use crate::model::validation::{ProfileValidation, answer_lines};
use crate::snapshot::{Snapshot, Template};

/// One line for each error that Palette gives about the record, such as `palette    refuses:
/// PackDependencyNotFound cni: <message>`. Empty when Palette accepts it, or strata did not ask.
///
/// The lines come before the error of the refusal, which is the last line.
pub fn refusal_lines(snap: &Snapshot) -> Vec<String> {
    match &snap.source.palette_validation {
        Some(v) if !v.accepts() => answer_lines(v.errors())
            .iter()
            .map(|line| palette_line(line))
            .collect(),
        _ => Vec::new(),
    }
}

/// The sentence that says that Palette accepts the record, such as `Palette accepts the packs of
/// 'aws-stack@1.1.1' as an update of itself`. `None` when Palette refuses it, or strata did not ask.
///
/// It is the last line, after the record. So a record on standard output does not hide it on a
/// terminal. It names what Palette checks: the name, the version, and the packs of a candidate, or
/// the packs of a version as an update of itself.
pub fn acceptance_sentence(snap: &Snapshot) -> Option<String> {
    let source = &snap.source;
    if !source
        .palette_validation
        .as_ref()
        .is_some_and(ProfileValidation::accepts)
    {
        return None;
    }
    let reference = &source.reference;
    Some(if source.template == Template::Candidate {
        format!("Palette accepts the name, the version, and the packs of '{reference}'")
    } else {
        format!("Palette accepts the packs of '{reference}' as an update of itself")
    })
}
