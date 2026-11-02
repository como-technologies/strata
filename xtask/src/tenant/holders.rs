//! The objects that hold profiles and are no cluster: edge hosts, cluster templates, cluster
//! groups, and app profiles.

use serde_json::{Value, json};
use strata::closed_set::ClosedSet;
use strata::model::scope::ProfileScope;

use super::macros::Pinner;
use super::profiles::template_of;
use super::registries::PUBLIC_REPO;
use super::routes::{
    EDGE_HOSTS, app_profile_route, app_tier_manifest_route, app_tiers_route, cluster_route,
    group_route, manifest_route, manifests_route, paged, profile_route, virtual_clusters_file,
};
use super::{TENANT, Tenant};

/// The host cluster of each cluster group. The book lists no host cluster.
const GROUP_HOST: &str = "64f5000000000000000000b1";

/// The values that a cluster group gives each virtual cluster: YAML with comments, as Palette
/// gives it.
const GROUP_VALUES: &str = "# Sync describes how to sync resources to the host cluster.\nsync:\n  toHost:\n    services:\n      enabled: true\n    ingresses:\n      enabled: false\n# The external database of each virtual cluster.\ndatabase:\n  user: vcluster\n  password: s3cr3t\n";

impl Tenant {
    /// An edge host `uid`, `name`, with `profiles` bound to it: its own copy of each profile as
    /// the profile is now. A live run gives the rules. The host keeps its copy of the values, and
    /// holds each manifest by uid alone. So strata reads the manifests of the profile.
    pub fn edge_host(&self, project: &str, uid: &str, name: &str, profiles: &[&str]) {
        let templates = self.holder_templates(project, uid, profiles, |_, _, copy| {
            format!("{EDGE_HOSTS}/{uid}/pack/manifests/{copy}")
        });
        self.write(
            project,
            &format!("{EDGE_HOSTS}/{uid}"),
            &json!({
                "metadata": {"uid": uid, "name": name,
                    "annotations": {"projectUid": project,
                        "scope": ProfileScope::Project.as_str()}},
                "spec": {"clusterProfileTemplates": templates},
                "status": {"state": "ready"},
            }),
        );
        // the search gives a summary of each template: uid, name, and version
        let items: Vec<Value> = self
            .documents_in(project, EDGE_HOSTS)
            .iter()
            .map(|h| {
                let summary: Vec<Value> = h["spec"]["clusterProfileTemplates"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .map(|t| {
                        json!({"uid": t["uid"], "name": t["name"],
                        "version": t["profileVersion"], "type": t["type"]})
                    })
                    .collect();
                json!({"metadata": h["metadata"], "spec": {"clusterProfileTemplates": summary},
                    "status": h["status"]})
            })
            .collect();
        let search = json!({"items": items, "listmeta": {"continue": ""}});
        for scope in [project, TENANT] {
            self.write(scope, &paged("v1/dashboard/edgehosts/search"), &search);
        }
    }

    /// A cluster template `uid`, `name`, that links `profiles`. Palette names each template that
    /// links a version in `status.inUseClusterTemplates` of the profile document, with the fields
    /// of `v1ProjectResourceReference`. Palette refuses the delete of the version while one does.
    pub fn cluster_template(&self, project: &str, uid: &str, name: &str, profiles: &[&str]) {
        for p in profiles {
            let path = profile_route(p);
            let mut doc = self.read(project, &path);
            let mut rows: Vec<Value> = doc["status"]["inUseClusterTemplates"]
                .as_array()
                .cloned()
                .unwrap_or_default()
                .into_iter()
                .filter(|t| t["uid"] != json!(uid))
                .collect();
            rows.push(json!({"kind": "clustertemplate", "name": name,
                             "projectUid": project, "uid": uid}));
            doc["status"]["inUseClusterTemplates"] = json!(rows);
            self.write(project, &path, &doc);
        }
    }

    /// A cluster group `uid`, `name`, that pins `profiles` for its virtual clusters, with the
    /// clusters `members` as those virtual clusters. The group gives each virtual cluster the
    /// config of `spec.clustersConfig`, as Palette gives it: limits, an endpoint, and values.
    pub fn cluster_group(
        &self,
        project: &str,
        uid: &str,
        name: &str,
        profiles: &[&str],
        members: &[&str],
    ) {
        let templates = self.holder_templates(project, uid, profiles, manifest_route);
        self.write(
            project,
            &group_route(uid),
            &json!({
                "metadata": {"uid": uid, "name": name},
                "spec": {
                    "type": "hostCluster",
                    "clusterProfileTemplates": templates,
                    "clustersConfig": {
                        "endpointType": "LoadBalancer",
                        "hostClustersConfig": [{"clusterUid": GROUP_HOST, "endpointConfig": {
                            "ingressConfig": {"host": "*", "port": 6443},
                            "loadBalancerConfig": {"externalIPs": null,
                                "loadBalancerSourceRanges": null},
                        }}],
                        "kubernetesDistroType": "vcluster-generic",
                        "limitConfig": {"cpu": 6, "cpuMilliCore": 6000, "memory": 12288,
                            "memoryMiB": 12288, "overSubscription": 120, "storageGiB": 10},
                        "values": GROUP_VALUES,
                    },
                },
            }),
        );
        self.write(
            project,
            "v1/clustergroups/hostCluster/metadata",
            &json!({"items": [{"uid": uid, "name": name,
                "scope": ProfileScope::Project.as_str()}]}),
        );
        let items: Vec<Value> = members
            .iter()
            .map(|m| {
                let doc = self.read(project, &cluster_route(m));
                json!({"metadata": {"uid": m, "name": doc["metadata"]["name"],
                    "annotations": {"projectUid": project}}})
            })
            .collect();
        self.write(
            project,
            &virtual_clusters_file(uid),
            &json!({"items": items, "listmeta": {"continue": ""}}),
        );
        self.resolve_macros(project, Pinner::Group, uid);
    }

    /// The app profile `shop@<version>`, `uid`: a helm tier `web` with the property `replicas`,
    /// and a manifest tier `config`. A second call for the same uid is a plain update, as
    /// Palette takes one: the uid and the version stay. `web_order` is the install order of
    /// `web`. Palette gives no `installOrder` for a tier whose order is 0, as for `config`.
    pub fn app_profile(
        &self,
        project: &str,
        uid: &str,
        version: &str,
        replicas: &str,
        web_order: u32,
    ) {
        let mut web = json!({"metadata": {"name": "web", "uid": format!("{uid}1")},
            "spec": {"type": "helm", "version": "2.1.0", "registryUid": PUBLIC_REPO,
                "sourceAppTierUid": "64ed000000000000000000a1",
                "values": "replicaCount: 2\nimage:\n  tag: 2.1.0\n",
                "properties": [{"name": "replicas", "type": "number", "value": replicas}]}});
        if web_order > 0 {
            web["spec"]["installOrder"] = web_order.into();
        }
        self.write(
            project,
            "v1/dashboard/appProfiles/metadata",
            &json!({"appProfiles": [{"metadata": {"name": "shop", "uid": uid},
                "spec": {"version": version}}]}),
        );
        self.write(
            project,
            &app_profile_route(uid),
            &json!({
                "metadata": {"name": "shop", "uid": uid},
                "spec": {"version": version,
                    "versions": [{"uid": uid, "version": version}],
                    "template": {"appTiers": [
                        {"name": "web", "uid": format!("{uid}1"), "type": "helm",
                            "version": "2.1.0"},
                        {"name": "config", "uid": format!("{uid}2"), "type": "manifest",
                            "version": "1.0.0"}]}},
                "status": {"inUseApps": []},
            }),
        );
        self.write(
            project,
            &app_tiers_route(uid),
            &json!({"spec": {"appTiers": [
                web,
                {"metadata": {"name": "config", "uid": format!("{uid}2")},
                 "spec": {"type": "manifest", "version": "1.0.0",
                    "manifests": [{"kind": "manifest", "name": "config",
                        "uid": format!("{uid}3")}]}},
            ]}}),
        );
        self.write(
            project,
            &app_tier_manifest_route(uid, &format!("{uid}2"), &format!("{uid}3")),
            &json!({"metadata": {"name": "config", "uid": format!("{uid}3")},
                "spec": {"published": {
                    "content": "apiVersion: v1\nkind: ConfigMap\nmetadata:\n  name: shop\ndata:\n  mode: live\n"}}}),
        );
    }

    /// The app deployment `name`, `uid`, of the app profile `profile` at its version now, in the
    /// state `Deployed`. Palette names it in `status.inUseApps` of the app profile.
    pub fn app_deployment(&self, project: &str, uid: &str, name: &str, profile: &str) {
        let path = app_profile_route(profile);
        let mut doc = self.read(project, &path);
        let version = doc["spec"]["version"].clone();
        doc["status"]["inUseApps"]
            .as_array_mut()
            .expect("an app profile")
            .push(json!({"kind": "appdeployment", "uid": uid, "name": name,
                "projectUid": project}));
        self.write(project, &path, &doc);
        self.write(
            project,
            &format!("v1/appDeployments/{uid}"),
            &json!({
                "metadata": {"name": name, "uid": uid,
                    "annotations": {"projectUid": project}},
                "spec": {"profile": {"metadata": {"name": doc["metadata"]["name"],
                    "uid": profile, "version": version}}},
                "status": {"state": "Deployed"},
            }),
        );
    }

    /// The templates of an object that holds profiles and is not a cluster: an edge host or a
    /// cluster group.
    ///
    /// Palette gives the holder its own copy of each manifest, under a uid of its own. The
    /// template names the copy by that uid, with no content. `route(profile, pack, copy)` is
    /// where the API serves a copy.
    fn holder_templates(
        &self,
        project: &str,
        holder: &str,
        profiles: &[&str],
        route: impl Fn(&str, &str, &str) -> String,
    ) -> Vec<Value> {
        let mut n = 0;
        profiles
            .iter()
            .map(|p| {
                let mut t = template_of(&self.read(project, &profile_route(p)));
                let contents = self.read(project, &manifests_route(p));
                for pack in t["packs"].as_array_mut().into_iter().flatten() {
                    let pname = pack["name"].as_str().unwrap_or_default().to_string();
                    let source: Vec<Value> = contents["spec"]["packs"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .filter(|q| q["spec"]["name"] == pname.as_str())
                        .flat_map(|q| q["spec"]["manifests"].as_array().cloned().unwrap_or_default())
                        .collect();
                    if source.is_empty() {
                        continue;
                    }
                    let mut refs = Vec::new();
                    for m in source {
                        n += 1;
                        let copy = format!("{}{n:04x}", &holder[..holder.len().min(20)]);
                        self.write(
                            project,
                            &route(p, &pname, &copy),
                            &json!({"metadata": {"uid": copy, "name": m["name"]},
                                "spec": {"published": {"content": m["content"], "digest": m["digest"]}}}),
                        );
                        refs.push(json!({"uid": copy, "name": m["name"]}));
                    }
                    pack["manifests"] = json!(refs);
                }
                t
            })
            .collect()
    }
}
