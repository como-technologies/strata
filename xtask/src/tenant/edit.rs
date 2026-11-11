//! The edits that a person makes to a published profile, as the chapters write them.

use serde_json::{Value, json};

use super::macros::user_macro;
use super::registries::{
    OCI_CSI_DIGEST, OCI_CSI_PACK, PALETTE_REGISTRY, listed_version, with_oci_csi_build,
};

/// One change to a published profile document, as a person makes it through the API.
#[derive(Clone, Debug)]
pub enum Edit {
    /// Set the CSI pack's reclaim policy, which is one value inside its values YAML.
    Reclaim(&'static str),
    /// Give a pack another tag name for the build it already pins.
    Tag {
        /// The pack whose tag changes.
        pack: &'static str,
        /// The tag name to write.
        tag: &'static str,
    },
    /// Take the CSI pack from the OCI registry, as a team that promotes a pack does.
    ///
    /// The type, the registry, the `packUid`, and the digest become those of the OCI copy of the
    /// pack. The pack summary takes the same registry, uid, and digest.
    OciCsi,
    /// Give the version another number, which a bump without the immutability flag does.
    ///
    /// In a draft, the profile also takes the new number at once, as a PUT of the profile does:
    /// see [`Tenant::draft`](super::Tenant::draft).
    Version(&'static str),
    /// Give the profile another name, as `PATCH /v1/clusterprofiles/{uid}/metadata` does: the
    /// uid and the version stay. The answer of `packs/manifests` takes the new name too.
    ///
    /// In a draft, the profile also takes the new name at once, as a PUT of the profile does: see
    /// [`Tenant::draft`](super::Tenant::draft).
    Name(&'static str),
    /// Give the profile another cloud type, such as `gcp` for a profile that the body of an AWS
    /// create names. `spec.published.cloudType` changes, and the packs stay.
    CloudType(&'static str),
    /// Give the published template a profile type that this release of strata does not know, as a
    /// later Palette release can add one. `spec.published.type` changes, and the packs stay.
    ProfileType(&'static str),
    /// Add one command to `kubeadmconfig.postKubeadmCommands` of the Kubernetes pack: a key that
    /// Palette repaves on.
    KubeadmCommand(&'static str),
    /// Move a pack to another version, as an upgrade of the pack does. The impact table calls this
    /// a repave for the OS and Kubernetes layers.
    ///
    /// The tag and the version change. The `packUid` and the digest become those of the other
    /// version in the pack listing, as Palette takes them from the record of that version: see
    /// [`listed_version`].
    Upgrade {
        /// The pack that moves.
        pack: &'static str,
        /// The other version, also its tag.
        version: &'static str,
    },
    /// Remove the digest of a pack, on its pack reference and in its pack summary, as a registry
    /// that Palette does not sync gives it.
    NoDigest(&'static str),
    /// Make one value of a pack read a project macro: a line
    /// `<macro>: "{{.spectro.macro.<macro>}}"`
    /// at the end of the pack's values.
    Macro {
        /// The pack whose values read the macro.
        pack: &'static str,
        /// The macro name, and the key of the new line.
        name: &'static str,
    },
    /// Name a machine image in the annotation `imageId` of a pack ref, as a
    /// bring-your-own-image OS pack does. The values and the pack build stay.
    ImageId {
        /// The pack that names the image.
        pack: &'static str,
        /// The id of the image, such as an AMI.
        id: &'static str,
    },
    /// Mark a pack ref invalid, as Palette does when the registry deletes the tag of the pack.
    /// `isInvalid` is true, and `inValidReason` is the message of the event of the registry. The
    /// values and the pack build stay.
    Invalid {
        /// The pack whose tag the registry deleted.
        pack: &'static str,
        /// The message of the event, `<display name> <tag> tag is deleted`.
        reason: &'static str,
    },
    /// Name one more image in the list `pack.content.images` of a pack, as the team that owns the
    /// pack does. The list is outside the root, and the values are in it.
    Image {
        /// The pack whose values hold the list.
        pack: &'static str,
        /// The reference of the image, such as `repository:tag` or `repository:tag@sha256:digest`.
        image: &'static str,
    },
    /// Give a pack a secret value: a line `<key>: '********'` at the end of the pack's values.
    /// The schema of the pack gives the key the format `password`, so Palette serves its mask in
    /// place of the secret, whatever the secret is.
    Masked {
        /// The pack whose values hold the secret.
        pack: &'static str,
        /// The key of the secret.
        key: &'static str,
    },
}

/// The pack listing of a pack name in the project of the profile, as `GET /v1/packs` gives it.
/// Null when the project has no listing of that pack.
pub(super) type Listing<'a> = &'a dyn Fn(&str) -> Value;

/// Apply each edit of `edits`, in order, to the profile document `doc` and to its pack summaries
/// `manifests`. `listing` gives the pack listing that an upgrade reads.
///
/// Palette builds the answer of `packs/manifests` from the profile and its pins at each read. So
/// each edit changes the two documents together: at a publish, at an edit in place, and in a
/// draft.
pub(super) fn apply_edits(
    edits: &[Edit],
    doc: &mut Value,
    manifests: &mut Value,
    listing: Listing,
) {
    for edit in edits {
        edit.apply(doc, listing);
        edit.apply_manifests(manifests, listing);
    }
    summary_values_from_pins(doc, manifests);
}

/// Give each pack summary of `manifests` the values of the pin of its name in the profile `doc`.
///
/// Palette takes the uid, the registry, and the digest of a summary from the record of the version
/// that the pin names. It takes the values from the pin. So the summary gets each edit of the
/// values of its pin.
fn summary_values_from_pins(doc: &Value, manifests: &mut Value) {
    for pin in doc["spec"]["published"]["packs"]
        .as_array()
        .into_iter()
        .flatten()
    {
        let (Some(name), Some(values)) = (pin["name"].as_str(), pin["values"].as_str()) else {
            continue;
        };
        for summary in summaries_named(manifests, name) {
            summary["spec"]["values"] = json!(values);
        }
    }
}

impl Edit {
    /// The part of an edit that lands in the answer of `packs/manifests`: the name of the answer,
    /// and the build of its pack summaries. `listing` gives the pack listing that an upgrade reads.
    ///
    /// Palette gives the summary of a pack from the record of the version that the profile pins:
    /// its uid, its version, its registry, and its digest. The answer has the metadata of the
    /// profile, so it takes the name of the profile.
    fn apply_manifests(&self, manifests: &mut Value, listing: Listing) {
        match self {
            Edit::Name(name) => manifests["metadata"]["name"] = json!(name),
            Edit::OciCsi => summaries_named(manifests, "csi-aws-ebs").for_each(with_oci_csi_build),
            Edit::NoDigest(name) => {
                for summary in summaries_named(manifests, name) {
                    summary["spec"]["digest"] = json!("");
                }
            }
            Edit::Upgrade { pack, version } => {
                let listing = listing(pack);
                for summary in summaries_named(manifests, pack) {
                    let registry = summary["spec"]["registryUid"].as_str().unwrap_or_default();
                    let listed = listed_version(&listing, pack, registry, version);
                    summary["metadata"]["uid"] = listed["metadata"]["uid"].clone();
                    summary["spec"]["version"] = json!(version);
                    summary["spec"]["digest"] = listed["spec"]["digest"].clone();
                }
            }
            _ => {}
        }
    }

    /// Apply the edit to the profile document `doc`. `listing` gives the pack listing that an
    /// upgrade reads.
    fn apply(&self, doc: &mut Value, listing: Listing) {
        match self {
            Edit::Reclaim(policy) => {
                for pack in packs_named(doc, "csi-aws-ebs") {
                    if let Some(values) = pack["values"].as_str() {
                        pack["values"] = json!(with_reclaim_policy(values, policy));
                    }
                }
            }
            Edit::Tag { pack, tag } => {
                for p in packs_named(doc, pack) {
                    p["tag"] = json!(tag);
                }
            }
            Edit::OciCsi => {
                for pack in packs_named(doc, "csi-aws-ebs") {
                    pack["type"] = json!("oci");
                    pack["registryUid"] = json!(PALETTE_REGISTRY);
                    pack["packUid"] = json!(OCI_CSI_PACK);
                    pack["digest"] = json!(OCI_CSI_DIGEST);
                }
            }
            Edit::Version(version) => doc["spec"]["version"] = json!(version),
            Edit::Name(name) => doc["metadata"]["name"] = json!(name),
            Edit::CloudType(cloud) => doc["spec"]["published"]["cloudType"] = json!(cloud),
            Edit::ProfileType(kind) => doc["spec"]["published"]["type"] = json!(kind),
            Edit::KubeadmCommand(command) => {
                for pack in packs_named(doc, "kubernetes") {
                    if let Some(values) = pack["values"].as_str() {
                        pack["values"] = json!(with_kubeadm_command(values, command));
                    }
                }
            }
            Edit::Upgrade { pack, version } => {
                let listing = listing(pack);
                for p in packs_named(doc, pack) {
                    let registry = p["registryUid"].as_str().unwrap_or_default();
                    let listed = listed_version(&listing, pack, registry, version);
                    p["packUid"] = listed["metadata"]["uid"].clone();
                    p["digest"] = listed["spec"]["digest"].clone();
                    p["tag"] = json!(version);
                    p["version"] = json!(version);
                }
            }
            Edit::NoDigest(name) => {
                for pack in packs_named(doc, name) {
                    pack["digest"] = json!("");
                }
            }
            Edit::ImageId { pack, id } => {
                for p in packs_named(doc, pack) {
                    p["annotations"]["imageId"] = json!(id);
                }
            }
            Edit::Invalid { pack, reason } => {
                for p in packs_named(doc, pack) {
                    p["isInvalid"] = json!(true);
                    p["inValidReason"] = json!(reason);
                }
            }
            Edit::Image { pack, image } => {
                for p in packs_named(doc, pack) {
                    if let Some(values) = p["values"].as_str() {
                        p["values"] = json!(with_image(values, image));
                    }
                }
            }
            Edit::Masked { pack, key } => {
                for p in packs_named(doc, pack) {
                    append_value_line(p, &format!("{key}: '********'"));
                }
            }
            Edit::Macro { pack, name } => {
                for p in packs_named(doc, pack) {
                    append_value_line(p, &format!("{name}: \"{}\"", user_macro(name)));
                }
            }
        }
    }
}

/// The values of the CSI pack with the reclaim policy `policy`: the line of the other policy
/// changes, and nothing else.
pub(super) fn with_reclaim_policy(values: &str, policy: &str) -> String {
    let from = if policy == "Retain" {
        "Delete"
    } else {
        "Retain"
    };
    values.replacen(
        &format!("reclaimPolicy: {from}"),
        &format!("reclaimPolicy: {policy}"),
        1,
    )
}

/// The values with the entry `- image: <image>` at the head of the list `pack.content.images`.
fn with_image(values: &str, image: &str) -> String {
    let head = "    images:\n";
    values.replacen(head, &format!("{head}      - image: {image}\n"), 1)
}

/// The values of the Kubernetes pack with the command `command` after the last command that
/// Palette repaves on.
fn with_kubeadm_command(values: &str, command: &str) -> String {
    let anchor = "    - 'chmod 600 /var/lib/kubelet/config.yaml'\n";
    values.replacen(anchor, &format!("{anchor}    - '{command}'\n"), 1)
}

/// Add the line `line` at the end of the values of the pack `pack`.
fn append_value_line(pack: &mut Value, line: &str) {
    let values = pack["values"].as_str().unwrap_or_default();
    pack["values"] = json!(format!("{}\n{line}\n", values.trim_end()));
}

/// Each pack of the published template that has the name `name`.
fn packs_named<'a>(doc: &'a mut Value, name: &'a str) -> impl Iterator<Item = &'a mut Value> {
    packs_mut(doc).filter(move |pack| pack["name"] == name)
}

/// Each pack summary of a `packs/manifests` answer that has the name `name`.
fn summaries_named<'a>(
    manifests: &'a mut Value,
    name: &'a str,
) -> impl Iterator<Item = &'a mut Value> {
    manifests["spec"]["packs"]
        .as_array_mut()
        .into_iter()
        .flatten()
        .filter(move |summary| summary["spec"]["name"] == name)
}

/// Each pack of the published template.
fn packs_mut(doc: &mut Value) -> impl Iterator<Item = &mut Value> {
    doc["spec"]["published"]["packs"]
        .as_array_mut()
        .map(|p| p.iter_mut())
        .into_iter()
        .flatten()
}

#[cfg(test)]
mod tests {
    use strata::api::{MemoryApi, Palette};
    use strata::diff::ChangeFlag;
    use strata::ops::{self, SnapshotOptions};
    use strata::reference::Ref;
    use strata::snapshot::Snapshot;
    use strata::term::progress::Progress;

    use super::super::Tenant;
    use super::super::registries::PUBLIC_REPO;
    use super::super::routes::{manifests_route, profile_route, variables_route};
    use super::*;
    use crate::variants::one_of_each;

    /// The pack listing of a project that lists no pack.
    fn no_listing(_: &str) -> Value {
        Value::Null
    }

    #[test]
    fn the_reclaim_edit_changes_the_csi_pack_alone() {
        let mut doc = json!({"spec": {"published": {"packs": [
            {"name": "csi-aws-ebs", "values": "a\nreclaimPolicy: Delete\nb"},
            {"name": "kubernetes", "values": "reclaimPolicy: Delete"}]}}});
        Edit::Reclaim("Retain").apply(&mut doc, &no_listing);
        let packs = doc["spec"]["published"]["packs"].as_array().unwrap();
        assert_eq!(packs[0]["values"], "a\nreclaimPolicy: Retain\nb");
        assert_eq!(packs[1]["values"], "reclaimPolicy: Delete");
    }

    #[test]
    fn the_oci_edit_moves_the_csi_pack_to_the_other_registry() {
        let mut doc = json!({"spec": {"published": {"packs": [{"name": "csi-aws-ebs"}]}}});
        Edit::OciCsi.apply(&mut doc, &no_listing);
        let pack = &doc["spec"]["published"]["packs"][0];
        assert_eq!(pack["type"], "oci");
        assert_eq!(pack["registryUid"], PALETTE_REGISTRY);
        assert_eq!(pack["packUid"], OCI_CSI_PACK);
    }

    /// The edits compose in order, as the chapter that writes `OCI_CSI | RETAIN` needs.
    #[test]
    fn the_edits_apply_in_order() {
        let mut doc = json!({"spec": {"version": "1.1.1", "published": {"packs": [
            {"name": "csi-aws-ebs", "values": "reclaimPolicy: Delete"}]}}});
        for edit in [
            Edit::OciCsi,
            Edit::Reclaim("Retain"),
            Edit::Version("1.1.2"),
        ] {
            edit.apply(&mut doc, &no_listing);
        }
        assert_eq!(doc["spec"]["version"], "1.1.2");
        let pack = &doc["spec"]["published"]["packs"][0];
        assert_eq!(pack["type"], "oci");
        assert_eq!(pack["values"], "reclaimPolicy: Retain");
    }

    #[test]
    fn the_secret_edit_adds_one_line_to_the_values_of_one_pack() {
        let mut doc = json!({"spec": {"published": {"packs": [
            {"name": "cert-manager", "values": "a: 1\n"},
            {"name": "kubernetes", "values": "b: 2\n"}]}}});
        Edit::Masked {
            pack: "cert-manager",
            key: "registryPassword",
        }
        .apply(&mut doc, &no_listing);
        let packs = doc["spec"]["published"]["packs"].as_array().unwrap();
        assert_eq!(packs[0]["values"], "a: 1\nregistryPassword: '********'\n");
        assert_eq!(packs[1]["values"], "b: 2\n");
    }

    #[test]
    fn the_macro_edit_adds_a_line_that_reads_the_macro() {
        let mut doc = json!({"spec": {"published": {"packs": [
            {"name": "csi-aws-ebs", "values": "a: 1\n\n"}]}}});
        Edit::Macro {
            pack: "csi-aws-ebs",
            name: "region",
        }
        .apply(&mut doc, &no_listing);
        assert_eq!(
            doc["spec"]["published"]["packs"][0]["values"],
            "a: 1\nregion: \"{{.spectro.macro.region}}\"\n"
        );
    }

    #[test]
    fn the_image_edit_adds_one_entry_to_the_image_list_of_one_pack() {
        let values = "pack:\n  content:\n    images:\n      - image: a:1\n  other: 2\n";
        let mut doc = json!({"spec": {"published": {"packs": [
            {"name": "kubernetes", "values": values},
            {"name": "cert-manager", "values": values}]}}});
        Edit::Image {
            pack: "kubernetes",
            image: "vendor.example/product:2026.2@sha256:aa",
        }
        .apply(&mut doc, &no_listing);
        let packs = doc["spec"]["published"]["packs"].as_array().unwrap();
        assert_eq!(
            packs[0]["values"],
            "pack:\n  content:\n    images:\n      - image: vendor.example/product:2026.2@sha256:aa\n      - image: a:1\n  other: 2\n"
        );
        assert_eq!(packs[1]["values"], values);
    }

    #[test]
    fn the_kubeadm_edit_adds_one_command_after_the_last_command_of_the_pack() {
        let mut doc = json!({"spec": {"published": {"packs": [
            {"name": "kubernetes",
             "values": "x:\n    - 'chmod 600 /var/lib/kubelet/config.yaml'\ny: 1\n"}]}}});
        Edit::KubeadmCommand("echo one").apply(&mut doc, &no_listing);
        assert_eq!(
            doc["spec"]["published"]["packs"][0]["values"],
            "x:\n    - 'chmod 600 /var/lib/kubelet/config.yaml'\n    - 'echo one'\ny: 1\n"
        );
    }

    #[test]
    fn the_upgrade_edit_gives_the_pin_the_uid_and_the_digest_of_the_listed_version() {
        let pin = json!({"name": "kubernetes", "packUid": "a", "tag": "1.36.2",
            "version": "1.36.2", "registryUid": "r1", "digest": "sha256:a"});
        let listing = json!({"items": [
            {"metadata": {"uid": "b"}, "spec": {"version": "1.36.3", "registryUid": "r1",
                "digest": "sha256:b"}},
            {"metadata": {"uid": "c"}, "spec": {"version": "1.36.3", "registryUid": "r2",
                "digest": "sha256:c"}}]});
        let mut doc = json!({"spec": {"published": {"packs": [pin]}}});
        let upgrade = Edit::Upgrade {
            pack: "kubernetes",
            version: "1.36.3",
        };
        upgrade.apply(&mut doc, &|_| listing.clone());
        let pack = &doc["spec"]["published"]["packs"][0];
        assert_eq!(pack["packUid"], "b");
        assert_eq!(pack["digest"], "sha256:b");
        assert_eq!(pack["tag"], "1.36.3");
        assert_eq!(pack["version"], "1.36.3");

        // a version that the project does not list has a build of its own
        let mut doc = json!({"spec": {"published": {"packs": [pin]}}});
        upgrade.apply(&mut doc, &no_listing);
        let pack = &doc["spec"]["published"]["packs"][0];
        assert_ne!(pack["packUid"], "a");
        assert_ne!(pack["digest"], "sha256:a");
    }

    /// A published profile with one pin: `csi-aws-ebs` at the version `version` of the Public
    /// Repo, with its uid and its digest in `listing`. Also the pack summary of the pin.
    fn csi_profile(uid: &str, listing: &Value, version: &str) -> (Value, Value) {
        let item = listed_version(listing, "csi-aws-ebs", PUBLIC_REPO, version);
        let (pack, digest) = (&item["metadata"]["uid"], &item["spec"]["digest"]);
        let doc = json!({"metadata": {"uid": uid, "name": "aws-stack"},
            "spec": {"version": "1.1.1", "published": {"cloudType": "aws", "type": "cluster",
                "packs": [{"name": "csi-aws-ebs", "layer": "csi", "type": "spectro",
                    "tag": version, "version": version, "packUid": pack,
                    "registryUid": PUBLIC_REPO, "digest": digest, "values": "a: 1\n"}]}}});
        let manifests = json!({"spec": {"packs": [
            {"metadata": {"name": "csi-aws-ebs", "uid": pack},
                "spec": {"name": "csi-aws-ebs", "layer": "csi", "version": version,
                    "registryUid": PUBLIC_REPO, "digest": digest, "manifests": []}}]}});
        (doc, manifests)
    }

    /// The record that `strata snapshot` writes of the profile `doc`, with the pack summaries
    /// `manifests`.
    fn record(doc: &Value, manifests: &Value) -> Snapshot {
        let uid = doc["metadata"]["uid"].as_str().expect("a uid");
        let api = MemoryApi::new()
            .with(format!("/{}", profile_route(uid)), doc.clone())
            .with(format!("/{}", manifests_route(uid)), manifests.clone())
            .with(
                format!("/{}", variables_route(uid)),
                json!({"variables": []}),
            );
        ops::snapshot_for(
            &Ref::Uid(uid.to_string()),
            &SnapshotOptions::default(),
            || Ok(Palette::new(api)),
            "2026-09-17T09:30:00Z",
            &Progress::silent(),
        )
        .expect("the record builds")
    }

    #[test]
    fn an_upgraded_pin_has_the_build_of_its_version_in_the_listing_and_compare_shows_upstream() {
        let dir = tempfile::tempdir().unwrap();
        let tenant = Tenant::new(dir.path(), &crate::repository());
        tenant.registries("dev");
        tenant.copy_pack_version("dev", "csi-aws-ebs", "1.65.0", "1.66.0-rc.1");
        let listing = |name: &str| tenant.listing_of("dev", name);
        let csi = listing("csi-aws-ebs");
        let uid = "64ea00000000000000000a01";
        let (old, old_manifests) = csi_profile(uid, &csi, "1.65.0");
        // a copy, a lower version of the listing, and a version that the listing does not hold
        for version in ["1.66.0-rc.1", "1.63.1", "1.67.0"] {
            let upgrade = Edit::Upgrade {
                pack: "csi-aws-ebs",
                version,
            };
            let (mut doc, mut manifests) = (old.clone(), old_manifests.clone());
            upgrade.apply(&mut doc, &listing);
            upgrade.apply_manifests(&mut manifests, &listing);
            let listed = listed_version(&csi, "csi-aws-ebs", PUBLIC_REPO, version);
            let build = (&listed["metadata"]["uid"], &listed["spec"]["digest"]);
            let pin = &doc["spec"]["published"]["packs"][0];
            let summary = &manifests["spec"]["packs"][0];
            assert_eq!((&pin["packUid"], &pin["digest"]), build, "{version}");
            let summary_build = (&summary["metadata"]["uid"], &summary["spec"]["digest"]);
            assert_eq!(summary_build, build, "{version}");
            assert_eq!(summary["spec"]["version"], version);

            let diff = ops::compare(&record(&old, &old_manifests), &record(&doc, &manifests), "");
            let pack = diff
                .packs
                .iter()
                .find(|p| p.name == "csi-aws-ebs")
                .expect("a csi pack");
            assert_eq!(
                pack.flags(),
                [ChangeFlag::Identity, ChangeFlag::Upstream],
                "{version}"
            );
        }
    }

    /// The pin and the pack summary of `csi-aws-ebs` after an edit in place with `edits`, on the
    /// profile of [`csi_profile`] at 1.65.0 in the project `dev`. Also the pack listing of the pack.
    fn csi_after_edit_in_place(edits: &[Edit]) -> (Value, Value, Value) {
        let dir = tempfile::tempdir().unwrap();
        let tenant = Tenant::new(dir.path(), &crate::repository());
        tenant.registries("dev");
        let csi = tenant.listing_of("dev", "csi-aws-ebs");
        let uid = "64ea00000000000000000a01";
        // `just test` runs before `just examples`, so the test publishes no example profile
        let (doc, manifests) = csi_profile(uid, &csi, "1.65.0");
        tenant.put("dev", &profile_route(uid), &doc);
        tenant.put("dev", &manifests_route(uid), &manifests);
        tenant.edit("dev", uid, edits);

        let mut doc = tenant.read("dev", &profile_route(uid));
        let pin = packs_named(&mut doc, "csi-aws-ebs")
            .next()
            .expect("a csi pin")
            .clone();
        let mut manifests = tenant.read("dev", &manifests_route(uid));
        let summary = summaries_named(&mut manifests, "csi-aws-ebs")
            .next()
            .expect("a csi summary")
            .clone();
        (pin, summary, csi)
    }

    #[test]
    fn an_upgrade_in_an_edit_in_place_moves_the_pin_and_its_pack_summary() {
        let upgrade = Edit::Upgrade {
            pack: "csi-aws-ebs",
            version: "1.63.1",
        };
        let (pin, summary, csi) = csi_after_edit_in_place(&[upgrade]);

        let listed = listed_version(&csi, "csi-aws-ebs", PUBLIC_REPO, "1.63.1");
        let build = (&listed["metadata"]["uid"], &listed["spec"]["digest"]);
        assert_eq!((&pin["packUid"], &pin["digest"]), build);
        assert_eq!(
            (&pin["tag"], &pin["version"]),
            (&json!("1.63.1"), &json!("1.63.1"))
        );
        let summary_build = (&summary["metadata"]["uid"], &summary["spec"]["digest"]);
        assert_eq!(summary_build, build);
        assert_eq!(summary["spec"]["version"], "1.63.1");
    }

    #[test]
    fn an_oci_edit_in_place_moves_the_pin_and_its_pack_summary() {
        let (pin, summary, _) = csi_after_edit_in_place(&[Edit::OciCsi]);

        let oci = (
            &json!(OCI_CSI_PACK),
            &json!(PALETTE_REGISTRY),
            &json!(OCI_CSI_DIGEST),
        );
        assert_eq!((&pin["packUid"], &pin["registryUid"], &pin["digest"]), oci);
        let summary_build = (
            &summary["metadata"]["uid"],
            &summary["spec"]["registryUid"],
            &summary["spec"]["digest"],
        );
        assert_eq!(summary_build, oci);
    }

    /// A published profile with a pin of each pack that [`each_kind_of_edit`] names, from the
    /// Public Repo. Also its answer of `packs/manifests`, with the name of the profile and a pack
    /// summary of each pin. A summary has the name, the build, and the values of its pin.
    fn pinned_profile() -> (Value, Value) {
        let kubernetes = "kubeadmconfig:\n  postKubeadmCommands:\n    \
            - 'chmod 600 /var/lib/kubelet/config.yaml'\npack:\n  content:\n    images:\n      \
            - image: a:1\n";
        let values = [
            ("csi-aws-ebs", "reclaimPolicy: Delete\n"),
            ("kubernetes", kubernetes),
            ("cert-manager", "a: 1\n"),
            ("ubuntu-aws", "b: 2\n"),
        ];
        let pins: Vec<Value> = values
            .iter()
            .enumerate()
            .map(|(n, (name, values))| {
                json!({"name": name, "tag": "1.0.0", "version": "1.0.0",
                    "packUid": format!("64ea0000000000000000000{n}"), "registryUid": PUBLIC_REPO,
                    "digest": format!("sha256:{n}"), "values": values})
            })
            .collect();
        let summaries: Vec<Value> = pins
            .iter()
            .map(|p| {
                json!({"metadata": {"name": p["name"], "uid": p["packUid"]},
                    "spec": {"name": p["name"], "version": p["version"],
                        "registryUid": p["registryUid"], "digest": p["digest"],
                        "values": p["values"], "manifests": []}})
            })
            .collect();
        let metadata = json!({"name": "aws-stack"});
        let manifests = json!({"metadata": metadata, "spec": {"packs": summaries}});
        let doc = json!({"metadata": metadata, "spec": {"version": "1.1.1",
            "published": {"cloudType": "aws", "type": "cluster", "packs": pins}}});
        (doc, manifests)
    }

    /// One edit of each kind, on the packs of [`pinned_profile`]. The CSI upgrade comes after the
    /// OCI edit, so in this order the upgrade reads the registry of the move. [`one_of_each!`]
    /// ties the list to each variant of [`Edit`].
    fn each_kind_of_edit() -> Vec<Edit> {
        one_of_each![
            Edit::Reclaim("Retain"),
            Edit::Tag {
                pack: "ubuntu-aws",
                tag: "24.04",
            },
            Edit::OciCsi,
            Edit::Version("1.1.2"),
            Edit::Name("aws-stack-2"),
            Edit::CloudType("gcp"),
            Edit::ProfileType("later-type"),
            Edit::KubeadmCommand("echo one"),
            Edit::Upgrade {
                pack: "csi-aws-ebs",
                version: "1.63.1",
            },
            Edit::NoDigest("cert-manager"),
            Edit::Macro {
                pack: "csi-aws-ebs",
                name: "region",
            },
            Edit::ImageId {
                pack: "ubuntu-aws",
                id: "ami-0a1b2c3d4e5f60001",
            },
            Edit::Invalid {
                pack: "cert-manager",
                reason: "Cert Manager 1.0.0 tag is deleted",
            },
            Edit::Image {
                pack: "kubernetes",
                image: "a:2",
            },
            Edit::Masked {
                pack: "cert-manager",
                key: "registryPassword",
            },
        ]
    }

    /// Fail when the profile `doc` and its answer `manifests` differ in the name. Also fail when a
    /// pin and its pack summary differ in the name, the build, or the values. `after` names the
    /// edits in the message.
    fn assert_summaries_follow_pins(doc: &Value, manifests: &Value, after: &str) {
        assert_eq!(
            doc["metadata"]["name"], manifests["metadata"]["name"],
            "{after}: the answer"
        );
        let summaries = manifests["spec"]["packs"].as_array().expect("summaries");
        for pin in doc["spec"]["published"]["packs"].as_array().expect("pins") {
            let summary = summaries
                .iter()
                .find(|s| s["spec"]["name"] == pin["name"])
                .expect("a summary of each pin");
            let of_pin = [
                &pin["name"],
                &pin["packUid"],
                &pin["registryUid"],
                &pin["version"],
                &pin["digest"],
                &pin["values"],
            ];
            let of_summary = [
                &summary["metadata"]["name"],
                &summary["metadata"]["uid"],
                &summary["spec"]["registryUid"],
                &summary["spec"]["version"],
                &summary["spec"]["digest"],
                &summary["spec"]["values"],
            ];
            assert_eq!(of_pin, of_summary, "{after}: {}", pin["name"]);
        }
    }

    #[test]
    fn each_edit_changes_the_pin_and_its_pack_summary_together() {
        let edits = each_kind_of_edit();
        for edit in &edits {
            let (mut doc, mut manifests) = pinned_profile();
            apply_edits(
                std::slice::from_ref(edit),
                &mut doc,
                &mut manifests,
                &no_listing,
            );
            assert_summaries_follow_pins(&doc, &manifests, &format!("{edit:?}"));
        }
        // a chapter composes edits, as `OCI_CSI | RETAIN` does
        let (mut doc, mut manifests) = pinned_profile();
        apply_edits(&edits, &mut doc, &mut manifests, &no_listing);
        assert_summaries_follow_pins(&doc, &manifests, "each edit in order");
    }

    #[test]
    fn two_unlisted_pins_of_one_version_in_two_registries_have_two_uids() {
        let upgrade = Edit::Upgrade {
            pack: "kubernetes",
            version: "1.36.3",
        };
        let [public, oci] = [PUBLIC_REPO, PALETTE_REGISTRY].map(|registry| {
            let mut doc = json!({"spec": {"published": {"packs": [{"name": "kubernetes",
                "packUid": "a", "tag": "1.36.2", "version": "1.36.2", "registryUid": registry,
                "digest": "sha256:a"}]}}});
            upgrade.apply(&mut doc, &no_listing);
            doc["spec"]["published"]["packs"][0].clone()
        });
        assert_ne!(public["packUid"], oci["packUid"]);
        assert_ne!(public["digest"], oci["digest"]);

        // the item of each registry names its registry
        let made = listed_version(&Value::Null, "kubernetes", PALETTE_REGISTRY, "1.36.3");
        assert_eq!(made["metadata"]["uid"], oci["packUid"]);
        assert_eq!(made["spec"]["registryUid"], PALETTE_REGISTRY);
    }
}
