//! The build of a record from the responses of Palette: of a profile, of its draft, or of a
//! cluster.

use std::collections::{BTreeMap, BTreeSet};

use serde_json::{Value, json};

use super::cluster::{
    AppliedProfile, ClusterSource, ClusterVariable, PendingUpdate, cluster_tiers, macro_digests,
    profile_macro_digests,
};
use super::pack::Pack;
use super::{Consumers, SNAPSHOT_SCHEMA, Snapshot, Source, SubjectKind, Template, Tool, macros_of};
use crate::api::{RawCluster, RawProfile};
use crate::content::canon::{canonical_json, sha256_hex};
use crate::content::hash::{content_of, root_of};
use crate::error::{Error, Result};
use crate::json::{
    arr, first_nonempty, flag, str_at, strings, text, text_map, text_or, value_or_null,
};
use crate::repave::{PendingRepave, is_pending};

/// The text at `ptr` in `v`, or `default` when `v` has no text there. An empty text stays empty,
/// so the hash of the variables does not change: this is not [`first_nonempty`].
fn str_or(v: &Value, ptr: &str, default: &str) -> Value {
    Value::from(text_or(v, ptr, default))
}

fn bool_or(v: &Value, ptr: &str) -> Value {
    Value::Bool(flag(v, ptr))
}

/// Same field set and defaults as the bash extraction, so hashes agree across implementations.
fn normalize_variable(v: &Value) -> Value {
    let options: Vec<Value> = arr(v, "/options")
        .iter()
        .map(|o| {
            json!({
                "label": str_or(o, "/label", ""),
                "value": str_or(o, "/value", ""),
                "description": str_or(o, "/description", ""),
            })
        })
        .collect();
    json!({
        "name": value_or_null(v, "/name"),
        "displayName": str_or(v, "/displayName", ""),
        "description": str_or(v, "/description", ""),
        "format": str_or(v, "/format", "string"),
        "defaultValue": str_or(v, "/defaultValue", ""),
        "regex": str_or(v, "/regex", ""),
        "required": bool_or(v, "/required"),
        "immutable": bool_or(v, "/immutable"),
        "hidden": bool_or(v, "/hidden"),
        "isSensitive": bool_or(v, "/isSensitive"),
        "inputType": str_or(v, "/inputType", "text"),
        "options": options,
    })
}

/// The manifests of pack `pk` of one template, each as [`manifest_of`] gives it, from the
/// answers of the manifests by uid (see [`RawProfile::manifest_entities`]).
fn manifests_of_template(pk: &Value, entities: &Value, template: Template) -> Vec<Value> {
    arr(pk, "/manifests")
        .iter()
        .map(|m| {
            let entity = entities.get(str_at(m, "/uid")).unwrap_or(&Value::Null);
            manifest_of(entity, m, template)
        })
        .collect()
}

/// One manifest as a record reads it: its `name`, `uid`, `digest`, and `content`. `entity` is
/// the answer of the route of the manifest, and `reference` is the entry of the manifest in the
/// pack of a template.
///
/// The name and the uid come from `reference`, else from `entity`. A manifest of the published
/// template has its published content. A manifest of the draft has its draft content, else its
/// published content when the draft did not change it. Palette writes the draft content of a
/// manifest at an edit, and empties it at the publish. A field that `entity` does not give is
/// empty text.
pub(crate) fn manifest_of(entity: &Value, reference: &Value, template: Template) -> Value {
    let edited = !str_at(entity, &format!("/spec/{template}/content")).is_empty();
    let from = if edited {
        template
    } else {
        Template::Published
    };
    manifest_entry(
        &first_nonempty(&[str_at(reference, "/name"), str_at(entity, "/metadata/name")]),
        &first_nonempty(&[str_at(reference, "/uid"), str_at(entity, "/metadata/uid")]),
        &text(entity, &format!("/spec/{from}/digest")),
        &text(entity, &format!("/spec/{from}/content")),
    )
}

