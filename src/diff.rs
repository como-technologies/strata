//! The diff document (schema strata-diff/1): two snapshots compared. Self-describing on
//! purpose: every enum is spelled out, a legend explains the flags and impact levels, and a summary
//! block answers the common questions without walking the packs.

use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::canon::canonical_json;
use crate::layer::Layer;
use crate::snapshot::{Pack, Snapshot, Source, Tool};

/// Schema id written into every diff document.
pub const DIFF_SCHEMA: &str = "strata-diff/1";

/// Two snapshots compared.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Diff {
    /// Always "strata-diff/1"
    pub schema: String,
    /// Which tool wrote the document.
    pub tool: Tool,
    /// When, RFC 3339 UTC.
    pub generated_at: String,
    /// What the flags, statuses, ops and impact levels mean
    pub legend: Legend,
    /// The two profiles compared; A is the base, B the candidate
    pub a: Source,
    /// The candidate side.
    pub b: Source,
    /// True when the Merkle roots are equal: same release
    pub same: bool,
    /// Root of A.
    pub root_a: String,
    /// Root of B.
    pub root_b: String,
    /// Counts and the maximum impact.
    pub summary: Summary,
    /// Profile variables compared.
    pub variables: VariablesDiff,
    /// True when packs present on both sides appear in a different order
    pub order_changed: bool,
    /// One entry per pack key present on either side, sorted by layer then name
    pub packs: Vec<PackDiff>,
    /// Present only for `verify`
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub verify: Option<Verify>,
}

/// Plain-language meaning of every flag and enum value, for readers without the docs.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Legend {
    /// The IDENTITY flag.
    pub identity: String,
    /// The UPSTREAM flag.
    pub upstream: String,
    /// The AUTHORED flag.
    pub authored: String,
    /// Each impact level.
    pub impact: BTreeMap<String, String>,
    /// Each pack status.
    pub status: BTreeMap<String, String>,
    /// Each leaf op.
    pub op: BTreeMap<String, String>,
}

impl Legend {
    /// The legend every document carries.
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

/// Counts that answer the common questions first.
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Summary {
    /// Packs only in B.
    pub packs_added: usize,
    /// Packs only in A.
    pub packs_removed: usize,
    /// Packs on both sides with a different node hash.
    pub packs_changed: usize,
    /// Packs on both sides with the same node hash.
    pub packs_unchanged: usize,
    /// Packs whose identity hash moved
    pub identity_changes: usize,
    /// Packs whose upstream hash moved.
    pub upstream_changes: usize,
    /// Packs whose authored hash moved.
    pub authored_changes: usize,
    /// True when the variables hash differs.
    pub variables_changed: bool,
    /// The most severe impact level across all changed packs
    pub max_impact: ImpactLevel,
}

/// Profile variables compared by name.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct VariablesDiff {
    /// True when the hashes differ.
    pub changed: bool,
    /// Variables hash of A.
    pub hash_a: String,
    /// Variables hash of B.
    pub hash_b: String,
    /// Leaf changes with paths like "<variable name>.<field>"
    pub diff: Vec<Leaf>,
    /// The same changes grouped per variable, the form the text reports print
    #[serde(default)]
    pub by_name: VariablesByName,
}

/// Variable changes grouped by variable name.
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
pub struct VariablesByName {
    /// Variables only in B.
    pub added: Vec<VariableSummary>,
    /// Variables only in A.
    pub removed: Vec<VariableSummary>,
    /// Variables on both sides with different fields.
    pub changed: Vec<VariableChange>,
}

/// One profile variable in brief: its name and the traits a reviewer cares about.
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
pub struct VariableSummary {
    /// Variable name.
    pub name: String,
    /// The variable must be set.
    pub required: bool,
    /// The value is a secret (`isSensitive` on the variable).
    pub sensitive: bool,
    /// The variable is not shown in the UI.
    pub hidden: bool,
    /// The value cannot change after the cluster is created.
    pub immutable: bool,
}

