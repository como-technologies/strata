//! The routes of Palette that the tenant serves, as the stand-in names their files.

// The routes of Palette, as the server names their files: the path, and the query after an `@`.
// A route that two helpers write has one const or one fn here, and the server reads the same
// one.

/// The projects that the key can see, `GET /v1/dashboard/projects/metadata`.
pub const PROJECT_LISTING: &str = "v1/dashboard/projects/metadata";
/// The profile search, `POST /v1/dashboard/clusterprofiles`.
///
/// The server keeps the items of the scope that the body names.
pub const PROFILE_SEARCH_PATH: &str = "v1/dashboard/clusterprofiles";
/// The cluster search, `POST /v1/dashboard/spectroclusters/search`.
///
/// The server keeps the clusters that the body selects.
pub const CLUSTER_SEARCH_PATH: &str = "v1/dashboard/spectroclusters/search";
/// The audit log, `GET /v1/audits`.
///
/// The server answers an empty log for a resource with no file.
pub const AUDITS: &str = "v1/audits";
/// The profile documents of a scope, `v1/clusterprofiles/{uid}`.
pub(super) const PROFILES: &str = "v1/clusterprofiles";
/// The cluster documents of a project, `v1/spectroclusters/{uid}`.
pub(super) const CLUSTERS: &str = "v1/spectroclusters";
/// The cluster group documents of a project, `v1/clustergroups/{uid}`.
pub(super) const GROUPS: &str = "v1/clustergroups";
/// The edge host documents of a project, `v1/edgehosts/{uid}`.
pub(super) const EDGE_HOSTS: &str = "v1/edgehosts";
/// The app profile documents of a project, `v1/appProfiles/{uid}`.
pub(super) const APP_PROFILES: &str = "v1/appProfiles";
/// The pack registries of a scope, `GET /v1/registries/metadata`.
pub(super) const REGISTRY_LISTING: &str = "v1/registries/metadata";

/// The file of a listing or a search that strata reads by pages: `path` with the page of
/// strata.
///
/// One page holds every item of the stand-in.
pub(super) fn paged(path: &str) -> String {
    format!("{path}@limit=50")
}

/// The document of the profile version `uid`, `GET /v1/clusterprofiles/{uid}`.
pub(super) fn profile_route(uid: &str) -> String {
    format!("{PROFILES}/{uid}")
}

/// The manifests of the profile version `uid`, `GET /v1/clusterprofiles/{uid}/packs/manifests`.
pub(super) fn manifests_route(uid: &str) -> String {
    format!("{}/packs/manifests", profile_route(uid))
}

/// The variables of the profile version `uid`, `GET /v1/clusterprofiles/{uid}/variables`.
pub(super) fn variables_route(uid: &str) -> String {
    format!("{}/variables", profile_route(uid))
}

/// One manifest of the pack `pack` of the profile version `uid`,
/// `GET /v1/clusterprofiles/{uid}/packs/{pack}/manifests/{manifest}`.
pub(super) fn manifest_route(uid: &str, pack: &str, manifest: &str) -> String {
    format!("{}/packs/{pack}/manifests/{manifest}", profile_route(uid))
}

/// The document of the cluster `uid`, `GET /v1/spectroclusters/{uid}`.
pub(super) fn cluster_route(uid: &str) -> String {
    format!("{CLUSTERS}/{uid}")
}

/// The variables of the cluster `uid`, `GET /v1/spectroclusters/{uid}/variables`.
pub(super) fn cluster_variables_route(uid: &str) -> String {
    format!("{}/variables", cluster_route(uid))
}

/// The role bindings of the cluster `uid`, `GET /v1/spectroclusters/{uid}/config/rbacs`.
pub fn rbacs_route(uid: &str) -> String {
    format!("{}/config/rbacs", cluster_route(uid))
}

/// The namespaces of the cluster `uid`, `GET /v1/spectroclusters/{uid}/config/namespaces`.
pub(super) fn namespaces_route(uid: &str) -> String {
    format!("{}/config/namespaces", cluster_route(uid))
}

/// The repave check of the cluster `uid`, `POST /v1/spectroclusters/{uid}/validate/repave`.
pub(super) fn repave_route(uid: &str) -> String {
    format!("{}/validate/repave", cluster_route(uid))
}

