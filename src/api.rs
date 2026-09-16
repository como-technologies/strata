//! Access to the Palette API behind one trait, so that every operation can run against canned
//! responses in tests and against HTTP in production.

use std::collections::HashMap;
use std::fs;
use std::path::Path;

use serde_json::Value;

use crate::config::Config;
use crate::error::{Error, Result};
use crate::progress::Progress;

/// One GET of a JSON document from Palette. Implementations add authentication and scoping.
pub trait PaletteApi {
    /// GET `path` (for example `/v1/clusterprofiles/{uid}`) and parse the body as JSON.
    fn get_json(&self, path: &str) -> Result<Value>;
}

/// The HTTP implementation.
pub struct HttpApi {
    config: Config,
    agent: ureq::Agent,
}

impl HttpApi {
    /// Build an HTTP client for `config`.
    pub fn new(config: Config) -> Self {
        let agent: ureq::Agent = ureq::Agent::config_builder()
            .http_status_as_error(false)
            .timeout_global(Some(config.timeout))
            .build()
            .into();
        HttpApi { config, agent }
    }
}

impl PaletteApi for HttpApi {
    fn get_json(&self, path: &str) -> Result<Value> {
        let url = format!("{}{}", self.config.base_url, path);
        let mut req = self
            .agent
            .get(url.as_str())
            .header("ApiKey", self.config.api_key.as_str())
            .header("Accept", "application/json");
        if let Some(p) = &self.config.project_uid {
            req = req.header("ProjectUid", p.as_str());
        }
        let mut resp = req.call().map_err(|e| Error::Transport {
            path: path.to_string(),
            message: e.to_string(),
        })?;
        let status = resp.status().as_u16();
        let body = resp
            .body_mut()
            .with_config()
            .limit(256 * 1024 * 1024)
            .read_to_string()
            .map_err(|e| Error::Transport {
                path: path.to_string(),
                message: format!("reading the response body: {e}"),
            })?;
        if !(200..300).contains(&status) {
            return Err(Error::Api {
                path: path.to_string(),
                status,
                body: body
                    .chars()
                    .take(300)
                    .collect::<String>()
                    .replace('\n', " "),
            });
        }
        serde_json::from_str(&body).map_err(|source| Error::NotJson {
            path: path.to_string(),
            source,
        })
    }
}

/// Canned responses keyed by path. Paths not registered answer 404.
#[derive(Debug, Default, Clone)]
pub struct MemoryApi {
    ok: HashMap<String, Value>,
    fail: HashMap<String, (u16, String)>,
}

impl MemoryApi {
    /// An API that knows no paths.
    pub fn new() -> Self {
        Self::default()
    }

    /// Answer `path` with `value`.
    pub fn with(mut self, path: impl Into<String>, value: Value) -> Self {
        self.ok.insert(path.into(), value);
        self
    }

    /// Answer `path` with an HTTP error.
    pub fn failing(
        mut self,
        path: impl Into<String>,
        status: u16,
        body: impl Into<String>,
    ) -> Self {
        self.fail.insert(path.into(), (status, body.into()));
        self
    }
}

impl PaletteApi for MemoryApi {
    fn get_json(&self, path: &str) -> Result<Value> {
        if let Some((status, body)) = self.fail.get(path) {
            return Err(Error::Api {
                path: path.to_string(),
                status: *status,
                body: body.clone(),
            });
        }
        self.ok.get(path).cloned().ok_or_else(|| Error::Api {
            path: path.to_string(),
            status: 404,
            body: r#"{"code":"ResourceNotFound"}"#.to_string(),
        })
    }
}

/// The three raw API responses a snapshot is built from. Saved as `profile.json`, `manifests.json`
/// and `variables.json` by `--raw-dir` and read back by `--from-raw`.
#[derive(Debug, Clone)]
pub struct RawProfile {
    /// `GET /v1/clusterprofiles/{uid}`
    pub profile: Value,
    /// `GET /v1/clusterprofiles/{uid}/packs/manifests`
    pub manifests: Value,
    /// `GET /v1/clusterprofiles/{uid}/variables`, or `{"variables": []}` when unavailable
    pub variables: Value,
}

impl RawProfile {
    /// Read the three files from `dir`.
    pub fn load_dir(dir: &Path) -> Result<RawProfile> {
        let read = |name: &str| -> Result<Value> {
            let path = dir.join(name);
            let text = fs::read_to_string(&path).map_err(|source| Error::Io {
                path: path.clone(),
                source,
            })?;
            if text.trim().is_empty() {
                return Err(Error::EmptyFile(path));
            }
            serde_json::from_str(&text).map_err(|source| Error::FileNotJson { path, source })
        };
        Ok(RawProfile {
            profile: read("profile.json")?,
            manifests: read("manifests.json")?,
            variables: read("variables.json")?,
        })
    }

    /// Write the three files under `dir`, creating it.
    pub fn save_dir(&self, dir: &Path) -> Result<()> {
        fs::create_dir_all(dir).map_err(|source| Error::Io {
            path: dir.to_path_buf(),
            source,
        })?;
        for (name, v) in [
            ("profile.json", &self.profile),
            ("manifests.json", &self.manifests),
            ("variables.json", &self.variables),
        ] {
            let path = dir.join(name);
            fs::write(&path, serde_json::to_string_pretty(v)?)
                .map_err(|source| Error::Io { path, source })?;
        }
        Ok(())
    }
}

