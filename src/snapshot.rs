//! The snapshot document (schema strata-snapshot/1): one profile version with its root,
//! per-pack hashes, canonical values and raw contents. Byte-compatible with the snapshots
//! written by the bash reference implementation.

use std::collections::BTreeMap;
use std::path::Path;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};

use crate::api::{RawCluster, RawProfile};
use crate::canon::{Canon, canon_yaml, canonical_json, hash_yaml, sha256_hex};
use crate::error::{Error, Result};

/// Schema id written into every snapshot.
pub const SNAPSHOT_SCHEMA: &str = "strata-snapshot/1";

/// The part of a snapshot schema id shared by every version of it.
const SNAPSHOT_FAMILY: &str = "strata-snapshot/";

/// The version of the content digest encoding. It is in the input of the digest, so a change to
/// what the digest covers is a change of this tag, and a digest of one version is never equal to
/// a digest of another. A tool that pins a version by content digest depends on this tag.
pub const CONTENT_DIGEST: &str = "content/1";

/// One published profile version: its root, per-pack hashes, canonical values, and raw contents.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    /// The schema id of this document
    #[schemars(extend("const" = SNAPSHOT_SCHEMA))]
    pub schema: String,
    /// Which tool wrote the snapshot.
    pub tool: Tool,
    /// Where it came from.
    pub source: Source,
    /// Profile variables, normalized and sorted by name
    pub variables: Vec<Value>,
    /// sha256 of the canonical JSON of `variables`
    pub variables_hash: String,
    /// Packs in profile order
    pub packs: Vec<Pack>,
    /// sha256 of canonical JSON {"packs": [[key, node]...], "variables": variablesHash}
    pub root: String,
    /// The content digest, as [`Snapshot::content_digest`] gives it. A record from before this
    /// field has none, so read the digest with the function and not from this field.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    content_digest: String,
}

/// One pack that a profile version takes from a registry: the reference that another tool
/// pins by. See [`Snapshot::pins`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PinRef {
    /// Pack name.
    pub name: String,
    /// Resolved version.
    pub version: String,
    /// The digest of the pack in its registry, as Palette stores it. A Helm digest is bare hex.
    pub digest: String,
}

/// Which tool wrote a document.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Tool {
    /// Always "strata".
    pub name: String,
    /// Crate version.
    pub version: String,
    /// "rust" or "bash"; absent in snapshots from the first bash release
    #[serde(default, skip_serializing_if = "Option::is_none", rename = "impl")]
    pub implementation: Option<String>,
}

impl Tool {
    /// This build of strata, as every document it writes records it.
    pub fn current() -> Tool {
        Tool {
            name: crate::TOOL_NAME.to_string(),
            version: crate::TOOL_VERSION.to_string(),
            implementation: Some(crate::TOOL_IMPL.to_string()),
        }
    }
}

/// Where a snapshot came from.
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Source {
    /// The ref the snapshot was taken from, as given on the command line
    #[serde(rename = "ref")]
    pub reference: String,
    /// Profile uid.
    pub uid: String,
    /// The Palette project the snapshot was taken in; empty for tenant scope. `verify` checks the
    /// record in this project unless told otherwise.
    #[serde(default)]
    pub project_uid: String,
    /// Profile name.
    pub name: String,
    /// Profile version string.
    pub version: String,
    /// Cloud type of the template, for example `aws`.
    pub cloud_type: String,
    /// Profile type: cluster, infra, add-on, system
    #[serde(rename = "type")]
    pub kind: String,
    /// "published" or "draft": the template the packs were read from
    pub template: String,
    /// Profile labels.
    #[serde(default)]
    pub tags: BTreeMap<String, String>,
    /// Clusters using this profile version at snapshot time. Absent when no request asked: a
    /// candidate is not in Palette, and a cluster record and a pack record are not a version that
    /// a cluster can use. `0` there would state a count that nothing counted.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub consumers: Option<Consumers>,
    /// True when Palette did not serve the variables endpoint, so the snapshot was taken as if
    /// the version had no variables. The root is the root of a version with no variables, so a
    /// record with this flag does not compare equal to one taken from a Palette that serves the
    /// endpoint. The flag is the only thing that tells the two apart.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub variables_unavailable: bool,
    /// When the snapshot was taken, RFC 3339 UTC.
    pub fetched_at: String,
    /// Present when the snapshot records what a cluster runs rather than a published profile.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cluster: Option<ClusterSource>,
    /// Present when the snapshot records one pack version from a registry.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pack: Option<PackSource>,
}

