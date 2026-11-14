//! `compare --repave`: ask Palette whether a change replaces the nodes of a cluster.
//!
//! The change goes from what the cluster runs to a profile version, or to the update that waits
//! for the cluster. `compare --validate` sends the same body for a profile version that the
//! cluster does not run: see the `validate` module.

use std::borrow::Cow;
use std::collections::BTreeSet;

use serde_json::{Value, json};

use crate::api::mask::console::console_values;
use crate::api::{Palette, PaletteApi};
use crate::closed_set::Open;
use crate::diff::{ImpactLevel, PaletteRepave, Side};
use crate::error::{Error, Result};
use crate::json::{arr, opt_str, str_at};
use crate::model::cluster_template::ClusterTemplateRef;
use crate::reference::{Ref, name_version};
use crate::snapshot::{AppliedProfile, ClusterSource, Pack, Snapshot, SubjectKind};

/// One entry of `packValues` in the body of `validate/repave`: the pack by name, with its tag,
/// its type, and its values.
fn pack_value(p: &Pack) -> Value {
    json!({"name": p.name, "tag": p.tag, "type": p.kind, "values": p.values})
}

/// The body of `validate/packs` and `validate/repave`: one profile for each uid of `uids`, in
/// order. `values_of` gives the `packValues` of a profile. The body names a profile with no
/// `packValues` by its uid alone.
fn profiles_body<'a>(
    uids: impl IntoIterator<Item = &'a str>,
    values_of: impl Fn(&str) -> Option<Vec<Value>>,
) -> Value {
    let profiles: Vec<Value> = uids
        .into_iter()
        .map(|uid| match values_of(uid) {
            Some(packs) => json!({"uid": uid, "packValues": packs}),
            None => json!({"uid": uid}),
        })
        .collect();
    json!({ "profiles": profiles })
}

/// The body of `validate/repave` for a move of the cluster that runs `running` to `target`.
///
/// The request names each profile that the cluster runs after the move, by uid. A profile it runs
/// now keeps its packs, or takes the pack values of `target` when it is the same uid. A profile
/// the request omits is not in the target. So `target` replaces the profile it has the name of,
/// or else the one that builds the cluster when `target` builds one too. The cluster side of
/// [`snapshot_pair`](super::snapshot_pair) holds that profile alone, by the same rule.
pub fn validate_body(running: &[AppliedProfile], target: &Snapshot) -> Value {
    let uid = target.source.uid.as_str();
    let pack_values = || -> Vec<Value> { target.packs.iter().map(pack_value).collect() };
    if running.iter().any(|p| p.uid == uid) {
        return profiles_body(running.iter().map(|p| p.uid.as_str()), |running_uid| {
            (running_uid == uid).then(pack_values)
        });
    }
    let replaced = AppliedProfile::replaced_by(running, &target.source);
    let kept = running
        .iter()
        .filter(|p| Some(p.uid.as_str()) != replaced.map(|r| r.uid.as_str()))
        .map(|p| p.uid.as_str());
    profiles_body(std::iter::once(uid).chain(kept), |_| None)
}

/// A question of `compare` that Palette answers about a cluster.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Question {
    /// `--repave`: does the change replace the nodes of the cluster?
    Repave,
    /// `--validate`: does Palette accept the packs of the change on the cluster?
    Validate,
}

impl Question {
    /// The option of `compare` that asks the question.
    fn option(self) -> &'static str {
        match self {
            Question::Repave => "--repave",
            Question::Validate => "--validate",
        }
    }

    /// The refusal for a pair of sides that has no cluster, or has two.
    fn needs_a_pair(self) -> Error {
        Error::Usage {
            message: format!(
                "{} needs one side that is a cluster and one that is a profile version",
                self.option()
            ),
        }
    }

    /// The refusal for two record files that hold no cluster.
    ///
    /// A record can be the profile side, so the message gives the command with a cluster in place
    /// of one record. Plain `compare` of the two records prints the impact.
    fn needs_a_cluster_for_records(self) -> Error {
        let option = self.option();
        Error::Usage {
            message: format!(
                "{option} needs one side that is a cluster, and both sides are records with no \
                 cluster. Palette answers only for a cluster, so give the cluster in place of one \
                 record: strata compare cluster:<name> <record> {option}. Plain compare of the \
                 two records prints the impact"
            ),
        }
    }
}

/// What `compare` asks Palette about its cluster side, beyond the comparison of the two sides.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PaletteQuestions {
    /// `--repave`: see [`palette_repave`].
    pub repave: bool,
    /// `--validate`: see [`palette_validation`](super::palette_validation).
    pub validate: bool,
}

