//! The machine-readable form of an error, for programs that read errors.

use std::borrow::Cow;

use schemars::{JsonSchema, Schema, SchemaGenerator, json_schema};
use serde_json::{Map, Value};

use super::Error;
use crate::closed_set::ClosedSet;

/// The key of the kind of an error in its document.
const KIND: &str = "kind";

/// The key of the message of an error in its document.
const MESSAGE: &str = "message";

/// The JSON pointer to [`MESSAGE`] in the document of an error. A reader of such a document uses
/// it with the helpers of `crate::json`.
pub(crate) const MESSAGE_AT: &str = "/message";

/// The schema of the document that [`Error::to_json`] gives: an object with `kind` and `message`,
/// and the fields of the kind.
///
/// A field of a report that holds such a document takes this schema with
/// `#[schemars(with = "ErrorDocument")]`, and the field stays a [`Value`] in the code.
pub(crate) struct ErrorDocument;

impl JsonSchema for ErrorDocument {
    fn schema_name() -> Cow<'static, str> {
        "ErrorDocument".into()
    }

    fn json_schema(_: &mut SchemaGenerator) -> Schema {
        json_schema!({
            "description": "The error of strata as a document: `kind` and `message`, and the fields of the kind. The page \"Errors\" of the book gives the fields of each kind.",
            "type": "object",
            "properties": {
                KIND: {
                    "description": "The kind of the error, in lowercase words with hyphens, such as `api`.",
                    "type": "string",
                },
                MESSAGE: {
                    "description": "The message that strata prints for the error.",
                    "type": "string",
                },
            },
            "required": [KIND, MESSAGE],
        })
    }
}

impl Error {
    /// The machine-readable form: `kind`, `message`, and the fields of the variant.
    ///
    /// The page "Errors" of the book gives the fields of each kind.
    pub fn to_json(&self) -> Value {
        let mut m = Map::new();
        put(&mut m, KIND, self.kind());
        put(&mut m, MESSAGE, self.to_string());
        // each function writes the fields of its own variants and no other
        self.put_setting_fields(&mut m);
        self.put_request_fields(&mut m);
        self.put_file_fields(&mut m);
        self.put_profile_fields(&mut m);
        self.put_cluster_fields(&mut m);
        self.put_pack_fields(&mut m);
        self.put_lookup_fields(&mut m);
        Value::Object(m)
    }

    /// Put the fields of a variant that a setting, a reference, or a refusal of Palette causes.
    fn put_setting_fields(&self, m: &mut Map<String, Value>) {
        match self {
            Error::MissingCredential { primary, alternate } => {
                put(m, "variable", *primary);
                put(m, "alternate", *alternate);
            }
            Error::InvalidSetting {
                variable, value, ..
            } => {
                put(m, "variable", *variable);
                put(m, "value", value.as_str());
            }
            Error::Pattern { pattern, .. } => put(m, "pattern", pattern.as_str()),
            Error::PaletteRefuses { reference, errors } => {
                put(m, "reference", reference.as_str());
                put(
                    m,
                    "errors",
                    serde_json::to_value(errors).unwrap_or_default(),
                );
            }
            Error::ProfileRefRequired { reference } => put(m, "reference", reference.as_str()),
            // the message carries the whole of it, and no field adds to that
            _ => {}
        }
    }

    /// Put the fields of a variant that a request to Palette causes.
    fn put_request_fields(&self, m: &mut Map<String, Value>) {
        match self {
            Error::Api {
                method,
                path,
                status,
                ..
            } => {
                put_request(m, method.as_str(), path);
                put(m, "status", *status);
            }
            Error::RetryAfterTooLong {
                method,
                path,
                status,
                retry_after,
                limit,
            } => {
                put_request(m, method.as_str(), path);
                put(m, "status", *status);
                put(m, "retryAfter", *retry_after);
                put(m, "limit", *limit);
            }
            Error::Transport { method, path, .. } | Error::NotJson { method, path, .. } => {
                put_request(m, method.as_str(), path);
            }
            _ => {}
        }
    }

    /// Put the fields of a variant that a file causes: a record, a candidate, or a file of any
    /// other kind.
    fn put_file_fields(&self, m: &mut Map<String, Value>) {
        match self {
            Error::RecordNotASource { path } => put(m, "path", path.as_str()),
            Error::CandidateInvalid { path, rule, .. } => {
                put_path(m, path);
                if let Some(rule) = rule {
                    put(m, "code", rule.code());
                }
            }
            Error::NotSnapshot(path)
            | Error::EmptyFile(path)
            | Error::Io { path, .. }
            | Error::FileNotJson { path, .. } => put_path(m, path),
            Error::UnsupportedSchema {
                path,
                found,
                supported,
            } => {
                put_path(m, path);
                put(m, "found", found.as_str());
                put(m, "supported", *supported);
            }
            Error::SnapshotSchema { path, schema, .. } => {
                put_path(m, path);
                put(m, "schema", *schema);
            }
            Error::LogInvalid { path, line, .. } => {
                put_path(m, path);
                put(m, "line", *line);
            }
            _ => {}
        }
    }