impl VariableSummary {
    /// The traits that are set, in the order the reports print them.
    pub fn traits(&self) -> Vec<&'static str> {
        [
            (self.required, "required"),
            (self.sensitive, "sensitive"),
            (self.hidden, "hidden"),
            (self.immutable, "immutable"),
        ]
        .into_iter()
        .filter_map(|(on, word)| on.then_some(word))
        .collect()
    }

    fn from_value(v: &Value) -> VariableSummary {
        let flag = |k: &str| v.get(k).and_then(Value::as_bool).unwrap_or(false);
        VariableSummary {
            name: v
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
            required: flag("required"),
            sensitive: flag("isSensitive"),
            hidden: flag("hidden"),
            immutable: flag("immutable"),
        }
    }
}

/// One variable present on both sides with different fields.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct VariableChange {
    /// Variable name.
    pub name: String,
    /// The fields that differ, as leaves with paths relative to the variable, for example `defaultValue`
    pub fields: Vec<Leaf>,
}

/// What happened to one leaf or manifest.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Op {
    /// Present on both sides with different values.
    Changed,
    /// Present only in B.
    Added,
    /// Present only in A.
    Removed,
}

/// One scalar (or empty container) that differs.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Leaf {
    /// What happened.
    pub op: Op,
    /// Dotted path; array items as [i], or [name=<value>] when every item has a unique name
    pub path: String,
    /// The same path as segments: strings for keys, integers for indexes, {"name": ...} for named items
    pub segments: Vec<Value>,
    /// Value in A, absent for additions.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub old: Option<Value>,
    /// Value in B, absent for removals.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub new: Option<Value>,
}

/// Ordered by severity, so `max` picks the worst.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "kebab-case")]
pub enum ImpactLevel {
    /// No change.
    #[default]
    None,
    /// Only the upstream digest changed; applied on the next profile sync.
    Sync,
    /// Add-on upgraded or reconfigured in place.
    InPlace,
    /// CNI or CSI changed; every node affected, may repave.
    ClusterWide,
    /// OS or Kubernetes configuration changed; Palette may repave.
    MayRepave,
    /// Nodes are replaced.
    Repave,
}

/// What a change does to running clusters; a hint from the layer and the flags.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Impact {
    /// Severity level.
    pub level: ImpactLevel,
    /// Human sentence, the same text the terminal report prints
    pub text: String,
}

/// What happened to one pack.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    /// Same node hash on both sides.
    Unchanged,
    /// Different node hash.
    Changed,
    /// Only in B.
    Added,
    /// Only in A.
    Removed,
}

/// One side of a pack comparison.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PackSide {
    /// Tag pinned on the profile.
    pub tag: String,
    /// Resolved version.
    pub version: String,
    /// Pack version uid.
    pub pack_uid: String,
    /// Registry uid.
    pub registry_uid: String,
    /// The raw digest that fed the upstream hash
    pub upstream: String,
    /// Which field fed it: `packRef.digest`, `packSummary.digest`, or `none`.
    pub upstream_source: String,
    /// Hash of the canonical values.
    pub values_hash: String,
    /// Node hash.
    pub node: String,
}

/// One pack across the two sides.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PackDiff {
    /// `<layer>/<name>`.
    pub key: String,
    /// Palette layer name.
    pub layer: String,
    /// Pack name.
    pub name: String,
    /// What happened to the pack.
    pub status: Status,
    /// Side A; absent when the pack was added.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub a: Option<PackSide>,
    /// Side B; absent when the pack was removed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub b: Option<PackSide>,
    /// The identity hash moved.
    pub identity: bool,
    /// The upstream hash moved.
    pub upstream: bool,
    /// The authored hash moved.
    pub authored: bool,
    /// The params differ.
    pub params_changed: bool,
    /// What the change does to clusters.
    pub impact: Impact,
    /// The values compared.
    pub values: ValuesDiff,
    /// Manifest changes by name.
    pub manifests: Vec<ManifestChange>,
}

/// The values of one pack compared.
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ValuesDiff {
    /// True when the values hashes differ.
    pub changed: bool,
    /// False when one side's values did not parse as YAML; then only the raw text differs and `diff` is empty
    pub parseable: bool,
    /// Leaves changed.
    pub changed_count: usize,
    /// Leaves added.
    pub added_count: usize,
    /// Leaves removed.
    pub removed_count: usize,
    /// The leaves, sorted by path.
    pub diff: Vec<Leaf>,
}

