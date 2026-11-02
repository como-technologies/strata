//! The pack registries of the tenant, the charts of a private registry, and the versions of a pack.

use std::path::{Path, PathBuf};

use serde_json::{Value, json};
use strata::reference::{LATEST, version};

use super::Tenant;
use super::routes::{
    REGISTRY_LISTING, manifests_route, pack_entity_route, pack_listing, profile_route,
    registry_summary_route, variables_route,
};

/// The uid of the Public Repo pack registry, as Palette gives it.
pub(super) const PUBLIC_REPO: &str = "5eecc89d0b150045ae661cef";
/// The uid of the OCI pack registry that the live run of 20260917T150025Z found.
pub(super) const PALETTE_REGISTRY: &str = "64ec000000000000000000a1";
pub(super) const OCI_CSI_PACK: &str = "64ec000000000000000000b1";
/// The uid of a private Helm registry, which Palette does not sync.
pub(super) const PRIVATE_CHARTS: &str = "64ed000000000000000000a1";
/// The chart that a profile pins from it.
pub(super) const CHART: &str = "podinfo";
pub(super) const OCI_CSI_DIGEST: &str =
    "sha256:a746e14689085b14531566f9c9cfb2bc7716e96a394547ef50a1c52eab510c9e";

impl Tenant {
    /// A Helm registry that is private, `charts-auth`. Palette does not sync it, so the pack
    /// listing has no chart of it until a profile pins one.
    pub fn private_charts(&self, project: &str) {
        let path = REGISTRY_LISTING;
        let mut listing = self.read(project, path);
        if let Some(items) = listing["items"].as_array_mut() {
            items.push(
                json!({"name": "charts-auth", "uid": PRIVATE_CHARTS, "kind": "helm",
                "isPrivate": true}),
            );
        }
        self.write(project, path, &listing);
        self.write(
            project,
            &pack_listing(CHART),
            &json!({"items": [], "listmeta": {"count": 0}}),
        );
    }

    /// Create and publish the add-on profile `charts-probe`, which pins the chart of
    /// [`Tenant::private_charts`] at `version`. Palette makes the pack at the create, with the uid
    /// `pack`, the layer addon, the tag as the version, and no digest. The listing then has it.
    pub fn chart_profile(&self, project: &str, uid: &str, pack: &str, version: &str) {
        let values = "replicaCount: 2\n";
        self.write(
            project,
            &profile_route(uid),
            &json!({
                "apiVersion": "v1", "kind": "ClusterProfile",
                "metadata": {"name": "charts-probe", "uid": uid},
                "spec": {"version": "1.0.0", "published": {"cloudType": "all", "type": "add-on",
                    "packs": [{"name": CHART, "layer": "addon", "type": "helm", "tag": version,
                        "version": version, "packUid": pack, "registryUid": PRIVATE_CHARTS,
                        "values": values, "presets": [], "schema": []}]}},
                "status": {"isPublished": true, "inUseClusterUids": [], "inUseClusters": []}
            }),
        );
        self.write(
            project,
            &manifests_route(uid),
            &json!({
                "metadata": {"name": "charts-probe", "uid": uid},
                "spec": {"packs": [{"metadata": {"name": CHART, "uid": pack},
                    "spec": {"name": CHART, "layer": "addon", "version": version,
                        "registryUid": PRIVATE_CHARTS, "digest": "", "manifests": []},
                    "status": {}}]}
            }),
        );
        self.write(project, &variables_route(uid), &json!({"variables": []}));
        let made = json!({"metadata": {"uid": pack, "name": CHART},
            "spec": {"name": CHART, "version": version, "registryUid": PRIVATE_CHARTS,
                "layer": "addon", "type": "helm", "digest": ""}});
        self.write(
            project,
            &pack_listing(CHART),
            &json!({"items": [made], "listmeta": {"count": 1}}),
        );
        self.write(
            project,
            &pack_entity_route(CHART, PRIVATE_CHARTS),
            &json!({"name": CHART, "layer": "addon",
                "tags": [{"tag": version, "version": version, "packUid": pack}],
                "packValues": [{"packUid": pack, "values": ""}]}),
        );
        self.listings();
    }

