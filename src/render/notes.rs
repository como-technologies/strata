//! The notes of a comparison: what the root does not cover, what Palette did not give, and what
//! waits.

use super::verify::edited_in_place_note;
use crate::closed_set::{ClosedSet, Open};
use crate::diff::pack::InvalidSides;
use crate::diff::{Diff, ImageIdChange, InvalidPack, PackDiff, Side};
use crate::reference::name_version;
use crate::repave::PaletteRepaveReason;
use crate::snapshot::{PendingUpdate, Source, Tier};
use crate::text::{and_list, dash};

/// True when the two sides are releases of each other: two profile-shaped sides, or two versions
/// of one pack. A pack against a profile, or one pack against another pack, is a comparison a
/// reader can ask for, and no cluster can make that transition. The pack rows and the impact then
/// describe a move that cannot happen.
pub(crate) fn releases_of_each_other(d: &Diff) -> bool {
    match (&d.a.pack, &d.b.pack) {
        (None, None) => true,
        (Some(a), Some(b)) => a.name == b.name,
        _ => false,
    }
}

/// The note for a comparison whose two sides are not releases of each other.
fn cross_kind_note(d: &Diff) -> Option<String> {
    if releases_of_each_other(d) {
        return None;
    }
    Some(
        "the two sides are not releases of each other, so the added and removed packs, and the \
         impact, describe no change a cluster can make"
            .to_string(),
    )
}

/// One note under the heading of a diff.
pub(crate) enum DiffNote {
    /// A fact that a reader must weigh: yellow on the terminal.
    Warn(String),
    /// The roots differ in nothing that a cluster sees: green on the terminal.
    Good(String),
    /// The answer of Palette to `--repave` or to `--validate`, one line each.
    Palette(Vec<String>),
}

/// The notes under the heading of a diff, in the order that the reports print them.
pub(crate) fn diff_notes(d: &Diff) -> Vec<DiffNote> {
    let mut notes: Vec<DiffNote> = Vec::new();
    notes.extend(edited_in_place_note(d).map(DiffNote::Warn));
    notes.extend(draft_note(d).map(DiffNote::Warn));
    notes.extend(
        overwritten_in_place_notes(d)
            .into_iter()
            .map(DiffNote::Warn),
    );
    notes.extend(updates_pending_notes(d).into_iter().map(DiffNote::Warn));
    notes.extend(repave_pending_notes(d).into_iter().map(DiffNote::Warn));
    notes.extend(unread_notes(d).into_iter().map(DiffNote::Warn));
    notes.extend(later_tier_notes(d).into_iter().map(DiffNote::Warn));
    notes.extend(running_behind_notes(d).into_iter().map(DiffNote::Warn));
    notes.extend(upstream_unknown_note(d).map(DiffNote::Warn));
    notes.extend(masked_notes(d).into_iter().map(DiffNote::Warn));
    notes.extend(macros_note(d).map(DiffNote::Warn));
    notes.extend(macros_changed_note(&d.macros_changed).map(DiffNote::Warn));
    notes.extend(image_ids_changed_notes(d).into_iter().map(DiffNote::Warn));
    let repave = palette_repave_lines(d);
    if !repave.is_empty() {
        notes.push(DiffNote::Palette(repave));
    }
    notes.extend(palette_repave_note(d).map(DiffNote::Warn));
    let validation = palette_validation_lines(d);
    if !validation.is_empty() {
        notes.push(DiffNote::Palette(validation));
    }
    notes.extend(same_content_note(d).map(DiffNote::Good));
    notes.extend(variables_unavailable_note(d).map(DiffNote::Warn));
    notes.extend(cross_kind_note(d).map(DiffNote::Warn));
    notes
}

/// `pack order changed between the two profiles`, for a diff whose packs are in another order.
///
/// Palette keeps the order of the packs, so two orders are two roots.
pub(crate) fn pack_order_note(d: &Diff) -> Option<&'static str> {
    d.order_changed
        .then_some("pack order changed between the two profiles")
}

