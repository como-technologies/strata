//! The API responses a snapshot is built from, and their directory on disk.

use std::path::Path;

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::mask::{MASKED, mask_group_values};
use crate::error::{Error, Result};
use crate::json::{arr, flag, opt_text, str_at, value_at, value_or_null};
use crate::snapshot::Template;

/// The three raw API responses a snapshot is built from, and the project they were fetched in.
/// Saved as `profile.json`, `manifests.json`, `variables.json` and `scope.json` by `--raw-dir` and
/// read back by `--from-raw`.
#[derive(Debug, Clone)]
pub struct RawProfile {
    /// `GET /v1/clusterprofiles/{uid}`
    pub profile: Value,
    /// `GET /v1/clusterprofiles/{uid}/packs/manifests`
    pub manifests: Value,
    /// `GET /v1/clusterprofiles/{uid}/variables`, or `{"variables": []}` when unavailable
    pub variables: Value,
    /// True when the variables endpoint was unavailable, so `variables` stands in for it
    pub variables_unavailable: bool,
    /// The project the responses came from; `None` at tenant scope, or for a directory saved
    /// without `scope.json`
    pub project_uid: Option<String>,
    /// When the responses were read from Palette; `None` for a fetch, which stamps the time
    /// itself, and for a directory that an older build saved
    pub fetched_at: Option<String>,
    /// For a profile that has a draft: each manifest of its two templates,
    /// `GET /v1/clusterprofiles/{uid}/packs/{packName}/manifests/{manifestUid}`, in one object by
    /// manifest uid. Each answer holds the draft content and the published content. `None` for a
    /// profile with no draft, where `manifests` holds the content, and for a directory that an
    /// older build saved.
    pub manifest_entities: Option<Value>,
    /// `GET /v1/clusterprofiles/{uid}/packs/resolvedValues`: the value Palette gives each macro
    /// at the scope of the profile. `None` unless asked for, with `snapshot --macros`.
    pub resolved_values: Option<Value>,
}

/// The file of a raw directory that holds [`RawProfile::manifest_entities`].
const MANIFEST_ENTITIES_FILE: &str = "manifest-entities.json";

/// True when a profile document has a draft: packs in `spec.draft`. Palette does not give
/// `status.hasDraft`, and the publish empties the draft.
pub fn has_draft(profile: &Value) -> bool {
    !arr(profile, "/spec/draft/packs").is_empty()
}

/// The template of the profile document `profile` that a record and `list` read: the published
/// template when it has packs, else the draft.
///
/// A version that nobody published has its packs in the draft. A version with no pack in either
/// template also reads the draft, so a record and `list` give it one type.
pub(crate) fn recorded_template(profile: &Value) -> Template {
    if arr(profile, "/spec/published/packs").is_empty() {
        Template::Draft
    } else {
        Template::Published
    }
}

/// The file of a raw directory that holds the profile document.
const PROFILE_FILE: &str = "profile.json";

/// The file of a raw directory that holds the answer of the manifests route.
const MANIFESTS_FILE: &str = "manifests.json";

/// The file of a raw directory that holds the answer of the variables route.
const VARIABLES_FILE: &str = "variables.json";

/// The file that records the project a raw directory's responses came from.
const SCOPE_FILE: &str = "scope.json";

/// The file of a raw directory that holds the values Palette gives the macros, when they were
/// read.
const RESOLVED_VALUES_FILE: &str = "resolved-values.json";

/// The scope file of a raw directory, read one time.
struct SavedScope {
    /// The project saved in the directory, or `None` when it has no scope file or names no
    /// project.
    project_uid: Option<String>,
    /// When the responses were read from Palette. `None` for a directory that an older build
    /// saved, which recorded no time.
    fetched_at: Option<String>,
    /// Whether the variables endpoint was unavailable. A directory that an older build saved
    /// records nothing, which reads as available. The stand-in and a real empty list look alike,
    /// and only this flag tells them apart.
    variables_unavailable: bool,
}

impl SavedScope {
    /// The scope file of `dir`, or each field empty when `dir` has no scope file.
    fn read(dir: &Path) -> Result<SavedScope> {
        let scope: Option<Value> = read_optional(dir, SCOPE_FILE)?;
        let scope = scope.unwrap_or(Value::Null);
        Ok(SavedScope {
            project_uid: opt_text(&scope, "/projectUid"),
            fetched_at: opt_text(&scope, "/fetchedAt"),
            variables_unavailable: flag(&scope, "/variablesUnavailable"),
        })
    }
}

