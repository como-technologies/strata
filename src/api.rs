//! Access to the Palette API behind one trait, so that every operation can run against canned
//! responses in tests and against HTTP in production.

use std::cell::RefCell;
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::time::Duration;

use serde_json::Value;

use crate::config::Config;
use crate::error::{Error, Result};
use crate::progress::Progress;
use crate::reference::LATEST;
use crate::version;

const PROFILE_LISTING: &str = "/v1/dashboard/clusterprofiles/metadata";
const CLUSTER_LISTING: &str = "/v1/dashboard/spectroclusters/meta";
const PROJECT_LISTING: &str = "/v1/dashboard/projects/metadata";

/// One GET of a JSON document from Palette. Implementations add authentication and scoping.
pub trait PaletteApi {
    /// GET `path` (for example `/v1/clusterprofiles/{uid}`) and parse the body as JSON.
    fn get_json(&self, path: &str) -> Result<Value>;

    /// The project the requests are scoped to, when they are; snapshots record it.
    fn project_uid(&self) -> Option<&str> {
        None
    }
}

impl<A: PaletteApi + ?Sized> PaletteApi for &A {
    fn get_json(&self, path: &str) -> Result<Value> {
        (**self).get_json(path)
    }

    fn project_uid(&self) -> Option<&str> {
        (**self).project_uid()
    }
}

/// The HTTP implementation. One attempt per call; wrap it in [`Retry`] for more.
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
        let retry_after = retry_after_seconds(
            resp.headers()
                .get("retry-after")
                .and_then(|v| v.to_str().ok()),
        );
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
                retry_after,
            });
        }
        serde_json::from_str(&body).map_err(|source| Error::NotJson {
            path: path.to_string(),
            source,
        })
    }

    fn project_uid(&self) -> Option<&str> {
        self.config.project_uid.as_deref()
    }
}

/// `Retry-After` as whole seconds. The HTTP-date form is not supported and reads as absent.
fn retry_after_seconds(header: Option<&str>) -> Option<u64> {
    header.and_then(|h| h.trim().parse().ok())
}

/// How many times to retry a failed GET and how long to wait between attempts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RetryPolicy {
    /// Attempts after the first. Zero means one attempt.
    pub retries: u32,
    /// Wait before the first retry; each later wait doubles.
    pub base_delay: Duration,
    /// The longest computed wait. A `Retry-After` header is honored as sent, up to
    /// `max_retry_after`.
    pub max_delay: Duration,
    /// The longest `Retry-After` honored. A longer one ends the request at once with
    /// [`Error::RetryAfterTooLong`]: the server said when to come back, and that is later than a
    /// job should sit waiting.
    pub max_retry_after: Duration,
}

impl RetryPolicy {
    /// `retries` retries, starting at one second and capped at thirty, honoring a `Retry-After` of
    /// up to five minutes.
    pub fn new(retries: u32) -> Self {
        RetryPolicy {
            retries,
            base_delay: Duration::from_secs(1),
            max_delay: Duration::from_secs(30),
            max_retry_after: Duration::from_secs(300),
        }
    }

    /// The retries and the `Retry-After` ceiling a [`Config`] asks for.
    pub fn from_config(config: &Config) -> Self {
        RetryPolicy {
            max_retry_after: config.max_retry_after,
            ..RetryPolicy::new(config.retries)
        }
    }

    /// The wait before retry number `attempt` (zero-based), or the server's `Retry-After`.
    pub fn delay(&self, attempt: u32, retry_after: Option<u64>) -> Duration {
        match retry_after {
            Some(secs) => Duration::from_secs(secs),
            None => self
                .base_delay
                .saturating_mul(2u32.saturating_pow(attempt))
                .min(self.max_delay),
        }
    }
}

/// Retries transient failures of any [`PaletteApi`]: HTTP 5xx, 429, and transport errors.
/// Other errors return at once. Every retry is announced on [`Progress`].
pub struct Retry<A> {
    inner: A,
    policy: RetryPolicy,
    progress: Progress,
    sleep: Box<dyn Fn(Duration)>,
}

impl<A: PaletteApi> Retry<A> {
    /// Wrap `inner`, sleeping on the current thread between attempts.
    pub fn new(inner: A, policy: RetryPolicy, progress: Progress) -> Self {
        Self::with_sleep(inner, policy, progress, std::thread::sleep)
    }

    /// Wrap `inner` with a caller-supplied wait, so tests can record delays instead of sleeping.
    pub fn with_sleep(
        inner: A,
        policy: RetryPolicy,
        progress: Progress,
        sleep: impl Fn(Duration) + 'static,
    ) -> Self {
        Retry {
            inner,
            policy,
            progress,
            sleep: Box::new(sleep),
        }
    }

