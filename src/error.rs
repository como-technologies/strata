//! One error type for the library. The CLI maps it to an exit status and a message; library users
//! can match on the variant.

use std::path::PathBuf;

use serde_json::{Map, Value};

/// Everything that can go wrong in the library.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// No API key in the environment or in `~/.palette.env`.
    #[error(
        "{primary} is required to fetch from Palette (or {alternate}, or a line in ~/.palette.env)"
    )]
    MissingCredential {
        /// The preferred variable name.
        primary: &'static str,
        /// The accepted alternate variable name.
        alternate: &'static str,
    },
    /// Palette answered with a non-2xx status.
    #[error("GET {path} failed with HTTP {status}: {body}")]
    Api {
        /// Request path.
        path: String,
        /// HTTP status code.
        status: u16,
        /// The first part of the response body, one line.
        body: String,
        /// The `Retry-After` header in whole seconds, when Palette sent one.
        retry_after: Option<u64>,
    },
    /// Palette asked strata to wait longer than it will before retrying.
    #[error(
        "GET {path} failed with HTTP {status} and Retry-After {retry_after}s, longer than the {limit}s strata waits (STRATA_MAX_RETRY_AFTER); try again later"
    )]
    RetryAfterTooLong {
        /// Request path.
        path: String,
        /// HTTP status code.
        status: u16,
        /// The wait Palette asked for, in whole seconds.
        retry_after: u64,
        /// The longest wait strata honors, in whole seconds.
        limit: u64,
    },
    /// A file pattern on the command line is invalid or matched nothing.
    #[error("'{pattern}' {message}")]
    Pattern {
        /// The pattern as given.
        pattern: String,
        /// What is wrong with it.
        message: String,
    },
    /// A setting from the environment has a value strata cannot use.
    #[error("{variable}: expected {expected}, got '{value}'")]
    InvalidSetting {
        /// The variable name.
        variable: &'static str,
        /// The value as given.
        value: String,
        /// What would have been accepted.
        expected: &'static str,
    },
    /// The request did not complete.
    #[error("GET {path}: {message}")]
    Transport {
        /// Request path.
        path: String,
        /// What the transport said.
        message: String,
    },
    /// Palette answered 2xx with a body that is not JSON.
    #[error("GET {path}: response is not JSON: {source}")]
    NotJson {
        /// Request path.
        path: String,
        /// The parse error.
        source: serde_json::Error,
    },
    /// `name@version` matched nothing in the current scope. The message points at `strata list`,
    /// because the name is often a pack's rather than a profile's.
    #[error("no cluster profile named '{name}' with version '{version}' in this scope; `strata list` shows the profiles in it, and `strata list --pack {name}` the profiles with a pack of that name")]
    ProfileNotFound {
        /// Profile name.
        name: String,
        /// Profile version.
        version: String,
    },
    /// `cluster:<uid>/<profile>` named a profile the cluster does not run.
    #[error("cluster '{cluster}' has no applied profile named '{profile}'; applied: {}", applied.join(", "))]
    ClusterProfileNotFound {
        /// Cluster uid.
        cluster: String,
        /// The profile name asked for.
        profile: String,
        /// The names of the profiles applied to the cluster, in order.
        applied: Vec<String>,
    },
    /// A project name matches no project the key can see.
    #[error(
        "no project named '{name}' that this key can see; projects: {}",
        list_or_none(available)
    )]
    ProjectNotFound {
        /// The name asked for.
        name: String,
        /// The names of the projects the key can see, in order.
        available: Vec<String>,
    },
    /// `cluster:<x>` is neither a cluster uid nor the name of a cluster in the scope.
    #[error(
        "no cluster with the uid or name '{cluster}' in this scope; clusters: {}",
        list_or_none(available)
    )]
    ClusterNotFound {
        /// The uid or name as given.
        cluster: String,
        /// The names of the clusters in the scope, in order.
        available: Vec<String>,
    },
    /// `cluster:<name>` matches more than one cluster in the scope.
    #[error("several clusters in this scope are named '{name}': {}; use cluster:<uid>", uids.join(", "))]
    ClusterNameAmbiguous {
        /// The name as given.
        name: String,
        /// The uids of the clusters with that name, in order.
        uids: Vec<String>,
    },
    /// A command that works on a profile was given a cluster, or a record of one.
    #[error("'{reference}' is a cluster; this command takes a profile uid, <name>@<version>, or a profile record")]
    ProfileRefRequired {
        /// The ref as given.
        reference: String,
    },
    /// A candidate file does not parse or breaks a rule of the format.
    #[error("'{}': {message}", path.display())]
    CandidateInvalid {
        /// The candidate file.
        path: PathBuf,
        /// What is wrong.
        message: String,
    },
    /// A candidate layer names a pack version, or a tag, that its registry does not have.
    #[error("no pack '{name}' at version '{version}'{} in registry '{registry}'", tag.as_ref().map(|t| format!(" with tag '{t}'")).unwrap_or_default())]
    PackNotFound {
        /// Pack name.
        name: String,
        /// Pack version.
        version: String,
        /// The tag asked for, when one was.
        tag: Option<String>,
        /// The registry, as the candidate names it.
        registry: String,
    },
    /// A candidate names a registry that is not visible with this key.
    #[error("no registry named '{registry}' is visible; registries: {}", available.join(", "))]
    RegistryNotFound {
        /// The registry name asked for.
        registry: String,
        /// The registry names that are visible.
        available: Vec<String>,
    },
    /// The file exists but is not a strata snapshot.
    #[error("'{0}' is not a strata snapshot")]
    NotSnapshot(PathBuf),
    /// The file is a strata snapshot of a schema version this strata does not read.
    #[error("'{}' is a {found} record; this strata reads {supported}", path.display())]
    UnsupportedSchema {
        /// File path.
        path: PathBuf,
        /// The schema id in the file.
        found: String,
        /// The schema id this strata reads.
        supported: &'static str,
    },
    /// The file claims the snapshot schema but does not match it.
    #[error("'{path}' does not match {schema}: {source}")]
    SnapshotSchema {
        /// File path.
        path: PathBuf,
        /// The schema id expected.
        schema: &'static str,
        /// The deserialization error.
        source: serde_json::Error,
    },
    /// A snapshot without `source.uid` cannot be verified.
    #[error("snapshot has no source.uid to verify against")]
    NoUid,
    /// A file could not be read or written.
    #[error("{path}: {source}")]
    Io {
        /// File path.
        path: PathBuf,
        /// The I/O error.
        source: std::io::Error,
    },
    /// A raw API response file is empty.
    #[error("{0} is empty")]
    EmptyFile(PathBuf),
    /// A raw API response file is not JSON.
    #[error("{path} is not JSON: {source}")]
    FileNotJson {
        /// File path.
        path: PathBuf,
        /// The parse error.
        source: serde_json::Error,
    },
    /// A document could not be serialized.
    #[error("serializing: {0}")]
    Serialize(#[from] serde_json::Error),
}