/// The scope file's document: the project uid, empty at tenant scope, the time the responses were
/// read, and whether the variables endpoint was unavailable. Without the time, a snapshot rebuilt
/// from the directory would stamp the rebuild as the fetch, and `verify` would report a version as
/// unchanged since a moment nothing was read. Without the flag, a rebuild would read the stand-in
/// `{"variables": []}` as a real empty list, and its root would claim to cover variables that
/// nothing read.
fn scope_document(
    project_uid: &Option<String>,
    fetched_at: &str,
    variables_unavailable: bool,
) -> Value {
    serde_json::json!({
        "projectUid": project_uid.as_deref().unwrap_or(""),
        "fetchedAt": fetched_at,
        "variablesUnavailable": variables_unavailable,
    })
}

/// Read the JSON file `name` from `dir` when it is there. `None` is a file that the build which
/// saved the directory did not write.
fn read_optional<T: DeserializeOwned>(dir: &Path, name: &str) -> Result<Option<T>> {
    if dir.join(name).is_file() {
        read_json_file(dir, name).map(Some)
    } else {
        Ok(None)
    }
}

/// Read one JSON file from `dir`; an empty file is an error of its own.
fn read_json_file<T: DeserializeOwned>(dir: &Path, name: &str) -> Result<T> {
    let path = dir.join(name);
    let text = crate::file::read_file(&path)?;
    if text.trim().is_empty() {
        return Err(Error::EmptyFile(path));
    }
    serde_json::from_str(&text).map_err(|source| Error::FileNotJson { path, source })
}

/// The fields of a notification event that hold the full values of a pack or an app tier. Palette
/// serves them with no mask, also the fields that `passwordFields` names.
const EVENT_VALUE_FIELDS: [&str; 3] = ["packValues", "appTierValues", "appTierProperties"];

/// A copy of `notifications` with each value field of each event masked, so a raw directory holds
/// no secret of a pack. The mask covers all of the values, not only the fields that
/// `passwordFields` names: a value can hold a secret that the pack does not mark. Strata reads
/// none of these fields.
fn masked_notifications(notifications: &Value) -> Value {
    let mut out = notifications.clone();
    let events = out
        .as_array_mut()
        .into_iter()
        .flatten()
        .filter_map(|n| n.pointer_mut("/action/events"))
        .filter_map(Value::as_object_mut)
        .flat_map(|m| m.values_mut());
    for event in events {
        for field in EVENT_VALUE_FIELDS {
            mask_field(event, field);
        }
    }
    out
}

/// A copy of `repave` with the values of each pack ref of each reason masked. Palette gives the
/// values of the pack on each side of the change, with the value of each macro in its place. A
/// macro value can be a secret. Strata reads none of these values.
fn masked_repave(repave: &Value) -> Value {
    let mut out = repave.clone();
    let sides = out
        .pointer_mut("/spec/reasons")
        .and_then(Value::as_array_mut)
        .into_iter()
        .flatten()
        .filter_map(|r| r.get_mut("pack"))
        .filter_map(Value::as_object_mut)
        .flat_map(|p| p.values_mut());
    for side in sides {
        mask_field(side, "values");
    }
    out
}

/// Put the mask over `field` of `object`, when the field holds a value. A null or empty value
/// stays as it is, so the directory still says that it held none.
fn mask_field(object: &mut serde_json::Value, field: &str) {
    if let Some(v) = object
        .get_mut(field)
        .filter(|v| !v.is_null() && v.as_str() != Some(""))
    {
        *v = Value::String(MASKED.to_string());
    }
}

/// A copy of `cluster` with `********` over each password in the values of a cluster group, as
/// the record masks them. Palette gives the values of a group with no mask. The document of a
/// cluster has no such values, and stays as it is.
fn masked_group(cluster: &Value) -> Result<Value> {
    let mut out = cluster.clone();
    if let Some(v) = out.pointer_mut("/spec/clustersConfig/values")
        && let Some(text) = v.as_str()
    {
        *v = Value::String(mask_group_values(text)?);
    }
    Ok(out)
}

/// Write each named document under `dir`, creating it, through [`crate::file::write_file`].
fn write_json_files(dir: &Path, files: &[(&str, &Value)]) -> Result<()> {
    for (name, v) in files {
        crate::file::write_file(&dir.join(name), &serde_json::to_string_pretty(v)?)?;
    }
    Ok(())
}