/// One manifest as a record reads it, from its four fields. A manifest of a candidate has no uid
/// and no digest yet, so those two are empty text.
pub(crate) fn manifest_entry(name: &str, uid: &str, digest: &str, content: &str) -> Value {
    json!({"name": name, "uid": uid, "digest": digest, "content": content})
}

impl Snapshot {
    /// Build a snapshot from the raw API responses of one profile: of its published template, or
    /// of its draft when it has no published template.
    pub fn from_raw(raw: &RawProfile, reference: &str, fetched_at: &str) -> Result<Snapshot> {
        let template = crate::api::recorded_template(&raw.profile);
        Self::from_raw_template(raw, reference, fetched_at, template)
    }

    /// Build a snapshot of an app profile from its raw API responses. Its consumers are app
    /// deployments, and not clusters, so the record counts none.
    pub(crate) fn from_app_profile(
        raw: &RawProfile,
        reference: &str,
        fetched_at: &str,
    ) -> Result<Snapshot> {
        let mut snap = Self::from_raw(raw, reference, fetched_at)?;
        snap.source.consumers = None;
        Ok(snap)
    }

    /// Build a snapshot of the draft template of one profile: what the next publish makes the
    /// version. A profile with no draft is the error [`Error::NoDraft`].
    pub fn from_raw_draft(raw: &RawProfile, reference: &str, fetched_at: &str) -> Result<Snapshot> {
        let prof = &raw.profile;
        if !crate::api::has_draft(prof) {
            return Err(Error::NoDraft {
                name: text(prof, "/metadata/name"),
                version: text(prof, "/spec/version"),
                uid: text(prof, "/metadata/uid"),
            });
        }
        Self::from_raw_template(raw, reference, fetched_at, Template::Draft)
    }

    /// Build a snapshot of one template of a profile: [`Template::Published`] or
    /// [`Template::Draft`].
    fn from_raw_template(
        raw: &RawProfile,
        reference: &str,
        fetched_at: &str,
        template: Template,
    ) -> Result<Snapshot> {
        let prof = &raw.profile;
        let tpl_ptr = format!("/spec/{template}");
        let tpl_packs = arr(prof, &format!("{tpl_ptr}/packs"));
        let mut mpacks = arr(&raw.manifests, "/spec/packs").to_vec();
        if crate::api::has_draft(prof) && template == Template::Published {
            // packs/manifests holds the packs of the draft: keep only the entries of the packs
            // that the published template holds, by pack uid
            let uids: BTreeSet<&str> = tpl_packs.iter().map(|pk| str_at(pk, "/packUid")).collect();
            mpacks.retain(|m| uids.contains(str_at(m, "/metadata/uid")));
        }

        let tags = text_map(prof, "/metadata/labels");
        let clusters = strings(prof, "/status/inUseClusterUids");
        let variables: Vec<Value> = arr(&raw.variables, "/variables")
            .iter()
            .map(normalize_variable)
            .collect();
        let packs: Vec<Pack> = tpl_packs
            .iter()
            .map(|pk| {
                // a ref that lists no manifest keeps the manifests of the packs/manifests entry
                let given = raw
                    .manifest_entities
                    .as_ref()
                    .filter(|_| !arr(pk, "/manifests").is_empty())
                    .map(|e| manifests_of_template(pk, e, template));
                Pack::from_api_with(pk, &mpacks, given)
            })
            .collect();
        let source = Source {
            reference: reference.to_string(),
            uid: text(prof, "/metadata/uid"),
            project_uid: String::new(),
            name: text(prof, "/metadata/name"),
            version: text(prof, "/spec/version"),
            cloud_type: text(prof, &format!("{tpl_ptr}/cloudType")),
            kind: text(prof, &format!("{tpl_ptr}/type")),
            template: template.into(),
            variables_unavailable: raw.variables_unavailable,
            tags,
            consumers: Some(Consumers {
                count: clusters.len(),
                clusters,
            }),
            fetched_at: fetched_at.to_string(),
            cluster: None,
            pack: None,
            palette_validation: None,
            macro_values: raw
                .resolved_values
                .as_ref()
                .map(|r| profile_macro_digests(r, &macros_of(&packs))),
        };
        Ok(Snapshot::assemble(source, variables, packs))
    }