impl PaletteQuestions {
    /// The questions that are on, in the order of the options.
    fn asked(&self) -> Vec<Question> {
        [
            (self.repave, Question::Repave),
            (self.validate, Question::Validate),
        ]
        .into_iter()
        .filter_map(|(on, question)| on.then_some(question))
        .collect()
    }

    /// Refuse a pair of refs that a question cannot ask about, before any request.
    ///
    /// A ref shows what its side is: `cluster:<c>` is a cluster, and `aws-stack@1.1.1` is a
    /// profile version. A ref to a record file shows it after strata reads the file, which makes
    /// no request. For two records with no cluster, the error names the command with a cluster in
    /// place of one record. [`palette_repave`] and
    /// [`palette_validation`](super::palette_validation) keep the same rules for the records they
    /// get. A fault that only the reads show comes from them.
    ///
    /// # Errors
    ///
    /// - [`Error::Usage`] for a pair that a question cannot ask about, and for a set of clusters.
    /// - The error of the read of a record file.
    pub fn check(&self, a: &Ref, b: &Ref) -> Result<()> {
        let asked = self.asked();
        if asked.is_empty() {
            return Ok(());
        }
        if let Some(set) = [a, b]
            .into_iter()
            .find(|r| matches!(r, Ref::ClusterSet { .. }))
        {
            return Err(super::names_a_set(set));
        }
        let record_of = |r: &Ref| match r {
            Ref::Snapshot(path) => Snapshot::load(path).map(Some),
            _ => Ok(None),
        };
        let (record_a, record_b) = (record_of(a)?, record_of(b)?);
        let (role_a, role_b) = (
            Role::of_ref(a, record_a.as_ref()),
            Role::of_ref(b, record_b.as_ref()),
        );
        // the plan refuses such a pair too; this error names the commands to use for two records
        let records_with_no_cluster = record_a.is_some()
            && record_b.is_some()
            && role_a.holder().is_none()
            && role_b.holder().is_none();
        asked.into_iter().try_for_each(|question| {
            if records_with_no_cluster {
                return Err(question.needs_a_cluster_for_records());
            }
            plan(&role_a, &role_b, question).map(drop)
        })
    }
}

/// An object that holds profiles: a cluster, an edge host, or a cluster group, or the update that
/// waits for a cluster.
struct Holder<'a> {
    kind: Open<SubjectKind>,
    /// True for the update that waits for the object.
    pending: bool,
    /// The uid of the object. A ref can name it by its name, so a ref gives none.
    uid: Option<&'a str>,
}

/// What one side of a comparison is to a question of Palette. A ref shows it, and so does a
/// record.
enum Role<'a> {
    Holder(Holder<'a>),
    /// A profile version that Palette holds, with its uid.
    Profile,
    /// Any other object: a candidate, a pack, or an app profile.
    Other,
}

impl<'a> Role<'a> {
    /// The role of a record.
    fn of_record(record: &'a Snapshot) -> Self {
        match &record.source.cluster {
            Some(c) => Role::Holder(Holder {
                kind: c.kind.clone(),
                pending: c.pending,
                uid: Some(&c.uid),
            }),
            None if record.source.is_profile_version() && !record.source.uid.is_empty() => {
                Role::Profile
            }
            None => Role::Other,
        }
    }

    /// The role of a ref. `record` is the record that a ref to a file names.
    fn of_ref(reference: &Ref, record: Option<&'a Snapshot>) -> Self {
        let holder = |kind: SubjectKind, pending: bool| {
            Role::Holder(Holder {
                kind: kind.into(),
                pending,
                uid: None,
            })
        };
        match (record, reference) {
            (Some(record), _) => Role::of_record(record),
            (None, Ref::Cluster { pending, .. }) => holder(SubjectKind::Cluster, *pending),
            (None, Ref::EdgeHost { .. }) => holder(SubjectKind::EdgeHost, false),
            (None, Ref::ClusterGroup { .. }) => holder(SubjectKind::ClusterGroup, false),
            (None, r) if r.is_profile_version() => Role::Profile,
            (None, _) => Role::Other,
        }
    }

    fn holder(&self) -> Option<&Holder<'a>> {
        match self {
            Role::Holder(h) => Some(h),
            _ => None,
        }
    }
}