/// The registry pack a snapshot was taken from.
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PackSource {
    /// Pack name.
    pub name: String,
    /// Pack version.
    pub version: String,
    /// Uid of the registry the pack was read from.
    pub registry_uid: String,
    /// Name of that registry, when Palette lists one.
    pub registry_name: String,
    /// Uid of the pack version in the registry.
    pub pack_uid: String,
}

impl Source {
    /// `name@version`, and `on cluster <name>` for a snapshot of a cluster.
    pub fn title(&self) -> String {
        let profile = format!("{}@{}", self.name, self.version);
        if self.pack.is_some() {
            return format!("pack {profile}");
        }
        match &self.cluster {
            None => profile,
            Some(c) if self.name.is_empty() => format!("cluster {}", c.name),
            Some(c) => format!("{profile} on cluster {}", c.name),
        }
    }

    /// The uid a reader looks up: the cluster's for a cluster snapshot, else the profile's.
    pub fn shown_uid(&self) -> &str {
        match (&self.pack, &self.cluster) {
            (Some(p), _) => &p.pack_uid,
            (None, Some(c)) => &c.uid,
            (None, None) => &self.uid,
        }
    }
}

/// The cluster a snapshot was taken from.
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ClusterSource {
    /// Cluster uid.
    pub uid: String,
    /// Cluster name.
    pub name: String,
    /// The applied profile selected with `cluster:<uid>/<profile>`; absent when the snapshot
    /// covers every applied profile.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profile: Option<String>,
    /// The applied profiles this snapshot covers, in the canonical order of
    /// [`AppliedProfile::of_cluster`]: the cluster or infra profile first, then the others by
    /// name.
    pub profiles: Vec<AppliedProfile>,
    /// The values the cluster gives the variables of those profiles, in profile order and then by
    /// name. Outside the root: two clusters on one release can differ here. A sensitive variable
    /// is listed without its value.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub variables: Vec<ClusterVariable>,
}

/// The value a cluster gives one profile variable.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ClusterVariable {
    /// Name of the profile the variable belongs to.
    pub profile: String,
    /// Uid of that profile.
    pub profile_uid: String,
    /// Variable name.
    pub name: String,
    /// The value; absent for a sensitive variable, which no document records, not even hashed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
    /// True for a sensitive variable.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub sensitive: bool,
}

impl ClusterVariable {
    /// The values the cluster in `raw` gives the variables of `profiles`, in that order and then
    /// by name.
    fn read(raw: &RawCluster, profiles: &[AppliedProfile]) -> Vec<ClusterVariable> {
        let entries = raw.variables.as_array().cloned().unwrap_or_default();
        let mut out = Vec::new();
        for p in profiles {
            let mut of_profile: Vec<ClusterVariable> = entries
                .iter()
                .filter(|e| s(e, "/profileUid") == p.uid)
                .flat_map(|e| arr(e, "/variables"))
                .map(|v| {
                    let sensitive = v.get("isSensitive").and_then(Value::as_bool) == Some(true);
                    ClusterVariable {
                        profile: p.name.clone(),
                        profile_uid: p.uid.clone(),
                        name: s(&v, "/name"),
                        value: (!sensitive).then(|| s(&v, "/value")),
                        sensitive,
                    }
                })
                .collect();
            of_profile.sort_by(|a, b| a.name.cmp(&b.name));
            out.extend(of_profile);
        }
        out
    }
}