/// Profile-level operations over any [`PaletteApi`].
pub struct Palette<A: PaletteApi> {
    api: A,
}

impl<A: PaletteApi> Palette<A> {
    /// Wrap an API.
    pub fn new(api: A) -> Self {
        Palette { api }
    }

    /// The underlying API.
    pub fn api(&self) -> &A {
        &self.api
    }

    /// Find the uid of `name` at `version` in the current scope.
    pub fn resolve(&self, name: &str, version: &str) -> Result<String> {
        let listing = self
            .api
            .get_json("/v1/dashboard/clusterprofiles/metadata")?;
        listing
            .get("items")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .find(|it| {
                it.pointer("/metadata/name").and_then(Value::as_str) == Some(name)
                    && it.pointer("/spec/version").and_then(Value::as_str) == Some(version)
            })
            .and_then(|it| it.pointer("/metadata/uid").and_then(Value::as_str))
            .map(str::to_string)
            .ok_or_else(|| Error::ProfileNotFound {
                name: name.to_string(),
                version: version.to_string(),
            })
    }

    /// Fetch the three documents for one profile. A missing variables endpoint (404 or 405, as on
    /// older Palette versions) is treated as "no variables"; other errors propagate.
    pub fn fetch_profile(&self, uid: &str, progress: &Progress) -> Result<RawProfile> {
        progress.note(&format!("fetching profile {uid}"));
        let profile = self.api.get_json(&format!("/v1/clusterprofiles/{uid}"))?;
        let manifests = self
            .api
            .get_json(&format!("/v1/clusterprofiles/{uid}/packs/manifests"))?;
        let variables = match self
            .api
            .get_json(&format!("/v1/clusterprofiles/{uid}/variables"))
        {
            Ok(v) => v,
            Err(Error::Api { status, .. }) if status == 404 || status == 405 => {
                progress.note(&format!(
                    "note: variables endpoint unavailable for {uid}; treating as none"
                ));
                serde_json::json!({ "variables": [] })
            }
            Err(e) => return Err(e),
        };
        Ok(RawProfile {
            profile,
            manifests,
            variables,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn listing() -> Value {
        json!({"items": [
            {"metadata": {"name": "aws-pxk", "uid": "u1"}, "spec": {"version": "1.0.0"}},
            {"metadata": {"name": "aws-pxk", "uid": "u2"}, "spec": {"version": "1.1.0"}},
        ]})
    }

    #[test]
    fn resolve_finds_the_exact_name_and_version() {
        let p = Palette::new(
            MemoryApi::new().with("/v1/dashboard/clusterprofiles/metadata", listing()),
        );
        assert_eq!(p.resolve("aws-pxk", "1.1.0").unwrap(), "u2");
        let err = p.resolve("aws-pxk", "9.9.9").unwrap_err();
        assert!(matches!(err, Error::ProfileNotFound { .. }));
        assert!(err.to_string().contains("'aws-pxk' with version '9.9.9'"));
    }

    #[test]
    fn fetch_profile_tolerates_a_missing_variables_endpoint_only() {
        let base = MemoryApi::new()
            .with("/v1/clusterprofiles/u1", json!({"metadata": {"uid": "u1"}}))
            .with(
                "/v1/clusterprofiles/u1/packs/manifests",
                json!({"spec": {"packs": []}}),
            );
        let raw = Palette::new(base.clone())
            .fetch_profile("u1", &Progress::silent())
            .unwrap();
        assert_eq!(raw.variables, json!({"variables": []}));

        let err = Palette::new(base.failing("/v1/clusterprofiles/u1/variables", 500, "boom"))
            .fetch_profile("u1", &Progress::silent())
            .unwrap_err();
        assert!(matches!(err, Error::Api { status: 500, .. }), "{err}");
    }

    #[test]
    fn api_errors_carry_path_status_and_body() {
        let api = MemoryApi::new().failing("/v1/x", 401, r#"{"code":"MissingAuthHeader"}"#);
        let err = api.get_json("/v1/x").unwrap_err();
        assert_eq!(
            err.to_string(),
            r#"GET /v1/x failed with HTTP 401: {"code":"MissingAuthHeader"}"#
        );
        assert_eq!(err.exit_code(), 2);
    }

    #[test]
    fn raw_profile_round_trips_through_a_directory() {
        let dir = tempfile::tempdir().unwrap();
        let raw = RawProfile {
            profile: json!({"a": 1}),
            manifests: json!({"b": 2}),
            variables: json!({"variables": []}),
        };
        raw.save_dir(dir.path()).unwrap();
        let back = RawProfile::load_dir(dir.path()).unwrap();
        assert_eq!(back.profile, raw.profile);
        fs::write(dir.path().join("variables.json"), "").unwrap();
        assert!(matches!(
            RawProfile::load_dir(dir.path()).unwrap_err(),
            Error::EmptyFile(_)
        ));
    }
}