impl RawProfile {
    /// Read the three files from `dir`, and the project from `scope.json` when it is there.
    pub fn load_dir(dir: &Path) -> Result<RawProfile> {
        let profile = read_json_file(dir, PROFILE_FILE)?;
        let manifests = read_json_file(dir, MANIFESTS_FILE)?;
        let variables = read_json_file(dir, VARIABLES_FILE)?;
        let scope = SavedScope::read(dir)?;
        Ok(RawProfile {
            profile,
            manifests,
            variables,
            variables_unavailable: scope.variables_unavailable,
            project_uid: scope.project_uid,
            fetched_at: scope.fetched_at,
            manifest_entities: read_optional(dir, MANIFEST_ENTITIES_FILE)?,
            resolved_values: read_optional(dir, RESOLVED_VALUES_FILE)?,
        })
    }

    /// Write the three files and `scope.json` under `dir`, and make `dir` if it is not there. For
    /// a profile that has a draft, also write its manifests, and with `--macros`, the values of
    /// the macros.
    pub fn save_dir(&self, dir: &Path, fetched_at: &str) -> Result<()> {
        let scope = scope_document(&self.project_uid, fetched_at, self.variables_unavailable);
        let mut files = vec![
            (PROFILE_FILE, &self.profile),
            (MANIFESTS_FILE, &self.manifests),
            (VARIABLES_FILE, &self.variables),
            (SCOPE_FILE, &scope),
        ];
        if let Some(entities) = &self.manifest_entities {
            files.push((MANIFEST_ENTITIES_FILE, entities));
        }
        if let Some(r) = &self.resolved_values {
            files.push((RESOLVED_VALUES_FILE, r));
        }
        write_json_files(dir, &files)
    }
}

/// The three raw API responses a cluster snapshot is built from, and the project they were fetched
/// in. Saved as `cluster.json`, `manifests.json`, `variables.json` and `scope.json` by `--raw-dir`
/// and read back by `--from-raw`.
#[derive(Debug, Clone)]
pub struct RawCluster {
    /// `GET /v1/spectroclusters/{uid}`: the applied profile templates are in
    /// `spec.clusterProfileTemplates`
    pub cluster: Value,
    /// `GET /v1/spectroclusters/{uid}/profiles/packs/manifests`: one entry per applied profile
    pub manifests: Value,
    /// `GET /v1/spectroclusters/{uid}/variables`: one entry per applied profile, or `[]` when
    /// unavailable
    pub variables: Value,
    /// True when the variables endpoint was unavailable, so `variables` stands in for it
    pub variables_unavailable: bool,
    /// The project the responses came from; `None` at tenant scope, or for a directory saved
    /// without `scope.json`
    pub project_uid: Option<String>,
    /// When the responses were read from Palette; `None` for a fetch, and for a directory that an
    /// older build saved
    pub fetched_at: Option<String>,
    /// `GET /v1/notifications/spectrocluster/{uid}`, every page, as one array of its items;
    /// `None` when they were not read, and for a directory that an older build saved
    pub notifications: Option<Value>,
    /// `GET /v1/spectroclusters/{uid}/repave/status`: the repave that waits for approval. `None`
    /// when no repave waits, when Palette did not give it, and for a directory that an older build
    /// saved.
    pub repave: Option<Value>,
    /// `GET /v1/spectroclusters/{uid}/packs/resolvedValues`: for each applied profile, the value
    /// Palette gives each macro. `None` unless asked for, with `snapshot --macros`.
    pub resolved_values: Option<Value>,
    /// `GET /v1/cloudconfigs/{cloudType}/{configUid}`: the machine pools and the cloud part of
    /// the cluster. `None` unless asked for, with `snapshot --shape`.
    pub cloud_config: Option<Value>,
    /// `GET /v1/spectroclusters/{uid}/config/rbacs`: the role bindings of the cluster. `None`
    /// when Palette did not give them, for an edge host and a cluster group, and for a directory
    /// that an older build saved.
    pub rbacs: Option<Value>,
    /// `GET /v1/spectroclusters/{uid}/config/namespaces`: the namespaces of the cluster, with the
    /// resources that each one can use. `None` in the same cases as `rbacs`.
    pub namespaces: Option<Value>,
    /// Why Palette did not give a document of the cluster that is beside the copy, when it
    /// answered with an error.
    pub errors: ReadErrors,
}

