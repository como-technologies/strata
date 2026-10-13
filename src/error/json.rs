//! The machine-readable form of an error, for programs that read errors.

use serde_json::{Map, Value};

use super::Error;

impl Error {
    /// The machine-readable form: `kind`, `message`, and the variant's own fields (`path`,
    /// `status`, `retryAfter`, `limit`, `name`, `version`, `schema`, `variable`, `alternate`, `value`, `cluster`,
    /// `profile`, `applied`, `reference`, `tag`, `registry`, `state`, `available`, `errors`) when it has them.
    pub fn to_json(&self) -> Value {
        let mut m = Map::new();
        m.insert("kind".into(), Value::from(self.kind()));
        m.insert("message".into(), Value::from(self.to_string()));
        let path = |m: &mut Map<String, Value>, p: &std::path::Path| {
            m.insert("path".into(), Value::from(p.display().to_string()));
        };
        match self {
            Error::MissingCredential { primary, alternate } => {
                m.insert("variable".into(), Value::from(*primary));
                m.insert("alternate".into(), Value::from(*alternate));
            }
            Error::RecordNotASource { path: p } => {
                m.insert("path".into(), Value::from(p.as_str()));
            }
            Error::PaletteRefuses { reference, errors } => {
                m.insert("reference".into(), Value::from(reference.as_str()));
                m.insert(
                    "errors".into(),
                    serde_json::to_value(errors).unwrap_or_default(),
                );
            }
            // the message carries the whole of it, and no field adds to that
            Error::Usage { .. } => {}
            Error::Api {
                method,
                path: p,
                status,
                ..
            } => {
                m.insert("method".into(), Value::from(method.as_str()));
                m.insert("path".into(), Value::from(p.as_str()));
                m.insert("status".into(), Value::from(*status));
            }
            Error::RetryAfterTooLong {
                method,
                path: p,
                status,
                retry_after,
                limit,
            } => {
                m.insert("method".into(), Value::from(method.as_str()));
                m.insert("path".into(), Value::from(p.as_str()));
                m.insert("status".into(), Value::from(*status));
                m.insert("retryAfter".into(), Value::from(*retry_after));
                m.insert("limit".into(), Value::from(*limit));
            }
            Error::Pattern { pattern, .. } => {
                m.insert("pattern".into(), Value::from(pattern.as_str()));
            }
            Error::InvalidSetting {
                variable, value, ..
            } => {
                m.insert("variable".into(), Value::from(*variable));
                m.insert("value".into(), Value::from(value.as_str()));
            }
            Error::Transport { path: p, .. } | Error::NotJson { path: p, .. } => {
                m.insert("path".into(), Value::from(p.as_str()));
            }
            Error::ClusterProfileNotFound {
                cluster,
                profile,
                applied,
            } => {
                m.insert("cluster".into(), Value::from(cluster.as_str()));
                m.insert("profile".into(), Value::from(profile.as_str()));
                m.insert("applied".into(), Value::from(applied.clone()));
            }
            Error::CandidateInvalid { path: p, rule, .. } => {
                path(&mut m, p);
                if let Some(rule) = rule {
                    m.insert("code".into(), Value::from(rule.code()));
                }
            }
            Error::PackNotFound {
                name,
                version,
                tag,
                registry,
                unsynced,
            } => {
                m.insert("name".into(), Value::from(name.as_str()));
                m.insert("version".into(), Value::from(version.as_str()));
                if let Some(t) = tag {
                    m.insert("tag".into(), Value::from(t.as_str()));
                }
                m.insert("registry".into(), Value::from(registry.as_str()));
                put_unsynced(&mut m, unsynced);
            }
            Error::PackUnavailable {
                name,
                version,
                registry,
                state,
            } => {
                m.insert("name".into(), Value::from(name.as_str()));
                m.insert("version".into(), Value::from(version.as_str()));
                m.insert("registry".into(), Value::from(registry.as_str()));
                m.insert("state".into(), Value::from(state.as_str()));
            }
            // no version and no registry was asked for, so the document names neither
            Error::PackNameNotFound { name, unsynced } => {
                m.insert("name".into(), Value::from(name.as_str()));
                put_unsynced(&mut m, unsynced);
            }
            Error::PackInSeveralRegistries {
                name,
                version,
                registries,
            } => {
                m.insert("name".into(), Value::from(name.as_str()));
                m.insert("version".into(), Value::from(version.as_str()));
                m.insert("registries".into(), Value::from(registries.clone()));
            }
            Error::RegistryNotFound {
                registry,
                available,
            } => {
                m.insert("registry".into(), Value::from(registry.as_str()));
                m.insert("available".into(), Value::from(available.clone()));
            }
            Error::ProfileRefRequired { reference } => {
                m.insert("reference".into(), Value::from(reference.as_str()));
            }
            Error::NoDraft { name, version, uid } => {
                m.insert("name".into(), Value::from(name.as_str()));
                m.insert("version".into(), Value::from(version.as_str()));
                m.insert("uid".into(), Value::from(uid.as_str()));
            }
            Error::ProfileNotFound {
                name,
                version,
                tenant_scope,
            } => {
                m.insert("name".into(), Value::from(name.as_str()));
                m.insert("version".into(), Value::from(version.as_str()));
                m.insert("tenantScope".into(), Value::from(*tenant_scope));
            }
            Error::ProfileNameAmbiguous {
                name,
                version,
                profiles,
            } => {
                m.insert("name".into(), Value::from(name.as_str()));
                m.insert("version".into(), Value::from(version.as_str()));
                m.insert(
                    "profiles".into(),
                    serde_json::to_value(profiles).unwrap_or_default(),
                );
            }
            Error::NotSnapshot(p) | Error::EmptyFile(p) => path(&mut m, p),
            Error::UnsupportedSchema {
                path: p,
                found,
                supported,
            } => {
                path(&mut m, p);
                m.insert("found".into(), Value::from(found.as_str()));
                m.insert("supported".into(), Value::from(*supported));
            }
            Error::SnapshotSchema {
                path: p, schema, ..
            } => {
                path(&mut m, p);
                m.insert("schema".into(), Value::from(*schema));
            }
            Error::Io { path: p, .. } | Error::FileNotJson { path: p, .. } => path(&mut m, p),
            Error::ProjectNotFound { name, available } => {
                m.insert("name".into(), Value::from(name.as_str()));
                m.insert("available".into(), Value::from(available.clone()));
            }
            Error::ClusterNotFound { cluster, available } => {
                m.insert("cluster".into(), Value::from(cluster.as_str()));
                m.insert("available".into(), Value::from(available.clone()));
            }
            Error::ClusterNameAmbiguous { name, uids }
            | Error::EdgeHostNameAmbiguous { name, uids }
            | Error::ClusterGroupNameAmbiguous { name, uids }
            | Error::FilterNameAmbiguous { name, uids } => {
                m.insert("name".into(), Value::from(name.as_str()));
                m.insert("uids".into(), Value::from(uids.clone()));
            }
            Error::EdgeHostNotFound {
                edge_host,
                available,
            } => {
                m.insert("edgeHost".into(), Value::from(edge_host.as_str()));
                m.insert("available".into(), Value::from(available.clone()));
            }
            Error::FilterNotFound { filter, available } => {
                m.insert("filter".into(), Value::from(filter.as_str()));
                m.insert("available".into(), Value::from(available.clone()));
            }
            Error::AppProfileNotFound {
                name,
                version,
                available,
            } => {
                m.insert("name".into(), Value::from(name.as_str()));
                m.insert("version".into(), Value::from(version.as_str()));
                m.insert("available".into(), Value::from(available.clone()));
            }
            Error::ClusterGroupNotFound {
                cluster_group,
                available,
            } => {
                m.insert("clusterGroup".into(), Value::from(cluster_group.as_str()));
                m.insert("available".into(), Value::from(available.clone()));
            }
            Error::NoUid | Error::Serialize(_) => {}
        }
        Value::Object(m)
    }
}

/// Put the registries that Palette does not sync into the document of an error, when there are
/// any.
fn put_unsynced(m: &mut serde_json::Map<String, Value>, unsynced: &[String]) {
    if !unsynced.is_empty() {
        m.insert("unsynced".into(), Value::from(unsynced.to_vec()));
    }
}
