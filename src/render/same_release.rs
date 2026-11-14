//! The lines of `same-release`: the word and the detail of each pair, and the verdict of the
//! chain.

use std::collections::BTreeSet;

use super::changes::status_word;
use super::list::side_projects;
use super::notes::{macros_changed_note, macros_not_compared_note, masked_sentence};
use super::{count_of, short};
use crate::diff::{Diff, Status};
use crate::ops::{Chain, Pair, PairStatus, RecordStatus};

/// The status word of a pair, as the text reports print it.
pub(crate) fn pair_word(status: PairStatus) -> &'static str {
    match status {
        PairStatus::Identical => "IDENTICAL",
        PairStatus::SameContent => "SAME CONTENT",
        PairStatus::RegistryMove => "REGISTRY MOVE",
        PairStatus::Renumber => "RENUMBER",
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
        pair_word(PairStatus::Identical)
    } else if d.same_content {
        pair_word(PairStatus::SameContent)
    } else {
        status_word(Status::Changed)
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
        PairStatus::Renumber if p.packs_repinned > 0 => format!(
            "{} renumbered, {} re-pinned, same configuration",
            count_of(p.renumbers.len(), "pack"),
            count_of(p.packs_repinned, "pack")
        ),
        PairStatus::Renumber => format!(
            "{} renumbered, same configuration",
            count_of(p.renumbers.len(), "pack")
        ),
        PairStatus::Different | PairStatus::UnknownUpstream => {
            format!("root {} -> {}", short(&p.root_a), short(&p.root_b))
        }
    }
}

/// The fact of a renumber whose two archives hold the same files, but for the version in
/// `pack.json` or in `Chart.yaml`.
const ARCHIVE_SAME: &str = "archive: same files";

/// The fact of a renumber whose two archives differ in another file. A reason names the file.
const ARCHIVE_CHANGED: &str = "archive: other files";

/// The fact of a renumber whose file in a download is no archive, or is not in the download. A
/// reason names the version and what the file holds.
const ARCHIVE_NOT_READABLE: &str = "archive: not readable";

/// The fact of a renumber whose two archives strata did not compare.
const ARCHIVE_NOT_COMPARED: &str = "archive: not compared";

/// `project <a> -> <b>`: the projects of the two records of the pair at `index` of `c`, when
/// the two differ. See [`side_projects`]. The pair at `index` compares the record at `index`
/// with the record after it.
pub(crate) fn pair_projects(c: &Chain, index: usize) -> Option<String> {
    let (a, b) = (c.records.get(index)?, c.records.get(index + 1)?);
    side_projects(&a.project_uid, &b.project_uid).map(|(a, b)| format!("project {a} -> {b}"))
}

/// One line for each renumber of a pair: `<name>  <old> -> <new>`, with its facts in brackets.
///
/// The facts are a move from a pre-release to its final version, and what strata found in the two
/// archives. That is the same files, another file, a file that is no archive, or no compare. See
/// [`Pair::archives_compared`], [`Pair::archives_changed`], [`Pair::archives_not_readable`], and
/// [`Pair::archives_not_compared`].
pub(crate) fn renumber_lines(p: &Pair) -> Vec<String> {
    let compared: BTreeSet<&str> = p.archives_compared.iter().map(String::as_str).collect();
    let changed: BTreeSet<&str> = p.archives_changed.iter().map(|c| c.key.as_str()).collect();
    let not_readable: BTreeSet<&str> = p
        .archives_not_readable
        .iter()
        .map(|n| n.key.as_str())
        .collect();
    let not_compared: BTreeSet<&str> = p.archives_not_compared.iter().map(String::as_str).collect();
    p.renumbers
        .iter()
        .map(|r| {
            let mut facts = Vec::new();
            if r.pre_release_to_final {
                facts.push("pre-release to final");
            }
            if compared.contains(r.key.as_str()) {
                facts.push(ARCHIVE_SAME);
            } else if changed.contains(r.key.as_str()) {
                facts.push(ARCHIVE_CHANGED);
            } else if not_readable.contains(r.key.as_str()) {
                facts.push(ARCHIVE_NOT_READABLE);
            } else if not_compared.contains(r.key.as_str()) {
                facts.push(ARCHIVE_NOT_COMPARED);
            }
            let line = format!("{}  {} -> {}", r.name, r.old, r.new);
            if facts.is_empty() {
                line
            } else {
                format!("{line} ({})", facts.join("; "))
            }
        })
        .collect()
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