/// `a, b, c`, or `none` for an empty list.
fn list_or_none(items: &[String]) -> String {
    if items.is_empty() {
        "none".to_string()
    } else {
        items.join(", ")
    }
}

impl Error {
    /// The process exit status the CLI uses for this error. Every error is 2 today; the method
    /// exists so a future distinction (for example, 3 for "cannot reach Palette") lands in one place.
    pub fn exit_code(&self) -> u8 {
        2
    }

    /// A stable kebab-case name for the variant, for programs that read errors. The `match` is
    /// exhaustive, so a new variant cannot be added without naming it here.
    pub fn kind(&self) -> &'static str {
        match self {
            Error::MissingCredential { .. } => "missing-credential",
            Error::Api { .. } => "api",
            Error::RetryAfterTooLong { .. } => "retry-after-too-long",
            Error::InvalidSetting { .. } => "invalid-setting",
            Error::Pattern { .. } => "pattern",
            Error::Transport { .. } => "transport",
            Error::NotJson { .. } => "not-json",
            Error::ProfileNotFound { .. } => "profile-not-found",
            Error::ClusterProfileNotFound { .. } => "cluster-profile-not-found",
            Error::ProfileRefRequired { .. } => "profile-ref-required",
            Error::CandidateInvalid { .. } => "candidate-invalid",
            Error::PackNotFound { .. } => "pack-not-found",
            Error::RegistryNotFound { .. } => "registry-not-found",
            Error::ProjectNotFound { .. } => "project-not-found",
            Error::ClusterNotFound { .. } => "cluster-not-found",
            Error::ClusterNameAmbiguous { .. } => "cluster-name-ambiguous",
            Error::NotSnapshot(_) => "not-snapshot",
            Error::UnsupportedSchema { .. } => "unsupported-schema",
            Error::SnapshotSchema { .. } => "snapshot-schema",
            Error::NoUid => "no-uid",
            Error::Io { .. } => "io",
            Error::EmptyFile(_) => "empty-file",
            Error::FileNotJson { .. } => "file-not-json",
            Error::Serialize(_) => "serialize",
        }
    }

    /// The machine-readable form: `kind`, `message`, and the variant's own fields (`path`,
    /// `status`, `retryAfter`, `limit`, `name`, `version`, `schema`, `variable`, `alternate`, `value`, `cluster`,
    /// `profile`, `applied`, `reference`, `tag`, `registry`, `available`) when it has them.
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
            Error::Api {
                path: p, status, ..
            } => {
                m.insert("path".into(), Value::from(p.as_str()));
                m.insert("status".into(), Value::from(*status));
            }
            Error::RetryAfterTooLong {
                path: p,
                status,
                retry_after,
                limit,
            } => {
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
            Error::CandidateInvalid { path: p, .. } => path(&mut m, p),
            Error::PackNotFound {
                name,
                version,
                tag,
                registry,
            } => {
                m.insert("name".into(), Value::from(name.as_str()));
                m.insert("version".into(), Value::from(version.as_str()));
                if let Some(t) = tag {
                    m.insert("tag".into(), Value::from(t.as_str()));
                }
                m.insert("registry".into(), Value::from(registry.as_str()));
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
            Error::ProfileNotFound { name, version } => {
                m.insert("name".into(), Value::from(name.as_str()));
                m.insert("version".into(), Value::from(version.as_str()));
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
            Error::ClusterNameAmbiguous { name, uids } => {
                m.insert("name".into(), Value::from(name.as_str()));
                m.insert("uids".into(), Value::from(uids.clone()));
            }
            Error::NoUid | Error::Serialize(_) => {}
        }
        Value::Object(m)
    }
}