/// One cluster profile version as applied to a cluster.
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
pub struct AppliedProfile {
    /// Profile uid.
    pub uid: String,
    /// Profile name.
    pub name: String,
    /// Profile version.
    pub version: String,
    /// Profile type: cluster, infra, add-on, system.
    #[serde(rename = "type")]
    pub kind: String,
}

impl AppliedProfile {
    /// The profiles a cluster document applies, in canonical order: the cluster or infra
    /// profile first, then the others by name. Palette lists them in
    /// `spec.clusterProfileTemplates` of `GET /v1/spectroclusters/{uid}` in an order that
    /// differs between two clusters built from one definition, so that order says nothing.
    pub fn of_cluster(doc: &Value) -> Vec<AppliedProfile> {
        AppliedProfile::templates_of(doc)
            .into_iter()
            .map(|(_, applied)| applied)
            .collect()
    }

    /// Each template of `spec.clusterProfileTemplates` with the profile it applies, in
    /// canonical order. Two profiles of one type are by name, then by version, then by uid.
    fn templates_of(doc: &Value) -> Vec<(Value, AppliedProfile)> {
        let mut pairs: Vec<(Value, AppliedProfile)> = doc
            .pointer("/spec/clusterProfileTemplates")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .map(|t| (t.clone(), AppliedProfile::from_template(t)))
            .collect();
        pairs.sort_by(|(_, a), (_, b)| {
            a.rank()
                .cmp(&b.rank())
                .then_with(|| a.name.cmp(&b.name))
                .then_with(|| a.version.cmp(&b.version))
                .then_with(|| a.uid.cmp(&b.uid))
        });
        pairs
    }

    /// The cluster or infra profile comes first: a cluster runs it before any add-on.
    fn rank(&self) -> u8 {
        match self.kind.as_str() {
            "cluster" | "infra" => 0,
            _ => 1,
        }
    }

    fn from_template(t: &Value) -> AppliedProfile {
        AppliedProfile {
            uid: s(t, "/uid"),
            name: s(t, "/name"),
            version: s(t, "/profileVersion"),
            kind: s(t, "/type"),
        }
    }
}

/// Clusters using a profile version.
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
pub struct Consumers {
    /// Cluster uids.
    pub clusters: Vec<String>,
    /// Number of clusters.
    pub count: usize,
}

/// One layer of the profile: a pack with its configuration and its hashes.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Pack {
    /// `<layer>/<name>`, the key of the pack's node hash in the root
    pub key: String,
    /// Palette layer name: os, k8s, cni, csi, addon.
    pub layer: String,
    /// Pack name.
    pub name: String,
    /// Pack type: spectro, helm, manifest, oci.
    #[serde(rename = "type")]
    pub kind: String,
    /// Tag as pinned on the profile.
    pub tag: String,
    /// Resolved version.
    pub version: String,
    /// Uid of the pack version in the registry.
    pub pack_uid: String,
    /// Uid of the registry.
    pub registry_uid: String,
    /// The digest Palette stamps on the pack ref of the profile template
    pub ref_digest: String,
    /// The digest of the pack in its registry, from the packs/manifests endpoint
    pub registry_digest: String,
    /// Pack params as set on the profile.
    pub params: Value,
    /// Raw values YAML as returned by the API
    pub values: String,
    /// The values parsed as YAML and converted to JSON; null when unparseable
    pub values_canon: Value,
    /// sha256 of canonical JSON of valuesCanon, or of the trimmed text when unparseable
    pub values_hash: String,
    /// Why the values did not parse as YAML, when they did not.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub values_parse_error: Option<String>,
    /// Attached manifests sorted by name
    pub manifests: Vec<Manifest>,
    /// The three sub-hashes and the node hash.
    pub hash: PackHash,
}

/// A manifest attached to a pack on the profile.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Manifest {
    /// Manifest name.
    pub name: String,
    /// Manifest uid.
    pub uid: String,
    /// Digest as reported by Palette.
    pub digest: String,
    /// Raw YAML content.
    pub content: String,
    /// sha256 of the canonical content, or of the trimmed text when unparseable.
    pub content_hash: String,
    /// Why the content did not parse as YAML, when it did not.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content_parse_error: Option<String>,
}