/// True when a side can have profile variables. A registry pack cannot, and no request asks for
/// any, so `variables unchanged` there would state a comparison that nothing compared.
pub(crate) fn lists_variables(s: &Source) -> bool {
    s.pack.is_none()
}

/// True when the reports can state anything about the variables of a comparison.
pub(crate) fn variables_comparable(d: &Diff) -> bool {
    lists_variables(&d.a) || lists_variables(&d.b)
}

/// A note for a side whose variables endpoint Palette did not serve. That side's root is the root
/// of a version with no variables, so it cannot be compared with a side read from a Palette that
/// serves the endpoint. Nothing else in the report tells the two apart.
fn variables_unavailable_note(d: &Diff) -> Option<String> {
    let sides = match (d.a.variables_unavailable, d.b.variables_unavailable) {
        (true, true) => "both sides",
        (true, false) => "side A",
        (false, true) => "side B",
        (false, false) => return None,
    };
    Some(format!(
        "{sides}: Palette did not serve the variables endpoint, so the root is the root of a \
         version with no variables"
    ))
}

/// `uid u is name@1.2.0 on one side and name@1.3.0 on the other: the version was overwritten in
/// place`, one per profile uid the two sides name under different versions. A verify whose verdict
/// already says the recorded version was overwritten gets none.
fn overwritten_in_place_notes(d: &Diff) -> Vec<String> {
    if d.verify
        .as_ref()
        .is_some_and(|v| v.overwritten_by.is_some() || v.renamed_to.is_some())
    {
        return Vec::new();
    }
    d.overwritten_in_place
        .iter()
        .map(|o| {
            let name_b = if o.name_b.is_empty() { &o.name } else { &o.name_b };
            format!(
                "uid {} is {} on one side and {} on the other: the version was overwritten in place",
                o.uid,
                name_version(&o.name, &o.version_a),
                name_version(name_b, &o.version_b)
            )
        })
        .collect()
}

/// `the roots differ only by the tag name a pack is pinned under: the same pack build`, or by the
/// packUid that Palette mints for a manifest pack or for a pack from a registry that it does not
/// sync, for a diff whose roots differ in nothing else.
fn same_content_note(d: &Diff) -> Option<String> {
    if d.same || !d.same_content {
        return None;
    }
    let retagged = d.packs.iter().any(|p| p.tag_renamed);
    let minted = d.packs.iter().any(|p| p.minted_uid_only);
    Some(match (retagged, minted) {
        (true, false) => {
            "the roots differ only by the tag name a pack is pinned under: the same pack build"
        }
        (true, true) => "the roots differ only by the tag name of a pack and the packUid that Palette mints: the same content",
        _ => "the roots differ only by the packUid that Palette mints: the same content",
    }.to_string())
}

/// `tag 24.04 -> LTS__24.04`, for a pack whose identity moved only by its tag name.
pub(crate) fn tag_rename(pd: &PackDiff) -> Option<String> {
    match (pd.tag_renamed, &pd.a, &pd.b) {
        (true, Some(a), Some(b)) => Some(format!("tag {} -> {}", a.tag, b.tag)),
        _ => None,
    }
}

/// The note for the user macros the content uses.
///
/// The note is `the content uses macro region: the root covers its name and not its value, so an
/// edit of the macro does not show`. None when the content uses no macro.
fn macros_note(d: &Diff) -> Option<String> {
    let (noun, pron) = match d.macros.len() {
        0 => return None,
        1 => ("macro", "its name and not its value"),
        _ => ("macros", "their names and not their values"),
    };
    Some(format!(
        "the content uses {noun} {}: the root covers {pron}, so an edit of a macro does not show",
        d.macros.join(", ")
    ))
}