/// The notifications of the cluster `uid`, `GET /v1/notifications/spectrocluster/{uid}`.
pub(super) fn notifications_route(uid: &str) -> String {
    format!("v1/notifications/spectrocluster/{uid}")
}

/// The document of the cluster group `uid`, `GET /v1/clustergroups/{uid}`.
pub(super) fn group_route(uid: &str) -> String {
    format!("{GROUPS}/{uid}")
}

/// The virtual clusters of the cluster group `uid`, as the server names the file of
/// `GET /v1/dashboard/clustergroups/{uid}/virtualClusters`.
pub(super) fn virtual_clusters_file(uid: &str) -> String {
    paged(&format!("v1/dashboard/clustergroups/{uid}/virtualClusters"))
}

/// The document of the app profile `uid`, `GET /v1/appProfiles/{uid}`.
pub(super) fn app_profile_route(uid: &str) -> String {
    format!("{APP_PROFILES}/{uid}")
}

/// The macros of the project `project`, `GET /v1/projects/{project}/macros`.
pub(super) fn macros_route(project: &str) -> String {
    format!("v1/projects/{project}/macros")
}
/// The last segment of the export route of a profile version.
pub(super) const EXPORT: &str = "export";

/// The value of each macro of the object at `route`, `GET {route}/packs/resolvedValues`.
pub(super) fn resolved_values_file(route: &str) -> String {
    format!("{route}/packs/resolvedValues")
}

/// The file of the export of the profile version `uid` as JSON,
/// `GET /v1/clusterprofiles/{uid}/export?format=json`.
pub(super) fn export_file(uid: &str) -> String {
    format!("{}/{EXPORT}@format=json", profile_route(uid))
}

/// The profile route of the export route `path`, `v1/clusterprofiles/{uid}/export`:
/// `v1/clusterprofiles/{uid}`. `None` for any other path.
pub fn exported_profile(path: &str) -> Option<&str> {
    let profile = path.strip_suffix(EXPORT)?.strip_suffix('/')?;
    let uid = profile.strip_prefix(PROFILES)?.strip_prefix('/')?;
    (!uid.is_empty() && !uid.contains('/')).then_some(profile)
}

/// The versions of the pack `name` in each registry, as the server names the file of
/// `GET /v1/packs?filters=spec.name={name}&limit=100&offset=0`: the first page of the listing.
pub(super) fn pack_listing(name: &str) -> String {
    format!("v1/packs@filters=spec.name={name}&limit=100&offset=0")
}

/// The pack `name` in the registry `registry`, `GET /v1/packs/{name}/registries/{registry}`.
pub(super) fn pack_entity_route(name: &str, registry: &str) -> String {
    format!("v1/packs/{name}/registries/{registry}")
}

/// The audit log of the resource `uid` of the kind `kind`, as the server names the file of
/// `GET /v1/audits?resourceKind={kind}&resourceUid={uid}&limit=100`.
///
/// The server leaves the time out of the query.
pub(super) fn audit_log_file(kind: &str, uid: &str) -> String {
    format!("{AUDITS}@resourceKind={kind}&resourceUid={uid}&limit=100")
}

/// The uid of the cloud config of the cluster `uid`: the uid of the cluster with a `c` at the end.
pub(super) fn cloud_config_uid(uid: &str) -> String {
    format!("{uid}c")
}

/// The cloud config of the cluster `uid` for the cloud type `kind`.
///
/// Palette serves the cloud config of each cloud type at its own path. The stand-in gives the
/// config the uid of the cluster with a `c` at the end.
pub(super) fn cloud_config_route(kind: &str, uid: &str) -> String {
    format!("v1/cloudconfigs/{kind}/{}", cloud_config_uid(uid))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_export_route_names_the_profile_it_exports() {
        let export = format!("{}/export", profile_route("u1"));
        assert_eq!(exported_profile(&export), Some("v1/clusterprofiles/u1"));
        // the file of the stand-in has the query after an `@`, and is not the route
        assert_eq!(exported_profile(&export_file("u1")), None);
        for other in [
            "v1/clusterprofiles/export",
            "v1/clusterprofiles//export",
            "v1/clusterprofiles/u1/packs/export",
            "v1/clusterprofiles/u1/export/more",
            "v1/spectroclusters/u1/export",
            "v1/clusterprofiles/u1",
        ] {
            assert_eq!(exported_profile(other), None, "{other}");
        }
    }
}
