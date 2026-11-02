//! The project macros, and the values that Palette resolves for them.

use serde_json::{Map, Value, json};

use super::Tenant;
use super::routes::{
    CLUSTERS, GROUPS, PROFILES, cluster_route, group_route, macros_route, profile_route,
    resolved_values_file,
};

/// The system macro of the cluster name. Palette gives it an empty value for a profile version.
pub(super) const CLUSTER_NAME_MACRO: &str = "{{.spectro.system.cluster.name}}";
/// The system macro of the profile name.
pub(super) const PROFILE_NAME_MACRO: &str = "{{.spectro.system.clusterprofile.name}}";

/// The placeholder of the user macro `name`, as a value reads it and as `packs/resolvedValues` keys
/// it: `{{.spectro.macro.NAME}}`.
///
/// The stand-in writes the text as Palette gives it. It does not call strata for it, so a change of
/// strata cannot move the answer of the stand-in.
pub(super) fn user_macro(name: &str) -> String {
    format!("{{{{.spectro.macro.{name}}}}}")
}

/// The object whose `packs/resolvedValues` answer lists the profiles that it pins.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Pinner {
    Cluster,
    Group,
}

impl Pinner {
    /// The document of the object `uid`.
    pub(super) fn route(self, uid: &str) -> String {
        match self {
            Pinner::Cluster => cluster_route(uid),
            Pinner::Group => group_route(uid),
        }
    }
}
impl Tenant {
    /// Give a project macro a value, as `PATCH /v1/projects/{uid}/macros` does. Then let each
    /// cluster, profile version, and cluster group of the project resolve it again. Palette
    /// answers `packs/resolvedValues` with the value that the macro has now.
    pub fn set_macro(&self, project: &str, name: &str, value: &str) {
        let path = macros_route(project);
        let mut doc = if self.doc(project, &path).is_file() {
            self.read(project, &path)
        } else {
            json!({"macros": []})
        };
        let macros = doc["macros"].as_array_mut().expect("macros is a list");
        macros.retain(|m| m["name"] != name);
        macros.push(json!({"name": name, "value": value}));
        self.write(project, &path, &doc);
        for uid in self.uids_in(project, CLUSTERS) {
            self.resolve_macros(project, Pinner::Cluster, &uid);
        }
        for uid in self.uids_in(project, PROFILES) {
            self.resolve_profile_macros(project, &uid);
        }
        for uid in self.uids_in(project, GROUPS) {
            self.resolve_macros(project, Pinner::Group, &uid);
        }
    }

    /// The uid of each document directly inside one directory of a project.
    fn uids_in(&self, project: &str, path: &str) -> Vec<String> {
        self.documents_in(project, path)
            .iter()
            .filter_map(|d| d["metadata"]["uid"].as_str().map(str::to_string))
            .collect()
    }

    /// The user macros of `project`, by placeholder: each `{{.spectro.macro.<name>}}` and its
    /// value.
    fn user_macros(&self, project: &str) -> Map<String, Value> {
        let macros_path = macros_route(project);
        let macros = if self.doc(project, &macros_path).is_file() {
            self.read(project, &macros_path)["macros"].clone()
        } else {
            json!([])
        };
        let mut resolved = Map::new();
        for m in macros.as_array().into_iter().flatten() {
            let name = m["name"].as_str().unwrap_or_default();
            resolved.insert(user_macro(name), m["value"].clone());
        }
        resolved
    }

    /// The `packs/resolvedValues` answer of one profile version, with no cluster: one `resolved`
    /// map with each project macro and the system macro of the profile name. The system macro of
    /// the cluster name has an empty value, as Palette gives it for a profile.
    pub(super) fn resolve_profile_macros(&self, project: &str, profile: &str) {
        let route = profile_route(profile);
        let doc = self.read(project, &route);
        let mut resolved = self.user_macros(project);
        resolved.insert(
            PROFILE_NAME_MACRO.to_string(),
            doc["metadata"]["name"].clone(),
        );
        resolved.insert(CLUSTER_NAME_MACRO.to_string(), json!(""));
        self.write(
            project,
            &resolved_values_file(&route),
            &json!({ "resolved": resolved }),
        );
    }

    /// The `packs/resolvedValues` answer of one cluster or one cluster group, `uid`: for each
    /// profile it pins, each project macro and its value. A cluster adds the system macro of its
    /// name, as Palette gives them.
    pub(super) fn resolve_macros(&self, project: &str, owner: Pinner, uid: &str) {
        let route = owner.route(uid);
        let doc = self.read(project, &route);
        let mut resolved = self.user_macros(project);
        if owner == Pinner::Cluster {
            resolved.insert(
                CLUSTER_NAME_MACRO.to_string(),
                doc["metadata"]["name"].clone(),
            );
        }
        let profiles: Vec<Value> = doc["spec"]["clusterProfileTemplates"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|t| json!({"uid": t["uid"], "resolved": resolved}))
            .collect();
        self.write(
            project,
            &resolved_values_file(&route),
            &json!({"profiles": profiles}),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_user_macro_is_its_name_in_double_braces_after_the_prefix_of_palette() {
        assert_eq!(user_macro("region"), "{{.spectro.macro.region}}");
    }

    /// One function resolves the macros of a cluster and of a cluster group. Both list each
    /// profile they pin with each project macro. Only the cluster adds the system macro of its
    /// name.
    #[test]
    fn a_macro_resolves_for_a_cluster_with_its_name_and_for_a_group_without_it() {
        let dir = tempfile::tempdir().unwrap();
        let tenant = Tenant::new(dir.path(), dir.path());
        let owner = |uid: &str, name: &str| {
            json!({"metadata": {"uid": uid, "name": name},
                "spec": {"clusterProfileTemplates": [{"uid": "p1"}, {"uid": "p2"}]}})
        };
        tenant.put("proj", &cluster_route("c1"), &owner("c1", "fred"));
        tenant.put("proj", &group_route("g1"), &owner("g1", "edge"));
        let resolved = |route: String| tenant.read("proj", &resolved_values_file(&route));
        let placeholder = user_macro("region");
        let region = placeholder.as_str();
        for value in ["us-east-2", "eu-west-1"] {
            tenant.set_macro("proj", "region", value);
            let cluster = json!({"profiles": [
                {"uid": "p1", "resolved": {region: value, CLUSTER_NAME_MACRO: "fred"}},
                {"uid": "p2", "resolved": {region: value, CLUSTER_NAME_MACRO: "fred"}}]});
            let group = json!({"profiles": [
                {"uid": "p1", "resolved": {region: value}},
                {"uid": "p2", "resolved": {region: value}}]});
            assert_eq!(resolved(cluster_route("c1")), cluster);
            assert_eq!(resolved(group_route("g1")), group);
        }
    }
}