/// Why a record with the recorded content drifted: `macro region has another value now`, and
/// `pack os/ubuntu-aws names image ami-2 now, not ami-1`. None when no macro value and no image
/// moved.
pub(super) fn outside_root_sentence(macros: &[String], images: &[ImageIdChange]) -> Option<String> {
    let mut parts = match macros {
        [] => Vec::new(),
        [one] => vec![format!("macro {one} has another value now")],
        many => vec![format!("macros {} have other values now", many.join(", "))],
    };
    parts.extend(
        images
            .iter()
            .map(|c| format!("pack {} names image {} now, not {}", c.key, c.b, c.a)),
    );
    (!parts.is_empty()).then(|| and_list(&parts))
}

/// The note for two records whose macro values differ: `macros` names each macro, as
/// [`Diff::macros_changed`] does. None when it is empty.
pub(super) fn macros_changed_note(macros: &[String]) -> Option<String> {
    let names = match macros {
        [] => return None,
        [one] => format!("macro {one} has another value on each side"),
        many => format!("macros {} have other values on each side", many.join(", ")),
    };
    Some(format!(
        "{names}: Palette applies other content, also when the roots are equal"
    ))
}

/// The note for a pair of records that use macros, when a record holds no digests of the macro
/// values: `the content uses macro region: a record holds no digest of its value, so strata does
/// not compare the value`. None when `macros` is empty.
pub(crate) fn macros_not_compared_note(macros: &[String]) -> Option<String> {
    let (names, digest, value) = match macros {
        [] => return None,
        [one] => (format!("macro {one}"), "digest of its value", "the value"),
        many => (
            format!("macros {}", many.join(", ")),
            "digests of their values",
            "the values",
        ),
    };
    Some(format!(
        "the content uses {names}: a record holds no {digest}, so strata does not compare {value}"
    ))
}

/// One note for each pack whose two sides name another machine image: `pack os/ubuntu-aws names
/// image ami-1 on one side and ami-2 on the other: the root does not cover the annotations`.
fn image_ids_changed_notes(d: &Diff) -> Vec<String> {
    d.image_ids_changed
        .iter()
        .map(ImageIdChange::sentence)
        .collect()
}

/// The mark on the line of a pack that Palette marks invalid: `INVALID on side B: <reason>`, or
/// `INVALID on both sides: <reason>` with the reason of side B. None when neither side is.
pub(crate) fn invalid_mark(pd: &PackDiff) -> Option<String> {
    let (sides, reason) = match pd.invalid_sides()? {
        InvalidSides::A(r) => ("side A", r),
        InvalidSides::B(r) => ("side B", r),
        InvalidSides::Both(r) => ("both sides", r),
    };
    Some(if reason.is_empty() {
        format!("INVALID on {sides}")
    } else {
        format!("INVALID on {sides}: {reason}")
    })
}

/// The start of each sentence and each warning about an invalid pack: `Palette marks pack
/// addon/hello-universe invalid`, then `when`, then ` (<reason>)` when Palette gives a reason.
pub(crate) fn marks_invalid(p: &InvalidPack, when: &str) -> String {
    let reason = Some(p.reason.as_str()).filter(|r| !r.is_empty());
    format!(
        "Palette marks pack {} invalid{when}{}",
        p.key,
        reason.map_or_else(String::new, |r| format!(" ({r})"))
    )
}

/// One sentence for each pack that Palette marks invalid now: `Palette marks pack
/// addon/hello-universe invalid now (<reason>)`.
pub(crate) fn invalid_pack_sentences(packs: &[InvalidPack]) -> Vec<String> {
    packs.iter().map(|p| marks_invalid(p, " now")).collect()
}

/// Palette's answer to `--repave`, one line for each reason: `repave: PackValuesUpdated
/// kubernetes (kubeadmconfig.postKubeadmCommands)`, or `no repave`. For a cluster in a cluster
/// template, a first line names the template that Palette answered for. Empty without the option.
fn palette_repave_lines(d: &Diff) -> Vec<String> {
    let Some(r) = &d.palette_repave else {
        return Vec::new();
    };
    let template = r.cluster_template.as_ref().map(|t| {
        format!(
            "the cluster template {} governs the cluster: Palette answers for the template, by \
             profile uid",
            t.uid
        )
    });
    let answer: Vec<String> = if !r.required {
        vec!["no repave".to_string()]
    } else if r.reasons.is_empty() {
        vec!["repave".to_string()]
    } else {
        r.reasons
            .iter()
            .map(|x| format!("repave: {}", repave_reason_text(x)))
            .collect()
    };
    template.into_iter().chain(answer).collect()
}