/// Why Palette did not give each document of a cluster that is beside the copy, when it answered
/// with an error.
///
/// The notifications, the repave that waits, the role bindings, and the namespaces are not part
/// of the copy. So strata takes the record when it cannot read one of them, and records why.
/// Saved as `errors.json` when it holds an error, so `--from-raw` gives the record of the fetch.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReadErrors {
    /// Why Palette did not give the notifications.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notifications: Option<String>,
    /// Why Palette did not give the repave that waits.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repave: Option<String>,
    /// Why Palette did not give the role bindings, when it answered with an error other than 404.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rbacs: Option<String>,
    /// Why Palette did not give the namespaces, when it answered with an error other than 404.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub namespaces: Option<String>,
}

impl ReadErrors {
    /// True when it holds no error.
    fn is_empty(&self) -> bool {
        *self == ReadErrors::default()
    }
}

impl RawCluster {
    /// The file that marks a raw directory as a cluster's.
    pub const MARKER: &'static str = "cluster.json";

    /// Read the three files from `dir`, and the project from `scope.json` when it is there.
    pub fn load_dir(dir: &Path) -> Result<RawCluster> {
        let cluster = read_json_file(dir, Self::MARKER)?;
        let manifests = read_json_file(dir, MANIFESTS_FILE)?;
        let variables = read_json_file(dir, VARIABLES_FILE)?;
        let scope = SavedScope::read(dir)?;
        Ok(RawCluster {
            cluster,
            manifests,
            variables,
            variables_unavailable: scope.variables_unavailable,
            project_uid: scope.project_uid,
            fetched_at: scope.fetched_at,
            notifications: read_optional(dir, Self::NOTIFICATIONS)?,
            repave: read_optional(dir, Self::REPAVE)?,
            resolved_values: read_optional(dir, Self::RESOLVED_VALUES)?,
            cloud_config: read_optional(dir, Self::CLOUD_CONFIG)?,
            rbacs: read_optional(dir, Self::RBACS)?,
            namespaces: read_optional(dir, Self::NAMESPACES)?,
            // a directory with no errors file is one where Palette gave each document, or one
            // that an older build saved, which did not save the errors
            errors: read_optional(dir, Self::ERRORS)?.unwrap_or_default(),
        })
    }

    /// The file that holds the read errors of the cluster, when Palette answered with an error.
    pub(crate) const ERRORS: &'static str = "errors.json";

    /// The file that holds the role bindings of the cluster, when they were read.
    pub(crate) const RBACS: &'static str = "rbacs.json";

    /// The file that holds the namespaces of the cluster, when they were read.
    pub(crate) const NAMESPACES: &'static str = "namespaces.json";

    /// The file that holds the values Palette gives the macros, when they were read.
    pub const RESOLVED_VALUES: &'static str = RESOLVED_VALUES_FILE;

    /// The file that holds the cloud config of the cluster, when it was read.
    pub const CLOUD_CONFIG: &'static str = "cloud-config.json";

    /// The file that holds the notifications of the cluster, when they were read.
    pub const NOTIFICATIONS: &'static str = "notifications.json";

    /// The file that holds the repave that waits for approval, when it was read.
    pub(crate) const REPAVE: &'static str = "repave.json";

    /// Write the three files, each other document that strata read, and `scope.json` under
    /// `dir`, and make `dir` if it is not there. Strata masks the values of
    /// each pack in the saved notifications and in the saved repave, and each password in the
    /// values of a cluster group.
    ///
    /// # Errors
    ///
    /// [`Error::Io`] when strata cannot write a file, and [`Error::Serialize`] when it cannot
    /// write a document or the masked values of a cluster group.
    pub fn save_dir(&self, dir: &Path, fetched_at: &str) -> Result<()> {
        let scope = scope_document(&self.project_uid, fetched_at, self.variables_unavailable);
        let cluster = masked_group(&self.cluster)?;
        let mut files: Vec<(&str, &Value)> = vec![
            (Self::MARKER, &cluster),
            (MANIFESTS_FILE, &self.manifests),
            (VARIABLES_FILE, &self.variables),
            (SCOPE_FILE, &scope),
        ];
        let notifications = self.notifications.as_ref().map(masked_notifications);
        if let Some(n) = &notifications {
            files.push((Self::NOTIFICATIONS, n));
        }
        let repave = self.repave.as_ref().map(masked_repave);
        if let Some(r) = &repave {
            files.push((Self::REPAVE, r));
        }
        if let Some(r) = &self.resolved_values {
            files.push((Self::RESOLVED_VALUES, r));
        }
        if let Some(c) = &self.cloud_config {
            files.push((Self::CLOUD_CONFIG, c));
        }
        if let Some(r) = &self.rbacs {
            files.push((Self::RBACS, r));
        }
        if let Some(n) = &self.namespaces {
            files.push((Self::NAMESPACES, n));
        }
        let errors = (!self.errors.is_empty())
            .then(|| serde_json::to_value(&self.errors))
            .transpose()?;
        if let Some(e) = &errors {
            files.push((Self::ERRORS, e));
        }
        write_json_files(dir, &files)
    }

