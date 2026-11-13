//! The answers of the routes that check a change, and of the routes that report a repave.

use serde_json::{Value, json};
use strata::api::codes::Denial;

use super::Tenant;
use super::routes::{CLUSTERS, PROFILES, cluster_route, profile_route, repave_route};

/// One entry of `packs.results` in the answer of a validate route: the pack `pack`, and one
/// error with its `code` and its `message`.
fn pack_result(pack: &str, code: &str, message: &str) -> Value {
    json!({
        "name": pack,
        "displayName": pack,
        "errors": [{"code": code, "message": message}],
    })
}

/// The message of the refusal of a profile name and version that another profile has, with
/// [`PROFILE_EXISTS_CODE`](strata::api::codes::PROFILE_EXISTS_CODE). The name route, the import
/// validate route, and a PUT of a profile give it.
pub fn profile_exists_message(name: &str, version: &str) -> String {
    format!("Cluster Profile with name '{name}' and version '{version}' already exists")
}

/// The message of the refusal of one object with [`Denial::Object`], to a role that
/// does not give `permission`, such as `cluster.get`. The message names the permission two times.
fn operation_forbidden_message(permission: &str) -> String {
    format!("Operation '{permission}' is forbidden. Verify the user has '{permission}' permission")
}

/// The message of the refusal of a route as a whole with [`Denial::Route`], to a role that does
/// not give `permission` in `scope`, such as `filter.list` in `tenant`. Palette names the
/// permission two times: as the operation, and as the permission that the route needs.
pub fn access_denied_message(permission: &str, scope: &str) -> String {
    format!(
        "{permission} operation is not permitted, required permissions '{permission}' in the scope(s) {scope}"
    )
}

/// The message of the refusal of a request in the project `name` with [`Denial::Project`]: no
/// role of the user in the project gives the access. Palette names the project by its name.
pub fn project_denied_message(name: &str) -> String {
    format!("Project '{name}' access denied")
}

/// The entries of `packs.results` for a move that a pack does not allow on the clusters of the
/// refusal: one error, `DependencyPackUpgradeNotSupported`. Empty when there is no refusal.
fn upgrade_results(refusal: Option<UpgradeRefusal>) -> Vec<Value> {
    refusal
        .map(|UpgradeRefusal { pack, to, clusters }| {
            pack_result(
                pack,
                "DependencyPackUpgradeNotSupported",
                &format!(
                    "Pack upgrade to '{to}' is not compatible with the clusters '{}'",
                    clusters.join(", ")
                ),
            )
        })
        .into_iter()
        .collect()
}

/// The entries of `packs.results` for a pack `pack` that requires the pack `required` at
/// `version` or newer. The entries apply when the packs do not hold it. They hold one error,
/// `DependencyPackWithMinVersionRequired`.
fn min_version_results(pack: &str, required: &str, version: &str) -> Vec<Value> {
    vec![pack_result(
        pack,
        "DependencyPackWithMinVersionRequired",
        &format!("Pack '{required}' is required with minimum compatible version '{version}'"),
    )]
}

/// The body of the answer of `validate/packs` for a cluster: the profile `profile` with `results`
/// under `packs`. With no `results`, Palette accepts, and the body has no profile.
fn cluster_packs_body(profile: &str, results: Vec<Value>) -> Value {
    let profiles: Vec<Value> = if results.is_empty() {
        Vec::new()
    } else {
        vec![json!({"uid": profile, "packs": {"results": results}})]
    };
    json!({ "profiles": profiles })
}

/// A refusal in the answer of `validate/packs` for a cluster: the pack `pack` of the profile
/// `profile` requires the pack `required` at `version` or newer.
#[derive(Debug, Clone, Copy)]
pub struct PackRefusal<'a> {
    /// The uid of the profile that holds the pack.
    pub profile: &'a str,
    /// The name of the pack that has the requirement.
    pub pack: &'a str,
    /// The name of the pack that it requires.
    pub required: &'a str,
    /// The minimum version of the required pack.
    pub version: &'a str,
}

