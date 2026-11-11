//! A release under new version numbers: `same-release --allow renumber` passes a pair whose packs
//! differ only by their versions, and `compare` gives such a pack the impact `none` with
//! `renumber only` when its version keeps its numbers.

use std::path::Path;

use serde_json::{Value, json};

use strata::diff::{self, ImpactLevel, Renumber};
use strata::ops::{self, PairStatus, Policy, SameReleaseOptions};
use strata::snapshot::Snapshot;

use crate::common;

/// The uid of the first version of a pair, such as a pre-release.
const UID_OLD: &str = "64f0000000000000000007a1";
/// The uid of the second version of a pair, such as the final version.
const UID_NEW: &str = "64f0000000000000000007a2";

/// The options of `same-release --allow renumber`.
fn renumber_allowed() -> SameReleaseOptions {
    SameReleaseOptions {
        allow_renumber: true,
        ..SameReleaseOptions::default()
    }
}

/// Each pack of the published template of the profile document `profile` with the name `name`.
fn pack_refs<'a>(profile: &'a mut Value, name: &'a str) -> impl Iterator<Item = &'a mut Value> {
    profile["spec"]["published"]["packs"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .filter(move |p| p["name"] == name)
}

/// Each entry of the packs/manifests document `manifests` with the name `name`.
fn pack_entries<'a>(
    manifests: &'a mut Value,
    name: &'a str,
) -> impl Iterator<Item = &'a mut Value> {
    manifests["spec"]["packs"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .filter(move |p| p["spec"]["name"] == name)
}

/// The record of a copy of the fixture `a`, as the profile version `version` with the uid `uid`.
///
/// Each pack of `builds` is a build of the same content under another version: its version, its
/// tag, its pack uid, and its digest change, and its values stay. `edit` then changes the profile
/// document and the packs/manifests document, as a person or a pipeline does.
fn release(
    root: &Path,
    uid: &str,
    version: &str,
    builds: &[(&str, &str)],
    edit: &dyn Fn(&mut Value, &mut Value),
) -> Snapshot {
    let name = format!("{uid}-{version}");
    let dir = root.join(&name);
    common::rewrite(&root.join("a"), &dir, &[]);
    let mut profile = common::read_json(&dir.join("profile.json"));
    let mut manifests = common::read_json(&dir.join("manifests.json"));
    profile["metadata"]["uid"] = json!(uid);
    profile["spec"]["version"] = json!(version);
    for (pack, to) in builds {
        for p in pack_refs(&mut profile, pack) {
            p["version"] = json!(to);
            p["tag"] = json!(to);
            p["packUid"] = json!(format!("pk-{pack}-{to}"));
            if p.get("digest").is_some() {
                p["digest"] = json!(format!("sha256:{pack}-{to}"));
            }
        }
        for p in pack_entries(&mut manifests, pack) {
            p["metadata"]["uid"] = json!(format!("pk-{pack}-{to}"));
            p["spec"]["version"] = json!(to);
            p["spec"]["digest"] = json!(format!("sha256:{pack}-{to}-registry"));
        }
    }
    edit(&mut profile, &mut manifests);
    common::write_record(&dir.join("profile.json"), &profile);
    common::write_record(&dir.join("manifests.json"), &manifests);
    ops::snapshot_from_dir(&dir, &name, "t0").unwrap()
}

/// An edit that changes nothing.
fn no_edit(_: &mut Value, _: &mut Value) {}

/// The two records, as `same-release` names them.
fn pair(a: Snapshot, b: Snapshot) -> Vec<(String, Snapshot)> {
    vec![("old.json".to_string(), a), ("new.json".to_string(), b)]
}

/// The fixture `a` under `root`, for a test that builds its releases.
fn fixtures() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    common::make_fixtures(dir.path());
    dir
}

/// One scheme of names: the versions of the Kubernetes pack and of the CSI pack, before and after
/// the renumber.
struct Scheme {
    /// The version of the Kubernetes pack on the old side and on the new side.
    k8s: (&'static str, &'static str),
    /// The version of the CSI pack on the old side and on the new side.
    csi: (&'static str, &'static str),
    /// True when each move is a pre-release to its final version.
    final_of_pre: bool,
}

