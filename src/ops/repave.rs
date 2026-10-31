//! `compare --repave`: ask Palette whether a change replaces the nodes of a cluster.
//!
//! The change goes from what the cluster runs to a profile version, or to the update that waits
//! for the cluster. `compare --validate` sends the same body for a profile version that the
//! cluster does not run: see the `validate` module.

use std::collections::BTreeSet;

use serde_json::{Value, json};

use crate::api::{Palette, PaletteApi};
use crate::cluster_template::ClusterTemplateRef;
use crate::diff::{ImpactLevel, PaletteRepave};
use crate::error::{Error, Result};
use crate::json::{arr, opt_str};
use crate::profile_type::ProfileType;
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

/// The body of `validate/repave` for a move of `cluster` to `target`. The request names each
/// profile the target cluster would run, by uid. A profile the cluster runs now keeps its packs,
/// or takes the pack values of `target` when it is the same uid. A profile the request leaves out
/// is not in the target. So `target` replaces the profile it has the name of, or else the one
/// that builds the cluster when `target` builds one too.
pub fn validate_body(running: &[AppliedProfile], target: &Snapshot) -> Value {
    let uid = target.source.uid.as_str();
    let pack_values = || -> Vec<Value> { target.packs.iter().map(pack_value).collect() };
    if running.iter().any(|p| p.uid == uid) {
        return profiles_body(running.iter().map(|p| p.uid.as_str()), |running_uid| {
            (running_uid == uid).then(pack_values)
        });
    }
    let replaced = running
        .iter()
        .find(|p| p.name == target.source.name)
        .or_else(|| {
            ProfileType::text_builds_cluster(&target.source.kind)
                .then(|| {
                    running
                        .iter()
                        .find(|p| ProfileType::text_builds_cluster(&p.kind))
                })
                .flatten()
        });
    let kept = running
        .iter()
        .filter(|p| Some(p.uid.as_str()) != replaced.map(|r| r.uid.as_str()))
        .map(|p| p.uid.as_str());
    profiles_body(std::iter::once(uid).chain(kept), |_| None)
}

/// The cluster side and the profile side of a comparison, for the question of `option`. One side
/// must be a cluster, and the other a profile version with a uid, in either order. Any other pair
/// is a usage error.
pub(super) fn cluster_and_target<'a>(
    a: &'a Snapshot,
    b: &'a Snapshot,
    option: &str,
) -> Result<(&'a ClusterSource, &'a Snapshot)> {
    let (cluster, target) = match (&a.source.cluster, &b.source.cluster) {
        (Some(c), None) => (c, b),
        (None, Some(c)) => (c, a),
        _ => {
            return Err(Error::Usage {
                message: format!(
                    "{option} needs one side that is a cluster and one that is a profile version"
                ),
            });
        }
    };
    if !cluster.kind.is_cluster() {
        return Err(Error::Usage {
            message: format!(
                "{option} needs a cluster: Palette answers it for a cluster, and not for {} {}",
                if cluster.kind == SubjectKind::EdgeHost {
                    "an"
                } else {
                    "a"
                },
                cluster.kind.noun()
            ),
        });
    }
    if !target.source.is_profile_version() || target.source.uid.is_empty() {
        return Err(Error::Usage {
            message: format!(
                "{option} needs the profile side to be a profile version that Palette holds, \
                 with its uid"
            ),
        });
    }
    Ok((cluster, target))
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

/// The record of the update that waits, and its cluster source, when each side of a comparison is
/// a record of a cluster. `None` when one side is not.
///
/// # Errors
///
/// [`Error::Usage`] for two records of clusters that are not one cluster and the update that
/// waits for it, `cluster:<c>` and `cluster:<c>@pending`.
fn cluster_and_pending<'a>(
    a: &'a Snapshot,
    b: &'a Snapshot,
) -> Result<Option<(&'a ClusterSource, &'a Snapshot)>> {
    let (Some(ca), Some(cb)) = (&a.source.cluster, &b.source.cluster) else {
        return Ok(None);
    };
    match (ca.pending, cb.pending) {
        (false, true) if ca.uid == cb.uid && ca.kind.is_cluster() => Ok(Some((cb, b))),
        (true, false) if ca.uid == cb.uid && cb.kind.is_cluster() => Ok(Some((ca, a))),
        _ => Err(Error::Usage {
            message: "--repave needs one side that is a cluster and one that is a profile \
                      version, or a cluster and the update that waits for it: \
                      cluster:<c> cluster:<c>@pending"
                .to_string(),
        }),
    }
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
/// [`Error::Usage`] when the record holds a profile that the cluster does not run now: the record
/// is older than the cluster.
fn pending_body(doc: &Value, pending: &Snapshot, source: &ClusterSource) -> Result<Value> {
    let running = AppliedProfile::of_cluster(doc);
    if let Some(old) = source
        .profiles
        .iter()
        .find(|p| !running.iter().any(|r| r.uid == p.uid))
    {
        return Err(Error::Usage {
            message: format!(
                "the record of the update that waits holds {}, and the cluster {} does not run \
                 it now. Record {} again",
                name_version(&old.name, &old.version),
                source.name,
                Ref::Cluster {
                    uid: source.name.clone(),
                    profile: None,
                    pending: true,
                }
            ),
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
/// cluster. One side must be a cluster. The other is a profile version with a uid, or the record
/// of the update that waits for that cluster, `cluster:<c>@pending`, in either order. It reads the
/// cluster document for the profiles it runs now, and sends one POST, which changes nothing.
/// `level` is the most severe level of the impact table, for `agrees`.
///
/// On the route of a cluster that a cluster template governs, Palette compares nothing and
/// answers no repave. So the cluster document, and not the answer, tells strata to send the POST
/// to the route of the template for such a cluster. The answer then names the template.
///
/// # Errors
///
/// [`Error::Usage`] for any other pair. Also for a record of the update that waits that holds a
/// profile that the cluster does not run now. Also for a cluster in a template, when the body
/// gives pack values to a profile that the cluster runs. This is a target version that the
/// cluster runs, or an update that changes a profile. The route of the template takes no pack
/// values, so it compares nothing for that profile. An error of a request.
pub fn palette_repave<A: PaletteApi>(
    palette: &Palette<A>,
    a: &Snapshot,
    b: &Snapshot,
    level: ImpactLevel,
) -> Result<PaletteRepave> {
    let (cluster, doc, body, asked) = match cluster_and_pending(a, b)? {
        Some((source, pending)) => {
            let doc = palette.subject_document(SubjectKind::Cluster, &source.uid)?;
            let body = pending_body(&doc, pending, source)?;
            let asked = format!(
                "the update that waits for the cluster {}: the update changes a profile that the \
                 cluster runs",
                source.name
            );
            (source, doc, body, asked)
        }
        None => {
            let (cluster, target) = cluster_and_target(a, b, "--repave")?;
            let doc = palette.subject_document(SubjectKind::Cluster, &cluster.uid)?;
            let body = validate_body(&AppliedProfile::of_cluster(&doc), target);
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
        return Err(Error::Usage {
            message: format!(
                "--repave cannot ask about {asked}, and the cluster template {} governs the \
                 cluster. Palette answers for the template by profile uid, with no pack values, \
                 so it compares nothing for a version that the cluster runs",
                template.uid
            ),
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
}