/// A refusal in the answer of `validate/packs` for a profile version. The pack `pack` cannot move
/// to the version `to` on `clusters`, which run the version of the profile.
#[derive(Debug, Clone, Copy)]
pub struct UpgradeRefusal<'a> {
    /// The name of the pack that does not move.
    pub pack: &'a str,
    /// The version that the pack moves to.
    pub to: &'a str,
    /// The names of the clusters that run the version of the profile.
    pub clusters: &'a [&'a str],
}

/// The reason `PackValuesUpdated` of a repave, as a live run gave it for a kubeadm command. The
/// values of the pack `kubernetes` changed in the key `kubeadmconfig.postKubeadmCommands`.
/// `pack` holds the pack of the reason, and this function adds the key.
fn values_updated(mut pack: Value) -> Value {
    pack["diffConfigKeys"] = json!(["kubeadmconfig.postKubeadmCommands"]);
    json!({
        "code": "PackValuesUpdated",
        "message": "Pack 'kubernetes' values are updated",
        "pack": pack,
    })
}

/// An answer of a `validate/repave` route. It holds a repave for a kubeadm command when
/// `required`, as a live run gave it. Otherwise it holds no repave and no reason.
fn repave_answer_of(required: bool) -> Value {
    let reasons = if required {
        json!([values_updated(json!({"current": {"name": "kubernetes"}}))])
    } else {
        json!([])
    };
    json!({"isRepaveRequired": required, "reasons": reasons})
}

impl Tenant {
    /// The answer of `POST /v1/spectroclusters/{uid}/validate/repave` for a cluster. It says
    /// whether the asked change repaves the cluster, and the reason, as a live run gave it for a
    /// kubeadm command. The stand-in gives one answer for every request; it does not compare
    /// anything.
    pub fn repave_answer(&self, project: &str, cluster: &str, required: bool) {
        self.write(project, &repave_route(cluster), &repave_answer_of(required));
    }

    /// Put a cluster in the cluster template `template`, as a cluster that the template launched.
    ///
    /// The cluster document names the template in `spec.clusterTemplate`. On `POST
    /// /v1/spectroclusters/{uid}/validate/repave` for the cluster, Palette then compares nothing,
    /// and answers as for a change that does not repave: `isRepaveRequired: false` and no reason.
    pub fn cluster_in_template(&self, project: &str, cluster: &str, template: &str) {
        self.cluster_field(
            project,
            cluster,
            "/spec/clusterTemplate",
            json!({"uid": template, "projectUid": project}),
        );
        self.write(project, &repave_route(cluster), &repave_answer_of(false));
    }

    /// The answer of `POST /v1/spectroclusters/clusterTemplates/{uid}/validate/repave` for a
    /// cluster template: whether the asked change of its profiles repaves its clusters. Palette
    /// gives the answer of the cluster route, for one cluster of the template. The stand-in gives
    /// one answer for every request, with the reason of [`Tenant::repave_answer`].
    pub fn template_repave_answer(&self, project: &str, template: &str, required: bool) {
        self.write(
            project,
            &format!("{CLUSTERS}/clusterTemplates/{template}/validate/repave"),
            &repave_answer_of(required),
        );
    }

    /// Make Palette refuse `GET path` in `project` to a key whose role does not give `cluster.get`.
    ///
    /// `path` is the route of the role bindings, the namespaces, or the repave of a cluster. The
    /// code of Palette shows the answer to such a key: HTTP 500 with
    /// the code of [`Denial::Object`], and a message that names the permission. No live run shows it
    /// yet.
    pub fn forbid(&self, project: &str, path: &str) {
        let message = operation_forbidden_message("cluster.get");
        self.deny(project, path, Denial::Object, &message);
    }

    /// Make Palette refuse `GET path` in `project` with `denial`, as it refuses a key whose roles do
    /// not give the access. The answer has the status of the denial, its code, and `message`.
    ///
    /// After the call, the route gives no document.
    pub fn deny(&self, project: &str, path: &str, denial: Denial, message: &str) {
        let file = self.doc(project, path);
        if file.is_file() {
            std::fs::remove_file(&file).unwrap_or_else(|e| panic!("{}: {e}", file.display()));
        }
        let refused = crate::palette::refused(&file, denial.http_status());
        let body = json!({"code": denial.as_str(), "message": message});
        std::fs::write(&refused, body.to_string() + "\n")
            .unwrap_or_else(|e| panic!("{}: {e}", refused.display()));
    }