    /// The wrapped API.
    pub fn inner(&self) -> &A {
        &self.inner
    }
}

fn is_transient(e: &Error) -> bool {
    match e {
        Error::Transport { .. } => true,
        Error::Api { status, .. } => *status == 429 || (500..600).contains(status),
        _ => false,
    }
}

impl<A: PaletteApi> PaletteApi for Retry<A> {
    fn get_json(&self, path: &str) -> Result<Value> {
        let mut attempt = 0;
        loop {
            match self.inner.get_json(path) {
                Ok(v) => return Ok(v),
                Err(e) if attempt < self.policy.retries && is_transient(&e) => {
                    let retry_after = match &e {
                        Error::Api { retry_after, .. } => *retry_after,
                        _ => None,
                    };
                    let limit = self.policy.max_retry_after.as_secs();
                    if let (Some(secs), Error::Api { status, .. }) = (retry_after, &e) {
                        if secs > limit {
                            return Err(Error::RetryAfterTooLong {
                                path: path.to_string(),
                                status: *status,
                                retry_after: secs,
                                limit,
                            });
                        }
                    }
                    let delay = self.policy.delay(attempt, retry_after);
                    attempt += 1;
                    self.progress.note(&format!(
                        "{e}; retry {attempt} of {} in {delay:?}",
                        self.policy.retries
                    ));
                    (self.sleep)(delay);
                }
                Err(e) => return Err(e),
            }
        }
    }

    fn project_uid(&self) -> Option<&str> {
        self.inner.project_uid()
    }
}

/// Canned responses keyed by path. Paths not registered answer 404.
#[derive(Debug, Default, Clone)]
pub struct MemoryApi {
    ok: HashMap<String, Value>,
    fail: HashMap<String, (u16, String)>,
    transient: RefCell<HashMap<String, Transient>>,
    calls: RefCell<HashMap<String, u32>>,
    project_uid: Option<String>,
}

/// A failure that goes away after `remaining` calls.
#[derive(Debug, Clone)]
struct Transient {
    remaining: u32,
    status: u16,
    body: String,
    retry_after: Option<u64>,
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

    /// Answer `path` with an HTTP error `n` times, then with whatever else is registered.
    pub fn failing_n_times(
        self,
        path: impl Into<String>,
        n: u32,
        status: u16,
        body: impl Into<String>,
    ) -> Self {
        self.transient.borrow_mut().insert(
            path.into(),
            Transient {
                remaining: n,
                status,
                body: body.into(),
                retry_after: None,
            },
        );
        self
    }

    /// Add a `Retry-After` of `seconds` to the transient failures registered for `path`.
    pub fn with_retry_after(self, path: impl Into<String>, seconds: u64) -> Self {
        if let Some(t) = self.transient.borrow_mut().get_mut(&path.into()) {
            t.retry_after = Some(seconds);
        }
        self
    }

    /// How many times `path` was requested.
    pub fn calls(&self, path: &str) -> u32 {
        self.calls.borrow().get(path).copied().unwrap_or(0)
    }

    /// Report `uid` as the project scope, as the HTTP client does for `PALETTE_PROJECT_UID`.
    pub fn in_project(mut self, uid: impl Into<String>) -> Self {
        self.project_uid = Some(uid.into());
        self
    }
}

impl PaletteApi for MemoryApi {
    fn get_json(&self, path: &str) -> Result<Value> {
        *self.calls.borrow_mut().entry(path.to_string()).or_insert(0) += 1;
        if let Some(t) = self.transient.borrow_mut().get_mut(path) {
            if t.remaining > 0 {
                t.remaining -= 1;
                return Err(Error::Api {
                    path: path.to_string(),
                    status: t.status,
                    body: t.body.clone(),
                    retry_after: t.retry_after,
                });
            }
        }
        if let Some((status, body)) = self.fail.get(path) {
            return Err(Error::Api {
                path: path.to_string(),
                status: *status,
                body: body.clone(),
                retry_after: None,
            });
        }
        self.ok.get(path).cloned().ok_or_else(|| Error::Api {
            path: path.to_string(),
            status: 404,
            body: r#"{"code":"ResourceNotFound"}"#.to_string(),
            retry_after: None,
        })
    }

    fn project_uid(&self) -> Option<&str> {
        self.project_uid.as_deref()
    }
}

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
    /// The project the responses came from; `None` at tenant scope, or for a directory saved
    /// without `scope.json`
    pub project_uid: Option<String>,
}

/// The file that records the project a raw directory's responses came from.
const SCOPE_FILE: &str = "scope.json";

