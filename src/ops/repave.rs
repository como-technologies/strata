//! `compare --repave` and `compare --validate`: ask Palette whether a change from what a cluster
//! runs to a profile version replaces the nodes of the cluster, and whether Palette accepts its
//! packs.

use serde_json::{Value, json};

use crate::api::{Palette, PaletteApi};
use crate::diff::{ImpactLevel, PaletteRefusal, PaletteRepave, PaletteValidation};
use crate::error::{Error, PACK_REFUSAL_CODES, Result, error_code};
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
fn cluster_and_target<'a>(
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

/// Ask Palette whether moving the cluster side of a comparison to its profile side repaves the
/// cluster. One side must be a cluster, and the other a profile version with a uid, in either
/// order. It reads the cluster document for the profiles it runs now, and sends one POST, which
/// changes nothing. `level` is the most severe level of the impact table, for `agrees`.
pub fn palette_repave<A: PaletteApi>(
    palette: &Palette<A>,
    a: &Snapshot,
    b: &Snapshot,
    level: ImpactLevel,
) -> Result<PaletteRepave> {
    let (cluster, target) = cluster_and_target(a, b, "--repave")?;
    let running = AppliedProfile::of_cluster(&palette.cluster_document(&cluster.uid)?);
    let answer = palette.validate_repave(&cluster.uid, &validate_body(&running, target))?;
    Ok(PaletteRepave::from_palette(&cluster.uid, &answer, level))
}

/// Ask Palette whether it accepts the packs of the profile side on the cluster side: `POST
/// validate/packs` with the body of `--repave`, which changes nothing. The sides follow the rules
/// of [`palette_repave`]. Palette answers `200` with an error for each pack it refuses. It
/// refuses the whole request with HTTP 500 and a code of [`PACK_REFUSAL_CODES`]: that is an
/// answer too, and the retry does not send it again. Each other error is an error of strata.
pub fn palette_validation<A: PaletteApi>(
    palette: &Palette<A>,
    a: &Snapshot,
    b: &Snapshot,
) -> Result<PaletteValidation> {
    let (cluster, target) = cluster_and_target(a, b, "--validate")?;
    let running = AppliedProfile::of_cluster(&palette.cluster_document(&cluster.uid)?);
    match palette.validate_packs(&cluster.uid, &validate_body(&running, target)) {
        Ok(answer) => Ok(PaletteValidation::from_palette(&cluster.uid, &answer)),
        Err(Error::Api {
            status: 500, body, ..
        }) if error_code(&body).is_some_and(|c| PACK_REFUSAL_CODES.contains(&c.as_str())) => {
            let error: Value = serde_json::from_str(&body).unwrap_or(Value::Null);
            let text = |key: &str| {
                error
                    .get(key)
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string()
            };
            let refusal = PaletteRefusal {
                pack: String::new(),
                code: text("code"),
                message: text("message"),
            };
            Ok(PaletteValidation::refused(&cluster.uid, vec![refusal]))
        }
        Err(e) => Err(e),
    }
}