/// One manifest across the two sides.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ManifestChange {
    /// What happened.
    pub op: Op,
    /// Manifest name.
    pub name: String,
    /// Content hash in A.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hash_a: Option<String>,
    /// Content hash in B.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hash_b: Option<String>,
}

/// The outcome of `verify`, present only in its documents.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Verify {
    /// Unchanged or drifted.
    pub status: VerifyStatus,
    /// Profile uid that was re-fetched.
    pub uid: String,
    /// When the record was taken.
    pub recorded_at: String,
    /// When the live profile was fetched.
    pub checked_at: String,
}

/// Whether a published version still matches its record.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum VerifyStatus {
    /// Same root.
    Unchanged,
    /// Different root: the published version was edited, or the record is stale.
    Drifted,
}

/// Which of the three hashes moved for one pack.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Flags {
    /// The identity hash moved.
    pub identity: bool,
    /// The upstream hash moved.
    pub upstream: bool,
    /// The authored hash moved.
    pub authored: bool,
}

/// Which layers a rule applies to.
#[derive(Debug, Clone, Copy)]
enum Layers {
    Only(Layer),
    NodeImage,
    InfraAddon,
    Any,
}

impl Layers {
    fn matches(self, layer: Layer) -> bool {
        match self {
            Layers::Only(l) => layer == l,
            Layers::NodeImage => matches!(layer, Layer::Kernel | Layer::Os | Layer::K8s),
            Layers::InfraAddon => matches!(layer, Layer::Cni | Layer::Csi),
            Layers::Any => true,
        }
    }
}

/// Which flags a rule needs.
#[derive(Debug, Clone, Copy)]
enum Needs {
    Identity,
    Authored,
    IdentityOrAuthored,
    UpstreamOnly,
}

impl Needs {
    fn matches(self, f: Flags) -> bool {
        match self {
            Needs::Identity => f.identity,
            Needs::Authored => f.authored,
            Needs::IdentityOrAuthored => f.identity || f.authored,
            Needs::UpstreamOnly => f.upstream && !f.identity && !f.authored,
        }
    }
}

struct Rule {
    layers: Layers,
    needs: Needs,
    level: ImpactLevel,
    text: &'static str,
}

/// The impact rules, first match wins. The order encodes severity: a Kubernetes version bump is a
/// repave even when its values also changed.
const RULES: &[Rule] = &[
    Rule {
        layers: Layers::Only(Layer::Os),
        needs: Needs::Identity,
        level: ImpactLevel::Repave,
        text: "node repave (OS image change)",
    },
    Rule {
        layers: Layers::Only(Layer::K8s),
        needs: Needs::Identity,
        level: ImpactLevel::Repave,
        text: "node repave (Kubernetes version change)",
    },
    Rule {
        layers: Layers::Only(Layer::Kernel),
        needs: Needs::Identity,
        level: ImpactLevel::Repave,
        text: "node repave (kernel change)",
    },
    Rule {
        layers: Layers::NodeImage,
        needs: Needs::Authored,
        level: ImpactLevel::MayRepave,
        text: "node/control plane config change, may repave",
    },
    Rule {
        layers: Layers::InfraAddon,
        needs: Needs::IdentityOrAuthored,
        level: ImpactLevel::ClusterWide,
        text: "cluster-wide infra addon update",
    },
    Rule {
        layers: Layers::Any,
        needs: Needs::IdentityOrAuthored,
        level: ImpactLevel::InPlace,
        text: "in-place addon upgrade",
    },
    Rule {
        layers: Layers::Any,
        needs: Needs::UpstreamOnly,
        level: ImpactLevel::Sync,
        text: "pack content changed under the same tag, applied on next profile sync",
    },
];

