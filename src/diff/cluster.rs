//! The comparison of two cluster records: the tiers, the profiles that run behind, the profiles
//! that Palette overwrites in place, and the values of the cluster variables.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use super::leaf::{Leaf, Op, leaf_diff};
use super::zip_by_key;
use crate::json::str_at;
use crate::snapshot::{ClusterTier, ClusterVariable, Snapshot, Tier};

/// The values two clusters give their profile variables, compared.
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ClusterVariablesDiff {
    /// True when any value differs.
    pub changed: bool,
    /// The values that differ.
    pub values: Vec<ClusterValueChange>,
}

/// One tier of two cluster records, compared.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct TierDiff {
    /// The tier compared. Between two clusters, `applied` compares `spcInfraHash` alone:
    /// Palette's `spcHash` is its own for each cluster.
    pub tier: Tier,
    /// True when the hashes are equal.
    pub same: bool,
    /// The fields that differ, when the tier differs.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub diff: Vec<Leaf>,
}

/// A side of a comparison: `a` or `b`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Side {
    /// Side A: the base of a comparison, and the record of a verify.
    A,
    /// Side B: the candidate of a comparison, and what Palette holds now in a verify.
    B,
}

/// A pack that a cluster runs at another version than its profile declares.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RunningBehind {
    /// The side whose cluster it is.
    pub side: Side,
    /// The pack name.
    pub pack: String,
    /// The version that the profile declares.
    pub declared: String,
    /// The version that the cluster reconciled.
    pub running: String,
}

/// `fields` less each field that strata could not read in the tier `a` or `b`.
///
/// An error of Palette is not a change of the cluster.
fn read_fields(mut fields: Value, a: &ClusterTier, b: &ClusterTier) -> Value {
    if let Some(map) = fields.as_object_mut() {
        for key in a.errors.keys().chain(b.errors.keys()) {
            map.remove(key);
        }
    }
    fields
}

/// The tiers that two cluster records both carry, and their labels, compared.
pub(super) fn tiers_diff(a: &Snapshot, b: &Snapshot) -> Vec<TierDiff> {
    let (Some(x), Some(y)) = (&a.source.cluster, &b.source.cluster) else {
        return Vec::new();
    };
    // the tiers of a cluster group hold what the group gives its virtual clusters, and not what
    // a cluster runs. A group and a cluster have no tier to compare.
    if x.kind != y.kind || x.tiers.is_empty() || y.tiers.is_empty() {
        return Vec::new();
    }
    let mut out: Vec<TierDiff> = x
        .tiers
        .iter()
        .filter_map(|(name, t)| {
            let u = y.tiers.get(name)?;
            // Palette's spcHash is its own for each cluster, and equal for no two: between two
            // clusters, applied compares spcInfraHash alone
            let (f, g) = match name {
                Tier::Applied if x.uid != y.uid => {
                    let infra = |v: &Value| json!({"spcInfraHash": v.get("spcInfraHash")});
                    (infra(&t.fields), infra(&u.fields))
                }
                // a record from before strata read the shape by cloud type holds fewer fields
                Tier::Shape => crate::snapshot::shape::comparable(&t.fields, &u.fields),
                // and one from before strata read the role bindings holds their references
                Tier::Policy => crate::snapshot::policy::comparable(&t.fields, &u.fields),
                _ => (t.fields.clone(), u.fields.clone()),
            };
            let (f, g) = (read_fields(f, t, u), read_fields(g, t, u));
            let same = f == g;
            Some(TierDiff {
                tier: *name,
                same,
                diff: if same { Vec::new() } else { leaf_diff(&f, &g) },
            })
        })
        .collect();
    let labels = |m: &std::collections::BTreeMap<String, String>| {
        serde_json::to_value(m).unwrap_or(Value::Null)
    };
    let (la, lb) = (labels(&x.labels), labels(&y.labels));
    out.push(TierDiff {
        tier: Tier::Labels,
        same: la == lb,
        diff: leaf_diff(&la, &lb),
    });
    out
}