/// One reason of Palette for a repave: `PackValuesUpdated kubernetes
/// (kubeadmconfig.postKubeadmCommands)`.
fn repave_reason_text(x: &PaletteRepaveReason) -> String {
    let keys = if x.keys.is_empty() {
        String::new()
    } else {
        format!(" ({})", x.keys.join(", "))
    };
    format!("{} {}{keys}", x.code, dash(&x.pack))
}

/// Palette's answer to `--validate`: `accepts`, or one line for each error, `refuses:
/// PackDependencyNotFound strata-live-pack: <message>`. Empty without the option.
fn palette_validation_lines(d: &Diff) -> Vec<String> {
    d.palette_validation
        .as_ref()
        .map(|v| crate::validation::answer_lines(v.answer.errors()))
        .unwrap_or_default()
}

/// The note when the impact table and Palette disagree about a repave.
fn palette_repave_note(d: &Diff) -> Option<String> {
    let r = d.palette_repave.as_ref().filter(|r| !r.agrees)?;
    Some(format!(
        "the impact table says {}, and Palette says {} for {}: the table does not describe its \
         upgrade engine",
        d.summary.max_impact.as_str(),
        if r.required { "repave" } else { "no repave" },
        if r.cluster_template.is_some() {
            "the cluster template"
        } else {
            "this cluster"
        }
    ))
}

/// The note for the packs with no upstream digest: `pack csi/x has no upstream digest: Palette
/// gives none, so a change of its archive does not show`. None when every pack has one.
fn upstream_unknown_note(d: &Diff) -> Option<String> {
    (!d.upstream_unknown.is_empty()).then(|| {
        format!(
            "{}: Palette gives none, {ARCHIVE_CHANGE_HIDDEN}",
            no_upstream_digest(&d.upstream_unknown)
        )
    })
}

/// `pack csi/x has no upstream digest`, or `packs csi/x, csi/y have no upstream digest`.
///
/// Each sentence about the packs whose archive a root cannot show starts with it.
pub(crate) fn no_upstream_digest(packs: &[String]) -> String {
    let (noun, verb) = if packs.len() == 1 {
        ("pack", "has")
    } else {
        ("packs", "have")
    };
    format!("{noun} {} {verb} no upstream digest", packs.join(", "))
}

/// What a pack with no upstream digest hides from a record and from a diff.
pub(super) const ARCHIVE_CHANGE_HIDDEN: &str = "so a change of the archive does not show";

/// `pack addon/x has a masked value at <path>: an equal root does not show an equal secret`, for
/// the pack `key` and its masked `paths`.
pub(crate) fn masked_sentence(key: &str, paths: &[String]) -> String {
    let at = match paths {
        [one] => format!("a masked value at {one}"),
        many => format!("masked values at {}", many.join(", ")),
    };
    format!("pack {key} has {at}: an equal root does not show an equal secret")
}

/// One note for each pack with a value that Palette masks: see [`masked_sentence`].
fn masked_notes(d: &Diff) -> Vec<String> {
    d.masked_packs()
        .iter()
        .map(|m| masked_sentence(&m.key, &m.paths))
        .collect()
}

