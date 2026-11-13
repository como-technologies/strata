//! The tenant that the stand-in for Palette serves.
//!
//! Each chapter of the book builds the tenant that one page needs, from the real pack data in
//! `examples/`. A chapter changes the tenant between two commands, in the way that Palette
//! changes it. Examples are an edit in place, a version bump, and a cluster that takes an update.
//!
//! A document here answers one request. `Tenant::doc` gives the file that answers a path, and
//! every helper writes one or more of those files.
//!
//! This file holds the tenant, and the read and the write of one document. The other files each
//! hold the helpers of one kind of object:
//!
//! - `routes.rs`: the routes of Palette, as the stand-in names their files.
//! - `edit.rs`: the edits that a person makes to a published profile.
//! - `profiles.rs`: the profile versions, the edits in place, and the drafts.
//! - `clusters.rs`: the clusters, their cloud configs, and the changes of a cluster.
//! - `holders.rs`: the edge hosts, the cluster templates, the cluster groups, and the app profiles.
//! - `listings.rs`: the listings and the searches.
//! - `macros.rs`: the project macros, and the values that Palette resolves for them.
//! - `registries.rs`: the pack registries, and the versions of a pack.
//! - `notifications.rs`: the notifications of a cluster, and the updates that wait for it.
//! - `answers.rs`: the answers of the routes that check a change.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use serde_json::{Value, json};

use crate::files::files_under;
use crate::palette::route_of;

mod answers;
mod clusters;
mod edit;
mod holders;
mod listings;
mod macros;
mod notifications;
mod profiles;
mod registries;
mod routes;

pub use answers::{
    PackRefusal, UpgradeRefusal, access_denied_message, profile_exists_message,
    project_denied_message,
};
pub use edit::Edit;
pub use routes::{
    AUDITS, CLUSTER_SEARCH_PATH, PROFILE_SEARCH_PATH, PROJECT_LISTING, cluster_validate_route,
    downloaded_profile, empty_audit_log, exported_profile, rbacs_route, validated_cloud,
};
// the routes and the sync answers that the chapter `fields` writes: each document there has the
// route and the answer of its kind
pub(crate) use registries::{BITNAMI, RegistrySync, registry_sync};
#[cfg(test)]
pub(crate) use routes::download_route;
pub(crate) use routes::{
    CLUSTER_LISTING, REGISTRY_LISTING, TAG_FILTER_LISTING, app_profile_route,
    app_tier_manifest_route, app_tiers_route, group_route, manifests_route, pack_listing,
    profile_route, registry_summary_route, variables_route,
};

/// The directory of the documents that Palette serves at tenant scope, with no `ProjectUid`.
pub const TENANT: &str = "tenant";

/// The tenant directory, and the repository that holds the example pack data.
pub struct Tenant {
    root: PathBuf,
    repo: PathBuf,
}

impl Tenant {
    /// A tenant under `root`, reading its example profiles from `repo`.
    pub fn new(root: &Path, repo: &Path) -> Self {
        Tenant {
            root: root.to_path_buf(),
            repo: repo.to_path_buf(),
        }
    }

    /// The directory that holds the documents of the tenant.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// The file that answers `GET path` in `project`, or at tenant scope for `"tenant"`. The
    /// directory above it is created, so a caller can write the file at once.
    pub fn doc(&self, project: &str, path: &str) -> PathBuf {
        let file = self.root.join(project).join(format!("{path}.json"));
        if let Some(parent) = file.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        file
    }

    /// Write the answer of one request, for a route that no other helper writes. `path` is the
    /// request path without the leading slash, such as `v1/spectroclusters/aws/validate`.
    pub fn put(&self, project: &str, path: &str, value: &Value) {
        self.write(project, path, value);
    }

    fn write(&self, project: &str, path: &str, value: &Value) {
        let file = self.doc(project, path);
        let text = serde_json::to_string_pretty(value).expect("a document is JSON");
        std::fs::write(&file, text + "\n").unwrap_or_else(|e| panic!("{}: {e}", file.display()));
    }

    fn read(&self, project: &str, path: &str) -> Value {
        let file = self.doc(project, path);
        let text =
            std::fs::read_to_string(&file).unwrap_or_else(|e| panic!("{}: {e}", file.display()));
        serde_json::from_str(&text).unwrap_or_else(|e| panic!("{}: {e}", file.display()))
    }

    /// Empty the tenant, and keep the directory: the server holds its path.
    pub fn clear(&self) {
        if let Ok(entries) = std::fs::read_dir(&self.root) {
            for entry in entries.flatten() {
                let path = entry.path();
                let _ = if path.is_dir() {
                    std::fs::remove_dir_all(&path)
                } else {
                    std::fs::remove_file(&path)
                };
            }
        }
    }

    /// The route of each document that the tenant holds now, in each project and at tenant scope.
    /// A route has no project, no query, and no status of a refusal: see [`route_of`].
    pub fn routes(&self) -> BTreeSet<String> {
        files_under(&self.root, "json")
            .iter()
            .filter_map(|file| file.strip_prefix(&self.root).ok()?.to_str())
            .filter_map(route_of)
            .collect()
    }

    /// Copy each document of the tenant into the directory `dest`, with the layout of the tenant.
    /// The layout has a directory for each project and one for the tenant scope.
    pub fn copy_to(&self, dest: &Path) {
        listings::copy_dir(&self.root, dest);
    }

    /// The projects that the key can see, as `name=uid` pairs.
    pub fn projects(&self, pairs: &[(&str, &str)]) {
        let items: Vec<Value> = pairs
            .iter()
            .map(|(name, uid)| json!({"metadata": {"name": name, "uid": uid}}))
            .collect();
        for (pair, item) in pairs.iter().zip(&items) {
            self.write(pair.1, PROJECT_LISTING, &json!({"items": [item]}));
        }
        self.write(TENANT, PROJECT_LISTING, &json!({"items": items}));
        self.listings();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_routes_of_a_tenant_are_the_routes_of_its_documents_in_each_project() {
        let dir = tempfile::tempdir().unwrap();
        let tenant = Tenant::new(dir.path(), Path::new("/nowhere"));
        tenant.put("p1", "v1/clusterprofiles/u1", &json!({}));
        tenant.put("p2", "v1/clusterprofiles/u1", &json!({}));
        tenant.put(TENANT, "v1/clusterprofiles/u2", &json!({}));
        tenant.put("p1", "v1/packs@filters=spec.name=a&limit=100", &json!({}));
        tenant.forbid("p1", "v1/spectroclusters/c1/config/rbacs");
        let routes = tenant.routes();
        let want = [
            "v1/clusterprofiles/u1",
            "v1/clusterprofiles/u2",
            "v1/packs",
            "v1/spectroclusters/c1/config/rbacs",
        ];
        assert_eq!(routes, want.iter().map(|r| r.to_string()).collect());
    }
}