    /// Hold a repave of a cluster for approval, as Palette does. Palette holds it at the next read
    /// of the agent after a macro in `kubeadmconfig.postKubeadmCommands` gets a new value.
    ///
    /// The cluster document says `status.repave.state: Pending`. `GET
    /// /v1/spectroclusters/{uid}/repave/status` gives the source `hubble` and one reason, with the
    /// resolved values on each side. `since` is the time of the state.
    pub fn repave_waits(&self, project: &str, cluster: &str, since: &str) {
        self.cluster_field(
            project,
            cluster,
            "/status/repave",
            json!({"state": "Pending"}),
        );
        let side = |command: &str| {
            json!({
                "name": "kubernetes",
                "values": format!("kubeadmconfig:\n  postKubeadmCommands:\n  - {command}\n"),
            })
        };
        self.write(
            project,
            &format!("{}/repave/status", cluster_route(cluster)),
            &json!({
                "metadata": {"uid": format!("{cluster}r"), "creationTimestamp": since},
                "spec": {
                    "spectroClusterUid": cluster,
                    "source": "hubble",
                    "reasons": [values_updated(
                        json!({"current": side("echo one"), "target": side("echo two")}),
                    )],
                },
                "status": {"state": "Pending", "repaveTransitionTime": since},
            }),
        );
    }

    /// The answer of `POST /v1/spectroclusters/{uid}/validate/packs` for a cluster. A
    /// [`PackRefusal`] says that a pack of a profile requires another pack at a minimum version,
    /// and the cluster runs an older one. Palette gives the error, with its code and
    /// its message, under the profile, and gives no profile when it accepts. With no refusal, the
    /// answer is also the answer to a move of a tag under a profile that the cluster runs.
    /// Palette keeps the tag of the copy that the cluster holds, and checks no sent tag. The
    /// stand-in gives one answer for every request; it does not check anything.
    pub fn packs_answer(&self, project: &str, cluster: &str, refusal: Option<PackRefusal>) {
        let (profile, results) = refusal.map_or(("", Vec::new()), |r| {
            (
                r.profile,
                min_version_results(r.pack, r.required, r.version),
            )
        });
        self.write_cluster_packs(project, cluster, profile, results);
    }

    /// The answer of `POST /v1/clusterprofiles/{uid}/validate/packs` with `profile`, or of `POST
    /// /v1/clusterprofiles/validate/packs` with none. A refusal is `(pack, required, version)`:
    /// `pack` requires `required` at `version` or newer, and the packs of the body do not hold it.
    /// Palette gives the error, with its code and its message, under `packs.results`, and gives an
    /// empty `results` when it accepts. The stand-in gives one answer for every request; it does
    /// not check anything.
    pub fn profile_packs_answer(
        &self,
        project: &str,
        profile: Option<&str>,
        refusal: Option<(&str, &str, &str)>,
    ) {
        let results = refusal.map_or_else(Vec::new, |(pack, required, version)| {
            min_version_results(pack, required, version)
        });
        self.write_profile_packs(project, profile, results);
    }

    /// The answer of `POST /v1/clusterprofiles/{uid}/validate/packs` for a move that a pack does
    /// not allow on the clusters of `refusal`, which run the version `profile`. Palette gives
    /// `DependencyPackUpgradeNotSupported` under `packs.results`, and names the clusters in the
    /// message. With no refusal, Palette accepts. The text of the message is the text of a live run
    /// for one cluster. The join of several names is the stand-in's, and no run shows it. The
    /// stand-in gives one answer for every request.
    pub fn profile_upgrade_answer(
        &self,
        project: &str,
        profile: &str,
        refusal: Option<UpgradeRefusal>,
    ) {
        self.write_profile_packs(project, Some(profile), upgrade_results(refusal));
    }

