//! The profile versions of the tenant: the publish of an example, the edits in place, and the
//! drafts.

use serde_json::{Value, json};
use strata::api::codes::PROFILE_EXISTS_CODE;
use strata::closed_set::ClosedSet;
use strata::model::scope::ProfileScope;

use super::answers::profile_exists_message;
use super::edit::{Edit, apply_edits};
use super::routes::{
    PROFILES, REGISTRY_LISTING, audit_log_file, empty_audit_log, export_file, manifest_route,
    manifests_route, profile_route, variables_route,
};
use super::{TENANT, Tenant};

impl Tenant {
    /// Publish a copy of the example profile `example`, such as `aws-stack@1.1.1`, in `project`
    /// under `uid` and `version`, with each edit applied.
    pub fn profile(&self, project: &str, uid: &str, version: &str, example: &str, edits: &[Edit]) {
        self.profile_of_scope(project, ProfileScope::Project, uid, version, example, edits);
    }

    /// Publish a copy of the example profile `example` at tenant scope, under `uid` and
    /// `version`. Palette lists a profile of the tenant in each project, beside the profiles of
    /// the project. It serves its document there too, so `listings` gives each project a copy.
    pub fn tenant_profile(&self, uid: &str, version: &str, example: &str, edits: &[Edit]) {
        self.profile_of_scope(TENANT, ProfileScope::Tenant, uid, version, example, edits);
    }

    /// Publish a copy of `example` in the directory `project`, with the scope `scope` in
    /// `metadata.annotations.scope`, as Palette gives it.
    fn profile_of_scope(
        &self,
        project: &str,
        scope: ProfileScope,
        uid: &str,
        version: &str,
        example: &str,
        edits: &[Edit],
    ) {
        let from = self.repo.join("examples/profiles/raw").join(example);
        let text = std::fs::read_to_string(from.join("profile.json"))
            .unwrap_or_else(|e| panic!("{}: {e}", from.display()));
        let mut doc: Value = serde_json::from_str(&text).expect("an example profile is JSON");
        doc["metadata"]["uid"] = json!(uid);
        doc["metadata"]["annotations"]["scope"] = json!(scope.as_str());
        doc["spec"]["version"] = json!(version);
        doc["status"]["inUseClusterUids"] = json!([]);
        doc["status"]["inUseClusters"] = json!([]);
        let text = std::fs::read_to_string(from.join("manifests.json")).unwrap_or_default();
        let mut manifests: Value = serde_json::from_str(&text).unwrap_or_else(|_| json!({}));
        manifests["metadata"]["uid"] = json!(uid);
        let listing = |name: &str| self.listing_of(project, name);
        apply_edits(edits, &mut doc, &mut manifests, &listing);
        self.write(project, &profile_route(uid), &doc);
        self.write(project, &manifests_route(uid), &manifests);

        let text = std::fs::read_to_string(from.join("variables.json")).unwrap_or_default();
        let variables: Value = serde_json::from_str(&text).unwrap_or_else(|_| json!({}));
        self.write(project, &variables_route(uid), &variables);
        self.resolve_profile_macros(project, uid);
        self.listings();
    }

