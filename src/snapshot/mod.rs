//! The snapshot document (schema strata-snapshot/1): one profile version with its root,
//! per-pack hashes, canonical values and raw contents. Byte-compatible with the snapshots
//! written by the bash reference implementation.

use std::collections::BTreeMap;
use std::fmt;
use std::path::Path;

use schemars::JsonSchema;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::closed_set::{ClosedSet, Open, impl_closed_set};
use crate::content::hash::content_of;
use crate::error::{Error, Result};
use crate::json::str_at;
use crate::profile_type::ProfileType;
use crate::reference::name_version;
pub use crate::repave::PendingRepave;
use crate::text::name_or_uid;
use crate::timestamp::Rfc3339;
pub use crate::validation::ProfileValidation;
use cluster::{PENDING_REPAVE, UPDATES_PENDING, read_older_errors};
use pack::masked_paths;

mod build;
pub(crate) use build::{manifest_entry, manifest_of};
mod cluster;
pub(crate) mod pack;
pub(crate) mod policy;
pub(crate) mod shape;
mod tier_schema;
mod variable;

pub use cluster::{
    AppliedProfile, ClusterSource, ClusterTier, ClusterVariable, GroupMember, PendingUpdate, Tier,
    cluster_tiers, macro_digests,
};
pub use pack::{Manifest, Pack, PackChart, PackHash, PresetOption, UpstreamSource, pack_content};
pub use variable::{Variable, VariableOption};

/// Schema id written into every snapshot.
pub const SNAPSHOT_SCHEMA: &str = "strata-snapshot/1";

/// The template of a profile that a record reads, as `source.template` names it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Template {
    /// The published template of a profile version.
    Published,
    /// The draft template of a profile version: of `<ref>@draft`, or of a version that was never
    /// published.
    Draft,
    /// The templates that a cluster applies: a record of a cluster, of a cluster group, or of an
    /// edge host.
    Applied,
    /// A candidate file, resolved against the registries. Palette holds no such template.
    Candidate,
    /// One pack version of a registry: a pack record.
    Registry,
}

impl_closed_set!(Template {
    Published => "published",
    Draft => "draft",
    Applied => "applied",
    Candidate => "candidate",
    Registry => "registry",
});

impl fmt::Display for Template {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// The version of the content digest encoding. It is in the input of the digest, so a change to
/// what the digest covers is a change of this tag, and a digest of one version is never equal to
/// a digest of another. A tool that pins a version by content digest depends on this tag.
pub const CONTENT_DIGEST: &str = "content/1";

/// The annotation of a pack ref that names the machine image of an OS pack, such as an AMI. A
/// bring-your-own-image OS pack holds it. The OS pack `ubuntu-aws` of the Public Repo does not.
pub const IMAGE_ID_ANNOTATION: &str = "imageId";

/// The annotation of a pack ref that names the provider type of the OCI registry of the pack:
/// `pack`, `helm`, or `zarf`. A published profile carries it.
pub(crate) const PROVIDER_TYPE_ANNOTATION: &str = "ociRegistryProviderType";

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
    pub variables: Vec<Variable>,
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
    /// The implementation that wrote the document: see [`Implementation`]. Absent in a snapshot
    /// of the first bash release.
    #[serde(default, skip_serializing_if = "Option::is_none", rename = "impl")]
    pub implementation: Option<Open<Implementation>>,
}

/// The implementation of strata that wrote a document, as `tool.impl` names it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Implementation {
    /// This implementation.
    Rust,
    /// The bash reference implementation, which wrote the first records.
    Bash,
}

impl_closed_set!(Implementation {
    Rust => "rust",
    Bash => "bash",
});

impl Tool {
    /// This build of strata, as every document it writes records it.
    pub fn current() -> Tool {
        Tool {
            name: crate::TOOL_NAME.to_string(),
            version: crate::TOOL_VERSION.to_string(),
            implementation: Some(Implementation::Rust.into()),
        }
    }
}

/// Where a snapshot came from.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
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
    /// The type of the profile: `cluster`, `infra`, `add-on`, or `system` for a cluster profile,
    /// as Palette gives it in `v1ProfileType`. Strata writes `app` for an app profile and `pack`
    /// for a pack record.
    #[serde(rename = "type")]
    pub kind: String,
    /// The template that strata read the packs from: see [`Template`].
    pub template: Open<Template>,
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
    #[schemars(with = "Rfc3339")]
    pub fetched_at: String,
    /// Present when the snapshot records what a cluster runs rather than a published profile.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cluster: Option<ClusterSource>,
    /// Present when the snapshot records one pack version from a registry.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pack: Option<PackSource>,
    /// Present after `snapshot --validate`: what Palette answered when strata asked whether it
    /// takes the packs of this content.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub palette_validation: Option<ProfileValidation>,
    /// With `snapshot --macros` on a profile version: each user macro that the recorded content
    /// uses, with a digest. The digest is the `sha256:` of the value that Palette gives the macro
    /// at the scope of the profile, or `unresolved` when Palette gives no value. A record of a
    /// cluster holds them in `cluster.macroValues`. Outside the root; `verify` reads the values
    /// again and compares.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub macro_values: Option<BTreeMap<String, String>>,
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

