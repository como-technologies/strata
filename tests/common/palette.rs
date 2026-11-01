//! The routes of Palette, and the documents that a test or a benchmark serves from them.
//!
//! The crate names each route in a const of its own that is not public. So this file names the
//! route again, in one place. `tests/common/mod.rs` re-exports it for the tests, and the library
//! of `benches/` includes it for the benchmarks, so that no second list of the routes exists.

use serde_json::{Value, json};

/// The profile listing, `GET /v1/dashboard/clusterprofiles/metadata`.
pub const PROFILE_LISTING: &str = "/v1/dashboard/clusterprofiles/metadata";

/// The profile search, `POST /v1/dashboard/clusterprofiles?limit=50`.
pub const PROFILE_SEARCH: &str = "/v1/dashboard/clusterprofiles?limit=50";

/// The cluster listing, `GET /v1/dashboard/spectroclusters/meta`.
pub const CLUSTER_LISTING: &str = "/v1/dashboard/spectroclusters/meta";

/// The project listing, `GET /v1/dashboard/projects/metadata`.
pub const PROJECT_LISTING: &str = "/v1/dashboard/projects/metadata";

/// The cluster search, `POST /v1/dashboard/spectroclusters/search?limit=50`.
pub const CLUSTER_SEARCH: &str = "/v1/dashboard/spectroclusters/search?limit=50";

/// The schema of the cluster search: the properties that a search can filter on.
pub const CLUSTER_SEARCH_SCHEMA: &str = "/v1/dashboard/spectroclusters/search/schema";

/// The edge host search, `POST /v1/dashboard/edgehosts/search?limit=50`.
pub const EDGE_HOST_SEARCH: &str = "/v1/dashboard/edgehosts/search?limit=50";

/// The key of a POST to `route` in the in-memory Palette, for one answer to each body.
pub fn post(route: &str) -> String {
    format!("POST {route}")
}

/// The document of the profile version `uid`, `GET /v1/clusterprofiles/{uid}`.
pub fn profile_route(uid: &str) -> String {
    format!("/v1/clusterprofiles/{uid}")
}

/// The pack manifests of the profile version `uid`.
pub fn manifests_route(uid: &str) -> String {
    format!("/v1/clusterprofiles/{uid}/packs/manifests")
}

/// The variables of the profile version `uid`.
pub fn variables_route(uid: &str) -> String {
    format!("/v1/clusterprofiles/{uid}/variables")
}

/// The value of each macro of the profile version `uid`, as Palette resolves it.
pub fn resolved_values_route(uid: &str) -> String {
    format!("/v1/clusterprofiles/{uid}/packs/resolvedValues")
}

/// The value of each macro of the cluster `uid`, as Palette resolves it.
pub fn cluster_resolved_values_route(uid: &str) -> String {
    format!("{}/packs/resolvedValues", cluster_route(uid))
}

/// The value of each macro of the cluster group `uid`, as Palette resolves it.
pub fn group_resolved_values_route(uid: &str) -> String {
    format!("{}/packs/resolvedValues", group_route(uid))
}

/// The document of the cluster `uid`, `GET /v1/spectroclusters/{uid}`.
pub fn cluster_route(uid: &str) -> String {
    format!("/v1/spectroclusters/{uid}")
}

/// The document of the cluster group `uid`, `GET /v1/clustergroups/{uid}`.
pub fn group_route(uid: &str) -> String {
    format!("/v1/clustergroups/{uid}")
}

/// The document of the app profile `uid`, `GET /v1/appProfiles/{uid}`.
pub fn app_profile_route(uid: &str) -> String {
    format!("/v1/appProfiles/{uid}")
}

/// The document of the edge host `uid`, `GET /v1/edgehosts/{uid}`.
pub fn edge_host_route(uid: &str) -> String {
    format!("/v1/edgehosts/{uid}")
}

/// The pack manifests of each profile of the cluster `uid`.
pub fn cluster_manifests_route(uid: &str) -> String {
    format!("/v1/spectroclusters/{uid}/profiles/packs/manifests")
}