    /// The export of the published profile `uid`, as `GET /v1/clusterprofiles/{uid}/export` gives
    /// it with `format=json` to a request that accepts a file. A request that accepts JSON gets
    /// `{"Reader":{}}`: see `palette.rs`.
    ///
    /// The export holds the name, the description, the labels, the version, the type, and the
    /// cloud. It also holds each pack with its values, its manifests, and the metadata of its
    /// registry, and the variables. Palette omits values that are empty and a pack with no
    /// manifest, and gives a manifest pack an empty registry. The export holds no params and no
    /// pack uid.
    pub fn export(&self, project: &str, uid: &str) {
        let doc = self.read(project, &profile_route(uid));
        let manifests = self.read(project, &manifests_route(uid));
        let variables = self.read(project, &variables_route(uid));
        let registries = self.read(project, REGISTRY_LISTING);
        let registry = |registry_uid: &Value| {
            let row = registries["items"]
                .as_array()
                .into_iter()
                .flatten()
                .find(|r| r["uid"] == *registry_uid);
            match row {
                Some(r) => json!({"uid": r["uid"], "name": r["name"], "kind": r["kind"],
                    "isPrivate": false, "providerType": "", "isSyncSupported": true}),
                None => json!({"uid": "", "name": "", "kind": "", "isPrivate": false,
                    "providerType": "", "isSyncSupported": false}),
            }
        };
        let published = &doc["spec"]["published"];
        let packs: Vec<Value> = published["packs"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|p| {
                let mut pack = json!({"name": p["name"], "type": p["type"], "layer": p["layer"],
                    "version": p["version"], "tag": p["tag"],
                    "registry": {"metadata": registry(&p["registryUid"])}});
                if p["values"].as_str().is_some_and(|v| !v.is_empty()) {
                    pack["values"] = p["values"].clone();
                }
                let content: Vec<Value> = manifests["spec"]["packs"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter(|m| m["metadata"]["name"] == p["name"])
                    .flat_map(|m| {
                        m["spec"]["manifests"]
                            .as_array()
                            .cloned()
                            .unwrap_or_default()
                    })
                    .map(|m| json!({"name": m["name"], "content": m["content"]}))
                    .collect();
                if !content.is_empty() {
                    pack["manifests"] = json!(content);
                }
                pack
            })
            .collect();
        let export = json!({
            "metadata": {"name": doc["metadata"]["name"],
                "description": doc["metadata"]["annotations"]["description"].as_str().unwrap_or(""),
                "labels": doc["metadata"]["labels"]},
            "spec": {"version": doc["spec"]["version"],
                "template": {"type": published["type"], "cloudType": published["cloudType"],
                    "packs": packs},
                "variables": variables["variables"]},
        });
        self.write(project, &export_file(uid), &export);
    }

    /// Publish `platform-addons`, an add-on profile with one manifest pack. Palette gives the
    /// manifest pack of each profile a new pack uid, so two copies of one manifest differ.
    pub fn addons(&self, project: &str, uid: &str, pack: &str, label: &str, version: &str) {
        self.addons_named(project, uid, "platform-addons", pack, label, version);
    }

    /// [`Tenant::addons`] under another profile name.
    pub fn addons_named(
        &self,
        project: &str,
        uid: &str,
        name: &str,
        pack: &str,
        label: &str,
        version: &str,
    ) {
        let content = format!(
            "apiVersion: v1\nkind: Namespace\nmetadata:\n  name: payments\n  labels:\n    \
             pod-security.kubernetes.io/enforce: {label}"
        );
        let profile = (uid, name, version);
        self.manifest_addon(project, profile, ("namespace-policy", pack), &content);
    }

    /// Publish an add-on profile with one manifest pack. `profile` is the uid, the name, and the
    /// version of the profile. `pack` is the name and the uid of the pack, whose one manifest has
    /// the name of the pack and holds `content`.
    pub fn manifest_addon(
        &self,
        project: &str,
        profile: (&str, &str, &str),
        pack: (&str, &str),
        content: &str,
    ) {
        let ((uid, name, version), (pack, pack_uid)) = (profile, pack);
        self.write(
            project,
            &profile_route(uid),
            &json!({
                "apiVersion": "v1", "kind": "ClusterProfile",
                "metadata": {"name": name, "uid": uid, "annotations": {"scope": ProfileScope::Project.as_str()}},
                "spec": {"version": version, "published": {"cloudType": "all", "type": "add-on",
                    "packs": [{"name": pack, "layer": "addon", "type": "manifest",
                        "tag": "1.0.0", "version": "1.0.0", "packUid": pack_uid,
                        "manifests": [{"kind": "manifest", "name": pack}],
                        "presets": [], "schema": []}]}},
                "status": {"isPublished": true, "inUseClusterUids": [], "inUseClusters": []}
            }),
        );
        self.write(
            project,
            &manifests_route(uid),
            &json!({
                "metadata": {"name": name, "uid": uid, "annotations": {"scope": ProfileScope::Project.as_str()}},
                "spec": {"packs": [{"metadata": {"name": pack, "uid": pack_uid},
                    "spec": {"name": pack, "layer": "addon", "version": "1.0.0",
                        "manifests": [{"name": pack, "content": content}]},
                    "status": {}}]}
            }),
        );
        self.write(project, &variables_route(uid), &json!({"variables": []}));
        self.listings();
    }

    /// Change a published profile in place, as an edit through the API does, and log the write
    /// in the audit log, as Palette does. The document takes the time of the write in
    /// `metadata.lastModifiedTimestamp`. The pack summaries of `packs/manifests` follow the pins.
    pub fn edit(&self, project: &str, uid: &str, edits: &[Edit]) {
        let (path, listed) = (profile_route(uid), manifests_route(uid));
        let mut doc = self.read(project, &path);
        let mut manifests = self.read(project, &listed);
        let listing = |name: &str| self.listing_of(project, name);
        apply_edits(edits, &mut doc, &mut manifests, &listing);
        let at = self.log_write(project, "clusterprofile", uid, &doc);
        doc["metadata"]["lastModifiedTimestamp"] = json!(at);
        self.write(project, &path, &doc);
        self.write(project, &listed, &manifests);
        self.notify_clusters(project, uid, &doc);
        self.listings();
    }

    /// Edit a published profile with no publish, as a PUT of `/v1/clusterprofiles/{uid}` does.
    /// The edits go to `spec.draft`, which starts as a copy of the published template, and
    /// `spec.published` stays. Palette gives no `status.hasDraft`. `packs/manifests` then lists
    /// the packs of the draft, with their values, and the published content of each manifest.
    /// Each manifest with a uid answers its own route, with its draft content and its published
    /// content. An edit of values leaves the draft content of a manifest empty.
    ///
    /// A new name and a new version do not wait for the publish. The PUT writes the name and the
    /// version of its body to the profile at once, and `packs/manifests` has the metadata of the
    /// profile.
    ///
    /// # Panics
    ///
    /// When the name and the version of the draft are those of another profile: see
    /// [`Tenant::refuse_a_taken_name_and_version`].
    pub fn draft(&self, project: &str, uid: &str, edits: &[Edit]) {
        let (path, listed) = (profile_route(uid), manifests_route(uid));
        let mut doc = self.read(project, &path);
        let published = doc["spec"]["published"].clone();
        let mut manifests = self.read(project, &listed);
        let listing = |name: &str| self.listing_of(project, name);
        apply_edits(edits, &mut doc, &mut manifests, &listing);
        // the edits of the template go to the draft, and the published template stays. The name
        // and the version are not in the template, so they change on the profile at once.
        doc["spec"]["draft"] = std::mem::replace(&mut doc["spec"]["published"], published);
        self.refuse_a_taken_name_and_version(project, &doc);
        let at = self.log_write(project, "clusterprofile", uid, &doc);
        doc["metadata"]["lastModifiedTimestamp"] = json!(at);
        self.write(project, &path, &doc);
        self.write(project, &listed, &manifests);
        for pack in manifests["spec"]["packs"].as_array().into_iter().flatten() {
            for m in pack["spec"]["manifests"].as_array().into_iter().flatten() {
                let Some(manifest) = m["uid"].as_str().filter(|u| !u.is_empty()) else {
                    continue;
                };
                self.write(
                    project,
                    &manifest_route(
                        uid,
                        pack["spec"]["name"].as_str().unwrap_or_default(),
                        manifest,
                    ),
                    &json!({"metadata": {"name": m["name"], "uid": manifest},
                        "spec": {"published": {"content": m["content"], "digest": m["digest"]},
                            "draft": {"content": "", "digest": ""}}}),
                );
            }
        }
        self.listings();
    }

    /// Refuse the profile `doc` when another profile of its scope in `project` has its name and
    /// its version, as a PUT of the profile does.
    ///
    /// Palette answers such a PUT with HTTP 409 and [`PROFILE_EXISTS_CODE`]. It ignores the case of
    /// the names, and compares the versions as text. The profile itself does not count. A profile
    /// of a project meets only the profiles of the project, and a profile of the tenant meets only
    /// the profiles of the tenant. So no chapter makes a state that Palette refuses.
    ///
    /// # Panics
    ///
    /// On such a name and version, with the code and the message of Palette.
    fn refuse_a_taken_name_and_version(&self, project: &str, doc: &Value) {
        let (name, version) = (&doc["metadata"]["name"], &doc["spec"]["version"]);
        let lowercase = |name: &Value| name.as_str().map(str::to_lowercase);
        let taken = self.documents_in(project, PROFILES).iter().any(|other| {
            other["metadata"]["uid"] != doc["metadata"]["uid"]
                && other["metadata"]["annotations"]["scope"]
                    == doc["metadata"]["annotations"]["scope"]
                && other["spec"]["version"] == *version
                && lowercase(&other["metadata"]["name"]) == lowercase(name)
        });
        assert!(
            !taken,
            "{PROFILE_EXISTS_CODE}: {}",
            profile_exists_message(
                name.as_str().unwrap_or_default(),
                version.as_str().unwrap_or_default()
            )
        );
    }

    /// Make profile `uid` a version that nobody published, as a create with no publish leaves
    /// it. The packs are in `spec.draft`, `spec.published` holds no pack, and
    /// `status.isPublished` is false. The profile listing still names the version.
    pub fn unpublish(&self, project: &str, uid: &str) {
        let path = profile_route(uid);
        let mut doc = self.read(project, &path);
        doc["spec"]["draft"] = doc["spec"]["published"].clone();
        doc["spec"]["published"] = json!({"packs": []});
        doc["status"]["isPublished"] = json!(false);
        self.write(project, &path, &doc);
        self.listings();
    }

    /// Publish the draft of a profile, as `PATCH /v1/clusterprofiles/{uid}/publish` does: the
    /// draft becomes the published template, and the draft is empty again. The version keeps its
    /// uid and its number, so each cluster that runs it learns of an update, as after an edit in
    /// place.
    pub fn publish(&self, project: &str, uid: &str) {
        let path = profile_route(uid);
        let mut doc = self.read(project, &path);
        doc["spec"]["published"] = doc["spec"]["draft"].clone();
        doc["spec"]["draft"] = json!({"packs": []});
        doc["status"]["isPublished"] = json!(true);
        let at = self.log_write(project, "clusterprofile", uid, &doc);
        doc["metadata"]["lastModifiedTimestamp"] = json!(at);
        self.write(project, &path, &doc);
        self.notify_clusters(project, uid, &doc);
        self.listings();
    }

    /// Add one entry to the audit log of a resource: an update by the person of the examples.
    /// The server omits the time from the query, so one file holds the log of the resource,
    /// newest first, as Palette lists it. Each entry is one minute after the one before it. It
    /// gives the time of the entry.
    fn log_write(&self, project: &str, kind: &str, uid: &str, doc: &Value) -> String {
        let path = audit_log_file(kind, uid);
        let mut log = if self.doc(project, &path).is_file() {
            self.read(project, &path)
        } else {
            empty_audit_log()
        };
        let items = log["items"].as_array_mut().expect("an audit log has items");
        let n = items.len();
        let name = doc["metadata"]["name"].as_str().unwrap_or_default();
        let at = format!("2026-09-18T14:{:02}:11.482Z", 2 + n);
        items.insert(
            0,
            json!({
                "metadata": {
                    "uid": format!("64fa{:020x}", n + 1),
                    "creationTimestamp": at,
                },
                "spec": {
                    "actionMsg": format!("John Doe updated {name} Profile"),
                    "actionType": "update",
                    "actor": {
                        "actorType": "user",
                        "user": {
                            "emailId": "john.doe@example.com",
                            "firstName": "John",
                            "lastName": "Doe",
                            "uid": "64fb00000000000000000001",
                        },
                    },
                    "resource": {"kind": kind, "label": "Profile", "name": name, "uid": uid},
                },
            }),
        );
        self.write(project, &path, &log);
        at
    }
}

/// The template of a profile on a cluster, from the profile document: the published profile.
///
/// The stand-in keeps its own builder, and does not use `cluster_template` of the tests. The
/// contract run holds the stand-in to Palette, and a helper of the tests has no such check.
pub(super) fn template_of(d: &Value) -> Value {
    json!({
        "uid": d["metadata"]["uid"], "name": d["metadata"]["name"],
        "profileVersion": d["spec"]["version"],
        "type": d["spec"]["published"]["type"],
        "cloudType": d["spec"]["published"]["cloudType"],
        "packs": d["spec"]["published"]["packs"],
    })
}

#[cfg(test)]
mod tests {
    use std::panic::{AssertUnwindSafe, catch_unwind};
    use std::path::Path;