/// One note for each cluster side that Palette says is behind its profile: `cluster fred is
/// behind: an update of strata-live-addon waits, since <time>`. A side read when nothing waited,
/// or whose notifications were not read, gets none.
fn updates_pending_notes(d: &Diff) -> Vec<String> {
    [&d.a, &d.b]
        .into_iter()
        .filter_map(|side| side.cluster.as_ref())
        .filter(|c| c.updates_pending == Some(true))
        .map(|c| {
            let since = c
                .pending_updates
                .first()
                .map(|u| format!(", since {}", u.since))
                .unwrap_or_default();
            format!(
                "cluster {} is behind: Palette says an update of {} waits for it{since}",
                c.name,
                pending_profile_names(&c.pending_updates)
            )
        })
        .collect()
}

/// The names of the profiles whose updates wait for a cluster, each one time, with commas.
pub(crate) fn pending_profile_names(updates: &[PendingUpdate]) -> String {
    let mut names: Vec<&str> = updates.iter().map(|u| u.profile.as_str()).collect();
    names.dedup();
    names.join(", ")
}

/// One note for each cluster side that Palette says holds a repave for approval: `a repave of
/// cluster fred waits for approval, since <time>: PackValuesUpdated kubernetes
/// (kubeadmconfig.postKubeadmCommands)`. Two sides that say the same get one note.
fn repave_pending_notes(d: &Diff) -> Vec<String> {
    let mut notes: Vec<String> = [&d.a, &d.b]
        .into_iter()
        .filter_map(|side| side.cluster.as_ref())
        .filter(|c| c.repave_pending == Some(true))
        .map(|c| {
            let mut note = format!("a repave of cluster {} waits for approval", c.name);
            if let Some(r) = &c.pending_repave {
                if !r.since.is_empty() {
                    note.push_str(&format!(", since {}", r.since));
                }
                let reasons: Vec<String> = r.reasons.iter().map(repave_reason_text).collect();
                if !reasons.is_empty() {
                    note.push_str(&format!(": {}", reasons.join("; ")));
                }
            } else if let Some(e) = c.pending_repave_error() {
                note.push_str(&format!(" (strata could not read its reasons: {e})"));
            }
            note.push_str(". Palette refuses an update of its profiles until a person approves it");
            note
        })
        .collect();
    notes.dedup();
    notes
}

/// One note for each field of a cluster tier that strata could not read on a side: `strata could
/// not read policy.rbacs of cluster fred (HTTP 403: forbidden), so it does not compare it`, in
/// text order. Two sides that say the same get one note.
fn unread_notes(d: &Diff) -> Vec<String> {
    let notes: std::collections::BTreeSet<String> = [&d.a, &d.b]
        .into_iter()
        .filter_map(|side| side.cluster.as_ref())
        .flat_map(|c| {
            c.tiers.iter().flat_map(move |(tier, t)| {
                t.errors.iter().map(move |(field, e)| {
                    format!(
                        "strata could not read {} of cluster {} ({e}), so it does not compare it",
                        tier_path(tier, field),
                        c.name
                    )
                })
            })
        })
        .collect();
    notes.into_iter().collect()
}

/// One note for each tier of a side that a later release wrote, when the other side does not
/// carry it.
///
/// For example: `a later release of strata wrote the tier zones of cluster fred, so strata does
/// not compare it`. Strata reads no such tier from Palette, so `verify` never compares one.
fn later_tier_notes(d: &Diff) -> Vec<String> {
    let (Some(a), Some(b)) = (&d.a.cluster, &d.b.cluster) else {
        return Vec::new();
    };
    [(a, b), (b, a)]
        .into_iter()
        .flat_map(|(c, other)| {
            c.tiers
                .keys()
                .filter(|t| t.known().is_none() && !other.tiers.contains_key(*t))
                .map(move |t| {
                    format!(
                        "a later release of strata wrote the tier {t} of {} {}, so strata does \
                         not compare it",
                        c.kind.noun(),
                        c.name
                    )
                })
        })
        .collect()
}

