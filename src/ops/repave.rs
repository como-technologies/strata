//! `compare --repave`: ask Palette whether a change from what a cluster runs to a profile version
//! replaces the nodes of the cluster. `compare --validate` sends the same body: see the `validate`
//! module.

use serde_json::{Value, json};

use crate::api::{Palette, PaletteApi};
use crate::cluster_template::ClusterTemplateRef;
use crate::diff::{ImpactLevel, PaletteRepave};
use crate::error::{Error, Result};
use crate::snapshot::{AppliedProfile, ClusterSource, Snapshot, SubjectKind};

/// A profile type that builds a cluster: its OS and Kubernetes layers.
fn is_infra(kind: &str) -> bool {
    matches!(kind, "cluster" | "infra")
}

/// The body of `validate/repave` for a move of `cluster` to `target`. The request names each
/// profile the target cluster would run, by uid. A profile the cluster runs now keeps its packs,
/// or takes the pack values of `target` when it is the same uid. A profile the request leaves out
/// is not in the target. So `target` replaces the profile it has the name of, or else the one
/// that builds the cluster when `target` builds one too.
pub fn validate_body(running: &[AppliedProfile], target: &Snapshot) -> Value {
    let uid = &target.source.uid;
    let pack_values = || -> Vec<Value> {
        target
            .packs
            .iter()
            .map(|p| json!({"name": p.name, "tag": p.tag, "type": p.kind, "values": p.values}))
            .collect()
    };
    let profiles: Vec<Value> = if running.iter().any(|p| &p.uid == uid) {
        running
            .iter()
            .map(|p| {
                if &p.uid == uid {
                    json!({"uid": p.uid, "packValues": pack_values()})
                } else {
                    json!({"uid": p.uid})
                }
            })
            .collect()
    } else {
        let replaced = running
            .iter()
            .find(|p| p.name == target.source.name)
            .or_else(|| {
                is_infra(&target.source.kind)
                    .then(|| running.iter().find(|p| is_infra(&p.kind)))
                    .flatten()
            });
        std::iter::once(json!({"uid": uid}))
            .chain(
                running
                    .iter()
                    .filter(|p| Some(p.uid.as_str()) != replaced.map(|r| r.uid.as_str()))
                    .map(|p| json!({"uid": p.uid})),
            )
            .collect()
    };
    json!({ "profiles": profiles })
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
    if target.source.pack.is_some() || target.source.uid.is_empty() {
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
    let profiles: Vec<Value> = body["profiles"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|p| json!({"uid": p["uid"]}))
        .collect();
    json!({"profiles": profiles, "isUpgradeNow": false})
}

/// Ask Palette whether a move of the cluster side of a comparison to its profile side repaves
/// the cluster. One side must be a cluster, and the other a profile version with a uid, in either
/// order. It reads the cluster document for the profiles it runs now, and sends one POST, which
/// changes nothing. `level` is the most severe level of the impact table, for `agrees`.
///
/// On the route of a cluster that a cluster template governs, Palette compares nothing and
/// answers no repave. So the cluster document, and not the answer, tells strata to send the POST
/// to the route of the template for such a cluster. The answer then names the template.
///
/// # Errors
///
/// [`Error::Usage`] for a pair that is not a cluster and a profile version, and for a cluster in
/// a template with a target version that the cluster runs now: the route of the template takes no
/// pack values, so it compares nothing for that version. An error of a request.
pub fn palette_repave<A: PaletteApi>(
    palette: &Palette<A>,
    a: &Snapshot,
    b: &Snapshot,
    level: ImpactLevel,
) -> Result<PaletteRepave> {
    let (cluster, target) = cluster_and_target(a, b, "--repave")?;
    let doc = palette.cluster_document(&cluster.uid)?;
    let running = AppliedProfile::of_cluster(&doc);
    let body = validate_body(&running, target);
    let Some(template) = ClusterTemplateRef::of_cluster(&doc) else {
        let answer = palette.validate_repave(&cluster.uid, &body)?;
        return Ok(PaletteRepave::from_palette(&cluster.uid, &answer, level));
    };
    if running.iter().any(|p| p.uid == target.source.uid) {
        return Err(Error::Usage {
            message: format!(
                "--repave cannot ask about {}@{}: the cluster {} runs it, and the cluster template \
                 {} governs the cluster. Palette answers for the template by profile uid, with no \
                 pack values, so it compares nothing for a version that the cluster runs",
                target.source.name, target.source.version, cluster.name, template.uid
            ),
        });
    }
    let answer = palette.validate_template_repave(&template.uid, &template_body(&body))?;
    Ok(PaletteRepave {
        cluster_template: Some(template),
        ..PaletteRepave::from_palette(&cluster.uid, &answer, level)
    })
}