/// The schemes of names of point 2 of #730.
const SCHEMES: [Scheme; 5] = [
    Scheme {
        k8s: ("1.29.4-g0123456789ab", "1.29.4"),
        csi: ("1.28.0-g0123456789ab", "1.28.0"),
        final_of_pre: true,
    },
    Scheme {
        k8s: ("1.29.4-rc.1", "1.29.4"),
        csi: ("1.28.0-rc.1", "1.28.0"),
        final_of_pre: true,
    },
    Scheme {
        k8s: ("1.29.4-mr12.345", "1.29.4"),
        csi: ("1.28.0-mr12.345", "1.28.0"),
        final_of_pre: true,
    },
    Scheme {
        k8s: ("1.29.4+build.5", "1.29.4"),
        csi: ("1.28.0+build.5", "1.28.0"),
        final_of_pre: false,
    },
    Scheme {
        k8s: ("1.2.0-build5", "2026.11.0"),
        csi: ("1.2.0-build5", "2026.11.0"),
        final_of_pre: false,
    },
];

#[test]
fn allow_renumber_passes_each_scheme_of_names_and_lists_each_renumber() {
    let dir = fixtures();
    for Scheme {
        k8s: (k8s, to_k8s),
        csi: (csi, to_csi),
        final_of_pre,
    } in SCHEMES
    {
        let old = release(
            dir.path(),
            UID_OLD,
            "1.4.0-rc.1",
            &[("kubernetes", k8s), ("csi-aws-ebs", csi)],
            &no_edit,
        );
        let new = release(
            dir.path(),
            UID_NEW,
            "1.4.0",
            &[("kubernetes", to_k8s), ("csi-aws-ebs", to_csi)],
            &no_edit,
        );
        assert_ne!(old.root, new.root, "{k8s}: the versions are in the root");
        let c = ops::same_release_with(&pair(old, new), renumber_allowed(), "t1");
        let p = &c.pairs[0];
        assert_eq!(p.status, PairStatus::Renumber, "{k8s}: {:?}", p.reasons);
        assert!(c.same && c.allow_renumber, "{k8s}");
        assert_eq!(c.exit_code(), 0, "{k8s}");
        assert!(p.reasons.is_empty(), "{k8s}: {:?}", p.reasons);
        let renumber = |key: &str, name: &str, old: &str, new: &str| Renumber {
            key: key.to_string(),
            name: name.to_string(),
            old: old.to_string(),
            new: new.to_string(),
            pre_release_to_final: final_of_pre,
        };
        assert_eq!(
            p.renumbers,
            [
                renumber("k8s/kubernetes", "kubernetes", k8s, to_k8s),
                renumber("csi/csi-aws-ebs", "csi-aws-ebs", csi, to_csi),
            ],
            "{k8s}"
        );
        assert_eq!(
            p.archives_not_compared,
            ["k8s/kubernetes", "csi/csi-aws-ebs"],
            "{k8s}: the digest of each new build changed"
        );
        let text = common::terminal().render_chain(&c).unwrap();
        let fact = if final_of_pre {
            "pre-release to final; archive: not compared"
        } else {
            "archive: not compared"
        };
        for line in [
            "RENUMBER       old.json -> new.json   2 packs renumbered, same configuration"
                .to_string(),
            format!("               kubernetes  {k8s} -> {to_k8s} ({fact})"),
            format!("               csi-aws-ebs  {csi} -> {to_csi} ({fact})"),
            "SAME RELEASE: 1 pair passed".to_string(),
        ] {
            assert!(text.lines().any(|l| l == line), "{line:?} in\n{text}");
        }
    }
}

#[test]
fn without_allow_renumber_the_verdict_of_a_renumber_is_the_one_that_it_was() {
    let dir = fixtures();
    let records = || {
        pair(
            release(
                dir.path(),
                UID_OLD,
                "1.4.0-rc.1",
                &[("kubernetes", "1.29.4-rc.1")],
                &no_edit,
            ),
            release(
                dir.path(),
                UID_NEW,
                "1.4.0",
                &[("kubernetes", "1.29.4")],
                &no_edit,
            ),
        )
    };
    let c = ops::same_release(&records(), Policy::Exact, "t1");
    let p = &c.pairs[0];
    assert_eq!(p.status, PairStatus::Different);
    assert_eq!(
        p.reasons,
        ["pack k8s/kubernetes IDENTITY: version 1.29.4-rc.1 -> 1.29.4"]
    );
    assert!(p.renumbers.is_empty() && p.archives_not_compared.is_empty());
    let json = serde_json::to_value(&c).unwrap();
    for field in [
        "/allowRenumber",
        "/pairs/0/renumbers",
        "/pairs/0/archivesNotCompared",
    ] {
        assert!(json.pointer(field).is_none(), "{field} in {json}");
    }
    // the registry-move policy and no-digest accept no renumber either
    let options = SameReleaseOptions {
        policy: Policy::RegistryMove,
        allow_no_digest: true,
        allow_renumber: false,
    };
    let c = ops::same_release_with(&records(), options, "t1");
    assert_eq!(c.pairs[0].status, PairStatus::Different);
    assert_eq!(c.pairs[0].reasons, p.reasons);
}