/// Result alias for the library.
pub type Result<T> = std::result::Result<T, Error>;

#[cfg(test)]
mod tests {
    use super::*;

    fn bad_json() -> serde_json::Error {
        serde_json::from_str::<serde_json::Value>("{").unwrap_err()
    }

    /// One value of every variant. The `match` in `kind` is exhaustive, so the compiler keeps the
    /// mapping total; this test keeps it distinct and keeps `to_json` in step with it.
    fn one_of_each() -> Vec<Error> {
        vec![
            Error::MissingCredential {
                primary: "PALETTE_API_KEY",
                alternate: "SPECTROCLOUD_APIKEY",
            },
            Error::Api {
                path: "/v1/x".into(),
                status: 503,
                body: "busy".into(),
                retry_after: Some(2),
            },
            Error::InvalidSetting {
                variable: "STRATA_RETRIES",
                value: "lots".into(),
                expected: "a whole number",
            },
            Error::RetryAfterTooLong {
                path: "/v1/y".into(),
                status: 429,
                retry_after: 900,
                limit: 300,
            },
            Error::Pattern {
                pattern: "releases/**/*.json".into(),
                message: "matched no files".into(),
            },
            Error::Transport {
                path: "/v1/x".into(),
                message: "refused".into(),
            },
            Error::NotJson {
                path: "/v1/x".into(),
                source: bad_json(),
            },
            Error::ProfileNotFound {
                name: "aws-pxk".into(),
                version: "9.9.9".into(),
            },
            Error::ClusterProfileNotFound {
                cluster: "c1".into(),
                profile: "nope".into(),
                applied: vec!["infra".into(), "addons".into()],
            },
            Error::ProfileRefRequired {
                reference: "cluster:c1".into(),
            },
            Error::CandidateInvalid {
                path: "profile.yaml".into(),
                message: "a candidate needs at least one layer".into(),
            },
            Error::PackNotFound {
                name: "kubernetes".into(),
                version: "9.9.9".into(),
                tag: Some("2.x".into()),
                registry: "Public Repo".into(),
            },
            Error::RegistryNotFound {
                registry: "Nope".into(),
                available: vec!["Public Repo".into()],
            },
            Error::ProjectNotFound {
                name: "staging".into(),
                available: vec!["lab".into(), "prod".into()],
            },
            Error::ClusterNotFound {
                cluster: "nope".into(),
                available: vec![],
            },
            Error::ClusterNameAmbiguous {
                name: "twin".into(),
                uids: vec!["c7".into(), "c8".into()],
            },
            Error::UnsupportedSchema {
                path: "newer.json".into(),
                found: "strata-snapshot/2".into(),
                supported: "strata-snapshot/1",
            },
            Error::NotSnapshot("profile.json".into()),
            Error::SnapshotSchema {
                path: "old.json".into(),
                schema: "strata-snapshot/1",
                source: bad_json(),
            },
            Error::NoUid,
            Error::Io {
                path: "rel.json".into(),
                source: std::io::Error::other("denied"),
            },
            Error::EmptyFile("variables.json".into()),
            Error::FileNotJson {
                path: "profile.json".into(),
                source: bad_json(),
            },
            Error::Serialize(bad_json()),
        ]
    }