/// The three sub-hashes and the node hash of one pack.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PackHash {
    /// sha256 over name, layer, type, tag, version, packUid, registryUid
    pub identity: String,
    /// sha256 of upstreamRaw
    pub upstream: String,
    /// The digest that fed `upstream`
    pub upstream_raw: String,
    /// "packRef.digest", "packSummary.digest", or "none"
    pub upstream_source: String,
    /// sha256 over valuesHash, [manifest name, contentHash]..., params
    pub authored: String,
    /// sha256(identity + upstream + authored)
    pub node: String,
}

/// The content digest: what a version is, without the ids that Palette gives. It covers the
/// variables hash and, for each pack in order, the key, the version, the upstream hash, and the
/// authored hash. It leaves out the tag, the pack uid, the registry uid, and the pack type, so it
/// is equal for two publishes of one definition and for a copy of a pack to another registry.
fn content_of(variables_hash: &str, packs: &[Pack]) -> String {
    sha256_hex(&canonical_json(&json!({
        // The key keeps this digest apart from every other one, and gives the encoding a version.
        "digest": CONTENT_DIGEST,
        "variables": variables_hash,
        "packs": packs
            .iter()
            .map(|p| json!([p.key, p.version, p.hash.upstream, p.hash.authored]))
            .collect::<Vec<_>>(),
    })))
}

/// The root: one sha256 digest over the variables hash and the node hash of each pack, in order.
fn root_of(variables_hash: &str, packs: &[Pack]) -> String {
    sha256_hex(&canonical_json(&json!({
        "variables": variables_hash,
        "packs": packs.iter().map(|p| json!([p.key, p.hash.node])).collect::<Vec<_>>(),
    })))
}

fn s(v: &Value, ptr: &str) -> String {
    v.pointer(ptr)
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string()
}

fn arr(v: &Value, ptr: &str) -> Vec<Value> {
    v.pointer(ptr)
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
}

fn or(v: String, default: &str) -> String {
    if v.is_empty() { default.to_string() } else { v }
}

fn first_nonempty(candidates: &[String]) -> String {
    candidates
        .iter()
        .find(|c| !c.is_empty())
        .cloned()
        .unwrap_or_default()
}

fn str_or(v: &Value, key: &str, default: &str) -> Value {
    Value::String(
        v.get(key)
            .and_then(Value::as_str)
            .unwrap_or(default)
            .to_string(),
    )
}

fn bool_or(v: &Value, key: &str) -> Value {
    Value::Bool(v.get(key).and_then(Value::as_bool).unwrap_or(false))
}