    /// Put the fields of a variant that a profile causes.
    fn put_profile_fields(&self, m: &mut Map<String, Value>) {
        match self {
            Error::ClusterProfileNotFound {
                cluster,
                profile,
                applied,
            } => {
                put(m, "cluster", cluster.as_str());
                put(m, "profile", profile.as_str());
                put(m, "applied", applied.clone());
            }
            Error::NoDraft { name, version, uid } => {
                put_name_version(m, name, version);
                put(m, "uid", uid.as_str());
            }
            Error::ProfileNotFound {
                name,
                version,
                tenant_scope,
            } => {
                put_name_version(m, name, version);
                put(m, "tenantScope", *tenant_scope);
            }
            Error::ProfileNameAmbiguous {
                name,
                version,
                profiles,
            } => {
                put_name_version(m, name, version);
                put(
                    m,
                    "profiles",
                    serde_json::to_value(profiles).unwrap_or_default(),
                );
            }
            Error::AppProfileNameAmbiguous {
                name,
                version,
                uids,
            } => {
                put_name_version(m, name, version);
                put(m, "uids", uids.clone());
            }
            Error::AppProfileNotFound {
                name,
                version,
                available,
            } => {
                put_name_version(m, name, version);
                put(m, "available", available.clone());
            }
            _ => {}
        }
    }

    /// Put the fields of a variant that a cluster causes.
    fn put_cluster_fields(&self, m: &mut Map<String, Value>) {
        match self {
            Error::StaleRecord { cluster, profile } => {
                put(m, "cluster", cluster.as_str());
                put(m, "profile", profile.as_str());
            }
            // `asked` is a part of the message, and no field adds to that
            Error::ClusterTemplateGoverns {
                cluster, template, ..
            } => {
                put(m, "cluster", cluster.as_str());
                put(m, "template", template.as_str());
            }
            Error::NoCloudConfig { cluster } => put(m, "cluster", cluster.as_str()),
            Error::ClusterNotFound { cluster, available } => {
                put(m, "cluster", cluster.as_str());
                put(m, "available", available.clone());
            }
            _ => {}
        }
    }

    /// Put the fields of a variant that a pack or a registry causes.
    fn put_pack_fields(&self, m: &mut Map<String, Value>) {
        match self {
            Error::PackNotFound {
                name,
                version,
                tag,
                registry,
                unsynced,
            } => {
                put_name_version(m, name, version);
                if let Some(t) = tag {
                    put(m, "tag", t.as_str());
                }
                put(m, "registry", registry.as_str());
                put_unsynced(m, unsynced);
            }
            Error::PackUnavailable {
                name,
                version,
                registry,
                state,
            } => {
                put_name_version(m, name, version);
                put(m, "registry", registry.as_str());
                put(m, "state", state.as_str());
            }
            // no version and no registry was asked for, so the document names neither
            Error::PackNameNotFound { name, unsynced } => {
                put(m, "name", name.as_str());
                put_unsynced(m, unsynced);
            }
            Error::PackInSeveralRegistries {
                name,
                version,
                registries,
            } => {
                put_name_version(m, name, version);
                put(m, "registries", registries.clone());
            }
            Error::RegistryNotFound {
                registry,
                available,
            } => {
                put(m, "registry", registry.as_str());
                put(m, "available", available.clone());
            }
            _ => {}
        }
    }

    /// Put the fields of a variant that a name or a uid of the scope causes: no match, or more
    /// than one.
    fn put_lookup_fields(&self, m: &mut Map<String, Value>) {
        match self {
            Error::ProjectNotFound { name, available } => {
                put(m, "name", name.as_str());
                put(m, "available", available.clone());
            }
            Error::ClusterNameAmbiguous { name, uids }
            | Error::EdgeHostNameAmbiguous { name, uids }
            | Error::ClusterGroupNameAmbiguous { name, uids }
            | Error::FilterNameAmbiguous { name, uids }
            | Error::RegistryNameAmbiguous { name, uids } => {
                put(m, "name", name.as_str());
                put(m, "uids", uids.clone());
            }
            Error::EdgeHostNotFound {
                edge_host,
                available,
            } => {
                put(m, "edgeHost", edge_host.as_str());
                put(m, "available", available.clone());
            }
            Error::FilterNotFound { filter, available } => {
                put(m, "filter", filter.as_str());
                put(m, "available", available.clone());
            }
            Error::ClusterGroupNotFound {
                cluster_group,
                available,
            } => {
                put(m, "clusterGroup", cluster_group.as_str());
                put(m, "available", available.clone());
            }
            _ => {}
        }
    }
}

/// Put one field into the document of an error.
fn put(m: &mut Map<String, Value>, key: &str, value: impl Into<Value>) {
    m.insert(key.into(), value.into());
}

/// Put the `method` and the `path` of a request into the document of an error.
fn put_request(m: &mut Map<String, Value>, method: &str, path: &str) {
    put(m, "method", method);
    put(m, "path", path);
}

/// Put a `path` that is a file into the document of an error.
fn put_path(m: &mut Map<String, Value>, path: &std::path::Path) {
    put(m, "path", path.display().to_string());
}

/// Put the `name` and the `version` of a profile or a pack into the document of an error.
fn put_name_version(m: &mut Map<String, Value>, name: &str, version: &str) {
    put(m, "name", name);
    put(m, "version", version);
}

/// Put the registries that Palette does not sync into the document of an error, when there are
/// any.
fn put_unsynced(m: &mut Map<String, Value>, unsynced: &[String]) {
    if !unsynced.is_empty() {
        put(m, "unsynced", unsynced.to_vec());
    }
}