    use super::*;

    /// The uid of the profile that each test drafts.
    const DRAFTED: &str = "64ea00000000000000000a01";

    /// Put the published profile `uid` with the name `name` and the version `version` in the
    /// project `dev`, with one pin. Also its answer of `packs/manifests`. Gives the published
    /// template.
    fn published_profile(tenant: &Tenant, uid: &str, name: &str, version: &str) -> Value {
        let metadata = json!({"uid": uid, "name": name,
            "annotations": {"scope": ProfileScope::Project.as_str()}});
        let published = json!({"cloudType": "aws", "type": "cluster",
            "packs": [{"name": "csi-aws-ebs", "version": "1.65.0", "values": "a: 1\n"}]});
        let doc =
            json!({"metadata": metadata, "spec": {"version": version, "published": published}});
        let manifests = json!({"metadata": metadata, "spec": {"packs": [
            {"metadata": {"name": "csi-aws-ebs"},
                "spec": {"name": "csi-aws-ebs", "version": "1.65.0", "values": "a: 1\n"}}]}});
        tenant.put("dev", &profile_route(uid), &doc);
        tenant.put("dev", &manifests_route(uid), &manifests);
        published
    }

    /// A PUT of the profile writes the name of its body to the profile at once. Only the
    /// template waits in the draft, and `packs/manifests` has the metadata of the profile.
    #[test]
    fn a_draft_with_a_new_name_gives_the_profile_and_its_answer_the_new_name_at_once() {
        let dir = tempfile::tempdir().unwrap();
        let tenant = Tenant::new(dir.path(), Path::new("/nowhere"));
        let published = published_profile(&tenant, DRAFTED, "aws-stack", "1.1.1");
        tenant.draft("dev", DRAFTED, &[Edit::Name("aws-stack-2")]);

        let drafted = tenant.read("dev", &profile_route(DRAFTED));
        let answer = tenant.read("dev", &manifests_route(DRAFTED));
        assert_eq!(drafted["metadata"]["name"], "aws-stack-2");
        assert_eq!(answer["metadata"]["name"], "aws-stack-2");
        assert_eq!(drafted["spec"]["draft"], published);
        assert_eq!(drafted["spec"]["published"], published);
    }