    /// Write the answer of the packs route of a cluster: the profile `profile` with `results`
    /// under `packs`, and no profile when `results` is empty.
    fn write_cluster_packs(
        &self,
        project: &str,
        cluster: &str,
        profile: &str,
        results: Vec<Value>,
    ) {
        let path = format!("{}/validate/packs", cluster_route(cluster));
        self.write(project, &path, &cluster_packs_body(profile, results));
    }

    /// Write the answer of the packs route of a profile with `results` under `packs`.
    fn write_profile_packs(&self, project: &str, profile: Option<&str>, results: Vec<Value>) {
        let path = match profile {
            Some(uid) => format!("{}/validate/packs", profile_route(uid)),
            None => format!("{PROFILES}/validate/packs"),
        };
        self.write(project, &path, &json!({ "packs": { "results": results } }));
    }

    /// The answers of the two routes that check a new profile version before its create. The
    /// validate route of the packs does not do these checks. The routes are `GET
    /// /v1/clusterprofiles/validate/name` for `name` and `version`, and `POST
    /// /v1/clusterprofiles/import/validate`. Palette answers the name route with `204` and no
    /// body, and the import route with the profile that it takes. A refusal is `(status, code,
    /// message)`, in the body that Palette gives. The stand-in gives one answer for every
    /// request; it does not check anything.
    pub fn profile_create_checks(
        &self,
        project: &str,
        (name, version): (&str, &str),
        name_refusal: Option<(u16, &str, &str)>,
        import_refusal: Option<(u16, &str, &str)>,
    ) {
        let name_route = format!("{PROFILES}/validate/name@name={name}&version={version}");
        let import_route = format!("{PROFILES}/import/validate");
        let taken = json!({"metadata": {"name": name}, "spec": {"version": version}});
        for (path, refusal, body) in [
            (name_route.as_str(), name_refusal, None),
            (import_route.as_str(), import_refusal, Some(taken)),
        ] {
            let file = self.doc(project, path);
            for status in std::iter::once(None).chain(crate::palette::REFUSALS.map(Some)) {
                let old = status.map_or(file.clone(), |s| crate::palette::refused(&file, s));
                if old.is_file() {
                    std::fs::remove_file(&old).unwrap_or_else(|e| panic!("{}: {e}", old.display()));
                }
            }
            let (target, text) = match refusal {
                Some((status, code, message)) => (
                    crate::palette::refused(&file, status),
                    json!({"code": code, "message": message}).to_string() + "\n",
                ),
                None => (file, body.map_or(String::new(), |b| b.to_string() + "\n")),
            };
            std::fs::write(&target, text).unwrap_or_else(|e| panic!("{}: {e}", target.display()));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::palette::refused;
    use crate::tenant::routes::{CLUSTER_LISTING, TAG_FILTER_LISTING, rbacs_route};
    use strata::api::codes::OPERATION_FORBIDDEN_CODE;

    #[test]
    fn a_read_without_cluster_get_gets_http_500_with_the_code_operation_forbidden() {
        let dir = tempfile::tempdir().unwrap();
        let tenant = Tenant::new(dir.path(), std::path::Path::new("/nowhere"));
        let route = rbacs_route("c1");
        tenant.put("p1", &route, &json!({"items": []}));
        tenant.forbid("p1", &route);
        let answer = tenant.doc("p1", &route);
        assert!(
            !answer.exists(),
            "the refusal takes the place of the answer"
        );
        assert!(!refused(&answer, 403).exists(), "Palette answers no 403");
        let body: Value =
            serde_json::from_slice(&std::fs::read(refused(&answer, 500)).unwrap()).unwrap();
        assert_eq!(body["code"], OPERATION_FORBIDDEN_CODE);
        assert_eq!(
            body["message"],
            "Operation 'cluster.get' is forbidden. Verify the user has 'cluster.get' permission"
        );
    }

    #[test]
    fn a_denial_of_a_route_gets_http_403_and_a_denial_in_a_project_gets_http_500() {
        let dir = tempfile::tempdir().unwrap();
        let tenant = Tenant::new(dir.path(), std::path::Path::new("/nowhere"));
        let filters = access_denied_message("filter.list", "tenant");
        tenant.deny("tenant", TAG_FILTER_LISTING, Denial::Route, &filters);
        let project = project_denied_message("dev");
        tenant.deny("p1", CLUSTER_LISTING, Denial::Project, &project);
        for (scope, route, status, code, message) in [
            (
                "tenant",
                TAG_FILTER_LISTING,
                403,
                "AccessDenied",
                filters.as_str(),
            ),
            (
                "p1",
                CLUSTER_LISTING,
                500,
                "ResourceAccessDenied",
                project.as_str(),
            ),
        ] {
            let file = refused(&tenant.doc(scope, route), status);
            let body: Value = serde_json::from_slice(&std::fs::read(&file).unwrap()).unwrap();
            assert_eq!(
                (body["code"].as_str(), body["message"].as_str()),
                (Some(code), Some(message))
            );
        }
        assert_eq!(
            filters,
            "filter.list operation is not permitted, required permissions 'filter.list' in the scope(s) tenant"
        );
        assert_eq!(project, "Project 'dev' access denied");
    }

    #[test]
    fn the_upgrade_refusal_has_the_message_of_a_live_run_for_one_cluster() {
        let refusal = |clusters| UpgradeRefusal {
            pack: "kubernetes",
            to: "1.37.0",
            clusters,
        };
        let results = upgrade_results(Some(refusal(&["fred"])));
        assert_eq!(results.len(), 1);
        assert_eq!(results[0]["name"], "kubernetes");
        let error = &results[0]["errors"][0];
        assert_eq!(error["code"], "DependencyPackUpgradeNotSupported");
        assert_eq!(
            error["message"],
            "Pack upgrade to '1.37.0' is not compatible with the clusters 'fred'"
        );
        assert!(upgrade_results(None).is_empty());
    }

    #[test]
    fn the_upgrade_refusal_joins_the_names_of_several_clusters() {
        let refusal = UpgradeRefusal {
            pack: "csi-aws-ebs",
            to: "1.65.0",
            clusters: &["payments", "billing"],
        };
        let results = upgrade_results(Some(refusal));
        let message = results[0]["errors"][0]["message"].as_str().unwrap();
        assert!(message.ends_with("'payments, billing'"), "{message}");
    }

    #[test]
    fn the_reason_of_a_repave_for_a_kubeadm_command_is_the_same_in_the_answer_and_in_the_status() {
        let answer = repave_answer_of(true);
        let reason = &answer["reasons"][0];
        assert_eq!(reason["code"], "PackValuesUpdated");
        assert_eq!(reason["message"], "Pack 'kubernetes' values are updated");
        assert_eq!(
            reason["pack"],
            json!({"current": {"name": "kubernetes"},
                "diffConfigKeys": ["kubeadmconfig.postKubeadmCommands"]})
        );
        assert_eq!(repave_answer_of(false)["reasons"], json!([]));
        let sides = json!({"current": {"name": "kubernetes"}, "target": {"name": "kubernetes"}});
        assert_eq!(values_updated(sides)["code"], reason["code"]);
    }

    #[test]
    fn the_min_version_refusal_names_the_pack_and_the_minimum_version_in_the_message() {
        let results = min_version_results("cni-cilium-oss", "kubernetes", "1.37.0");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0]["name"], "cni-cilium-oss");
        let error = &results[0]["errors"][0];
        assert_eq!(error["code"], "DependencyPackWithMinVersionRequired");
        assert_eq!(
            error["message"],
            "Pack 'kubernetes' is required with minimum compatible version '1.37.0'"
        );
    }

    #[test]
    fn the_answer_for_a_cluster_holds_the_profile_with_its_results_and_no_profile_without_them() {
        let results = min_version_results("cni-cilium-oss", "kubernetes", "1.37.0");
        let body = cluster_packs_body("profile-1", results.clone());
        assert_eq!(body["profiles"].as_array().map(Vec::len), Some(1));
        assert_eq!(body["profiles"][0]["uid"], "profile-1");
        assert_eq!(body["profiles"][0]["packs"]["results"], json!(results));
        assert_eq!(
            cluster_packs_body("profile-1", Vec::new()),
            json!({"profiles": []})
        );
    }
}