    /// The Public Repo registry and the versions of each pack in `examples/packs/`, as the
    /// registry lookups of a candidate read them.
    pub fn registries(&self, project: &str) {
        self.write(
            project,
            REGISTRY_LISTING,
            &json!({"items": [
                {"name": "Public Repo", "uid": PUBLIC_REPO, "kind": "pack"},
                {"name": "Palette Registry", "uid": PALETTE_REGISTRY, "kind": "oci"}]}),
        );
        // the last sync of each registry, as v1RegistrySyncStatus gives it in the summary listing
        // of its kind. The sync of the OCI registry stopped before its end and recovers, as the
        // Bitnami registry of the tenant gives it. Its copy is the copy of the last complete sync
        let synced = json!({"isSyncSupported": true, "status": "Completed",
            "lastRunTime": "2026-09-17T09:00:00.000Z", "lastSyncedTime": "2026-09-17T09:00:00.000Z",
            "syncRecovery": {"syncRecoveryState": "Recovered"}});
        let stopped = json!({"isSyncSupported": true, "status": "InProgress",
            "lastRunTime": "2026-09-17T09:00:00.000Z", "lastSyncedTime": "2026-08-29T00:00:11.032Z",
            "syncRecovery": {"syncRecoveryState": "Recovering", "message": "sync interrupted"}});
        for (kind, uid, sync) in [
            ("pack", PUBLIC_REPO, &synced),
            ("oci", PALETTE_REGISTRY, &stopped),
        ] {
            self.write(
                project,
                &registry_summary_route(kind),
                &json!({"items": [{"metadata": {"uid": uid}, "status": {"sync": sync}}]}),
            );
        }
        let packs = self.repo.join("examples/packs");
        let mut dirs: Vec<PathBuf> = std::fs::read_dir(&packs)
            .into_iter()
            .flatten()
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.is_dir())
            .collect();
        dirs.sort();
        for dir in dirs {
            let name = dir.file_name().unwrap().to_string_lossy().into_owned();
            let summaries: Value = read_json(&dir.join("_summaries.json"));
            let count = summaries.as_array().map(Vec::len).unwrap_or(0);
            let mut listing = json!({"items": summaries, "listmeta": {"count": count}});
            let entity: Value = read_json(&dir.join("_entity.json"));
            self.write(project, &pack_entity_route(&name, PUBLIC_REPO), &entity);
            // The answer with the states query: the tags of the retired versions, which the
            // answer above does not include. A pack with no disabled or deleted version has no
            // file.
            let retired = dir.join("_entity_retired.json");
            if retired.exists() {
                self.write(
                    project,
                    &format!(
                        "{}@states=deprecated,disabled,deleted",
                        pack_entity_route(&name, PUBLIC_REPO)
                    ),
                    &read_json(&retired),
                );
            }
            // The CSI pack is in the OCI registry too, as the live run of 20260917T150025Z found.
            // A pack ref that names no registry then has to say so.
            if name == "csi-aws-ebs" {
                add_csi_to_oci(&mut listing);
                self.write(
                    project,
                    &pack_entity_route(&name, PALETTE_REGISTRY),
                    &csi_entity_in_oci(&entity),
                );
            }
            self.write(project, &pack_listing(&name), &listing);
        }
    }

    /// Take one version of a pack out of the listing of the Public Repo, as before the registry
    /// published it. `registries` restores it.
    pub fn hide_pack_version(&self, project: &str, name: &str, version: &str) {
        let path = pack_listing(name);
        let mut listing = self.read(project, &path);
        if let Some(items) = listing["items"].as_array_mut() {
            items.retain(|i| {
                !(i["spec"]["version"] == version && i["spec"]["registryUid"] == PUBLIC_REPO)
            });
            let count = items.len();
            listing["listmeta"]["count"] = json!(count);
        }
        self.write(project, &path, &listing);
    }

    /// Give one version of a pack in the listing of the Public Repo the mark `disabled: "true"`, as
    /// Spectro Cloud marks `gloo` 1.2.18. The version keeps no `system_state`, and Palette serves
    /// it. `registries` removes the mark.
    pub fn mark_pack_disabled(&self, project: &str, name: &str, version: &str) {
        let path = pack_listing(name);
        let mut listing = self.read(project, &path);
        for item in listing["items"].as_array_mut().into_iter().flatten() {
            if item["spec"]["version"] == version && item["spec"]["registryUid"] == PUBLIC_REPO {
                item["spec"]["annotations"]["disabled"] = json!("true");
            }
        }
        self.write(project, &path, &listing);
    }

    /// Publish a copy of one version of a pack in the Public Repo as the version `copy`. The copy
    /// is a rebuild when its version is `<version>-rev1`, as Spectro Cloud publishes `1.19.3-rev1`
    /// of cert-manager, or a pre-release such as `1.66.0-rc.1`. Palette orders the versions by
    /// semantic version, so a pre-release sorts below its version. The mark `latestVersion` and
    /// the floating tags stay on the base, and the copy gets only its exact tag.
    pub fn copy_pack_version(&self, project: &str, name: &str, version: &str, copy_version: &str) {
        let path = pack_listing(name);
        let mut listing = self.read(project, &path);
        let Some(items) = listing["items"].as_array_mut() else {
            return;
        };
        let Some(mut copy) = items
            .iter()
            .find(|i| i["spec"]["version"] == version && i["spec"]["registryUid"] == PUBLIC_REPO)
            .cloned()
        else {
            return;
        };
        // a pack uid of Palette is 24 hex digits: the copy_version changes the last one
        let base = copy["metadata"]["uid"].as_str().unwrap_or_default();
        let uid = format!("{}f", &base[..base.len().saturating_sub(1)]);
        copy["metadata"]["uid"] = json!(uid);
        copy["metadata"]["annotations"]["latestVersion"] = json!("false");
        copy["spec"]["version"] = json!(copy_version);
        items.push(copy);
        let count = items.len();
        listing["listmeta"]["count"] = json!(count);
        self.write(project, &path, &listing);

        let path = pack_entity_route(name, PUBLIC_REPO);
        let mut entity = self.read(project, &path);
        let above = entity["tags"]
            .as_array()
            .into_iter()
            .flatten()
            .find(|t| t["tag"] == version)
            .map(|t| t["parentTags"].clone())
            .unwrap_or(Value::Null);
        if let Some(tags) = entity["tags"].as_array_mut() {
            tags.push(
                json!({"packUid": uid, "parentTags": above, "tag": copy_version,
                             "version": copy_version}),
            );
        }
        self.write(project, &path, &entity);
    }

    /// Give one version of a pack in the listing of the Public Repo the architectures
    /// `architectures`. The annotation `architecture` of the listing names them: `amd64`, or
    /// `amd64,arm64`. The Public Repo names them on most versions. A newer version can name fewer
    /// than an older one, as `loki` 3.6.7 names `amd64` below 2.10.2 with `amd64,arm64`.
    pub fn pack_architectures(
        &self,
        project: &str,
        name: &str,
        version: &str,
        architectures: &str,
    ) {
        let path = pack_listing(name);
        let mut listing = self.read(project, &path);
        for item in listing["items"].as_array_mut().into_iter().flatten() {
            if item["spec"]["version"] == version && item["spec"]["registryUid"] == PUBLIC_REPO {
                item["spec"]["annotations"]["architecture"] = json!(architectures);
            }
        }
        self.write(project, &path, &listing);
    }

    /// Give one version of a pack in the listing of the Public Repo a template, as
    /// `spec.template` of `v1PackSummarySpec`. Palette gives each option of an input parameter
    /// with its label and dependencies, and with no presets.
    pub fn pack_template(&self, project: &str, name: &str, version: &str, template: &Value) {
        let path = pack_listing(name);
        let mut listing = self.read(project, &path);
        for item in listing["items"].as_array_mut().into_iter().flatten() {
            if item["spec"]["version"] == version && item["spec"]["registryUid"] == PUBLIC_REPO {
                item["spec"]["template"] = template.clone();
            }
        }
        self.write(project, &path, &listing);
    }
}