/// Set the values of the pack `pack` in both documents of a release: the text `from` in them
/// becomes `to`.
fn values_edit<'a>(
    pack: &'a str,
    from: &'a str,
    to: &'a str,
) -> impl Fn(&mut Value, &mut Value) + 'a {
    move |profile, manifests| {
        for p in pack_refs(profile, pack) {
            let values = p["values"].as_str().unwrap().replace(from, to);
            p["values"] = json!(values);
        }
        for p in pack_entries(manifests, pack) {
            let values = p["spec"]["values"].as_str().unwrap().replace(from, to);
            p["spec"]["values"] = json!(values);
        }
    }
}

/// A renumber of one pack with one more change on the new side.
struct Change<'a> {
    /// The pack that moves.
    pack: &'a str,
    /// The version of the old side.
    from: &'a str,
    /// The version of the new side.
    to: &'a str,
    /// The change of the new side, on the profile document and the packs/manifests document.
    edit: &'a dyn Fn(&mut Value, &mut Value),
    /// The start of the reason that names the change.
    reason: &'a str,
}

#[test]
fn allow_renumber_fails_a_pair_that_changes_more_than_the_numbers_and_names_the_change() {
    let dir = fixtures();
    let value = values_edit("kubernetes", "k8sHardening: true", "k8sHardening: false");
    let image = values_edit("cni-cilium-oss", "cilium:v1.15.3", "cilium:v1.15.4");
    let manifest = |_: &mut Value, manifests: &mut Value| {
        for p in pack_entries(manifests, "prometheus-operator") {
            let content = p["spec"]["manifests"][0]["content"].as_str().unwrap();
            p["spec"]["manifests"][0]["content"] = json!(content.replace("for: 5m", "for: 9m"));
        }
    };
    let param = |profile: &mut Value, _: &mut Value| {
        for p in pack_refs(profile, "csi-aws-ebs") {
            p["params"] = json!({"replicas": "3"});
        }
    };
    let machine_image = |id: &'static str| {
        move |profile: &mut Value, _: &mut Value| {
            for p in pack_refs(profile, "ubuntu-aws") {
                p["annotations"] = json!({"imageId": id});
            }
        }
    };
    let machine_image_b = machine_image("ami-0bbb");
    let changes = [
        Change {
            pack: "kubernetes",
            from: "1.29.4-rc.1",
            to: "1.29.4",
            edit: &value,
            reason: "pack k8s/kubernetes AUTHORED: pack.k8sHardening",
        },
        Change {
            pack: "prometheus-operator",
            from: "55.8.3-rc.1",
            to: "55.8.3",
            edit: &manifest,
            reason: "pack addon/prometheus-operator AUTHORED: manifest extra-rules changed",
        },
        Change {
            pack: "csi-aws-ebs",
            from: "1.28.0-rc.1",
            to: "1.28.0",
            edit: &param,
            reason: "pack csi/csi-aws-ebs AUTHORED: params changed",
        },
        Change {
            pack: "cni-cilium-oss",
            from: "1.15.3-rc.1",
            to: "1.15.3",
            edit: &image,
            reason: "pack cni/cni-cilium-oss AUTHORED: pack.content.images[0].image",
        },
        Change {
            pack: "ubuntu-aws",
            from: "22.04-rc.1",
            to: "22.04",
            edit: &machine_image_b,
            reason: "pack os/ubuntu-aws names image ami-0aaa on one side and ami-0bbb on the other",
        },
    ];
    for Change {
        pack,
        from,
        to,
        edit,
        reason,
    } in changes
    {
        let old = release(
            dir.path(),
            UID_OLD,
            "1.4.0-rc.1",
            &[(pack, from)],
            &machine_image("ami-0aaa"),
        );
        let new = release(dir.path(), UID_NEW, "1.4.0", &[(pack, to)], edit);
        let c = ops::same_release_with(&pair(old.clone(), new.clone()), renumber_allowed(), "t1");
        let p = &c.pairs[0];
        assert_eq!(p.status, PairStatus::Different, "{pack}");
        assert!(
            p.reasons.iter().any(|r| r.starts_with(reason)),
            "{pack}: {reason:?} in {:?}",
            p.reasons
        );
        assert!(
            p.renumbers.is_empty(),
            "{pack}: a pack that changes more than its numbers is no renumber"
        );
        let d = diff::compute(&old, &new, "t1", None);
        let pd = d.packs.iter().find(|q| q.name == pack).unwrap();
        assert!(!pd.renumbered, "{pack}");
        assert_ne!(pd.impact.text, "none (renumber only)", "{pack}");
    }
}