/// The impact of a change on a layer, from the rules table.
pub fn impact(layer: Layer, flags: Flags) -> Impact {
    RULES
        .iter()
        .find(|r| r.layers.matches(layer) && r.needs.matches(flags))
        .map(|r| Impact {
            level: r.level,
            text: r.text.to_string(),
        })
        .unwrap_or(Impact {
            level: ImpactLevel::None,
            text: String::new(),
        })
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

/// Every scalar or empty container that differs between two JSON documents, sorted by path.
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

/// Group two normalized variable lists by name: added, removed, and changed with the fields that
/// differ. Names are sorted.
pub fn group_variables(a: &[Value], b: &[Value]) -> VariablesByName {
    let (ma, mb) = (by_name(a), by_name(b));
    let (ma, mb) = (
        ma.as_object().cloned().unwrap_or_default(),
        mb.as_object().cloned().unwrap_or_default(),
    );
    let mut names: Vec<&String> = ma.keys().chain(mb.keys()).collect();
    names.sort();
    names.dedup();
    let mut g = VariablesByName::default();
    for n in names {
        match (ma.get(n), mb.get(n)) {
            (None, Some(y)) => g.added.push(VariableSummary::from_value(y)),
            (Some(x), None) => g.removed.push(VariableSummary::from_value(x)),
            (Some(x), Some(y)) => {
                let fields = leaf_diff(x, y);
                if !fields.is_empty() {
                    g.changed.push(VariableChange {
                        name: n.clone(),
                        fields,
                    });
                }
            }
            (None, None) => {}
        }
    }
    g
}

/// Compare two snapshots. `verify` is attached when this is the second half of a verification.
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
                let imp = impact(
                    Layer::parse(&x.layer),
                    Flags {
                        identity,
                        upstream,
                        authored,
                    },
                );
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
    packs.sort_by(|p, q| {
        (Layer::parse(&p.layer).rank(), &p.name).cmp(&(Layer::parse(&q.layer).rank(), &q.name))
    });

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
            by_name: if variables_changed {
                group_variables(&a.variables, &b.variables)
            } else {
                VariablesByName::default()
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
    fn variables_are_grouped_by_name_with_relative_field_paths() {
        let var = |name: &str, default: &str, sensitive: bool| {
            json!({"name": name, "defaultValue": default, "required": true, "hidden": false,
                   "immutable": false, "isSensitive": sensitive, "format": "string"})
        };
        let a = vec![var("cluster_env", "dev", false), var("gone", "", true)];
        let b = vec![
            var("cluster_env", "staging", false),
            var("db_password", "", true),
        ];
        let g = group_variables(&a, &b);
        assert_eq!(g.added.len(), 1);
        assert_eq!(g.added[0].name, "db_password");
        assert!(g.added[0].required && g.added[0].sensitive && !g.added[0].hidden);
        assert_eq!(g.removed.len(), 1);
        assert_eq!(g.removed[0].name, "gone");
        assert_eq!(g.changed.len(), 1);
        assert_eq!(g.changed[0].name, "cluster_env");
        assert_eq!(g.changed[0].fields.len(), 1);
        assert_eq!(g.changed[0].fields[0].path, "defaultValue");
        assert_eq!(g.changed[0].fields[0].segments, vec![json!("defaultValue")]);
        assert_eq!(g.changed[0].fields[0].old, Some(json!("dev")));
        assert_eq!(g.changed[0].fields[0].new, Some(json!("staging")));
        assert_eq!(
            g.added[0].traits(),
            vec!["required", "sensitive"],
            "the words the report prints in parentheses"
        );
    }

    #[test]
    fn impact_levels_order_by_severity() {
        assert!(ImpactLevel::Repave > ImpactLevel::MayRepave);
        assert!(ImpactLevel::MayRepave > ImpactLevel::ClusterWide);
        assert!(ImpactLevel::ClusterWide > ImpactLevel::InPlace);
        assert!(ImpactLevel::InPlace > ImpactLevel::Sync);
        assert!(ImpactLevel::Sync > ImpactLevel::None);
        let f = |identity, upstream, authored| Flags {
            identity,
            upstream,
            authored,
        };
        assert_eq!(
            impact(Layer::K8s, f(true, true, true)).level,
            ImpactLevel::Repave
        );
        assert_eq!(
            impact(Layer::Cni, f(false, true, false)).level,
            ImpactLevel::Sync
        );
        assert_eq!(
            impact(Layer::Addon, f(false, false, true)).level,
            ImpactLevel::InPlace
        );
        assert_eq!(
            impact(Layer::Os, f(false, false, true)).level,
            ImpactLevel::MayRepave
        );
        assert_eq!(
            impact(Layer::Other, f(true, false, false)).level,
            ImpactLevel::InPlace
        );
        assert_eq!(
            impact(Layer::Addon, f(false, false, false)).level,
            ImpactLevel::None
        );
    }
}