/// What a pair of sides asks of Palette. The side is the cluster, or the update that waits for it.
enum Plan {
    /// A move of the cluster to the profile version on the other side.
    Move(Side),
    /// A move of the cluster to the update that waits, which is the record on the side.
    Pending(Side),
}

/// The plan of a question for two sides, or the usage error for a pair that the question cannot
/// ask about. `--repave` also takes a cluster and the update that waits for it.
fn plan(a: &Role<'_>, b: &Role<'_>, question: Question) -> Result<Plan> {
    match (a.holder(), b.holder()) {
        (Some(x), Some(y)) if question == Question::Repave => pending_plan(x, y),
        _ => move_plan(a, b, question).map(Plan::Move),
    }
}

/// The side that is the cluster, for the question of `question`. One side must be a cluster, and
/// the other a profile version with a uid, in either order. Any other pair is a usage error.
fn move_plan(a: &Role<'_>, b: &Role<'_>, question: Question) -> Result<Side> {
    let (side, holder, target) = match (a.holder(), b.holder()) {
        (Some(h), None) => (Side::A, h, b),
        (None, Some(h)) => (Side::B, h, a),
        _ => return Err(question.needs_a_pair()),
    };
    if !holder.kind.is_cluster() {
        return Err(Error::Usage {
            message: format!(
                "{} needs a cluster: Palette answers it for a cluster, and not for {} {}",
                question.option(),
                if holder.kind == SubjectKind::EdgeHost {
                    "an"
                } else {
                    "a"
                },
                holder.kind.noun()
            ),
        });
    }
    if !matches!(target, Role::Profile) {
        return Err(Error::Usage {
            message: format!(
                "{} needs the profile side to be a profile version that Palette holds, with its \
                 uid",
                question.option()
            ),
        });
    }
    Ok(side)
}

/// The plan for two holders: a cluster, and the update that waits for the same cluster.
fn pending_plan(a: &Holder<'_>, b: &Holder<'_>) -> Result<Plan> {
    // a name and a uid can name one cluster, so only two uids show two clusters
    let one_cluster = a.uid.zip(b.uid).is_none_or(|(x, y)| x == y);
    match (a.pending, b.pending) {
        (false, true) if one_cluster && a.kind.is_cluster() => Ok(Plan::Pending(Side::B)),
        (true, false) if one_cluster && b.kind.is_cluster() => Ok(Plan::Pending(Side::A)),
        _ => Err(Error::Usage {
            message: "--repave needs one side that is a cluster and one that is a profile \
                      version, or a cluster and the update that waits for it: \
                      cluster:<c> cluster:<c>@pending"
                .to_string(),
        }),
    }
}

/// The record on `side`, and the record on the other side.
fn pick<'a>(side: Side, a: &'a Snapshot, b: &'a Snapshot) -> (&'a Snapshot, &'a Snapshot) {
    match side {
        Side::A => (a, b),
        Side::B => (b, a),
    }
}

/// The cluster source of a record that a plan chose as a holder.
fn cluster_of(record: &Snapshot, question: Question) -> Result<&ClusterSource> {
    // the plan read the role from this field, so the guard gives the error of the plan
    record
        .source
        .cluster
        .as_ref()
        .ok_or_else(|| question.needs_a_pair())
}

/// The cluster side and the profile side of a comparison, for the question of `question`: see
/// [`move_plan`].
pub(super) fn cluster_and_target<'a>(
    a: &'a Snapshot,
    b: &'a Snapshot,
    question: Question,
) -> Result<(&'a ClusterSource, &'a Snapshot)> {
    moved(
        move_plan(&Role::of_record(a), &Role::of_record(b), question)?,
        a,
        b,
        question,
    )
}

/// The record of the cluster side and the record of the profile side, as
/// [`cluster_and_target`] chooses them.
pub(super) fn cluster_record_and_target<'a>(
    a: &'a Snapshot,
    b: &'a Snapshot,
    question: Question,
) -> Result<(&'a Snapshot, &'a Snapshot)> {
    let side = move_plan(&Role::of_record(a), &Role::of_record(b), question)?;
    Ok(pick(side, a, b))
}

/// The cluster source and the profile side of two records, when `side` holds the cluster.
fn moved<'a>(
    side: Side,
    a: &'a Snapshot,
    b: &'a Snapshot,
    question: Question,
) -> Result<(&'a ClusterSource, &'a Snapshot)> {
    let (cluster, target) = pick(side, a, b);
    Ok((cluster_of(cluster, question)?, target))
}

