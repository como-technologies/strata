//! The API responses a snapshot is built from, and their directory on disk.

use std::fs;
use std::path::Path;

use serde_json::Value;

use crate::error::{Error, Result};

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
}

/// The file that records the project a raw directory's responses came from.
const SCOPE_FILE: &str = "scope.json";

/// The project saved in `dir`, or `None` when it has no scope file or names no project.
fn read_scope(dir: &Path) -> Result<Option<String>> {
    scope_field(dir, "projectUid")
}

/// When the responses in `dir` were read from Palette. `None` for a directory that an older build
/// saved, which recorded no time.
fn read_scope_fetched_at(dir: &Path) -> Result<Option<String>> {
    scope_field(dir, "fetchedAt")
}

/// Whether the variables endpoint was unavailable when `dir` was saved. A directory that an older
/// build saved records nothing, which reads as available: the stand-in and a real empty list look
/// alike, and only this flag tells them apart.
fn read_scope_variables_unavailable(dir: &Path) -> Result<bool> {
    if !dir.join(SCOPE_FILE).is_file() {
        return Ok(false);
    }
    let scope = read_json_file(dir, SCOPE_FILE)?;
    Ok(scope
        .get("variablesUnavailable")
        .and_then(Value::as_bool)
        .unwrap_or(false))
}