fn read_json(path: &Path) -> Value {
    let text = std::fs::read_to_string(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

/// The newest csi-aws-ebs version, copied into the OCI registry with its own uid and digest.
///
/// The newest is the one that `name@latest` picks. 1.9.0 is older than 1.10.0. A pre-release is
/// newest only when no item has another version.
fn add_csi_to_oci(listing: &mut Value) {
    let items = listing["items"].as_array().cloned().unwrap_or_default();
    let with_values = items.iter().filter(|i| !i["spec"]["values"].is_null());
    let newest = version::pick(
        with_values,
        |i| i["spec"]["version"].as_str().unwrap_or_default(),
        LATEST,
    );
    let Some(newest) = newest else { return };
    let mut copy = newest.clone();
    copy["metadata"]["uid"] = json!(OCI_CSI_PACK);
    copy["spec"]["registryUid"] = json!(PALETTE_REGISTRY);
    copy["spec"]["digest"] = json!(OCI_CSI_DIGEST);
    copy["spec"]["type"] = json!("oci");
    listing["items"]
        .as_array_mut()
        .expect("an array")
        .push(copy);
}

/// The OCI copy holds the newest version, under its exact tag. A tag row carries its version, as
/// run 20260918T020941Z found, and the newest is the one that `name@latest` picks.
fn csi_entity_in_oci(entity: &Value) -> Value {
    let tags = entity["tags"].as_array().cloned().unwrap_or_default();
    let exact = tags.iter().filter(|t| {
        let tag = t["tag"].as_str().unwrap_or_default();
        tag.split('.').count() == 3 && tag.split('.').all(|p| p.parse::<u64>().is_ok())
    });
    let newest = version::pick(exact, |t| t["version"].as_str().unwrap_or_default(), LATEST);
    let mut out = entity.clone();
    out["tags"] = match newest {
        Some(t) => json!([{"tag": t["tag"], "version": t["version"], "packUid": OCI_CSI_PACK}]),
        None => json!([]),
    };
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A listing item of csi-aws-ebs with `values` when `kept`, as the stand-in keeps the content
    /// of a few versions only.
    fn item(version: &str, kept: bool) -> Value {
        let values = if kept { json!("values") } else { Value::Null };
        json!({"metadata": {"uid": version}, "spec": {"version": version, "values": values}})
    }

    /// The version of the copy that `add_csi_to_oci` adds to a listing of `versions`.
    fn oci_copy(versions: &[&str]) -> Option<String> {
        let mut listing =
            json!({"items": versions.iter().map(|v| item(v, true)).collect::<Vec<_>>()});
        add_csi_to_oci(&mut listing);
        let items = listing["items"].as_array().expect("an array");
        let copy = items.get(versions.len())?;
        assert_eq!(copy["metadata"]["uid"], OCI_CSI_PACK);
        Some(copy["spec"]["version"].as_str()?.to_string())
    }

    /// The version that `csi_entity_in_oci` keeps from tags of `(tag, version)`.
    fn oci_tag(tags: &[(&str, &str)]) -> Vec<Value> {
        let rows: Vec<Value> = tags
            .iter()
            .map(|(tag, version)| json!({"tag": tag, "version": version}))
            .collect();
        let out = csi_entity_in_oci(&json!({"tags": rows}));
        out["tags"].as_array().cloned().unwrap_or_default()
    }

    #[test]
    fn a_version_is_newest_by_number_and_not_by_text() {
        assert_eq!(
            oci_copy(&["1.9.0", "1.63.1", "1.10.0"]).as_deref(),
            Some("1.63.1")
        );
        assert_eq!(oci_copy(&["1.9.0", "1.10.0"]).as_deref(), Some("1.10.0"));
    }

    #[test]
    fn a_pre_release_is_newest_only_when_no_other_version_exists() {
        assert_eq!(
            oci_copy(&["1.65.0", "1.66.0-rc.1", "1.64.0"]).as_deref(),
            Some("1.65.0")
        );
        assert_eq!(
            oci_copy(&["1.66.0-rc.1", "1.66.0-rc.2"]).as_deref(),
            Some("1.66.0-rc.2")
        );
    }

    #[test]
    fn a_version_with_a_prefix_is_ordered_by_its_numbers() {
        assert_eq!(oci_copy(&["1.65.0", "v1.66.0"]).as_deref(), Some("v1.66.0"));
    }

    #[test]
    fn a_version_without_content_gets_no_copy() {
        let mut listing = json!({"items": [item("1.65.0", false)]});
        add_csi_to_oci(&mut listing);
        assert_eq!(listing["items"].as_array().map(Vec::len), Some(1));
    }

    #[test]
    fn the_oci_tag_is_the_newest_exact_tag() {
        let kept = oci_tag(&[
            ("1.x", "1.66.0"),
            ("1.9.0", "1.9.0"),
            ("1.10.0", "1.10.0"),
            ("1.66.0-rc.1", "1.66.0-rc.1"),
        ]);
        assert_eq!(kept.len(), 1);
        assert_eq!(kept[0]["tag"], "1.10.0");
        assert_eq!(kept[0]["packUid"], OCI_CSI_PACK);
        assert!(oci_tag(&[("1.x", "1.66.0")]).is_empty());
    }
}
