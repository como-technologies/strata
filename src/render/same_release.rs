//! The lines of `same-release`: the word and the detail of each pair, and the verdict of the
//! chain.

use crate::diff::Diff;
use crate::ops::{Chain, Pair, PairStatus, RecordStatus};

use super::notes::{macros_changed_note, macros_not_compared_note, masked_sentence};
use super::{count_of, short};

/// The status word of a pair, as the text reports print it.
pub(crate) fn pair_word(status: PairStatus) -> &'static str {
    match status {
        PairStatus::Identical => "IDENTICAL",
        PairStatus::SameContent => "SAME CONTENT",
        PairStatus::RegistryMove => "REGISTRY MOVE",
        PairStatus::Different => "DIFFERENT",
        PairStatus::UnknownUpstream => "NO DIGEST",
    }
}

/// The status word of a record, as the text reports print it: on the line of its verdict, and in
/// the row of the summary.
pub(crate) fn record_word(status: RecordStatus) -> &'static str {
    match status {
        RecordStatus::Unchanged => "OK",
        RecordStatus::Drifted => "DRIFT",
        RecordStatus::Unverified => "UNVERIFIED",
        RecordStatus::Error => "ERROR",
    }
}

/// The word of a diff: `IDENTICAL`, `SAME CONTENT`, or `CHANGED`.
///
/// The roots of the same content differ only by the tag name of a pack, or by the uid that
/// Palette mints.
pub(crate) fn diff_word(d: &Diff) -> &'static str {
    if d.same {
        "IDENTICAL"
    } else if d.same_content {
        "SAME CONTENT"
    } else {
        "CHANGED"
    }
}

/// The short sentence after a pair's status.
pub(crate) fn pair_detail(p: &Pair) -> String {
    match p.status {
        PairStatus::Identical => format!("root {}", short(&p.root_a)),
        PairStatus::SameContent if p.packs_retagged > 0 => format!(
            "root {} -> {}, {} pinned under another tag name, the same build",
            short(&p.root_a),
            short(&p.root_b),
            count_of(p.packs_retagged, "pack")
        ),
        PairStatus::SameContent => format!(
            "root {} -> {}, only the packUid that Palette mints differs",
            short(&p.root_a),
            short(&p.root_b)
        ),
        PairStatus::RegistryMove => {
            format!(
                "{} re-pinned, same configuration",
                count_of(p.packs_repinned, "pack")
            )
        }
        PairStatus::Different | PairStatus::UnknownUpstream => {
            format!("root {} -> {}", short(&p.root_a), short(&p.root_b))
        }
    }
}

/// The notes under a pair that do not change its status.
///
/// A note names a macro with another value on each side, a macro that strata does not compare, or
/// a pack with a masked value.
pub(crate) fn pair_notes(p: &Pair) -> Vec<String> {
    macros_changed_note(&p.macros_changed)
        .into_iter()
        .chain(macros_not_compared_note(&p.macros_not_compared))
        .chain(p.masked.iter().map(|m| masked_sentence(&m.key, &m.paths)))
        .collect()
}

/// `SAME RELEASE: 2 pairs passed`, or `NOT THE SAME RELEASE: 1 of 2 pairs failed`. One pair is
/// a pair, not `1 pairs`: the reports count in the singular for one.
pub(crate) fn chain_verdict(c: &Chain) -> String {
    if c.same {
        format!(
            "SAME RELEASE: {} passed",
            count_of(c.summary.passed, "pair")
        )
    } else {
        format!(
            "NOT THE SAME RELEASE: {} of {} failed",
            c.summary.failed,
            count_of(c.summary.pairs, "pair")
        )
    }
}
