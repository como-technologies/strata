//! The routes of Palette, and the documents that a test or a benchmark serves from them.
//!
//! The crate names each route in a const of its own that is not public. So this file names the
//! route again, in one place. `tests/common/mod.rs` re-exports it for the tests. The libraries
//! of `benches/` and `fuzz/` include it with `#[path]`. So no second list of the routes exists.
//!
//! The route helpers write the route that Palette serves, on purpose. They do not call
//! `strata::api::subject_path` or another function of `strata::api` that builds a route. A test
//! that builds its routes with the code under test cannot see a change of a route. A test of a
//! shipped script or of a stand-in server needs the same route text as the script.

use serde_json::{Value, json};

/// The uid of the fixture a, `lab-profile@1.2.0`, and of each fixture made from a. The fuzz
/// targets give it to their profile too, so that one text names the uid of a profile document.
pub const UID_A: &str = "64f0000000000000000000a1";

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

/// The template of the route of a profile version, with the text `{uid}` for the segment that varies.
/// The log of the unknown values names a read by this template, and never by a real uid.
pub const PROFILE_TEMPLATE: &str = "/v1/clusterprofiles/{uid}";

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

/// The summary of the cluster `uid`, `GET /v1/dashboard/spectroclusters/{uid}`.
pub fn cluster_summary_route(uid: &str) -> String {
    format!("/v1/dashboard/spectroclusters/{uid}")
}

/// The events of the cluster `uid`, as strata reads the first page of them.
pub fn cluster_events_route(uid: &str) -> String {
    format!("/v1/events/components/spectrocluster/{uid}?limit=50")
}

/// The cloud config `uid` of the cloud type `kind`, `GET /v1/cloudconfigs/{kind}/{uid}`.
pub fn cloud_config_route(kind: &str, uid: &str) -> String {
    format!("/v1/cloudconfigs/{kind}/{uid}")
}

/// The cloud account `uid` of the cloud type `kind`, `GET /v1/cloudaccounts/{kind}/{uid}`.
pub fn cloud_account_route(kind: &str, uid: &str) -> String {
    format!("/v1/cloudaccounts/{kind}/{uid}")
}

/// The document of the pack `uid`, `GET /v1/packs/{uid}`: its values and the schema of its values.
pub fn pack_route(uid: &str) -> String {
    format!("/v1/packs/{uid}")
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
/// tenant. They are the version listing filtered by name, and the pack entity with its tags.
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

/// The entry of `spec.clusterProfileTemplates` that a cluster holds for a profile version. It has
/// the uid, the name, `profileVersion`, and the published type, cloud type, and packs of
/// `document`. `document` is a profile as [`profile_document`] builds it.
pub fn cluster_template(document: &Value) -> Value {
    let published = &document["spec"]["published"];
    json!({
        "uid": document["metadata"]["uid"], "name": document["metadata"]["name"],
        "profileVersion": document["spec"]["version"], "type": published["type"],
        "cloudType": published["cloudType"], "packs": published["packs"],
    })
}

// ---- the pack documents ------------------------------------------------------------------------

/// The registry of a pack when a fixture names none.
pub const PUBLIC_REGISTRY: &str = "reg-public";

/// A pack as a fixture states it: the fields that its pack ref, its pack manifests, and its entry
/// in the pack listing share.
///
/// The tag of the pack ref is the version.
#[derive(Debug, Clone, Copy)]
pub struct PackFixture<'a> {
    name: &'a str,
    layer: &'a str,
    version: &'a str,
    pack_uid: &'a str,
    registry_uid: &'a str,
    digest: &'a str,
    values: &'a str,
}

