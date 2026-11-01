//! The edits that a person makes to a published profile, as the chapters write them.

use serde_json::{Value, json};

use super::registries::{OCI_CSI_DIGEST, OCI_CSI_PACK, PALETTE_REGISTRY};

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
    OciCsi,
    /// Give the version another number, which a bump without the immutability flag does.
    Version(&'static str),
    /// Give the profile another name, as `PATCH /v1/clusterprofiles/{uid}/metadata` does: the
    /// uid and the version stay.
    Name(&'static str),
    /// Give the profile another cloud type, such as `gcp` for a profile that the body of an AWS
    /// create names. `spec.published.cloudType` changes, and the packs stay.
    CloudType(&'static str),
    /// Add one command to `kubeadmconfig.postKubeadmCommands` of the Kubernetes pack: a key that
    /// Palette repaves on.
    KubeadmCommand(&'static str),
    /// Move a pack to another version: its `packUid`, tag, and version change, as an upgrade of
    /// the pack does. The impact table calls this a repave for the OS and Kubernetes layers.
    Upgrade {
        /// The pack that moves.
        pack: &'static str,
        /// The `packUid` of the other version.
        uid: &'static str,
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

impl Edit {
    /// The part of an edit that lands in the pack summaries of `packs/manifests`.
    pub(super) fn apply_manifests(&self, manifests: &mut Value) {
        if let Edit::NoDigest(name) = self {
            for pack in manifests["spec"]["packs"]
                .as_array_mut()
                .into_iter()
                .flatten()
            {
                if pack["spec"]["name"] == *name {
                    pack["spec"]["digest"] = json!("");
                }
            }
        }
    }

    /// Apply the edit to the profile document `doc`.
    pub(super) fn apply(&self, doc: &mut Value) {
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
            Edit::KubeadmCommand(command) => {
                for pack in packs_named(doc, "kubernetes") {
                    if let Some(values) = pack["values"].as_str() {
                        pack["values"] = json!(with_kubeadm_command(values, command));
                    }
                }
            }
            Edit::Upgrade { pack, uid, version } => {
                for p in packs_named(doc, pack) {
                    p["packUid"] = json!(uid);
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
            Edit::Masked { pack, key } => {
                for p in packs_named(doc, pack) {
                    append_value_line(p, &format!("{key}: '********'"));
                }
            }
            Edit::Macro { pack, name } => {
                for p in packs_named(doc, pack) {
                    append_value_line(p, &format!("{name}: \"{{{{.spectro.macro.{name}}}}}\""));
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
    use super::*;

    #[test]
    fn the_reclaim_edit_changes_the_csi_pack_alone() {
        let mut doc = json!({"spec": {"published": {"packs": [
            {"name": "csi-aws-ebs", "values": "a\nreclaimPolicy: Delete\nb"},
            {"name": "kubernetes", "values": "reclaimPolicy: Delete"}]}}});
        Edit::Reclaim("Retain").apply(&mut doc);
        let packs = doc["spec"]["published"]["packs"].as_array().unwrap();
        assert_eq!(packs[0]["values"], "a\nreclaimPolicy: Retain\nb");
        assert_eq!(packs[1]["values"], "reclaimPolicy: Delete");
    }

    #[test]
    fn the_oci_edit_moves_the_csi_pack_to_the_other_registry() {
        let mut doc = json!({"spec": {"published": {"packs": [{"name": "csi-aws-ebs"}]}}});
        Edit::OciCsi.apply(&mut doc);
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
            edit.apply(&mut doc);
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
        .apply(&mut doc);
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
        .apply(&mut doc);
        assert_eq!(
            doc["spec"]["published"]["packs"][0]["values"],
            "a: 1\nregion: \"{{.spectro.macro.region}}\"\n"
        );
    }

    #[test]
    fn the_kubeadm_edit_adds_one_command_after_the_last_command_of_the_pack() {
        let mut doc = json!({"spec": {"published": {"packs": [
            {"name": "kubernetes",
             "values": "x:\n    - 'chmod 600 /var/lib/kubelet/config.yaml'\ny: 1\n"}]}}});
        Edit::KubeadmCommand("echo one").apply(&mut doc);
        assert_eq!(
            doc["spec"]["published"]["packs"][0]["values"],
            "x:\n    - 'chmod 600 /var/lib/kubelet/config.yaml'\n    - 'echo one'\ny: 1\n"
        );
    }

    #[test]
    fn the_upgrade_edit_moves_the_pack_uid_the_tag_and_the_version_together() {
        let mut doc = json!({"spec": {"published": {"packs": [
            {"name": "kubernetes", "packUid": "a", "tag": "1.36.2", "version": "1.36.2"}]}}});
        Edit::Upgrade {
            pack: "kubernetes",
            uid: "b",
            version: "1.36.3",
        }
        .apply(&mut doc);
        let pack = &doc["spec"]["published"]["packs"][0];
        assert_eq!(pack["packUid"], "b");
        assert_eq!(pack["tag"], "1.36.3");
        assert_eq!(pack["version"], "1.36.3");
    }
}