/// The project saved in `dir`, or `None` when it has no scope file or names no project.
fn read_scope(dir: &Path) -> Result<Option<String>> {
    if !dir.join(SCOPE_FILE).is_file() {
        return Ok(None);
    }
    let scope = read_json_file(dir, SCOPE_FILE)?;
    Ok(scope
        .get("projectUid")
        .and_then(Value::as_str)
        .filter(|p| !p.is_empty())
        .map(str::to_string))
}

/// The scope file's document: the project uid, empty at tenant scope.
fn scope_document(project_uid: &Option<String>) -> Value {
    serde_json::json!({ "projectUid": project_uid.as_deref().unwrap_or("") })
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
            project_uid: read_scope(dir)?,
        })
    }

    /// Write the three files and `scope.json` under `dir`, creating it.
    pub fn save_dir(&self, dir: &Path) -> Result<()> {
        write_json_files(
            dir,
            &[
                ("profile.json", &self.profile),
                ("manifests.json", &self.manifests),
                ("variables.json", &self.variables),
                (SCOPE_FILE, &scope_document(&self.project_uid)),
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
    /// The project the responses came from; `None` at tenant scope, or for a directory saved
    /// without `scope.json`
    pub project_uid: Option<String>,
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
            project_uid: read_scope(dir)?,
        })
    }

    /// Write the three files and `scope.json` under `dir`, creating it.
    pub fn save_dir(&self, dir: &Path) -> Result<()> {
        write_json_files(
            dir,
            &[
                (Self::MARKER, &self.cluster),
                ("manifests.json", &self.manifests),
                ("variables.json", &self.variables),
                (SCOPE_FILE, &scope_document(&self.project_uid)),
            ],
        )
    }

    /// The applied profile templates, in the order Palette applies them.
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
            project_uid: self.project_uid.clone(),
        }
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

    /// The project the API is scoped to, if any; snapshots taken through it record this.
    pub fn project_uid(&self) -> Option<&str> {
        self.api.project_uid()
    }

    /// Find the uid of `name` at `version` in the current scope. The version [`LATEST`] picks
    /// the highest version, see [`Palette::resolve_latest`].
    pub fn resolve(&self, name: &str, version: &str) -> Result<String> {
        if version == LATEST {
            return self.resolve_latest(name);
        }
        let listing = self.profile_listing()?;
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

    /// Find the uid of the highest version of `name` in the current scope, by
    /// [`version::compare`]. A name with no versions is [`Error::ProfileNotFound`].
    pub fn resolve_latest(&self, name: &str) -> Result<String> {
        let listing = self.profile_listing()?;
        listing
            .get("items")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter(|it| it.pointer("/metadata/name").and_then(Value::as_str) == Some(name))
            .filter_map(|it| {
                let version = it.pointer("/spec/version")?.as_str()?;
                let uid = it.pointer("/metadata/uid")?.as_str()?;
                Some((version, uid))
            })
            .max_by(|(x, _), (y, _)| version::compare(x, y))
            .map(|(_, uid)| uid.to_string())
            .ok_or_else(|| Error::ProfileNotFound {
                name: name.to_string(),
                version: LATEST.to_string(),
            })
    }

    /// The profile listing, `GET /v1/dashboard/clusterprofiles/metadata`: `items[]` with each
    /// version's name, uid, version, and cloud type. In a project it holds that project's
    /// profiles; at tenant scope, the tenant's own.
    pub fn profile_listing(&self) -> Result<Value> {
        self.api.get_json(PROFILE_LISTING)
    }

    /// The cluster listing, `GET /v1/dashboard/spectroclusters/meta`: an array with each cluster's
    /// name, uid, cloud type, and state. At tenant scope it holds the clusters of every project.
    pub fn cluster_listing(&self) -> Result<Value> {
        self.api.get_json(CLUSTER_LISTING)
    }

    /// The project listing, `GET /v1/dashboard/projects/metadata`: `items[]` with each project's
    /// name and uid. At tenant scope it holds every project the key can see; in a project, only
    /// that project.
    pub fn project_listing(&self) -> Result<Value> {
        self.api.get_json(PROJECT_LISTING)
    }

    /// GET a variables endpoint; a missing endpoint (404 or 405, as on older Palette versions) is
    /// `none`, other errors propagate.
    fn variables_or(&self, path: &str, none: Value, progress: &Progress) -> Result<Value> {
        match self.api.get_json(path) {
            Ok(v) => Ok(v),
            Err(Error::Api { status, .. }) if status == 404 || status == 405 => {
                progress.note(&format!(
                    "note: {path} is unavailable; treating as no variables"
                ));
                Ok(none)
            }
            Err(e) => Err(e),
        }
    }

    /// Fetch the three documents for one profile. A missing variables endpoint (404 or 405, as on
    /// older Palette versions) is treated as "no variables"; other errors propagate.
    pub fn fetch_profile(&self, uid: &str, progress: &Progress) -> Result<RawProfile> {
        progress.note(&format!("fetching profile {uid}"));
        let profile = self.api.get_json(&format!("/v1/clusterprofiles/{uid}"))?;
        let manifests = self
            .api
            .get_json(&format!("/v1/clusterprofiles/{uid}/packs/manifests"))?;
        let variables = self.variables_or(
            &format!("/v1/clusterprofiles/{uid}/variables"),
            serde_json::json!({ "variables": [] }),
            progress,
        )?;
        Ok(RawProfile {
            profile,
            manifests,
            variables,
            project_uid: self.project_uid().map(str::to_string),
        })
    }

    /// The profile document alone, `GET /v1/clusterprofiles/{uid}`: metadata, versions, and the
    /// clusters that use it in `status`.
    pub fn profile_document(&self, uid: &str) -> Result<Value> {
        self.api.get_json(&format!("/v1/clusterprofiles/{uid}"))
    }

    /// The cluster document alone, `GET /v1/spectroclusters/{uid}`: its name, its project in the
    /// `projectUid` annotation, and its state in `status.state`.
    pub fn cluster_document(&self, uid: &str) -> Result<Value> {
        self.api.get_json(&format!("/v1/spectroclusters/{uid}"))
    }

    /// Fetch the three documents for one cluster: the cluster with its applied profile templates,
    /// the pack manifests of each applied profile, and each applied profile's variables. A missing
    /// variables endpoint is treated as "no variables"; other errors propagate.
    pub fn fetch_cluster(&self, uid: &str, progress: &Progress) -> Result<RawCluster> {
        progress.note(&format!("fetching cluster {uid}"));
        let cluster = self.api.get_json(&format!("/v1/spectroclusters/{uid}"))?;
        let manifests = self.api.get_json(&format!(
            "/v1/spectroclusters/{uid}/profiles/packs/manifests"
        ))?;
        let variables = self.variables_or(
            &format!("/v1/spectroclusters/{uid}/variables"),
            serde_json::json!([]),
            progress,
        )?;
        Ok(RawCluster {
            cluster,
            manifests,
            variables,
            project_uid: self.project_uid().map(str::to_string),
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
        assert!(
            err.to_string().contains("`strata list --pack aws-pxk`"),
            "a miss points at the listing, since the name may be a pack's: {err}"
        );
    }

    #[test]
    fn latest_resolves_to_the_highest_version_by_natural_order() {
        let listing = json!({"items": [
            {"metadata": {"name": "aws-pxk", "uid": "u1"}, "spec": {"version": "1.9.0"}},
            {"metadata": {"name": "aws-pxk", "uid": "u2"}, "spec": {"version": "1.10.0"}},
            {"metadata": {"name": "aws-pxk", "uid": "u0"}, "spec": {"version": "1.2.0"}},
            {"metadata": {"name": "other", "uid": "u3"}, "spec": {"version": "9.0.0"}},
        ]});
        let p =
            Palette::new(MemoryApi::new().with("/v1/dashboard/clusterprofiles/metadata", listing));
        assert_eq!(p.resolve_latest("aws-pxk").unwrap(), "u2");
        assert_eq!(p.resolve("aws-pxk", "latest").unwrap(), "u2");
        let err = p.resolve_latest("nope").unwrap_err();
        assert!(matches!(err, Error::ProfileNotFound { .. }), "{err}");
        assert!(err.to_string().contains("'nope'"));
        assert!(err.to_string().contains("'latest'"));
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
    fn retry_after_header_parses_whole_seconds_only() {
        assert_eq!(retry_after_seconds(Some("2")), Some(2));
        assert_eq!(retry_after_seconds(Some(" 10 ")), Some(10));
        assert_eq!(
            retry_after_seconds(Some("Wed, 21 Oct 2015 07:28:00 GMT")),
            None
        );
        assert_eq!(retry_after_seconds(None), None);
    }

    #[test]
    fn retry_policy_backs_off_exponentially_up_to_the_maximum() {
        let p = RetryPolicy::new(5);
        assert_eq!(p.delay(0, None), Duration::from_secs(1));
        assert_eq!(p.delay(1, None), Duration::from_secs(2));
        assert_eq!(p.delay(2, None), Duration::from_secs(4));
        assert_eq!(p.delay(9, None), p.max_delay);
        assert_eq!(p.delay(0, Some(7)), Duration::from_secs(7));
    }

    #[test]
    fn raw_profile_round_trips_through_a_directory() {
        let dir = tempfile::tempdir().unwrap();
        let raw = RawProfile {
            profile: json!({"a": 1}),
            manifests: json!({"b": 2}),
            variables: json!({"variables": []}),
            project_uid: Some("p1".into()),
        };
        raw.save_dir(dir.path()).unwrap();
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