/// One note for each pack that a cluster side runs at another version than its profile
/// declares: `cluster fred runs kubernetes 1.36.1, and its profile declares 1.36.2`.
fn running_behind_notes(d: &Diff) -> Vec<String> {
    d.running_behind
        .iter()
        .map(|r| {
            let side = match r.side {
                Side::A => &d.a,
                Side::B => &d.b,
            };
            let name = side.cluster.as_ref().map_or("", |c| c.name.as_str());
            format!(
                "cluster {name} runs {} {}, and its profile declares {}",
                r.pack, r.running, r.declared
            )
        })
        .collect()
}

/// The path of a leaf inside a tier: `policy.timezone`, and `running[name=kubernetes].version`
/// for a tier that is a list.
pub(crate) fn tier_path(tier: &Open<Tier>, path: &str) -> String {
    if path.starts_with('[') {
        format!("{tier}{path}")
    } else {
        format!("{tier}.{path}")
    }
}

/// How the tiers of two cluster records compare, for the tier line of a report.
pub(crate) enum TierLine<'a> {
    /// The diff compared no tier.
    NotCompared,
    /// Each tier compared is the same.
    Unchanged,
    /// At least one tier differs: each tier compared, with whether it is the same.
    Differ(Vec<(&'a Open<Tier>, bool)>),
}

/// How the tiers of `d` compare: see [`TierLine`].
pub(crate) fn tier_line(d: &Diff) -> TierLine<'_> {
    if d.cluster_tiers.is_empty() {
        TierLine::NotCompared
    } else if d.cluster_tiers.iter().all(|t| t.same) {
        TierLine::Unchanged
    } else {
        TierLine::Differ(d.cluster_tiers.iter().map(|t| (&t.tier, t.same)).collect())
    }
}

/// `aws-stack@1.1.1@draft is the draft of aws-stack@1.1.1 (uid u): its publish makes this
/// change`, for a diff of a version and its draft whose roots differ.
fn draft_note(d: &Diff) -> Option<String> {
    let draft = match (d.a.is_draft(), d.b.is_draft()) {
        (false, true) => &d.b,
        (true, false) => &d.a,
        _ => return None,
    };
    (!d.same && !draft.uid.is_empty() && d.a.uid == d.b.uid).then(|| {
        format!(
            "{} is the draft of {} (uid {}): its publish makes this change",
            draft.reference,
            name_version(&draft.name, &draft.version),
            draft.uid
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_causes_of_a_drift_outside_the_root_join_as_one_list_of_strata() {
        let image = |key: &str| ImageIdChange {
            key: key.to_string(),
            a: "ami-1".to_string(),
            b: "ami-2".to_string(),
        };
        let macros = ["region".to_string()];
        // two causes have no comma before `and`
        assert_eq!(
            outside_root_sentence(&macros, &[image("os/ubuntu-aws")]).as_deref(),
            Some(
                "macro region has another value now and pack os/ubuntu-aws names image ami-2 \
                 now, not ami-1"
            )
        );
        // three causes have one before the last
        assert_eq!(
            outside_root_sentence(&macros, &[image("os/a"), image("os/b")]).as_deref(),
            Some(
                "macro region has another value now, pack os/a names image ami-2 now, not \
                 ami-1, and pack os/b names image ami-2 now, not ami-1"
            )
        );
        assert_eq!(outside_root_sentence(&[], &[]), None);
    }

    #[test]
    fn a_pack_that_palette_marks_invalid_reads_the_same_with_and_without_a_reason() {
        let pack = |reason: &str| InvalidPack {
            key: "os/ubuntu-aws".to_string(),
            reason: reason.to_string(),
        };
        assert_eq!(
            marks_invalid(&pack(""), ""),
            "Palette marks pack os/ubuntu-aws invalid"
        );
        assert_eq!(
            marks_invalid(&pack("tag is deleted"), " now"),
            "Palette marks pack os/ubuntu-aws invalid now (tag is deleted)"
        );
        assert_eq!(
            invalid_pack_sentences(&[pack(""), pack("tag is deleted")]),
            [
                "Palette marks pack os/ubuntu-aws invalid now",
                "Palette marks pack os/ubuntu-aws invalid now (tag is deleted)"
            ]
        );
    }
}