/// The packs of a cluster record whose reconciled version differs from the declared one.
pub(super) fn running_behind(side: Side, s: &Snapshot) -> Vec<RunningBehind> {
    let Some(tier) = s
        .source
        .cluster
        .as_ref()
        .and_then(|c| c.tiers.get(&Tier::Running))
    else {
        return Vec::new();
    };
    let running = tier.fields.as_array().cloned().unwrap_or_default();
    s.packs
        .iter()
        .filter_map(|p| {
            let r = running.iter().find(|r| str_at(r, "/name") == p.name)?;
            let version = str_at(r, "/version");
            (!version.is_empty() && !p.version.is_empty() && version != p.version).then(|| {
                RunningBehind {
                    side,
                    pack: p.name.clone(),
                    declared: p.version.clone(),
                    running: version.to_string(),
                }
            })
        })
        .collect()
}

/// One value that differs between two clusters.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ClusterValueChange {
    /// What happened.
    pub op: Op,
    /// Profile name.
    pub profile: String,
    /// Variable name.
    pub name: String,
    /// Value in A; absent for an addition or a sensitive variable.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub old: Option<String>,
    /// Value in B; absent for a removal or a sensitive variable.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub new: Option<String>,
    /// True when the variable is sensitive on either side; its values are never compared.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub sensitive: bool,
}

/// A profile uid that the two sides of a comparison name under different versions.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct OverwrittenProfile {
    /// The profile uid.
    pub uid: String,
    /// The profile name on side A.
    pub name: String,
    /// The version side A names under the uid.
    pub version_a: String,
    /// The profile name on side B, when it differs from side A's.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub name_b: String,
    /// The version side B names under the uid.
    pub version_b: String,
}

/// The profiles a snapshot names, as (uid, name, version): every profile a cluster runs, or the
/// one profile a profile record holds.
fn named_profiles(s: &Snapshot) -> Vec<(&str, &str, &str)> {
    match &s.source.cluster {
        Some(c) => c
            .profiles
            .iter()
            .map(|p| (p.uid.as_str(), p.name.as_str(), p.version.as_str()))
            .collect(),
        None if s.source.uid.is_empty() => Vec::new(),
        None => vec![(
            s.source.uid.as_str(),
            s.source.name.as_str(),
            s.source.version.as_str(),
        )],
    }
}

/// The profile uids that `a` and `b` name under different versions.
pub(super) fn overwritten_in_place(a: &Snapshot, b: &Snapshot) -> Vec<OverwrittenProfile> {
    let named_b = named_profiles(b);
    named_profiles(a)
        .into_iter()
        .filter(|(uid, _, _)| !uid.is_empty())
        .filter_map(|(uid, name, version)| {
            let (_, name_b, version_b) = named_b.iter().find(|(u, _, _)| *u == uid)?;
            (version != *version_b).then(|| OverwrittenProfile {
                uid: uid.to_string(),
                name: name.to_string(),
                version_a: version.to_string(),
                name_b: if *name_b == name {
                    String::new()
                } else {
                    name_b.to_string()
                },
                version_b: version_b.to_string(),
            })
        })
        .collect()
}

/// The values two clusters give their variables, matched by profile name and variable name.
pub(super) fn cluster_values_diff(
    a: &[ClusterVariable],
    b: &[ClusterVariable],
) -> ClusterVariablesDiff {
    fn keyed(vs: &[ClusterVariable]) -> impl Iterator<Item = ((&str, &str), &ClusterVariable)> {
        vs.iter()
            .map(|v| ((v.profile.as_str(), v.name.as_str()), v))
    }
    let values: Vec<ClusterValueChange> = zip_by_key(keyed(a), keyed(b))
        .filter_map(|((profile, name), x, y)| {
            let op = match (x, y) {
                (Some(x), Some(y)) if x.value == y.value && x.sensitive == y.sensitive => {
                    return None;
                }
                (Some(_), Some(_)) => Op::Changed,
                (None, Some(_)) => Op::Added,
                _ => Op::Removed,
            };
            Some(ClusterValueChange {
                op,
                profile: profile.to_string(),
                name: name.to_string(),
                old: x.and_then(|v| v.value.clone()),
                new: y.and_then(|v| v.value.clone()),
                sensitive: x.is_some_and(|v| v.sensitive) || y.is_some_and(|v| v.sensitive),
            })
        })
        .collect();
    ClusterVariablesDiff {
        changed: !values.is_empty(),
        values,
    }
}