    /// A PUT of the profile writes the version of its body to the profile at once, as it does
    /// the name. Only the template waits in the draft.
    #[test]
    fn a_draft_with_a_new_version_gives_the_profile_the_new_version_at_once() {
        let dir = tempfile::tempdir().unwrap();
        let tenant = Tenant::new(dir.path(), Path::new("/nowhere"));
        let published = published_profile(&tenant, DRAFTED, "aws-stack", "1.1.1");
        tenant.draft("dev", DRAFTED, &[Edit::Version("1.2.0")]);

        let drafted = tenant.read("dev", &profile_route(DRAFTED));
        assert_eq!(drafted["spec"]["version"], "1.2.0");
        assert_eq!(drafted["metadata"]["name"], "aws-stack");
        assert_eq!(drafted["spec"]["draft"], published);
        assert_eq!(drafted["spec"]["published"], published);
    }

    /// The message of the panic of a draft of [`DRAFTED`] with `edits`. `None` when the draft
    /// takes the edits.
    fn refusal_of(tenant: &Tenant, edits: &[Edit]) -> Option<String> {
        let refused = catch_unwind(AssertUnwindSafe(|| tenant.draft("dev", DRAFTED, edits)));
        refused.err().map(|panic| match panic.downcast::<String>() {
            Ok(message) => *message,
            Err(_) => "a panic with no message".to_string(),
        })
    }