/// Where a line names the object of a record: see [`Source::named`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Naming {
    /// A side of a diff: `aws-stack@1.1.1 on cluster payments`, as [`Source::title`] gives it.
    Side,
    /// The subject of a verdict: `cluster payments running aws-stack@1.1.1`, or `published
    /// version aws-stack@1.1.1`.
    Verdict,
}

impl Source {
    /// True for a record of a draft: the draft template of a version, which its publish makes
    /// the published template. The record is of `<ref>@draft`, or of a version that nobody
    /// published, which has only its draft.
    pub fn is_draft(&self) -> bool {
        self.template == Template::Draft
    }

    /// True for a record of an app profile: its tiers are its packs.
    pub(crate) fn is_app(&self) -> bool {
        ProfileType::parse(&self.kind) == Some(ProfileType::App)
    }

    /// True for a record of one version of a cluster profile. A record of a cluster, of an edge
    /// host, of a cluster group, of one pack, or of an app profile is not one.
    ///
    /// [`Ref::is_profile_version`](crate::reference::Ref::is_profile_version) keeps the rule for
    /// a ref.
    pub(crate) fn is_profile_version(&self) -> bool {
        self.cluster.is_none() && self.pack.is_none() && !self.is_app()
    }

    /// `name@version`, and `on cluster <name>` for a snapshot of a cluster, or `pending for cluster
    /// <name>` for the update that waits for it.
    pub fn title(&self) -> String {
        self.named(Naming::Side)
    }

    /// The object of the record in the words of `naming`: the side of a diff, or the subject of a
    /// verdict.
    ///
    /// One rule picks the words for each kind of record. So the heading and the verdict name the
    /// same object. This is also true for a cluster with no applied profile, and for the update
    /// that waits for a cluster. Only the side of a published version, of a pack, or of a cluster
    /// that runs a
    /// profile has other words in a verdict.
    pub(crate) fn named(&self, naming: Naming) -> String {
        let profile = name_version(&self.name, &self.version);
        if let Some(p) = &self.pack {
            return match naming {
                Naming::Side => format!("pack {profile}"),
                Naming::Verdict => format!(
                    "pack {profile} in registry {}",
                    name_or_uid(&p.registry_name, &p.registry_uid)
                ),
            };
        }
        let Some(c) = &self.cluster else {
            return match naming {
                Naming::Side => profile,
                Naming::Verdict => format!("published version {profile}"),
            };
        };
        match c.kind.known() {
            _ if self.name.is_empty() && !c.kind.is_cluster() => {
                format!("{} {}", c.kind.noun(), c.name)
            }
            // a kind that a later release wrote: the name of the kind in the documents
            None => format!("{profile} on {} {}", c.kind.noun(), c.name),
            Some(SubjectKind::EdgeHost) => format!("{profile} on edge host {}", c.name),
            Some(SubjectKind::ClusterGroup) => {
                format!("{profile} pinned by cluster group {}", c.name)
            }
            Some(SubjectKind::Cluster) if self.name.is_empty() && c.pending => {
                format!("cluster {} pending", c.name)
            }
            Some(SubjectKind::Cluster) if self.name.is_empty() => format!("cluster {}", c.name),
            Some(SubjectKind::Cluster) if c.pending => {
                format!("{profile} pending for cluster {}", c.name)
            }
            Some(SubjectKind::Cluster) => match naming {
                Naming::Side => format!("{profile} on cluster {}", c.name),
                Naming::Verdict => format!("cluster {} running {profile}", c.name),
            },
        }
    }

    /// The digests of the macro values that the record holds: those of the cluster for a cluster
    /// record, else those of the profile. `None` when the record was taken without `--macros`.
    pub fn macro_values(&self) -> Option<&BTreeMap<String, String>> {
        match &self.cluster {
            Some(c) => c.macro_values.as_ref(),
            None => self.macro_values.as_ref(),
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

/// The kind of object that a cluster record was taken from. Each holds profiles in
/// `spec.clusterProfileTemplates`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum SubjectKind {
    /// A cluster: the copies of the profiles that it runs.
    #[default]
    Cluster,
    /// An edge host: the profiles bound to it.
    EdgeHost,
    /// A cluster group: the profiles that it pins for its virtual clusters.
    ClusterGroup,
}

impl SubjectKind {
    /// True for a cluster.
    pub fn is_cluster(&self) -> bool {
        *self == SubjectKind::Cluster
    }

    /// The noun that names the kind in a report.
    pub fn noun(&self) -> &'static str {
        match self {
            SubjectKind::Cluster => "cluster",
            SubjectKind::EdgeHost => "edge host",
            SubjectKind::ClusterGroup => "cluster group",
        }
    }
}

impl_closed_set!(SubjectKind {
    Cluster => "cluster",
    EdgeHost => "edgeHost",
    ClusterGroup => "clusterGroup",
});

impl Open<SubjectKind> {
    /// True for a cluster. A kind that a later release wrote is not a cluster.
    pub fn is_cluster(&self) -> bool {
        self.known().is_some_and(|k| k.is_cluster())
    }

