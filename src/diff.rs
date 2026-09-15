//! The diff document (schema strata-diff/1): two snapshots compared. Self-describing on
//! purpose: every enum is spelled out, a legend explains the flags and impact levels, and a summary
//! block answers the common questions without walking the packs.

use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::canon::canonical_json;
use crate::snapshot::{Pack, Snapshot, Source, Tool};

pub const DIFF_SCHEMA: &str = "strata-diff/1";

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Diff {
    /// Always "strata-diff/1"
    pub schema: String,
    pub tool: Tool,
    pub generated_at: String,
    /// What the flags, statuses, ops and impact levels mean
    pub legend: Legend,
    /// The two profiles compared; A is the base, B the candidate
    pub a: Source,
    pub b: Source,
    /// True when the Merkle roots are equal: same release
    pub same: bool,
    pub root_a: String,
    pub root_b: String,
    pub summary: Summary,
    pub variables: VariablesDiff,
    /// True when packs present on both sides appear in a different order
    pub order_changed: bool,
    /// One entry per pack key present on either side, sorted by layer then name
    pub packs: Vec<PackDiff>,
    /// Present only for `verify`
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub verify: Option<Verify>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Legend {
    pub identity: String,
    pub upstream: String,
    pub authored: String,
    pub impact: BTreeMap<String, String>,
    pub status: BTreeMap<String, String>,
    pub op: BTreeMap<String, String>,
}

impl Legend {
    pub fn standard() -> Legend {
        let m = |pairs: &[(&str, &str)]| -> BTreeMap<String, String> {
            pairs
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect()
        };
        Legend {
            identity: "The pack reference changed: name, layer, type, tag, version, packUid or registryUid. The release points at a different pack build.".into(),
            upstream: "The digest on the pack reference changed: pack content moved under the hood, possibly under the same version. Which field fed it is hash.upstreamSource in the snapshot.".into(),
            authored: "Profile-level configuration changed: the pack's values YAML (compared canonically, so formatting does not count), its attached manifests, or its params.".into(),
            impact: m(&[
                ("repave", "Nodes are replaced: OS image or Kubernetes version change."),
                ("may-repave", "Node or control plane configuration changed on the OS or Kubernetes layer; Palette may repave."),
                ("cluster-wide", "CNI or CSI layer changed; affects every node and may repave."),
                ("in-place", "Add-on pack upgraded or reconfigured in place."),
                ("sync", "Only the upstream digest changed; applied on the next profile sync."),
                ("none", "No change."),
            ]),
            status: m(&[
                ("unchanged", "Same node hash on both sides."),
                ("changed", "Node hash differs; see identity, upstream and authored."),
                ("added", "Present only in B."),
                ("removed", "Present only in A."),
            ]),
            op: m(&[
                ("changed", "The leaf exists on both sides with different values."),
                ("added", "The leaf exists only in B."),
                ("removed", "The leaf exists only in A."),
            ]),
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Summary {
    pub packs_added: usize,
    pub packs_removed: usize,
    pub packs_changed: usize,
    pub packs_unchanged: usize,
    /// Packs whose identity hash moved
    pub identity_changes: usize,
    pub upstream_changes: usize,
    pub authored_changes: usize,
    pub variables_changed: bool,
    /// The most severe impact level across all changed packs
    pub max_impact: ImpactLevel,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct VariablesDiff {
    pub changed: bool,
    pub hash_a: String,
    pub hash_b: String,
    /// Leaf changes with paths like "<variable name>.<field>"
    pub diff: Vec<Leaf>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Op {
    Changed,
    Added,
    Removed,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Leaf {
    pub op: Op,
    /// Dotted path; array items as [i], or [name=<value>] when every item has a unique name
    pub path: String,
    /// The same path as segments: strings for keys, integers for indexes, {"name": ...} for named items
    pub segments: Vec<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub old: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub new: Option<Value>,
}

/// Ordered by severity, so `max` picks the worst.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "kebab-case")]
pub enum ImpactLevel {
    #[default]
    None,
    Sync,
    InPlace,
    ClusterWide,
    MayRepave,
    Repave,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Impact {
    pub level: ImpactLevel,
    /// Human sentence, the same text the terminal report prints
    pub text: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    Unchanged,
    Changed,
    Added,
    Removed,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PackSide {
    pub tag: String,
    pub version: String,
    pub pack_uid: String,
    pub registry_uid: String,
    /// The raw digest that fed the upstream hash
    pub upstream: String,
    pub upstream_source: String,
    pub values_hash: String,
    pub node: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PackDiff {
    pub key: String,
    pub layer: String,
    pub name: String,
    pub status: Status,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub a: Option<PackSide>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub b: Option<PackSide>,
    pub identity: bool,
    pub upstream: bool,
    pub authored: bool,
    pub params_changed: bool,
    pub impact: Impact,
    pub values: ValuesDiff,
    pub manifests: Vec<ManifestChange>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ValuesDiff {
    pub changed: bool,
    /// False when one side's values did not parse as YAML; then only the raw text differs and `diff` is empty
    pub parseable: bool,
    pub changed_count: usize,
    pub added_count: usize,
    pub removed_count: usize,
    pub diff: Vec<Leaf>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ManifestChange {
    pub op: Op,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hash_a: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hash_b: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Verify {
    pub status: VerifyStatus,
    pub uid: String,
    pub recorded_at: String,
    pub checked_at: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum VerifyStatus {
    Unchanged,
    Drifted,
}

pub fn layer_rank(layer: &str) -> u8 {
    match layer {
        "os" => 0,
        "k8s" => 1,
        "cni" => 2,
        "csi" => 3,
        "addon" => 4,
        _ => 9,
    }
}

pub fn impact(layer: &str, identity: bool, authored: bool, upstream: bool) -> Impact {
    let (level, text) = if layer == "os" && identity {
        (ImpactLevel::Repave, "node repave (OS image change)")
    } else if layer == "k8s" && identity {
        (
            ImpactLevel::Repave,
            "node repave (Kubernetes version change)",
        )
    } else if (layer == "os" || layer == "k8s") && authored {
        (
            ImpactLevel::MayRepave,
            "node/control plane config change, may repave",
        )
    } else if (layer == "cni" || layer == "csi") && (identity || authored) {
        (ImpactLevel::ClusterWide, "cluster-wide infra addon update")
    } else if identity || authored {
        (ImpactLevel::InPlace, "in-place addon upgrade")
    } else if upstream {
        (
            ImpactLevel::Sync,
            "pack content changed under the same tag, applied on next profile sync",
        )
    } else {
        (ImpactLevel::None, "")
    };
    Impact {
        level,
        text: text.to_string(),
    }
}

enum Seg {
    Key(String),
    Index(usize),
    Named(String),
}

fn render_path(segs: &[Seg]) -> String {
    let mut out = String::new();
    for seg in segs {
        match seg {
            Seg::Key(k) => {
                if !out.is_empty() {
                    out.push('.');
                }
                out.push_str(k);
            }
            Seg::Index(i) => out.push_str(&format!("[{i}]")),
            Seg::Named(n) => out.push_str(&format!("[name={n}]")),
        }
    }
    out
}

fn segments_json(segs: &[Seg]) -> Vec<Value> {
    segs.iter()
        .map(|s| match s {
            Seg::Key(k) => Value::String(k.clone()),
            Seg::Index(i) => Value::from(*i),
            Seg::Named(n) => json!({ "name": n }),
        })
        .collect()
}

/// When every item is an object with a unique string `name`, address items by name so that an
/// insertion shows as one addition rather than shifted neighbors.
fn name_keys(items: &[Value]) -> Option<Vec<String>> {
    let mut names = Vec::with_capacity(items.len());
    for it in items {
        let n = it.as_object()?.get("name")?.as_str()?;
        if names.iter().any(|x: &String| x == n) {
            return None;
        }
        names.push(n.to_string());
    }
    Some(names)
}

type Leaves = BTreeMap<String, (Vec<Value>, Value)>;

fn flatten(v: &Value, path: &mut Vec<Seg>, out: &mut Leaves) {
    match v {
        Value::Object(m) if !m.is_empty() => {
            for (k, x) in m {
                path.push(Seg::Key(k.clone()));
                flatten(x, path, out);
                path.pop();
            }
        }
        Value::Array(a) if !a.is_empty() => {
            if let Some(names) = name_keys(a) {
                for (x, n) in a.iter().zip(names) {
                    path.push(Seg::Named(n));
                    flatten(x, path, out);
                    path.pop();
                }
            } else {
                for (i, x) in a.iter().enumerate() {
                    path.push(Seg::Index(i));
                    flatten(x, path, out);
                    path.pop();
                }
            }
        }
        _ => {
            out.insert(render_path(path), (segments_json(path), v.clone()));
        }
    }
}

pub fn leaf_diff(a: &Value, b: &Value) -> Vec<Leaf> {
    let (mut la, mut lb) = (Leaves::new(), Leaves::new());
    flatten(a, &mut Vec::new(), &mut la);
    flatten(b, &mut Vec::new(), &mut lb);
    // An empty or scalar document on one side against a structured document on the other reads
    // as additions or removals of the structure, not as a change at the root path.
    if la.keys().chain(lb.keys()).any(|k| !k.is_empty()) {
        la.remove("");
        lb.remove("");
    }
    let mut keys: Vec<&String> = la.keys().chain(lb.keys()).collect();
    keys.sort();
    keys.dedup();
    let mut out = Vec::new();
    for k in keys {
        match (la.get(k), lb.get(k)) {
            (Some((segs, x)), Some((_, y))) => {
                if x != y {
                    out.push(Leaf {
                        op: Op::Changed,
                        path: k.clone(),
                        segments: segs.clone(),
                        old: Some(x.clone()),
                        new: Some(y.clone()),
                    });
                }
            }
            (Some((segs, x)), None) => out.push(Leaf {
                op: Op::Removed,
                path: k.clone(),
                segments: segs.clone(),
                old: Some(x.clone()),
                new: None,
            }),
            (None, Some((segs, y))) => out.push(Leaf {
                op: Op::Added,
                path: k.clone(),
                segments: segs.clone(),
                old: None,
                new: Some(y.clone()),
            }),
            (None, None) => {}
        }
    }
    out
}

fn side(p: &Pack) -> PackSide {
    PackSide {
        tag: p.tag.clone(),
        version: p.version.clone(),
        pack_uid: p.pack_uid.clone(),
        registry_uid: p.registry_uid.clone(),
        upstream: p.hash.upstream_raw.clone(),
        upstream_source: p.hash.upstream_source.clone(),
        values_hash: p.values_hash.clone(),
        node: p.hash.node.clone(),
    }
}

fn values_diff(x: &Pack, y: &Pack) -> ValuesDiff {
    let changed = x.values_hash != y.values_hash;
    let parseable = x.values_parse_error.is_none() && y.values_parse_error.is_none();
    let diff = if changed && parseable {
        leaf_diff(&x.values_canon, &y.values_canon)
    } else {
        Vec::new()
    };
    let count = |op: Op| diff.iter().filter(|l| l.op == op).count();
    ValuesDiff {
        changed,
        parseable,
        changed_count: count(Op::Changed),
        added_count: count(Op::Added),
        removed_count: count(Op::Removed),
        diff,
    }
}

fn manifests_diff(x: &Pack, y: &Pack) -> Vec<ManifestChange> {
    let mx: BTreeMap<&str, &str> = x
        .manifests
        .iter()
        .map(|m| (m.name.as_str(), m.content_hash.as_str()))
        .collect();
    let my: BTreeMap<&str, &str> = y
        .manifests
        .iter()
        .map(|m| (m.name.as_str(), m.content_hash.as_str()))
        .collect();
    let mut names: Vec<&str> = mx.keys().chain(my.keys()).copied().collect();
    names.sort();
    names.dedup();
    let mut out = Vec::new();
    for n in names {
        match (mx.get(n), my.get(n)) {
            (Some(a), Some(b)) => {
                if a != b {
                    out.push(ManifestChange {
                        op: Op::Changed,
                        name: n.to_string(),
                        hash_a: Some(a.to_string()),
                        hash_b: Some(b.to_string()),
                    });
                }
            }
            (Some(a), None) => out.push(ManifestChange {
                op: Op::Removed,
                name: n.to_string(),
                hash_a: Some(a.to_string()),
                hash_b: None,
            }),
            (None, Some(b)) => out.push(ManifestChange {
                op: Op::Added,
                name: n.to_string(),
                hash_a: None,
                hash_b: Some(b.to_string()),
            }),
            (None, None) => {}
        }
    }
    out
}

fn by_name(vars: &[Value]) -> Value {
    let mut m = serde_json::Map::new();
    for v in vars {
        let name = v
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        m.insert(name, v.clone());
    }
    Value::Object(m)
}

pub fn compute(a: &Snapshot, b: &Snapshot, generated_at: &str, verify: Option<Verify>) -> Diff {
    let pa: BTreeMap<&str, &Pack> = a.packs.iter().map(|p| (p.key.as_str(), p)).collect();
    let pb: BTreeMap<&str, &Pack> = b.packs.iter().map(|p| (p.key.as_str(), p)).collect();
    let mut keys: Vec<&str> = pa.keys().chain(pb.keys()).copied().collect();
    keys.sort();
    keys.dedup();

    let mut summary = Summary::default();
    let mut packs = Vec::with_capacity(keys.len());
    for k in keys {
        let pd = match (pa.get(k), pb.get(k)) {
            (None, Some(y)) => {
                summary.packs_added += 1;
                PackDiff {
                    key: k.to_string(),
                    layer: y.layer.clone(),
                    name: y.name.clone(),
                    status: Status::Added,
                    a: None,
                    b: Some(side(y)),
                    identity: false,
                    upstream: false,
                    authored: false,
                    params_changed: false,
                    impact: Impact {
                        level: ImpactLevel::None,
                        text: String::new(),
                    },
                    values: ValuesDiff::default(),
                    manifests: Vec::new(),
                }
            }
            (Some(x), None) => {
                summary.packs_removed += 1;
                PackDiff {
                    key: k.to_string(),
                    layer: x.layer.clone(),
                    name: x.name.clone(),
                    status: Status::Removed,
                    a: Some(side(x)),
                    b: None,
                    identity: false,
                    upstream: false,
                    authored: false,
                    params_changed: false,
                    impact: Impact {
                        level: ImpactLevel::None,
                        text: String::new(),
                    },
                    values: ValuesDiff::default(),
                    manifests: Vec::new(),
                }
            }
            (Some(x), Some(y)) => {
                let identity = x.hash.identity != y.hash.identity;
                let upstream = x.hash.upstream != y.hash.upstream;
                let authored = x.hash.authored != y.hash.authored;
                let status = if x.hash.node == y.hash.node {
                    Status::Unchanged
                } else {
                    Status::Changed
                };
                match status {
                    Status::Changed => summary.packs_changed += 1,
                    _ => summary.packs_unchanged += 1,
                }
                summary.identity_changes += usize::from(identity);
                summary.upstream_changes += usize::from(upstream);
                summary.authored_changes += usize::from(authored);
                let imp = impact(&x.layer, identity, authored, upstream);
                if imp.level > summary.max_impact {
                    summary.max_impact = imp.level;
                }
                PackDiff {
                    key: k.to_string(),
                    layer: x.layer.clone(),
                    name: x.name.clone(),
                    status,
                    a: Some(side(x)),
                    b: Some(side(y)),
                    identity,
                    upstream,
                    authored,
                    params_changed: canonical_json(&x.params) != canonical_json(&y.params),
                    impact: imp,
                    values: values_diff(x, y),
                    manifests: manifests_diff(x, y),
                }
            }
            (None, None) => unreachable!("key came from one of the maps"),
        };
        packs.push(pd);
    }
    packs.sort_by(|p, q| (layer_rank(&p.layer), &p.name).cmp(&(layer_rank(&q.layer), &q.name)));

    let order_a: Vec<&str> = a
        .packs
        .iter()
        .map(|p| p.key.as_str())
        .filter(|k| pb.contains_key(k))
        .collect();
    let order_b: Vec<&str> = b
        .packs
        .iter()
        .map(|p| p.key.as_str())
        .filter(|k| pa.contains_key(k))
        .collect();

    let variables_changed = a.variables_hash != b.variables_hash;
    summary.variables_changed = variables_changed;

    Diff {
        schema: DIFF_SCHEMA.to_string(),
        tool: Tool {
            name: crate::TOOL_NAME.to_string(),
            version: crate::TOOL_VERSION.to_string(),
            implementation: Some(crate::TOOL_IMPL.to_string()),
        },
        generated_at: generated_at.to_string(),
        legend: Legend::standard(),
        a: a.source.clone(),
        b: b.source.clone(),
        same: a.root == b.root,
        root_a: a.root.clone(),
        root_b: b.root.clone(),
        summary,
        variables: VariablesDiff {
            changed: variables_changed,
            hash_a: a.variables_hash.clone(),
            hash_b: b.variables_hash.clone(),
            diff: if variables_changed {
                leaf_diff(&by_name(&a.variables), &by_name(&b.variables))
            } else {
                Vec::new()
            },
        },
        order_changed: order_a != order_b,
        packs,
        verify,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn leaf_diff_reports_changed_added_removed_and_keeps_false_and_null() {
        let a = json!({"x": {"on": true, "n": null, "gone": 1}, "list": [1, 2]});
        let b = json!({"x": {"on": false, "n": null, "new": "v"}, "list": [1, 2, 3]});
        let d = leaf_diff(&a, &b);
        let paths: Vec<(Op, &str)> = d.iter().map(|l| (l.op, l.path.as_str())).collect();
        assert_eq!(
            paths,
            vec![
                (Op::Added, "list[2]"),
                (Op::Removed, "x.gone"),
                (Op::Added, "x.new"),
                (Op::Changed, "x.on"),
            ]
        );
    }

    #[test]
    fn arrays_of_named_objects_are_addressed_by_name() {
        let a = json!({"env": [{"name": "A", "value": "1"}, {"name": "B", "value": "2"}]});
        let b = json!({"env": [{"name": "Z", "value": "0"}, {"name": "A", "value": "1"}, {"name": "B", "value": "3"}]});
        let d = leaf_diff(&a, &b);
        let paths: Vec<(Op, &str)> = d.iter().map(|l| (l.op, l.path.as_str())).collect();
        assert_eq!(
            paths,
            vec![
                (Op::Changed, "env[name=B].value"),
                (Op::Added, "env[name=Z].name"),
                (Op::Added, "env[name=Z].value"),
            ]
        );
        assert_eq!(
            d[0].segments,
            vec![json!("env"), json!({"name": "B"}), json!("value")]
        );
    }

    #[test]
    fn empty_or_null_root_against_structure_shows_only_the_structure() {
        for empty in [json!({}), json!(null), json!([])] {
            let d = leaf_diff(&empty, &json!({"a": {"b": 1}}));
            assert_eq!(d.len(), 1, "from {empty}");
            assert_eq!((d[0].op, d[0].path.as_str()), (Op::Added, "a.b"));
            let d = leaf_diff(&json!({"a": {"b": 1}}), &empty);
            assert_eq!((d[0].op, d[0].path.as_str()), (Op::Removed, "a.b"));
        }
    }

    #[test]
    fn empty_containers_are_leaves() {
        let d = leaf_diff(&json!({"a": {}}), &json!({"a": []}));
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].op, Op::Changed);
    }

    #[test]
    fn impact_levels_order_by_severity() {
        assert!(ImpactLevel::Repave > ImpactLevel::MayRepave);
        assert!(ImpactLevel::MayRepave > ImpactLevel::ClusterWide);
        assert!(ImpactLevel::ClusterWide > ImpactLevel::InPlace);
        assert!(ImpactLevel::InPlace > ImpactLevel::Sync);
        assert!(ImpactLevel::Sync > ImpactLevel::None);
        assert_eq!(impact("k8s", true, true, true).level, ImpactLevel::Repave);
        assert_eq!(impact("cni", false, false, true).level, ImpactLevel::Sync);
        assert_eq!(
            impact("addon", false, true, false).level,
            ImpactLevel::InPlace
        );
    }
}
