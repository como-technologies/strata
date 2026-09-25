//! The operations over any [`PaletteApi`]: find a profile or a cluster, and read the documents a
//! snapshot is built from.

use serde_json::Value;

use super::{PaletteApi, RawCluster, RawProfile};
use crate::error::{Error, Result};
use crate::progress::Progress;
use crate::reference::LATEST;
use crate::version;

const PROFILE_LISTING: &str = "/v1/dashboard/clusterprofiles/metadata";
const CLUSTER_LISTING: &str = "/v1/dashboard/spectroclusters/meta";
const PROJECT_LISTING: &str = "/v1/dashboard/projects/metadata";

/// `(name, uid)` of each entry of a listing: an array, or an object with `items`.
fn named_entries(listing: &Value) -> Vec<(String, String)> {
    let field = |v: &Value, k: &str| {
        v.pointer(&format!("/metadata/{k}"))
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string()
    };
    listing
        .get("items")
        .unwrap_or(listing)
        .as_array()
        .into_iter()
        .flatten()
        .map(|e| (field(e, "name"), field(e, "uid")))
        .collect()
}

/// `text` as a value in a query string: each byte that is not a letter, a digit, or one of
/// `-._~:` becomes `%XX`, so a time with `+00:00` keeps its `+`.
fn query_value(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for b in text.bytes() {
        if b.is_ascii_alphanumeric() || b"-._~:".contains(&b) {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// The names of `entries`, sorted.
fn sorted_names(entries: &[(String, String)]) -> Vec<String> {
    let mut names: Vec<String> = entries.iter().map(|(n, _)| n.clone()).collect();
    names.sort();
    names
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
                tenant_scope: self.project_uid().is_none(),
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
                tenant_scope: self.project_uid().is_none(),
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

    /// The uid of the project named `name`, from [`Palette::project_listing`]. Call it at tenant
    /// scope: in a project, Palette lists only that project.
    pub fn project_uid_named(&self, name: &str) -> Result<String> {
        let listing = self.project_listing()?;
        let projects = named_entries(&listing);
        match projects.iter().find(|(n, _)| n == name) {
            Some((_, uid)) => Ok(uid.clone()),
            None => Err(Error::ProjectNotFound {
                name: name.to_string(),
                available: sorted_names(&projects),
            }),
        }
    }

    /// The uid of the one cluster named `name` in the scope, from [`Palette::cluster_listing`].
    pub fn cluster_uid_named(&self, name: &str) -> Result<String> {
        let listing = self.cluster_listing()?;
        let clusters = named_entries(&listing);
        let mut uids: Vec<String> = clusters
            .iter()
            .filter(|(n, _)| n == name)
            .map(|(_, uid)| uid.clone())
            .collect();
        uids.sort();
        match uids.len() {
            1 => Ok(uids.remove(0)),
            0 => Err(Error::ClusterNotFound {
                cluster: name.to_string(),
                available: sorted_names(&clusters),
            }),
            _ => Err(Error::ClusterNameAmbiguous {
                name: name.to_string(),
                uids,
            }),
        }
    }

    /// The project listing, `GET /v1/dashboard/projects/metadata`: `items[]` with each project's
    /// name and uid. At tenant scope it holds every project the key can see; in a project, only
    /// that project.
    pub fn project_listing(&self) -> Result<Value> {
        self.api.get_json(PROJECT_LISTING)
    }

    /// GET a variables endpoint; a missing endpoint (404 or 405, as on older Palette versions) is
    /// `none`, other errors propagate.
    /// The second value is true when the endpoint was unavailable and `none` stands in for it. A
    /// caller must record that: "no variables" and "the variables could not be read" give the same
    /// root, and only this flag tells them apart afterwards.
    fn variables_or(&self, path: &str, none: Value, progress: &Progress) -> Result<(Value, bool)> {
        match self.api.get_json(path) {
            Ok(v) => Ok((v, false)),
            Err(Error::Api { status, .. }) if status == 404 || status == 405 => {
                progress.note(&format!(
                    "note: {path} is unavailable; treating as no variables"
                ));
                Ok((none, true))
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
        let (variables, variables_unavailable) = self.variables_or(
            &format!("/v1/clusterprofiles/{uid}/variables"),
            serde_json::json!({ "variables": [] }),
            progress,
        )?;
        Ok(RawProfile {
            profile,
            manifests,
            variables,
            variables_unavailable,
            project_uid: self.project_uid().map(str::to_string),
            fetched_at: None,
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

    /// The entries of the audit log of Palette for one resource, `GET /v1/audits`, that it logged
    /// at `since` or after, every page. `kind` is `clusterprofile` or `spectrocluster`. Palette
    /// lists them newest first, and at most 100 on a page; `listmeta.continue` names the next
    /// page, and is empty on the last one. An empty `since` reads the full log of the resource.
    pub fn audits(&self, kind: &str, uid: &str, since: &str) -> Result<Vec<Value>> {
        let mut query = format!(
            "resourceKind={}&resourceUid={}",
            query_value(kind),
            query_value(uid)
        );
        if !since.is_empty() {
            query.push_str(&format!("&startTime={}", query_value(since)));
        }
        query.push_str("&limit=100");
        self.paged(&format!("/v1/audits?{query}"))
    }

    /// The cluster summary, `GET /v1/dashboard/spectroclusters/{uid}`: what the dashboard shows
    /// for one cluster. Its `status.health` is the health of the cluster, which the cluster
    /// document does not carry.
    pub fn fetch_cluster_summary(&self, uid: &str) -> Result<Value> {
        self.api
            .get_json(&format!("/v1/dashboard/spectroclusters/{uid}"))
    }

    /// Fetch the three documents for one cluster: the cluster with its applied profile templates,
    /// the pack manifests of each applied profile, and each applied profile's variables. A missing
    /// variables endpoint is treated as "no variables"; other errors propagate. `cluster` is a uid,
    /// or, when no cluster has that uid, the name of one cluster in the scope; the documents are
    /// always read by uid.
    pub fn fetch_cluster(&self, cluster: &str, progress: &Progress) -> Result<RawCluster> {
        progress.note(&format!("fetching cluster {cluster}"));
        let (uid, cluster) = match self.cluster_document(cluster) {
            Ok(doc) => (cluster.to_string(), doc),
            Err(Error::Api { status: 404, .. }) => {
                let uid = self.cluster_uid_named(cluster)?;
                progress.note(&format!("cluster '{cluster}' is {uid}"));
                let doc = self.cluster_document(&uid)?;
                (uid, doc)
            }
            Err(e) => return Err(e),
        };
        let manifests = self.api.get_json(&format!(
            "/v1/spectroclusters/{uid}/profiles/packs/manifests"
        ))?;
        let (variables, variables_unavailable) = self.variables_or(
            &format!("/v1/spectroclusters/{uid}/variables"),
            serde_json::json!([]),
            progress,
        )?;
        // the updates that wait for the cluster are a statement of Palette beside the copy, not
        // part of it: a failed read is recorded, and the record is still taken
        let (notifications, notifications_error) = match self.cluster_notifications(&uid) {
            Ok(items) => (Some(Value::Array(items)), None),
            Err(Error::Api { status, body, .. }) => {
                progress.note(&format!(
                    "note: the notifications of cluster {uid} are unavailable (HTTP {status})"
                ));
                (None, Some(format!("HTTP {status}: {body}")))
            }
            Err(e) => return Err(e),
        };
        Ok(RawCluster {
            cluster,
            manifests,
            variables,
            variables_unavailable,
            project_uid: self.project_uid().map(str::to_string),
            fetched_at: None,
            notifications,
            notifications_error,
        })
    }

    /// The notifications of one cluster, `GET /v1/notifications/spectrocluster/{uid}`, every page,
    /// newest first. Palette gives `items: null` when there are none, and at most 50 on a page;
    /// `listmeta.continue` names the next page, and is empty on the last one.
    pub fn cluster_notifications(&self, uid: &str) -> Result<Vec<Value>> {
        self.paged(&format!(
            "/v1/notifications/spectrocluster/{}",
            query_value(uid)
        ))
    }

    /// Every item of a paged listing at `path`, which may carry a query already: each page's
    /// `items`, in order. `listmeta.continue` names the next page, and is empty on the last one;
    /// a page with no items ends the listing too, and `items: null` is such a page.
    fn paged(&self, path: &str) -> Result<Vec<Value>> {
        let join = if path.contains('?') { '&' } else { '?' };
        let mut items = Vec::new();
        let mut next = String::new();
        loop {
            let page = if next.is_empty() {
                self.api.get_json(path)?
            } else {
                self.api
                    .get_json(&format!("{path}{join}continue={}", query_value(&next)))?
            };
            let got = page
                .get("items")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            let done = got.is_empty();
            items.extend(got);
            next = page
                .pointer("/listmeta/continue")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
            if next.is_empty() || done {
                return Ok(items);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::MemoryApi;
    use serde_json::json;

    fn listing() -> Value {
        json!({"items": [
            {"metadata": {"name": "aws-stack", "uid": "u1"}, "spec": {"version": "1.0.0"}},
            {"metadata": {"name": "aws-stack", "uid": "u2"}, "spec": {"version": "1.1.0"}},
        ]})
    }

    #[test]
    fn resolve_finds_the_exact_name_and_version() {
        let p = Palette::new(
            MemoryApi::new().with("/v1/dashboard/clusterprofiles/metadata", listing()),
        );
        assert_eq!(p.resolve("aws-stack", "1.1.0").unwrap(), "u2");
        let err = p.resolve("aws-stack", "9.9.9").unwrap_err();
        assert!(matches!(err, Error::ProfileNotFound { .. }));
        assert!(err.to_string().contains("'aws-stack' with version '9.9.9'"));
        // at tenant scope the listing advice is a dead end: the scope holds no profiles at all
        assert!(
            err.to_string().contains("this is tenant scope"),
            "a miss at tenant scope asks for a project first: {err}"
        );

        // in a project, the listing is the right next command, and the name may be a pack's
        let in_project = Palette::new(
            MemoryApi::new()
                .with("/v1/dashboard/clusterprofiles/metadata", listing())
                .in_project("p1"),
        );
        let err = in_project.resolve("aws-stack", "9.9.9").unwrap_err();
        assert!(
            err.to_string().contains("`strata list --pack aws-stack`"),
            "a miss in a project points at the listing: {err}"
        );
    }

    #[test]
    fn latest_resolves_to_the_highest_version_by_natural_order() {
        let listing = json!({"items": [
            {"metadata": {"name": "aws-stack", "uid": "u1"}, "spec": {"version": "1.9.0"}},
            {"metadata": {"name": "aws-stack", "uid": "u2"}, "spec": {"version": "1.10.0"}},
            {"metadata": {"name": "aws-stack", "uid": "u0"}, "spec": {"version": "1.2.0"}},
            {"metadata": {"name": "other", "uid": "u3"}, "spec": {"version": "9.0.0"}},
        ]});
        let p =
            Palette::new(MemoryApi::new().with("/v1/dashboard/clusterprofiles/metadata", listing));
        assert_eq!(p.resolve_latest("aws-stack").unwrap(), "u2");
        assert_eq!(p.resolve("aws-stack", "latest").unwrap(), "u2");
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
}