/// Same field set and defaults as the bash extraction, so hashes agree across implementations.
fn normalize_variable(v: &Value) -> Value {
    let options: Vec<Value> = v
        .get("options")
        .and_then(Value::as_array)
        .map(|opts| {
            opts.iter()
                .map(|o| {
                    json!({
                        "label": str_or(o, "label", ""),
                        "value": str_or(o, "value", ""),
                        "description": str_or(o, "description", ""),
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    json!({
        "name": v.get("name").cloned().unwrap_or(Value::Null),
        "displayName": str_or(v, "displayName", ""),
        "description": str_or(v, "description", ""),
        "format": str_or(v, "format", "string"),
        "defaultValue": str_or(v, "defaultValue", ""),
        "regex": str_or(v, "regex", ""),
        "required": bool_or(v, "required"),
        "immutable": bool_or(v, "immutable"),
        "hidden": bool_or(v, "hidden"),
        "isSensitive": bool_or(v, "isSensitive"),
        "inputType": str_or(v, "inputType", "text"),
        "options": options,
    })
}

impl Pack {
    /// Build one pack from its ref on the profile template and the matching entry of the
    /// packs/manifests response (matched by name; missing entries leave those fields empty).
    pub fn from_api(pk: &Value, manifests_packs: &[Value]) -> Pack {
        let name = s(pk, "/name");
        let mm = manifests_packs
            .iter()
            .find(|m| s(m, "/metadata/name") == name || s(m, "/spec/name") == name)
            .cloned()
            .unwrap_or(Value::Null);
        let layer = or(s(pk, "/layer"), "addon");
        let kind = or(s(pk, "/type"), "spectro");
        let tag = s(pk, "/tag");
        let version = first_nonempty(&[s(pk, "/version"), s(&mm, "/spec/version")]);
        let pack_uid = s(pk, "/packUid");
        let registry_uid = s(pk, "/registryUid");
        let ref_digest = s(pk, "/digest");
        let registry_digest = s(&mm, "/spec/digest");
        let params = pk
            .get("params")
            .cloned()
            .filter(Value::is_object)
            .unwrap_or_else(|| Value::Object(Map::new()));
        let values = first_nonempty(&[s(pk, "/values"), s(&mm, "/spec/values")]);

        let mut manifests: Vec<Manifest> = arr(&mm, "/spec/manifests")
            .iter()
            .map(|m| {
                let content = s(m, "/content");
                let (content_hash, content_parse_error) = hash_yaml(&content);
                Manifest {
                    name: s(m, "/name"),
                    uid: s(m, "/uid"),
                    digest: s(m, "/digest"),
                    content,
                    content_hash,
                    content_parse_error,
                }
            })
            .collect();
        manifests.sort_by(|a, b| a.name.cmp(&b.name));

        let (values_canon, values_hash, values_parse_error) = match canon_yaml(&values) {
            Canon::Parsed(v) => {
                let h = sha256_hex(&canonical_json(&v));
                (v, h, None)
            }
            Canon::Unparseable(e) => (Value::Null, sha256_hex(values.trim()), Some(e)),
        };

        let identity = sha256_hex(&canonical_json(&json!({
            "name": name, "layer": layer, "type": kind, "tag": tag, "version": version,
            "packUid": pack_uid, "registryUid": registry_uid,
        })));
        let (upstream_raw, upstream_source) = if !ref_digest.is_empty() {
            (ref_digest.clone(), "packRef.digest")
        } else if !registry_digest.is_empty() {
            (registry_digest.clone(), "packSummary.digest")
        } else {
            (String::new(), "none")
        };
        let upstream = sha256_hex(&upstream_raw);
        let authored = sha256_hex(&canonical_json(&json!({
            "values": values_hash,
            "manifests": manifests.iter().map(|m| json!([m.name, m.content_hash])).collect::<Vec<_>>(),
            "params": params,
        })));
        let node = sha256_hex(&format!("{identity}{upstream}{authored}"));

        Pack {
            key: format!("{layer}/{name}"),
            layer,
            name,
            kind,
            tag,
            version,
            pack_uid,
            registry_uid,
            ref_digest,
            registry_digest,
            params,
            values,
            values_canon,
            values_hash,
            values_parse_error,
            manifests,
            hash: PackHash {
                identity,
                upstream,
                upstream_raw,
                upstream_source: upstream_source.to_string(),
                authored,
                node,
            },
        }
    }
}

impl Snapshot {
    /// Build a snapshot from the raw API responses of one profile.
    pub fn from_raw(raw: &RawProfile, reference: &str, fetched_at: &str) -> Result<Snapshot> {
        let prof = &raw.profile;
        let published = arr(prof, "/spec/published/packs");
        let (tpl_packs, tpl_ptr, template) = if !published.is_empty() {
            (published, "/spec/published", "published")
        } else {
            (arr(prof, "/spec/draft/packs"), "/spec/draft", "draft")
        };
        let mpacks = arr(&raw.manifests, "/spec/packs");

        let tags: BTreeMap<String, String> = prof
            .pointer("/metadata/labels")
            .and_then(Value::as_object)
            .map(|o| {
                o.iter()
                    .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string())))
                    .collect()
            })
            .unwrap_or_default();
        let clusters: Vec<String> = arr(prof, "/status/inUseClusterUids")
            .iter()
            .filter_map(|v| v.as_str().map(str::to_string))
            .collect();
        let source = Source {
            reference: reference.to_string(),
            uid: s(prof, "/metadata/uid"),
            project_uid: String::new(),
            name: s(prof, "/metadata/name"),
            version: s(prof, "/spec/version"),
            cloud_type: s(prof, &format!("{tpl_ptr}/cloudType")),
            kind: s(prof, &format!("{tpl_ptr}/type")),
            template: template.to_string(),
            variables_unavailable: raw.variables_unavailable,
            tags,
            consumers: Some(Consumers {
                count: clusters.len(),
                clusters,
            }),
            fetched_at: fetched_at.to_string(),
            cluster: None,
            pack: None,
        };

        let mut variables: Vec<Value> = arr(&raw.variables, "/variables")
            .iter()
            .map(normalize_variable)
            .collect();
        variables.sort_by_key(|v| s(v, "/name"));
        let variables_hash = sha256_hex(&canonical_json(&Value::Array(variables.clone())));

        let packs: Vec<Pack> = tpl_packs
            .iter()
            .map(|pk| Pack::from_api(pk, &mpacks))
            .collect();

        let root = root_of(&variables_hash, &packs);

        Ok(Snapshot {
            schema: SNAPSHOT_SCHEMA.to_string(),
            tool: Tool::current(),
            source,
            variables,
            content_digest: content_of(&variables_hash, &packs),
            variables_hash,
            packs,
            root,
        })
    }

    /// The content digest: equal for two objects that are the same version, where the root is
    /// not. Palette gives a manifest pack a new uid for each profile, and a pack uid belongs to
    /// one registry, so two publishes of one definition have two roots and one content digest.
    /// The root answers "did this object change". This digest answers "is this the same version".
    pub fn content_digest(&self) -> String {
        content_of(&self.variables_hash, &self.packs)
    }

    /// The packs that this version takes from a registry, each with its version and the digest
    /// of the registry. A manifest pack comes from no registry, so it is authored content and
    /// not a pin.
    pub fn pins(&self) -> Vec<PinRef> {
        self.packs
            .iter()
            .filter(|p| p.kind != "manifest")
            .map(|p| PinRef {
                name: p.name.clone(),
                version: p.version.clone(),
                digest: p.hash.upstream_raw.clone(),
            })
            .collect()
    }

    /// A snapshot for each profile that a cluster applies, from one read of the cluster, in the
    /// canonical order of [`AppliedProfile::of_cluster`]. Each one is the snapshot that
    /// `cluster:<cluster>/<profile>` gives.
    pub fn each_from_cluster(
        raw: &RawCluster,
        reference: &str,
        fetched_at: &str,
    ) -> Result<Vec<Snapshot>> {
        AppliedProfile::of_cluster(&raw.cluster)
            .iter()
            .map(|applied| Snapshot::from_cluster(raw, Some(&applied.uid), reference, fetched_at))
            .collect()
    }

    /// Record the project scope the snapshot was taken in; `None` is tenant scope. The scope is
    /// metadata: it does not enter the root.
    pub fn with_project(mut self, project_uid: Option<&str>) -> Snapshot {
        self.source.project_uid = project_uid.unwrap_or("").to_string();
        self
    }

    /// Build a snapshot of what a cluster runs from its raw API responses. Each applied profile is
    /// hashed as the profile itself would be, so a cluster that runs a recorded version has the
    /// root of that record. With `profile`, only the applied profile of that name (or uid) is
    /// covered; without, every applied profile in order, packs concatenated and variables merged.
    pub fn from_cluster(
        raw: &RawCluster,
        profile: Option<&str>,
        reference: &str,
        fetched_at: &str,
    ) -> Result<Snapshot> {
        // In canonical order, so two clusters built from one definition have one merged root
        // whatever order Palette lists their profiles in.
        let (templates, applied): (Vec<Value>, Vec<AppliedProfile>) =
            AppliedProfile::templates_of(&raw.cluster)
                .into_iter()
                .unzip();
        let chosen: Vec<usize> = match profile {
            None => (0..templates.len()).collect(),
            Some(p) => {
                let chosen: Vec<usize> = applied
                    .iter()
                    .enumerate()
                    .filter(|(_, a)| a.name == p || a.uid == p)
                    .map(|(i, _)| i)
                    .collect();
                if chosen.is_empty() {
                    return Err(Error::ClusterProfileNotFound {
                        cluster: s(&raw.cluster, "/metadata/uid"),
                        profile: p.to_string(),
                        applied: applied.iter().map(|a| a.name.clone()).collect(),
                    });
                }
                chosen
            }
        };
        let parts = chosen
            .iter()
            .map(|&i| Snapshot::from_raw(&raw.template_raw(&templates[i]), reference, fetched_at))
            .collect::<Result<Vec<Snapshot>>>()?;
        let covered: Vec<AppliedProfile> = chosen.iter().map(|&i| applied[i].clone()).collect();

        let mut variables: Vec<Value> = parts.iter().flat_map(|p| p.variables.clone()).collect();
        variables.sort_by_key(|v| s(v, "/name"));
        let variables_hash = sha256_hex(&canonical_json(&Value::Array(variables.clone())));
        let packs: Vec<Pack> = parts.iter().flat_map(|p| p.packs.clone()).collect();
        let root = root_of(&variables_hash, &packs);

        let joined = |f: fn(&Source) -> &String| {
            parts
                .iter()
                .map(|p| f(&p.source).as_str())
                .collect::<Vec<_>>()
                .join("+")
        };
        let source = Source {
            reference: reference.to_string(),
            uid: match parts.as_slice() {
                [only] => only.source.uid.clone(),
                _ => String::new(),
            },
            project_uid: String::new(),
            name: joined(|s| &s.name),
            version: joined(|s| &s.version),
            cloud_type: parts
                .first()
                .map(|p| p.source.cloud_type.clone())
                .filter(|c| !c.is_empty())
                .unwrap_or_else(|| s(&raw.cluster, "/spec/cloudType")),
            kind: joined(|s| &s.kind),
            template: "applied".to_string(),
            variables_unavailable: raw.variables_unavailable,
            tags: BTreeMap::new(),
            consumers: None,
            fetched_at: fetched_at.to_string(),
            pack: None,
            cluster: Some(ClusterSource {
                uid: s(&raw.cluster, "/metadata/uid"),
                name: s(&raw.cluster, "/metadata/name"),
                profile: profile.map(str::to_string),
                variables: ClusterVariable::read(raw, &covered),
                profiles: covered,
            }),
        };
        Ok(Snapshot {
            schema: SNAPSHOT_SCHEMA.to_string(),
            tool: Tool::current(),
            source,
            variables,
            content_digest: content_of(&variables_hash, &packs),
            variables_hash,
            packs,
            root,
        })
    }

    /// Read a snapshot file, checking its schema id.
    pub fn load(path: &Path) -> Result<Snapshot> {
        let text = std::fs::read_to_string(path).map_err(|source| Error::Io {
            path: path.to_path_buf(),
            source,
        })?;
        let v: Value =
            serde_json::from_str(&text).map_err(|_| Error::NotSnapshot(path.to_path_buf()))?;
        match v.get("schema").and_then(Value::as_str) {
            Some(SNAPSHOT_SCHEMA) => {}
            Some(other) if other.starts_with(SNAPSHOT_FAMILY) => {
                return Err(Error::UnsupportedSchema {
                    path: path.to_path_buf(),
                    found: other.to_string(),
                    supported: SNAPSHOT_SCHEMA,
                });
            }
            _ => return Err(Error::NotSnapshot(path.to_path_buf())),
        }
        serde_json::from_value(v).map_err(|source| Error::SnapshotSchema {
            path: path.to_path_buf(),
            schema: SNAPSHOT_SCHEMA,
            source,
        })
    }

    /// Serialize, pretty-printed unless `compact`, with a trailing newline.
    pub fn to_json(&self, compact: bool) -> Result<String> {
        let mut s = if compact {
            serde_json::to_string(self)?
        } else {
            serde_json::to_string_pretty(self)?
        };
        s.push('\n');
        Ok(s)
    }
}