/// The variables of the cluster `uid`.
pub fn cluster_variables_route(uid: &str) -> String {
    format!("/v1/spectroclusters/{uid}/variables")
}

/// The pack listing of the pack `name`, as strata asks for its first page.
pub fn pack_listing_route(name: &str) -> String {
    format!("/v1/packs?filters=spec.name={name}&limit=100&offset=0")
}

/// The pack `name` in the registry `registry`, `GET /v1/packs/{name}/registries/{registry}`: its
/// tags.
pub fn pack_entity_route(name: &str, registry: &str) -> String {
    format!("/v1/packs/{name}/registries/{registry}")
}

/// The registry listing, `GET /v1/registries/metadata`.
pub const REGISTRY_LISTING: &str = "/v1/registries/metadata";

/// The summary of the OCI registries, `GET /v1/registries/oci/summary`.
pub const OCI_REGISTRY_SUMMARY: &str = "/v1/registries/oci/summary";

/// Serve the pack listing of the pack `name`: the pack summaries `items`, an array, and their count.
pub fn serve_pack_listing(
    api: strata::api::MemoryApi,
    name: &str,
    items: Value,
) -> strata::api::MemoryApi {
    let count = items.as_array().map_or(0, Vec::len);
    api.with(
        pack_listing_route(name),
        json!({"items": items, "listmeta": {"count": count}}),
    )
}

/// The registry endpoints strata reads for one pack, as the examples cached them from a live
/// tenant: the version listing filtered by name, and the pack entity with its tags.
pub fn serve_registry_pack(
    api: strata::api::MemoryApi,
    name: &str,
    summaries: Value,
    entity: Value,
    registry: &str,
) -> strata::api::MemoryApi {
    serve_pack_listing(api, name, summaries).with(pack_entity_route(name, registry), entity)
}

/// One manifest `manifest` of the pack `pack` of the profile version `uid`, with the content of
/// each template.
pub fn manifest_route(uid: &str, pack: &str, manifest: &str) -> String {
    format!("/v1/clusterprofiles/{uid}/packs/{pack}/manifests/{manifest}")
}

// ---- the profile documents ---------------------------------------------------------------------

/// A template of a profile version: the cloud type, the type of the profile, and its packs.
pub fn template(cloud: &str, kind: &str, packs: Value) -> Value {
    json!({"cloudType": cloud, "type": kind, "packs": packs})
}

/// A template with no pack: the published template of a version that nobody published, or the
/// draft of a version with no draft.
pub fn no_template() -> Value {
    json!({"packs": []})
}

/// The document of the profile version `uid`, as `GET /v1/clusterprofiles/{uid}` gives it.
///
/// `published` and `draft` are its two templates. `status.isPublished` is true when the published
/// template has a pack. With `draft` None, the document has no draft template at all.
pub fn profile_document(
    uid: &str,
    name: &str,
    version: &str,
    published: Value,
    draft: Option<Value>,
) -> Value {
    let is_published = published["packs"].as_array().is_some_and(|p| !p.is_empty());
    let mut doc = json!({
        "metadata": {"name": name, "uid": uid},
        "spec": {"version": version, "published": published},
        "status": {"isPublished": is_published, "inUseClusterUids": []},
    });
    if let Some(draft) = draft {
        doc["spec"]["draft"] = draft;
    }
    doc
}

/// Serve the three documents of the profile version `uid`: its document, its pack manifests,
/// and its variables.
pub fn serve_profile(
    api: strata::api::MemoryApi,
    uid: &str,
    document: Value,
    manifests: Value,
    variables: Value,
) -> strata::api::MemoryApi {
    api.with(profile_route(uid), document)
        .with(manifests_route(uid), manifests)
        .with(variables_route(uid), variables)
}

/// The answer of `packs/manifests` for a version whose packs have no manifest.
pub fn no_manifests() -> Value {
    json!({"spec": {"packs": []}})
}

/// The answer of `variables` for a version with no variable.
pub fn no_variables() -> Value {
    json!({"variables": []})
}