fn scope_field(dir: &Path, key: &str) -> Result<Option<String>> {
    if !dir.join(SCOPE_FILE).is_file() {
        return Ok(None);
    }
    let scope = read_json_file(dir, SCOPE_FILE)?;
    Ok(scope
        .get(key)
        .and_then(Value::as_str)
        .filter(|p| !p.is_empty())
        .map(str::to_string))
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

/// Read one JSON file from `dir`; an empty file is an error of its own.
fn read_json_file(dir: &Path, name: &str) -> Result<Value> {
    let path = dir.join(name);
    let text = fs::read_to_string(&path).map_err(|source| Error::Io {
        path: path.clone(),
        source,
    })?;
    if text.trim().is_empty() {
        return Err(Error::EmptyFile(path));
    }
    serde_json::from_str(&text).map_err(|source| Error::FileNotJson { path, source })
}

/// Write each named document under `dir`, creating it.
fn write_json_files(dir: &Path, files: &[(&str, &Value)]) -> Result<()> {
    fs::create_dir_all(dir).map_err(|source| Error::Io {
        path: dir.to_path_buf(),
        source,
    })?;
    for (name, v) in files {
        let path = dir.join(name);
        fs::write(&path, serde_json::to_string_pretty(v)?)
            .map_err(|source| Error::Io { path, source })?;
    }
    Ok(())
}

impl RawProfile {
    /// Read the three files from `dir`, and the project from `scope.json` when it is there.
    pub fn load_dir(dir: &Path) -> Result<RawProfile> {
        Ok(RawProfile {
            profile: read_json_file(dir, "profile.json")?,
            manifests: read_json_file(dir, "manifests.json")?,
            variables: read_json_file(dir, "variables.json")?,
            variables_unavailable: read_scope_variables_unavailable(dir)?,
            project_uid: read_scope(dir)?,
            fetched_at: read_scope_fetched_at(dir)?,
        })
    }

    /// Write the three files and `scope.json` under `dir`, creating it.
    pub fn save_dir(&self, dir: &Path, fetched_at: &str) -> Result<()> {
        write_json_files(
            dir,
            &[
                ("profile.json", &self.profile),
                ("manifests.json", &self.manifests),
                ("variables.json", &self.variables),
                (
                    SCOPE_FILE,
                    &scope_document(&self.project_uid, fetched_at, self.variables_unavailable),
                ),
            ],
        )
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
    /// Why Palette did not give the notifications, when it did not
    pub notifications_error: Option<String>,
    /// `GET /v1/spectroclusters/{uid}/packs/resolvedValues`: for each applied profile, the value
    /// Palette gives each macro. `None` unless asked for, with `snapshot --macros`.
    pub resolved_values: Option<Value>,
}

impl RawCluster {
    /// The file that marks a raw directory as a cluster's.
    pub const MARKER: &'static str = "cluster.json";

    /// Read the three files from `dir`, and the project from `scope.json` when it is there.
    pub fn load_dir(dir: &Path) -> Result<RawCluster> {
        Ok(RawCluster {
            cluster: read_json_file(dir, Self::MARKER)?,
            manifests: read_json_file(dir, "manifests.json")?,
            variables: read_json_file(dir, "variables.json")?,
            variables_unavailable: read_scope_variables_unavailable(dir)?,
            project_uid: read_scope(dir)?,
            fetched_at: read_scope_fetched_at(dir)?,
            notifications: if dir.join(Self::NOTIFICATIONS).is_file() {
                Some(read_json_file(dir, Self::NOTIFICATIONS)?)
            } else {
                None
            },
            notifications_error: None,
            resolved_values: if dir.join(Self::RESOLVED_VALUES).is_file() {
                Some(read_json_file(dir, Self::RESOLVED_VALUES)?)
            } else {
                None
            },
        })
    }

    /// The file that holds the values Palette gives the macros, when they were read.
    pub const RESOLVED_VALUES: &'static str = "resolved-values.json";

    /// The file that holds the notifications of the cluster, when they were read.
    pub const NOTIFICATIONS: &'static str = "notifications.json";

    /// Write the three files, the notifications when they were read, and `scope.json` under
    /// `dir`, creating it.
    pub fn save_dir(&self, dir: &Path, fetched_at: &str) -> Result<()> {
        let scope = scope_document(&self.project_uid, fetched_at, self.variables_unavailable);
        let mut files: Vec<(&str, &Value)> = vec![
            (Self::MARKER, &self.cluster),
            ("manifests.json", &self.manifests),
            ("variables.json", &self.variables),
            (SCOPE_FILE, &scope),
        ];
        if let Some(n) = &self.notifications {
            files.push((Self::NOTIFICATIONS, n));
        }
        if let Some(r) = &self.resolved_values {
            files.push((Self::RESOLVED_VALUES, r));
        }
        write_json_files(dir, &files)
    }

    /// The applied profile templates, in the order the cluster document lists them. That order is
    /// no install order; `AppliedProfile::of_cluster` puts them in canonical order.
    pub fn templates(&self) -> Vec<Value> {
        self.cluster
            .pointer("/spec/clusterProfileTemplates")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default()
    }

    /// One applied template as the three documents of a profile, so a cluster's profile is hashed
    /// exactly as the profile itself: the template's packs as the published packs, the cluster's
    /// manifests entry for that profile, and that profile's variable definitions. Variable values
    /// are left out; they are run-time input, not part of the release.
    pub fn template_raw(&self, template: &Value) -> RawProfile {
        let field = |k: &str| template.get(k).cloned().unwrap_or(Value::Null);
        let uid = template.get("uid").and_then(Value::as_str).unwrap_or("");
        let name = template.get("name").and_then(Value::as_str).unwrap_or("");
        let profile = serde_json::json!({
            "metadata": {"uid": uid, "name": name},
            "spec": {
                "version": field("profileVersion"),
                "published": {
                    "cloudType": field("cloudType"),
                    "type": field("type"),
                    "packs": field("packs"),
                },
            },
            "status": {},
        });
        let entries = |doc: &Value, key: &str| -> Vec<Value> {
            match key {
                "" => doc.as_array().cloned().unwrap_or_default(),
                k => doc
                    .get(k)
                    .and_then(Value::as_array)
                    .cloned()
                    .unwrap_or_default(),
            }
        };
        let meta = |m: &Value, k: &str| {
            m.pointer(&format!("/metadata/{k}"))
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string()
        };
        let manifests = entries(&self.manifests, "profiles")
            .into_iter()
            .find(|m| (!uid.is_empty() && meta(m, "uid") == uid) || meta(m, "name") == name)
            .unwrap_or_else(|| serde_json::json!({"spec": {"packs": []}}));
        let variables = entries(&self.variables, "")
            .into_iter()
            .find(|v| v.get("profileUid").and_then(Value::as_str) == Some(uid))
            .and_then(|v| v.get("variables").cloned())
            .unwrap_or_else(|| serde_json::json!([]));
        RawProfile {
            profile,
            manifests,
            variables: serde_json::json!({ "variables": variables }),
            variables_unavailable: self.variables_unavailable,
            project_uid: self.project_uid.clone(),
            fetched_at: self.fetched_at.clone(),
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
        };
        raw.save_dir(dir.path(), "t0").unwrap();
        let back = RawProfile::load_dir(dir.path()).unwrap();
        assert_eq!(back.profile, raw.profile);
        assert_eq!(back.project_uid.as_deref(), Some("p1"));
        fs::write(dir.path().join("variables.json"), "").unwrap();
        assert!(matches!(
            RawProfile::load_dir(dir.path()).unwrap_err(),
            Error::EmptyFile(_)
        ));
    }
}