    /// A PUT of the profile refuses a name and a version that another profile of the project has,
    /// with [`PROFILE_EXISTS_CODE`]. It ignores the case of the names. A profile of the tenant,
    /// and the profile itself, do not count.
    #[test]
    fn a_draft_with_the_name_and_the_version_of_another_profile_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let tenant = Tenant::new(dir.path(), Path::new("/nowhere"));
        let published = published_profile(&tenant, DRAFTED, "aws-stack", "1.1.1");
        published_profile(&tenant, "64ea00000000000000000a02", "aws-stack", "1.2.0");
        // the copy of a profile of the tenant, which Palette lists in each project
        let tenant_copy = "64ea00000000000000000a03";
        published_profile(&tenant, tenant_copy, "aws-stack", "1.3.0");
        let mut copy = tenant.read("dev", &profile_route(tenant_copy));
        copy["metadata"]["annotations"]["scope"] = json!(ProfileScope::Tenant.as_str());
        tenant.put("dev", &profile_route(tenant_copy), &copy);

        let refusal = format!(
            "{PROFILE_EXISTS_CODE}: {}",
            profile_exists_message("aws-stack", "1.2.0")
        );
        assert_eq!(
            refusal_of(&tenant, &[Edit::Version("1.2.0")]),
            Some(refusal)
        );
        let message = refusal_of(&tenant, &[Edit::Name("AWS-Stack"), Edit::Version("1.2.0")]);
        assert!(message.is_some_and(|m| m.starts_with(PROFILE_EXISTS_CODE)));
        // the refused draft writes nothing
        let doc = tenant.read("dev", &profile_route(DRAFTED));
        assert_eq!(doc["spec"]["version"], "1.1.1");
        assert!(doc["spec"]["draft"].is_null());

        // the copy of the tenant profile has the name and this version
        assert_eq!(refusal_of(&tenant, &[Edit::Version("1.3.0")]), None);
        // now the profile itself has them
        assert_eq!(refusal_of(&tenant, &[Edit::Version("1.3.0")]), None);
        let doc = tenant.read("dev", &profile_route(DRAFTED));
        assert_eq!(doc["spec"]["version"], "1.3.0");
        assert_eq!(doc["spec"]["draft"], published);
    }
}