/// The body of the template route of `validate/repave`: the profile uids of `body`, the body of
/// the cluster route, with no pack values. `isUpgradeNow` is false: when it is true, Palette
/// ignores the profiles of the request and uses the profiles of the template.
fn template_body(body: &Value) -> Value {
    let profiles: Vec<Value> = arr(body, "/profiles")
        .iter()
        .map(|p| json!({"uid": p["uid"]}))
        .collect();
    json!({"profiles": profiles, "isUpgradeNow": false})
}

/// The body of `validate/repave` for a move of the cluster of `doc` to the update that waits for
/// it. `pending` is the record of that update, and `source` is its cluster source.
///
/// An update that waits is an edit in place: each profile keeps its uid. Palette keeps the packs
/// of a profile that the body names with no pack values. For a profile with pack values, it keeps
/// only the packs that they name. It takes a pack that the cluster does not run from the profile
/// as it is now. So a profile of the record whose packs change gets the pack values. These are
/// its packs by the names that the cluster runs, and each pack that is new to the cluster. The
/// body names each other profile by its uid alone.
///
/// # Errors
///
/// [`Error::StaleRecord`] when the record holds a profile that the cluster does not run now: the
/// record is older than the cluster.
fn pending_body(doc: &Value, pending: &Snapshot, source: &ClusterSource) -> Result<Value> {
    let running = AppliedProfile::of_cluster(doc);
    if let Some(old) = source
        .profiles
        .iter()
        .find(|p| !running.iter().any(|r| r.uid == p.uid))
    {
        return Err(Error::StaleRecord {
            cluster: source.uid.clone(),
            name: source.name.clone(),
            profile: name_version(&old.name, &old.version),
        });
    }
    let templates = arr(doc, "/spec/clusterProfileTemplates");
    let running_names: BTreeSet<&str> = templates
        .iter()
        .flat_map(|t| arr(t, "/packs"))
        .filter_map(|p| opt_str(p, "/name"))
        .collect();
    let new_packs: Vec<&Pack> = pending
        .packs
        .iter()
        .filter(|p| !running_names.contains(p.name.as_str()))
        .collect();
    Ok(profiles_body(
        running.iter().map(|r| r.uid.as_str()),
        |uid| {
            let covered = source.profiles.iter().any(|p| p.uid == uid);
            covered
                .then(|| {
                    let current = AppliedProfile::template_packs(doc, uid);
                    pending_packs(current, &pending.packs, &new_packs)
                })
                .flatten()
        },
    ))
}

/// `snap` with the values that a question to Palette sends for each pack of virtual machines: the
/// console address that the cluster `doc` holds, in place of the mask of strata. See
/// [`console_values`]. Only the values text of such a pack changes, because a body reads no other
/// field of it. A record with no such mask gives `snap` as it is.
pub(super) fn with_console_of<'a>(snap: &'a Snapshot, doc: &Value) -> Result<Cow<'a, Snapshot>> {
    let mut out: Option<Snapshot> = None;
    for (i, p) in snap.packs.iter().enumerate() {
        let held = arr(doc, "/spec/clusterProfileTemplates")
            .iter()
            .flat_map(|t| arr(t, "/packs"))
            .find(|pk| str_at(pk, "/name") == p.name)
            .and_then(|pk| opt_str(pk, "/values"));
        if let Some(values) = console_values(&p.name, &p.values, held)? {
            out.get_or_insert_with(|| snap.clone()).packs[i].values = values;
        }
    }
    Ok(out.map_or(Cow::Borrowed(snap), Cow::Owned))
}

/// The pack values of one profile in the body of [`pending_body`], or `None` when the update
/// changes no pack of the profile.
///
/// `current` is the packs of the profile that the cluster runs. `pending` is each pack of the
/// record. `new` is each pack of the record that the cluster does not run.
fn pending_packs(current: &[Value], pending: &[Pack], new: &[&Pack]) -> Option<Vec<Value>> {
    let own: Vec<&Pack> = pending
        .iter()
        .filter(|p| current.iter().any(|c| c["name"] == p.name.as_str()))
        .collect();
    let same = |p: &&Pack| {
        current.iter().any(|c| {
            c["name"] == p.name.as_str()
                && c["tag"] == p.tag.as_str()
                && c["values"] == p.values.as_str()
        })
    };
    // a new pack can belong to this profile, and only Palette knows the profile as it is now
    let changed = !new.is_empty() || own.len() != current.len() || !own.iter().all(same);
    changed.then(|| own.iter().chain(new).map(|p| pack_value(p)).collect())
}