impl<'a> PackFixture<'a> {
    /// A pack of the registry `registry_uid`.
    ///
    /// `name` is the name in the profile. `values` is YAML. An empty `digest` omits the digest
    /// from the pack ref.
    pub fn new(
        name: &'a str,
        layer: &'a str,
        version: &'a str,
        pack_uid: &'a str,
        registry_uid: &'a str,
        digest: &'a str,
        values: &'a str,
    ) -> PackFixture<'a> {
        PackFixture {
            name,
            layer,
            version,
            pack_uid,
            registry_uid,
            digest,
            values,
        }
    }

    /// A pack of the registry [`PUBLIC_REGISTRY`]: [`PackFixture::new`] with no registry.
    pub fn public(
        name: &'a str,
        layer: &'a str,
        version: &'a str,
        pack_uid: &'a str,
        digest: &'a str,
        values: &'a str,
    ) -> PackFixture<'a> {
        PackFixture::new(
            name,
            layer,
            version,
            pack_uid,
            PUBLIC_REGISTRY,
            digest,
            values,
        )
    }

    /// The pack ref on the template of a profile (`V1PackRef`). It lists each manifest of
    /// `manifests` by its name and its uid.
    pub fn pack_ref(&self, manifests: &[Value]) -> Value {
        let refs: Vec<Value> = manifests
            .iter()
            .map(|m| json!({"name": m["name"], "uid": m["uid"]}))
            .collect();
        let mut v = json!({
            "name": self.name, "layer": self.layer, "type": "spectro", "tag": self.version,
            "version": self.version, "packUid": self.pack_uid, "registryUid": self.registry_uid,
            "values": self.values, "manifests": refs, "params": {}, "presets": [],
        });
        if !self.digest.is_empty() {
            v["digest"] = json!(self.digest);
        }
        v
    }

    /// The entry of the pack in `packs/manifests` (`V1ClusterProfilePackManifests`), with the
    /// manifests `manifests`.
    pub fn pack_manifests(&self, manifests: Vec<Value>) -> Value {
        json!({
            "metadata": {"name": self.name, "uid": self.pack_uid},
            "spec": {
                "name": self.name, "layer": self.layer, "version": self.version,
                "digest": self.digest, "registryUid": self.registry_uid, "values": self.values,
                "manifests": manifests,
            },
            "status": {},
        })
    }

    /// The entry of the pack in the pack listing of [`pack_listing_route`], for the clouds
    /// `clouds`. The uid of the entry is the uid of the pack, and its type is `spectro`. An empty
    /// `clouds` omits `cloudTypes`, and an empty digest omits the digest.
    pub fn summary(&self, clouds: &[&str]) -> Value {
        let mut v = json!({
            "metadata": {"uid": self.pack_uid},
            "spec": {"name": self.name, "version": self.version, "registryUid": self.registry_uid,
                     "layer": self.layer, "type": "spectro", "values": self.values},
        });
        if !self.digest.is_empty() {
            v["spec"]["digest"] = json!(self.digest);
        }
        if !clouds.is_empty() {
            v["spec"]["cloudTypes"] = json!(clouds);
        }
        v
    }
}

/// A manifest of a pack, as `packs/manifests` lists it.
pub fn manifest(name: &str, uid: &str, digest: &str, content: &str) -> Value {
    json!({"name": name, "uid": uid, "digest": digest, "content": content})
}

/// A variable of a profile, as `variables` lists it. It is required, and neither hidden nor
/// immutable.
pub fn variable(name: &str, display: &str, default: &str, sensitive: bool) -> Value {
    json!({
        "name": name, "displayName": display, "defaultValue": default, "format": "string",
        "inputType": "text", "required": true, "hidden": false, "immutable": false,
        "isSensitive": sensitive, "options": [],
    })
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

/// The answer of `packs/manifests` for a version with these `packs`, such as
/// [`PackFixture::pack_manifests`] builds them.
pub fn manifests_answer(packs: Vec<Value>) -> Value {
    json!({"spec": {"packs": packs}})
}

/// The answer of `packs/manifests` for a version whose packs have no manifest.
pub fn no_manifests() -> Value {
    manifests_answer(Vec::new())
}

/// The answer of `variables` for a version with no variable.
pub fn no_variables() -> Value {
    json!({"variables": []})
}