#[test]
fn compare_gives_a_renumber_that_keeps_its_numbers_no_impact_also_on_the_os_and_kubernetes() {
    let dir = fixtures();
    let builds = [("ubuntu-aws", "22.04"), ("kubernetes", "1.29.4")];
    let old = release(
        dir.path(),
        UID_OLD,
        "1.4.0-g0123456789ab",
        &[
            ("ubuntu-aws", "22.04-g0123456789ab"),
            ("kubernetes", "1.29.4-g0123456789ab"),
        ],
        &no_edit,
    );
    let new = release(dir.path(), UID_NEW, "1.4.0", &builds, &no_edit);
    let d = diff::compute(&old, &new, "t1", None);
    for key in ["os/ubuntu-aws", "k8s/kubernetes"] {
        let pd = d.packs.iter().find(|p| p.key == key).unwrap();
        assert!(
            pd.renumbered && pd.identity && pd.upstream && !pd.authored,
            "{key}"
        );
        assert_eq!(pd.impact.level, ImpactLevel::None, "{key}");
        assert_eq!(pd.impact.text, "none (renumber only)", "{key}");
    }
    assert_eq!(d.summary.max_impact, ImpactLevel::None);
    let json = serde_json::to_value(&d).unwrap();
    let renumbered: Vec<&str> = json["packs"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|p| p["renumbered"] == true)
        .map(|p| p["key"].as_str().unwrap())
        .collect();
    assert_eq!(renumbered, ["os/ubuntu-aws", "k8s/kubernetes"]);
    let text = common::terminal().render(&d, &old, &new, false).unwrap();
    assert_eq!(
        text.matches("impact: none (renumber only)").count(),
        2,
        "{text}"
    );
    assert!(!text.contains("repave"), "{text}");
}

#[test]
fn an_os_pack_with_new_numbers_and_the_same_values_keeps_its_repave_and_same_release_can_accept_it()
{
    let dir = fixtures();
    // an OS pack names its OS in its archive, not in its values: 20.04 and 24.04 have one set of
    // values and two digests
    let old = release(
        dir.path(),
        UID_OLD,
        "1.4.0",
        &[("ubuntu-aws", "20.04")],
        &no_edit,
    );
    let new = release(
        dir.path(),
        UID_NEW,
        "1.5.0",
        &[("ubuntu-aws", "24.04")],
        &no_edit,
    );
    let d = diff::compute(&old, &new, "t1", None);
    let os = d.packs.iter().find(|p| p.key == "os/ubuntu-aws").unwrap();
    assert!(!os.authored && os.upstream, "the same values, a new digest");
    assert!(!os.renumbered, "the numbers change");
    assert_eq!(os.impact.level, ImpactLevel::Repave);
    assert_eq!(os.impact.text, "node repave (OS image change)");
    assert_eq!(d.summary.max_impact, ImpactLevel::Repave);
    // the person asks for the renumber: the archive is not compared
    let c = ops::same_release_with(&pair(old, new), renumber_allowed(), "t1");
    let p = &c.pairs[0];
    assert_eq!(p.status, PairStatus::Renumber, "{:?}", p.reasons);
    assert_eq!(p.renumbers.len(), 1);
    assert_eq!(
        (p.renumbers[0].old.as_str(), p.renumbers[0].new.as_str()),
        ("20.04", "24.04")
    );
    assert!(!p.renumbers[0].pre_release_to_final);
    assert_eq!(p.archives_not_compared, ["os/ubuntu-aws"]);
}