/// Ask Palette whether a move of the cluster side of a comparison to its other side repaves the
/// cluster.
///
/// One side must be a cluster. The other is a profile version with a uid, or the record of the
/// update that waits for that cluster, `cluster:<c>@pending`, in either order. It reads the
/// cluster document for the profiles it runs now, and sends one POST. The POST changes no setting,
/// and it can save the address of the console of a pack: see [`Palette::validate_repave`]. So the
/// body holds the console address of the cluster in place of the mask of strata.
/// `a` and `b` are the two sides of the comparison. `level` is the most severe level of the impact
/// table, for `agrees`.
///
/// On the route of a cluster that a cluster template governs, Palette compares nothing and
/// answers no repave. So the cluster document, and not the answer, tells strata to send the POST
/// to the route of the template for such a cluster. The answer then names the template.
///
/// # Errors
///
/// - [`Error::Usage`] for any other pair, before any request. This includes a cluster and the
///   update that waits for another cluster: the uids of the two records show it.
/// - [`Error::StaleRecord`] for a record of the update that waits that holds a profile that the
///   cluster does not run now.
/// - [`Error::ClusterTemplateGoverns`] for a cluster in a template, when the body gives pack
///   values to a profile that the cluster runs. This is a target version that the cluster runs,
///   or an update that changes a profile. The route of the template takes no pack values, so it
///   compares nothing for that profile.
/// - The error of a request.
pub fn palette_repave<A: PaletteApi>(
    palette: &Palette<A>,
    a: &Snapshot,
    b: &Snapshot,
    level: ImpactLevel,
) -> Result<PaletteRepave> {
    let question = Question::Repave;
    let (cluster, doc, body, asked) = match plan(
        &Role::of_record(a),
        &Role::of_record(b),
        question,
    )? {
        Plan::Pending(side) => {
            let (pending, _) = pick(side, a, b);
            let source = cluster_of(pending, question)?;
            let doc = palette.subject_document(SubjectKind::Cluster, &source.uid)?;
            let body = pending_body(&doc, &*with_console_of(pending, &doc)?, source)?;
            let asked = format!(
                "the update that waits for the cluster {}: the update changes a profile that the \
                 cluster runs",
                source.name
            );
            (source, doc, body, asked)
        }
        Plan::Move(side) => {
            let (cluster, target) = moved(side, a, b, question)?;
            let doc = palette.subject_document(SubjectKind::Cluster, &cluster.uid)?;
            let body = validate_body(
                &AppliedProfile::of_cluster(&doc),
                &*with_console_of(target, &doc)?,
            );
            let asked = format!(
                "{}: the cluster {} runs it",
                name_version(&target.source.name, &target.source.version),
                cluster.name
            );
            (cluster, doc, body, asked)
        }
    };
    let Some(template) = ClusterTemplateRef::of_cluster(&doc) else {
        let answer = palette.validate_repave(&cluster.uid, &body)?;
        return Ok(PaletteRepave::from_palette(&cluster.uid, &answer, level));
    };
    let has_pack_values = arr(&body, "/profiles")
        .iter()
        .any(|p| p.get("packValues").is_some());
    if has_pack_values {
        return Err(Error::ClusterTemplateGoverns {
            cluster: cluster.uid.clone(),
            template: template.uid,
            asked,
        });
    }
    let answer = palette.validate_template_repave(&template.uid, &template_body(&body))?;
    Ok(PaletteRepave {
        cluster_template: Some(template),
        ..PaletteRepave::from_palette(&cluster.uid, &answer, level)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_body_names_a_profile_with_no_pack_values_by_its_uid_alone() {
        let body = profiles_body(["p1", "p2", "p3"], |uid| {
            (uid == "p2").then(|| vec![json!({"name": "kubernetes"})])
        });
        assert_eq!(
            body,
            json!({"profiles": [
                {"uid": "p1"},
                {"uid": "p2", "packValues": [{"name": "kubernetes"}]},
                {"uid": "p3"},
            ]})
        );
    }

    fn refs(a: &str, b: &str) -> (Ref, Ref) {
        let no_files = |_: &std::path::Path| false;
        (Ref::parse_with(a, no_files), Ref::parse_with(b, no_files))
    }

    fn checked(a: &str, b: &str, questions: PaletteQuestions) -> Result<()> {
        let (a, b) = refs(a, b);
        questions.check(&a, &b)
    }

    const REPAVE: PaletteQuestions = PaletteQuestions {
        repave: true,
        validate: false,
    };
    const VALIDATE: PaletteQuestions = PaletteQuestions {
        repave: false,
        validate: true,
    };

    #[test]
    fn a_pair_of_refs_that_a_question_takes_passes_in_either_order() {
        for (a, b) in [
            ("cluster:payments", "aws-stack@1.1.1"),
            ("aws-stack@1.1.1", "cluster:payments"),
            ("cluster:payments/addons", "64e0000000000000000000c3"),
            ("cluster:payments", "aws-stack@1.1.1@draft"),
            ("cluster:payments@pending", "aws-stack@1.1.1"),
        ] {
            for questions in [REPAVE, VALIDATE] {
                assert!(checked(a, b, questions).is_ok(), "{a} {b} {questions:?}");
            }
        }
        // only --repave takes a cluster and the update that waits for it, in either order. A
        // name and a uid can name one cluster, so only the reads show that two refs name two
        for (a, b) in [
            ("cluster:payments", "cluster:payments@pending"),
            ("cluster:payments@pending", "cluster:payments"),
            (
                "cluster:payments",
                "cluster:64f1000000000000000000e1@pending",
            ),
        ] {
            assert!(checked(a, b, REPAVE).is_ok(), "{a} {b}");
            let err = checked(a, b, VALIDATE).unwrap_err();
            assert_eq!(
                err.to_string(),
                "--validate needs one side that is a cluster and one that is a profile version"
            );
        }
    }

    #[test]
    fn a_pair_of_refs_that_a_question_refuses_gives_usage_with_no_read() {
        let pair = "--repave needs one side that is a cluster and one that is a profile version";
        let pending = "--repave needs one side that is a cluster and one that is a profile \
                       version, or a cluster and the update that waits for it: \
                       cluster:<c> cluster:<c>@pending";
        let profile_side = "needs the profile side to be a profile version that Palette holds, \
                            with its uid";
        let cases = [
            ("aws-stack@1.1.1", "aws-stack@1.1.6", REPAVE, pair),
            ("cluster:payments", "cluster:search", REPAVE, pending),
            ("cluster:a@pending", "cluster:b@pending", REPAVE, pending),
            (
                "edgehost:e1",
                "aws-stack@1.1.1",
                REPAVE,
                "--repave needs a cluster: Palette answers it for a cluster, and not for an edge \
                 host",
            ),
            (
                "aws-stack@1.1.1",
                "clustergroup:g1",
                VALIDATE,
                "--validate needs a cluster: Palette answers it for a cluster, and not for a \
                 cluster group",
            ),
            ("cluster:payments", "clustergroup:g1", REPAVE, pending),
            (
                "cluster:payments",
                "pack:kubernetes@1.30.0",
                REPAVE,
                "--repave needs the profile side to be a profile version that Palette holds, \
                 with its uid",
            ),
            (
                "appprofile:shop@1.0.0",
                "cluster:payments",
                VALIDATE,
                profile_side,
            ),
            (
                "aws-stack@1.1.1",
                "aws-stack@1.1.6",
                VALIDATE,
                "--validate needs one side that is a cluster and one that is a profile version",
            ),
        ];
        for (a, b, questions, message) in cases {
            let err = checked(a, b, questions).unwrap_err();
            assert!(matches!(err, Error::Usage { .. }), "{a} {b}: {err}");
            assert!(err.to_string().contains(message), "{a} {b}: {err}");
        }
    }

    #[test]
    fn a_set_of_clusters_is_refused_with_the_message_that_snapshot_gives() {
        let err = checked("tag:env=dev", "cluster:payments", REPAVE).unwrap_err();
        assert!(
            err.to_string()
                .starts_with("tag:env=dev names a set of clusters, and this command reads one"),
            "{err}"
        );
    }

    #[test]
    fn the_check_asks_nothing_when_no_question_is_on() {
        // not even the read of a record file
        let questions = PaletteQuestions::default();
        let file = Ref::Snapshot("no/such/record.json".into());
        assert!(questions.check(&file, &file).is_ok());
        let err = REPAVE.check(&file, &file).unwrap_err();
        assert_eq!(err.kind(), "io", "{err}");
    }

    #[test]
    fn both_questions_are_checked_and_the_stricter_one_refuses() {
        let both = PaletteQuestions {
            repave: true,
            validate: true,
        };
        let err = checked("cluster:payments", "cluster:payments@pending", both).unwrap_err();
        assert!(err.to_string().starts_with("--validate needs"), "{err}");
    }
}