    /// The record of `source`, with `variables` sorted by name, `packs` in profile order, and
    /// the root and the content digest over both.
    ///
    /// Each kind of record comes from this function. A change here moves the root of each one, so
    /// it is a change of the schema id.
    pub(super) fn assemble(
        source: Source,
        mut variables: Vec<Value>,
        packs: Vec<Pack>,
    ) -> Snapshot {
        variables.sort_by_key(|v| text(v, "/name"));
        let variables_hash = sha256_hex(&canonical_json(&Value::Array(variables.clone())));
        Snapshot {
            schema: SNAPSHOT_SCHEMA.to_string(),
            tool: Tool::current(),
            source,
            root: root_of(&variables_hash, &packs),
            content_digest: content_of(&variables_hash, &packs),
            variables,
            variables_hash,
            packs,
        }
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
                // `a+b` selects several profiles, as compare does for a cluster group
                let wanted: Vec<&str> = p.split('+').collect();
                let chosen: Vec<usize> = applied
                    .iter()
                    .enumerate()
                    .filter(|(_, a)| {
                        wanted.contains(&a.name.as_str()) || wanted.contains(&a.uid.as_str())
                    })
                    .map(|(i, _)| i)
                    .collect();
                if chosen.is_empty() {
                    return Err(Error::ClusterProfileNotFound {
                        cluster: text(&raw.cluster, "/metadata/uid"),
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

        let variables: Vec<Value> = parts.iter().flat_map(|p| p.variables.clone()).collect();
        let packs: Vec<Pack> = parts.iter().flat_map(|p| p.packs.clone()).collect();

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
                .unwrap_or_else(|| RawCluster::cloud_type(&raw.cluster).to_string()),
            kind: joined(|s| &s.kind),
            template: Template::Applied.into(),
            variables_unavailable: raw.variables_unavailable,
            tags: BTreeMap::new(),
            consumers: None,
            fetched_at: fetched_at.to_string(),
            pack: None,
            palette_validation: None,
            // a cluster record holds the digests in cluster.macroValues
            macro_values: None,
            cluster: Some({
                let pending_updates = raw
                    .notifications
                    .as_ref()
                    .map(|n| PendingUpdate::read(n, &covered))
                    .unwrap_or_default();
                ClusterSource {
                    uid: text(&raw.cluster, "/metadata/uid"),
                    name: text(&raw.cluster, "/metadata/name"),
                    profile: profile.map(str::to_string),
                    variables: ClusterVariable::read(raw, &covered),
                    updates_pending: raw
                        .notifications
                        .as_ref()
                        .map(|_| !pending_updates.is_empty()),
                    updates_pending_error: raw.errors.notifications.clone(),
                    pending_updates,
                    repave_pending: Some(is_pending(&raw.cluster)),
                    pending_repave: raw.repave.as_ref().map(PendingRepave::read),
                    pending_repave_error: raw.errors.repave.clone(),
                    macro_values: raw
                        .resolved_values
                        .as_ref()
                        .map(|r| macro_digests(r, &covered, &macros_of(&packs))),
                    labels: text_map(&raw.cluster, "/metadata/labels"),
                    tiers: cluster_tiers(raw, &covered),
                    pending: false,
                    kind: SubjectKind::Cluster.into(),
                    members: Vec::new(),
                    profiles: covered,
                }
            }),
        };
        Ok(Snapshot::assemble(source, variables, packs))
    }
}