    #[test]
    fn every_variant_has_a_distinct_kind() {
        let all = one_of_each();
        let mut kinds: Vec<&str> = all.iter().map(Error::kind).collect();
        kinds.sort_unstable();
        kinds.dedup();
        assert_eq!(kinds.len(), all.len(), "kinds: {kinds:?}");
        for k in &kinds {
            assert!(
                k.chars().all(|c| c.is_ascii_lowercase() || c == '-'),
                "kind {k} is not kebab-case"
            );
        }
    }

    #[test]
    fn to_json_carries_kind_message_and_the_variant_fields() {
        for e in one_of_each() {
            let j = e.to_json();
            assert_eq!(j["kind"], e.kind());
            assert_eq!(j["message"], e.to_string());
        }
        let by_kind = |k: &str| {
            one_of_each()
                .into_iter()
                .find(|e| e.kind() == k)
                .unwrap()
                .to_json()
        };
        assert_eq!(by_kind("api")["status"], 503);
        assert_eq!(by_kind("api")["path"], "/v1/x");
        assert_eq!(by_kind("missing-credential")["variable"], "PALETTE_API_KEY");
        assert_eq!(
            by_kind("missing-credential")["alternate"],
            "SPECTROCLOUD_APIKEY"
        );
        assert_eq!(by_kind("not-snapshot")["path"], "profile.json");
        assert_eq!(by_kind("profile-not-found")["name"], "aws-pxk");
        assert_eq!(by_kind("snapshot-schema")["schema"], "strata-snapshot/1");
        assert_eq!(by_kind("invalid-setting")["variable"], "STRATA_RETRIES");
        assert_eq!(by_kind("invalid-setting")["value"], "lots");
        assert_eq!(by_kind("retry-after-too-long")["path"], "/v1/y");
        assert_eq!(by_kind("retry-after-too-long")["status"], 429);
        assert_eq!(by_kind("retry-after-too-long")["retryAfter"], 900);
        assert_eq!(by_kind("retry-after-too-long")["limit"], 300);
        assert_eq!(by_kind("project-not-found")["name"], "staging");
        assert_eq!(by_kind("project-not-found")["available"][1], "prod");
        assert_eq!(by_kind("cluster-not-found")["cluster"], "nope");
        assert!(by_kind("cluster-not-found")["message"]
            .as_str()
            .unwrap()
            .ends_with("clusters: none"));
        assert_eq!(by_kind("cluster-name-ambiguous")["uids"][0], "c7");
        assert_eq!(by_kind("pattern")["pattern"], "releases/**/*.json");
        let cp = by_kind("cluster-profile-not-found");
        assert_eq!(
            (cp["cluster"].clone(), cp["profile"].clone()),
            ("c1".into(), "nope".into())
        );
        assert_eq!(cp["applied"], serde_json::json!(["infra", "addons"]));
        assert_eq!(by_kind("profile-ref-required")["reference"], "cluster:c1");
        assert_eq!(by_kind("candidate-invalid")["path"], "profile.yaml");
        assert_eq!(by_kind("unsupported-schema")["found"], "strata-snapshot/2");
        let pnf = by_kind("pack-not-found");
        assert_eq!(
            (pnf["name"].clone(), pnf["tag"].clone()),
            ("kubernetes".into(), "2.x".into())
        );
        assert_eq!(
            by_kind("registry-not-found")["available"],
            serde_json::json!(["Public Repo"])
        );
    }
}