#[test]
fn the_json_of_same_release_holds_each_renumber_and_the_archives_that_it_does_not_compare() {
    let dir = fixtures();
    let old = release(
        dir.path(),
        UID_OLD,
        "1.4.0-rc.1",
        &[("csi-aws-ebs", "1.28.0-rc.1")],
        &no_edit,
    );
    let new = release(
        dir.path(),
        UID_NEW,
        "1.4.0",
        &[("csi-aws-ebs", "1.28.0")],
        &no_edit,
    );
    let c = ops::same_release_with(&pair(old, new), renumber_allowed(), "t1");
    let json: Value = serde_json::from_str(
        &strata::render::Renderer::render_chain(&*json_renderer(), &c).unwrap(),
    )
    .unwrap();
    assert_eq!(json["allowRenumber"], true);
    assert_eq!(json["pairs"][0]["status"], "renumber");
    assert_eq!(
        json["pairs"][0]["renumbers"],
        json!([{"key": "csi/csi-aws-ebs", "name": "csi-aws-ebs", "old": "1.28.0-rc.1",
                "new": "1.28.0", "preReleaseToFinal": true}])
    );
    assert_eq!(
        json["pairs"][0]["archivesNotCompared"],
        json!(["csi/csi-aws-ebs"])
    );
    assert!(
        json["legend"]["status"]["renumber"]
            .as_str()
            .unwrap()
            .contains("--allow renumber")
    );
    let markdown = common::markdown().render_chain(&c).unwrap();
    assert!(
        markdown.contains(
            "| RENUMBER | `old.json` | `new.json` | 1 pack renumbered, same configuration<br>csi-aws-ebs  1.28.0-rc.1 -> 1.28.0 (pre-release to final; archive: not compared) |"
        ),
        "{markdown}"
    );
}

/// The JSON renderer.
fn json_renderer() -> Box<dyn strata::render::Renderer> {
    strata::render::renderer(
        strata::render::Format::Json,
        strata::term::color::Paint::off(),
        false,
    )
}

#[test]
fn a_manifest_pack_that_is_renumbered_has_no_archive_to_compare() {
    let dir = fixtures();
    // a manifest pack comes from no registry, and Palette gives it no digest
    let manifest_pack = |version: &'static str| {
        move |profile: &mut Value, manifests: &mut Value| {
            for p in pack_refs(profile, "metrics-server") {
                p["type"] = json!("manifest");
                p["registryUid"] = json!("");
                p["version"] = json!(version);
                p["tag"] = json!(version);
                if let Some(o) = p.as_object_mut() {
                    o.remove("digest");
                }
            }
            for p in pack_entries(manifests, "metrics-server") {
                p["spec"]["registryUid"] = json!("");
                p["spec"]["version"] = json!(version);
                p["spec"]["digest"] = json!("");
            }
        }
    };
    let old = release(
        dir.path(),
        UID_OLD,
        "1.4.0-rc.1",
        &[],
        &manifest_pack("1.0.0-rc.1"),
    );
    let new = release(dir.path(), UID_NEW, "1.4.0", &[], &manifest_pack("1.0.0"));
    let c = ops::same_release_with(&pair(old, new), renumber_allowed(), "t1");
    let p = &c.pairs[0];
    assert_eq!(p.status, PairStatus::Renumber, "{:?}", p.reasons);
    assert_eq!(p.renumbers.len(), 1);
    assert!(
        p.archives_not_compared.is_empty(),
        "a manifest pack has no archive"
    );
    let text = common::terminal().render_chain(&c).unwrap();
    assert!(
        text.lines()
            .any(|l| l
                == "               metrics-server  1.0.0-rc.1 -> 1.0.0 (pre-release to final)"),
        "{text}"
    );
}

#[test]
fn two_equal_image_lists_hold_no_image_change_when_a_path_names_the_version_of_one_side() {
    let dir = fixtures();
    // the values name the image under the final version, as Palette mirrors the images of a pack
    // version under a path that holds that version
    let mirrored = values_edit(
        "cni-cilium-oss",
        "quay.io/cilium/cilium:v1.15.3",
        "us-docker.pkg.dev/packs/cni-cilium-oss/1.15.3/cilium:v1.15.3",
    );
    let old = release(
        dir.path(),
        UID_OLD,
        "1.4.0-rc.1",
        &[("cni-cilium-oss", "1.15.3-rc.1")],
        &mirrored,
    );
    let new = release(
        dir.path(),
        UID_NEW,
        "1.4.0",
        &[("cni-cilium-oss", "1.15.3")],
        &mirrored,
    );
    let d = diff::compute(&old, &new, "t1", None);
    let cni = d
        .packs
        .iter()
        .find(|p| p.key == "cni/cni-cilium-oss")
        .unwrap();
    assert!(
        !cni.images && cni.image_changes.is_empty(),
        "{:?}",
        cni.image_changes
    );
    assert_eq!(d.summary.images_changed, 0);
    assert!(cni.renumbered);
    assert_eq!(cni.impact.text, "none (renumber only)");
}