    /// The noun that names the kind in a report: the name in the documents, for a kind that a
    /// later release wrote.
    pub fn noun(&self) -> &str {
        self.known().map_or(self.as_str(), |k| k.noun())
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

/// The user macros that `packs` use, by name and sorted: in the values and the manifests.
fn macros_of(packs: &[Pack]) -> Vec<String> {
    let mut names = std::collections::BTreeSet::new();
    for p in packs {
        names.extend(macro_names(&p.values));
        for m in &p.manifests {
            names.extend(macro_names(&m.content));
        }
    }
    names.into_iter().collect()
}

/// The prefix of a user macro in a value or a manifest: `{{.spectro.macro.NAME}}`.
///
/// A system macro, `{{.spectro.system.NAME}}`, has another prefix.
const MACRO_PREFIX: &str = ".spectro.macro.";

/// The placeholder of the user macro `name`, as the resolved values of Palette key it:
/// `{{.spectro.macro.NAME}}`.
fn macro_placeholder(name: &str) -> String {
    format!("{{{{{MACRO_PREFIX}{name}}}}}")
}

/// The names of the user macros that `text` uses: each `{{.spectro.macro.NAME}}`, with spaces
/// after `{{` allowed. A name holds letters, digits, `_`, `-`, and `.`. A system macro,
/// `{{.spectro.system.NAME}}`, is not a user macro: Palette gives it from the cluster, and no
/// person edits it.
pub fn macro_names(text: &str) -> std::collections::BTreeSet<String> {
    let mut names = std::collections::BTreeSet::new();
    let mut rest = text;
    while let Some(i) = rest.find("{{") {
        rest = &rest[i + 2..];
        let body = rest.trim_start();
        if let Some(after) = body.strip_prefix(MACRO_PREFIX) {
            let name: String = after
                .chars()
                .take_while(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.'))
                .collect();
            let name = name.trim_end_matches('.').to_string();
            if !name.is_empty() {
                names.insert(name);
            }
        }
    }
    names
}

impl Snapshot {
    /// The user macros that the recorded content uses, by name and sorted: in the values and the
    /// manifests of each pack. The root covers these names and not the values Palette gives
    /// them, so an edit of a macro does not move the root.
    pub fn macros(&self) -> Vec<String> {
        macros_of(&self.packs)
    }

    /// The key of each pack whose upstream is unknown, in pack order: see
    /// [`Pack::upstream_unknown`].
    pub fn upstream_unknown(&self) -> Vec<String> {
        self.packs
            .iter()
            .filter(|p| p.upstream_unknown())
            .map(|p| p.key.clone())
            .collect()
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
            .filter(|p| !p.is_manifest())
            .map(|p| PinRef {
                name: p.name.clone(),
                version: p.version.clone(),
                digest: p.hash.upstream_raw.clone(),
            })
            .collect()
    }

    /// What Palette holds of a version that it no longer has: the source of the record, fetched
    /// at `checked_at`, with no pack and no variable. `verify` compares a record with it when the
    /// uid of the record is gone, so each pack of the record shows as removed.
    pub fn gone(&self, checked_at: &str) -> Snapshot {
        let mut source = self.source.clone();
        source.fetched_at = checked_at.to_string();
        source.consumers = None;
        Snapshot::assemble(source, Vec::new(), Vec::new())
    }

    /// Mark a record of a cluster as the profiles of the update that waits for it. Whether an
    /// update or a repave waits is a statement about the cluster as it runs now, and not about
    /// these profiles, so the record does not carry it.
    pub fn mark_pending(mut self) -> Snapshot {
        if let Some(c) = self.source.cluster.as_mut() {
            c.pending = true;
            c.updates_pending = None;
            c.pending_updates.clear();
            c.repave_pending = None;
            c.pending_repave = None;
            c.errors.remove(UPDATES_PENDING);
            c.errors.remove(PENDING_REPAVE);
        }
        self
    }

    /// Mark a record of a cluster as a record of another kind of object that holds profiles. Such
    /// an object gives no values to the variables, so the record holds no cluster values: a
    /// definition with no value is not the value `""`. Palette repaves only a cluster, so the
    /// record holds no repave either.
    pub fn with_kind(mut self, kind: SubjectKind) -> Snapshot {
        if let Some(c) = self.source.cluster.as_mut() {
            c.kind = kind.into();
            if !kind.is_cluster() {
                c.variables.clear();
                c.repave_pending = None;
                c.pending_repave = None;
                c.errors.remove(PENDING_REPAVE);
            }
        }
        self
    }

    /// Read a snapshot file, and check its schema id.
    ///
    /// # Errors
    ///
    /// - [`Error::UnsupportedSchema`] for a snapshot of another schema id.
    /// - [`Error::SnapshotSchema`] when the fields do not match.
    /// - [`Error::NotSnapshot`] for a file that is not a snapshot.
    /// - [`Error::Io`] when strata cannot read the file.
    pub fn load(path: &Path) -> Result<Snapshot> {
        let mut document = read_document(path, SNAPSHOT_SCHEMA, || {
            Error::NotSnapshot(path.to_path_buf())
        })?;
        read_older_errors(&mut document);
        let mut snap: Snapshot = parse_document(path, SNAPSHOT_SCHEMA, document)?;
        snap.fill_content();
        Ok(snap)
    }

    /// Give each pack the images, the charts, and the masked paths of its values, when the record
    /// holds none: a record written before those fields has the values, and so the lists.
    pub fn fill_content(&mut self) {
        for p in &mut self.packs {
            if p.images.is_empty() && p.charts.is_empty() {
                (p.images, p.charts) = pack_content(&p.values_canon);
            }
            if p.masked.is_empty() {
                p.masked = masked_paths(&p.values_canon);
            }
        }
    }

    /// Serialize, pretty-printed unless `compact`, with a trailing newline: the document of the
    /// record, as [`JsonRenderer`](crate::render::JsonRenderer) writes each document.
    pub fn to_json(&self, compact: bool) -> Result<String> {
        crate::render::JsonRenderer { compact }.document(self)
    }
}

/// Read the document of the schema id `schema` at `path`, with its id checked.
///
/// A file with the id `schema` gives the document, or [`Error::SnapshotSchema`] when its
/// fields do not match. A file whose id names another version of the same document, such as
/// `strata-snapshot/2` for `strata-snapshot/1`, gives [`Error::UnsupportedSchema`], which names
/// both ids. Each other file gives `not_one`: it is not JSON, it has no id, or its id names
/// another document. The records and the report of `behind --since` follow this one rule.
pub(crate) fn load_document<T: DeserializeOwned>(
    path: &Path,
    schema: &'static str,
    not_one: impl FnOnce() -> Error,
) -> Result<T> {
    parse_document(path, schema, read_document(path, schema, not_one)?)
}

/// Read the JSON of the document of the schema id `schema` at `path`, with its id checked. The
/// errors are those of [`load_document`] for a file that is not the document.
fn read_document(
    path: &Path,
    schema: &'static str,
    not_one: impl FnOnce() -> Error,
) -> Result<Value> {
    let text = crate::file::read_file(path)?;
    let Ok(v) = serde_json::from_str::<Value>(&text) else {
        return Err(not_one());
    };
    let found = str_at(&v, "/schema");
    if found != schema {
        return Err(if schema_document(found) == schema_document(schema) {
            Error::UnsupportedSchema {
                path: path.to_path_buf(),
                found: found.to_string(),
                supported: schema,
            }
        } else {
            not_one()
        });
    }
    Ok(v)
}

/// Read the fields of `document`, the JSON that [`read_document`] gave for the file at `path`.
fn parse_document<T: DeserializeOwned>(
    path: &Path,
    schema: &'static str,
    document: Value,
) -> Result<T> {
    serde_json::from_value(document).map_err(|source| Error::SnapshotSchema {
        path: path.to_path_buf(),
        schema,
        source,
    })
}

/// The name of the document in the schema id `id`: the text before its last `/`, which each
/// version of the document shares. None when `id` is no schema id.
fn schema_document(id: &str) -> Option<&str> {
    id.rsplit_once('/').map(|(name, _)| name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_map_of_the_tiers_keeps_the_order_of_their_names() {
        let tiers: BTreeMap<Tier, u8> = [
            (Tier::Shape, 0),
            (Tier::Running, 0),
            (Tier::Policy, 0),
            (Tier::Applied, 0),
        ]
        .into_iter()
        .collect();
        assert_eq!(
            serde_json::to_string(&tiers).unwrap(),
            r#"{"applied":0,"policy":0,"running":0,"shape":0}"#
        );
    }

    #[test]
    fn the_placeholder_of_a_macro_is_what_macro_names_reads() {
        let key = macro_placeholder("region");
        assert_eq!(key, "{{.spectro.macro.region}}");
        assert_eq!(
            macro_names(&key).into_iter().collect::<Vec<_>>(),
            ["region"]
        );
    }
}