    /// The cloud type of the cluster document `cluster`: the kind of its `spec.cloudConfigRef`,
    /// else its `spec.cloudType`, or empty when it names neither.
    ///
    /// The record of a cluster, the route of its cloud config, and its shape all read the cloud
    /// type with this rule.
    pub fn cloud_type(cluster: &Value) -> &str {
        match str_at(cluster, "/spec/cloudConfigRef/kind") {
            "" => str_at(cluster, "/spec/cloudType"),
            k => k,
        }
    }

    /// The applied profile templates, in the order the cluster document lists them. That order is
    /// no install order; `AppliedProfile::of_cluster` puts them in canonical order.
    pub fn templates(&self) -> Vec<Value> {
        arr(&self.cluster, "/spec/clusterProfileTemplates").to_vec()
    }

    /// One applied template as the three documents of a profile, so a cluster's profile is hashed
    /// exactly as the profile itself: the template's packs as the published packs, the cluster's
    /// manifests entry for that profile, and that profile's variable definitions. Variable values
    /// are left out; they are run-time input, not part of the release.
    pub fn template_raw(&self, template: &Value) -> RawProfile {
        let field = |ptr: &str| value_or_null(template, ptr);
        let uid = str_at(template, "/uid");
        let name = str_at(template, "/name");
        let profile = serde_json::json!({
            "metadata": {"uid": uid, "name": name},
            "spec": {
                "version": field("/profileVersion"),
                "published": {
                    "cloudType": field("/cloudType"),
                    "type": field("/type"),
                    "packs": field("/packs"),
                },
            },
            "status": {},
        });
        let manifests = arr(&self.manifests, "/profiles")
            .iter()
            .find(|m| {
                (!uid.is_empty() && str_at(m, "/metadata/uid") == uid)
                    || str_at(m, "/metadata/name") == name
            })
            .cloned()
            .unwrap_or_else(|| serde_json::json!({"spec": {"packs": []}}));
        // the variables route answers an array at the top level
        let variables = arr(&self.variables, "")
            .iter()
            .find(|v| str_at(v, "/profileUid") == uid)
            .map_or_else(
                || serde_json::json!([]),
                |v| value_at(v, "/variables").clone(),
            );
        RawProfile {
            profile,
            manifests,
            variables: serde_json::json!({ "variables": variables }),
            variables_unavailable: self.variables_unavailable,
            project_uid: self.project_uid.clone(),
            fetched_at: self.fetched_at.clone(),
            manifest_entities: None,
            // the macros of a cluster resolve for the cluster, in RawCluster::resolved_values
            resolved_values: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn raw_profile_round_trips_through_a_directory() {
        let dir = tempfile::tempdir().unwrap();
        let raw = RawProfile {
            profile: json!({"a": 1}),
            manifests: json!({"b": 2}),
            variables: json!({"variables": []}),
            variables_unavailable: false,
            project_uid: Some("p1".into()),
            fetched_at: None,
            manifest_entities: Some(json!({"m1": {"spec": {"draft": {"content": "c"}}}})),
            resolved_values: Some(json!({"resolved": {"{{.spectro.macro.region}}": "us-east-2"}})),
        };
        raw.save_dir(dir.path(), "t0").unwrap();
        let back = RawProfile::load_dir(dir.path()).unwrap();
        assert_eq!(back.profile, raw.profile);
        assert_eq!(back.project_uid.as_deref(), Some("p1"));
        assert_eq!(back.manifest_entities, raw.manifest_entities);
        assert_eq!(back.resolved_values, raw.resolved_values);
        std::fs::write(dir.path().join(VARIABLES_FILE), "").unwrap();
        assert!(matches!(
            RawProfile::load_dir(dir.path()).unwrap_err(),
            Error::EmptyFile(_)
        ));
    }
}
