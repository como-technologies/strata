//! A release under new version numbers: `same-release --allow renumber` passes a pair whose packs
//! differ only by their versions, and `compare` gives such a pack the impact `none` with
//! `renumber only` when its version keeps its numbers.

use std::path::Path;

use serde_json::{Value, json};

use strata::api::MemoryApi;
use strata::diff::{self, ImpactLevel, Renumber};
use strata::model::pack_type::PackType;
use strata::ops::{self, ArchiveChange, Chain, PairStatus, Policy, SameReleaseOptions};
use strata::snapshot::{Pack, Snapshot, Template};

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

/// The values of the pack `p`, as the file `values.yaml` of the archive of its build. Two builds of
/// one content hold the same file.
fn values_file(p: &Pack) -> Option<Vec<(&'static str, String)>> {
    Some(vec![("values.yaml", p.values.clone())])
}

/// The files of the build of the pack `p`. The CSI pack is a chart of a Helm registry: its
/// `Chart.yaml` names the version of the app `app`, and the files `more` follow its values.
fn chart_build(
    p: &Pack,
    app: &str,
    more: &[(&'static str, &str)],
) -> Option<Vec<(&'static str, String)>> {
    let mut files = values_file(p)?;
    if p.name == "csi-aws-ebs" {
        let chart = common::chart_file(&p.name, &p.version, Some(app));
        files.push(("Chart.yaml", chart));
        files.extend(more.iter().map(|(path, text)| (*path, text.to_string())));
    }
    Some(files)
}

/// The chain of `--allow renumber` of `records`, with a Palette that serves the download of each
/// record. The archive of each pack holds `pack.json` and the files that `files` gives.
fn renumber_chain(records: &[(String, Snapshot)], files: &common::BuildFiles) -> Chain {
    let api = common::serve_downloads(MemoryApi::new(), records, files);
    common::same_release_served(records, renumber_allowed(), &api, "t1")
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
        let c = renumber_chain(&pair(old, new), &values_file);
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
            p.archives_compared,
            ["k8s/kubernetes", "csi/csi-aws-ebs"],
            "{k8s}: the digest of each new build changed, and the archives hold the same files"
        );
        assert!(p.archives_not_compared.is_empty() && p.archives_changed.is_empty());
        let text = common::terminal().render_chain(&c).unwrap();
        let fact = if final_of_pre {
            "pre-release to final; archive: same files"
        } else {
            "archive: same files"
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
    let c = common::same_release_offline(&records(), options, "t1");
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
        let c = renumber_chain(&pair(old.clone(), new.clone()), &values_file);
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

/// A number that a version omits is `0`, as Palette reads it: `1.29-rc.1` and `1.29.0` keep their
/// numbers. `same-release` says `pre-release to final` for the move, and `compare` gives it no
/// impact.
#[test]
fn compare_gives_no_impact_to_a_renumber_that_omits_a_number_that_is_zero() {
    let dir = fixtures();
    let old = release(
        dir.path(),
        UID_OLD,
        "1.4-rc.1",
        &[("kubernetes", "1.29-rc.1")],
        &no_edit,
    );
    let new = release(
        dir.path(),
        UID_NEW,
        "1.4.0",
        &[("kubernetes", "1.29.0")],
        &no_edit,
    );
    let d = diff::compute(&old, &new, "t1", None);
    let pd = d.packs.iter().find(|p| p.key == "k8s/kubernetes").unwrap();
    assert!(pd.renumbered);
    assert_eq!(pd.impact.level, ImpactLevel::None);
    assert_eq!(pd.impact.text, "none (renumber only)");
    let c = renumber_chain(&pair(old, new), &values_file);
    let p = &c.pairs[0];
    assert_eq!(p.status, PairStatus::Renumber, "{:?}", p.reasons);
    assert!(p.renumbers[0].pre_release_to_final);
    // one more number moves the version: the impact of the layer stays
    let old = release(
        dir.path(),
        UID_OLD,
        "1.4-rc.1",
        &[("kubernetes", "1.29-rc.1")],
        &no_edit,
    );
    let new = release(
        dir.path(),
        UID_NEW,
        "1.4.1",
        &[("kubernetes", "1.29.1")],
        &no_edit,
    );
    let d = diff::compute(&old, &new, "t1", None);
    let pd = d.packs.iter().find(|p| p.key == "k8s/kubernetes").unwrap();
    assert!(!pd.renumbered);
    assert_ne!(pd.impact.level, ImpactLevel::None);
}

#[test]
fn an_os_pack_with_new_numbers_and_the_same_values_keeps_its_repave_and_same_release_reads_its_archive()
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
    assert!(
        !os.renumbered,
        "the numbers change: compare reads no archive"
    );
    assert_eq!(os.impact.level, ImpactLevel::Repave);
    assert_eq!(os.impact.text, "node repave (OS image change)");
    assert_eq!(d.summary.max_impact, ImpactLevel::Repave);
    // the person asks for the renumber, and the archive of each build names its OS
    let os_file = |p: &Pack| {
        let mut files = values_file(p)?;
        if p.name == "ubuntu-aws" {
            files.push(("os-release", format!("VERSION_ID=\"{}\"\n", p.version)));
        }
        Some(files)
    };
    let c = renumber_chain(&pair(old.clone(), new.clone()), &os_file);
    let p = &c.pairs[0];
    assert_eq!(p.status, PairStatus::Different, "{:?}", p.reasons);
    assert_eq!(
        (p.renumbers[0].old.as_str(), p.renumbers[0].new.as_str()),
        ("20.04", "24.04")
    );
    assert!(!p.renumbers[0].pre_release_to_final);
    assert_eq!(
        p.reasons,
        ["pack os/ubuntu-aws archive: os-release differs"]
    );
    assert_eq!(
        p.archives_changed,
        [ArchiveChange {
            key: "os/ubuntu-aws".to_string(),
            files: vec!["os-release".to_string()],
        }]
    );
    // two builds that differ only by the version in pack.json are a renumber
    let c = renumber_chain(&pair(old, new), &values_file);
    let p = &c.pairs[0];
    assert_eq!(p.status, PairStatus::Renumber, "{:?}", p.reasons);
    assert_eq!(p.archives_compared, ["os/ubuntu-aws"]);
}

/// The criterion of #751: a pair whose archives differ only in `version` of `pack.json` passes, and
/// a pair whose archives differ in another file fails and names the file.
#[test]
fn allow_renumber_fails_a_pair_whose_archives_differ_in_another_file_and_names_the_file() {
    let dir = fixtures();
    let old = release(
        dir.path(),
        UID_OLD,
        "1.4.0-rc.1",
        &[
            ("kubernetes", "1.29.4-rc.1"),
            ("csi-aws-ebs", "1.28.0-rc.1"),
        ],
        &no_edit,
    );
    let new = release(
        dir.path(),
        UID_NEW,
        "1.4.0",
        &[("kubernetes", "1.29.4"), ("csi-aws-ebs", "1.28.0")],
        &no_edit,
    );
    // the final build of the CSI pack has another chart and one more preset
    let rebuilt = |p: &Pack| {
        let mut files = values_file(p)?;
        if p.name == "csi-aws-ebs" {
            let chart = if p.version == "1.28.0" { "two" } else { "one" };
            files.push(("charts/aws-ebs-csi-driver.tgz", chart.to_string()));
            if p.version == "1.28.0" {
                files.push(("presets.yaml", "presets: []\n".to_string()));
            }
        }
        Some(files)
    };
    let records = pair(old, new);
    let c = renumber_chain(&records, &rebuilt);
    let p = &c.pairs[0];
    assert_eq!(p.status, PairStatus::Different);
    assert!(!c.same);
    assert_eq!(c.exit_code(), 1);
    assert_eq!(
        p.reasons,
        ["pack csi/csi-aws-ebs archive: charts/aws-ebs-csi-driver.tgz and 1 more file differ"]
    );
    assert_eq!(
        p.archives_changed,
        [ArchiveChange {
            key: "csi/csi-aws-ebs".to_string(),
            files: vec![
                "charts/aws-ebs-csi-driver.tgz".to_string(),
                "presets.yaml".to_string()
            ],
        }]
    );
    assert_eq!(p.archives_compared, ["k8s/kubernetes"]);
    assert_eq!(
        p.renumbers.len(),
        2,
        "a pair that fails lists its renumbers too"
    );
    let text = common::terminal().render_chain(&c).unwrap();
    for line in [
        "               kubernetes  1.29.4-rc.1 -> 1.29.4 (pre-release to final; archive: same files)",
        "               csi-aws-ebs  1.28.0-rc.1 -> 1.28.0 (pre-release to final; archive: other files)",
        "               pack csi/csi-aws-ebs archive: charts/aws-ebs-csi-driver.tgz and 1 more file differ",
        "NOT THE SAME RELEASE: 1 of 1 pair failed",
    ] {
        assert!(text.lines().any(|l| l == line), "{line:?} in\n{text}");
    }
    // one file that differs is named alone
    let one = |p: &Pack| {
        let mut files = values_file(p)?;
        if p.name == "kubernetes" {
            files[0].1.push_str(&format!("# {}\n", p.version));
        }
        Some(files)
    };
    let c = renumber_chain(&records, &one);
    assert_eq!(
        c.pairs[0].reasons,
        ["pack k8s/kubernetes archive: values.yaml differs"]
    );
}

/// The criterion of #817: Palette serves the chart of a public Helm registry with no `pack.json`,
/// and its `Chart.yaml` names the version. A renumber whose two charts differ only in the line
/// `version` of `Chart.yaml` passes. A pair whose charts differ in another line of `Chart.yaml`,
/// or in another file, fails, and a reason names the file.
#[test]
fn allow_renumber_passes_a_helm_chart_whose_archives_differ_only_in_the_version_of_chart_yaml() {
    let dir = fixtures();
    let helm = |profile: &mut Value, _: &mut Value| {
        for p in pack_refs(profile, "csi-aws-ebs") {
            p["type"] = json!(PackType::Helm);
        }
    };
    let records = pair(
        release(
            dir.path(),
            UID_OLD,
            "1.4.0-rc.1",
            &[("csi-aws-ebs", "1.28.0-rc.1")],
            &helm,
        ),
        release(
            dir.path(),
            UID_NEW,
            "1.4.0",
            &[("csi-aws-ebs", "1.28.0")],
            &helm,
        ),
    );
    // each build of the chart installs the app 1.28.0
    let same = |p: &Pack| chart_build(p, "1.28.0", &[]);
    let c = renumber_chain(&records, &same);
    let p = &c.pairs[0];
    assert_eq!(p.status, PairStatus::Renumber, "{:?}", p.reasons);
    assert_eq!(p.archives_compared, ["csi/csi-aws-ebs"]);
    let text = common::terminal().render_chain(&c).unwrap();
    let line = "               csi-aws-ebs  1.28.0-rc.1 -> 1.28.0 (pre-release to final; archive: same files)";
    assert!(text.lines().any(|l| l == line), "{line:?} in\n{text}");
    // the version of the app is another line of Chart.yaml, and a template is another file
    let app = |p: &Pack| chart_build(p, &p.version, &[]);
    let template = |p: &Pack| {
        let more: &[(&'static str, &str)] = match p.version.as_str() {
            "1.28.0" => &[("templates/node.yaml", "kind: DaemonSet\n")],
            _ => &[],
        };
        chart_build(p, "1.28.0", more)
    };
    let changes: [(&common::BuildFiles, &str); 2] =
        [(&app, "Chart.yaml"), (&template, "templates/node.yaml")];
    for (files, file) in changes {
        let c = renumber_chain(&records, files);
        let p = &c.pairs[0];
        assert_eq!(p.status, PairStatus::Different, "{file}");
        assert_eq!(
            p.reasons,
            [format!("pack csi/csi-aws-ebs archive: {file} differs")]
        );
        assert_eq!(
            p.archives_changed,
            [ArchiveChange {
                key: "csi/csi-aws-ebs".to_string(),
                files: vec![file.to_string()],
            }]
        );
    }
}

/// Palette serves no archive of a pack of a private registry, and of a pack of an OCI registry of
/// Helm charts or of Zarf packages. The list of the download names such a pack with no file. Such
/// a renumber keeps `archive: not compared`, and the pair passes. A manifest pack has no archive:
/// its content is its manifests, which the records hold.
#[test]
fn a_renumber_whose_download_holds_no_archive_keeps_archive_not_compared_in_each_case_of_palette() {
    let dir = fixtures();
    let records = |kind: PackType| {
        let edit = move |profile: &mut Value, _: &mut Value| {
            for p in pack_refs(profile, "csi-aws-ebs") {
                p["type"] = json!(kind);
            }
        };
        pair(
            release(
                dir.path(),
                UID_OLD,
                "1.4.0-rc.1",
                &[("csi-aws-ebs", "1.28.0-rc.1")],
                &edit,
            ),
            release(
                dir.path(),
                UID_NEW,
                "1.4.0",
                &[("csi-aws-ebs", "1.28.0")],
                &edit,
            ),
        )
    };
    // the CSI pack is of a private registry, or of an OCI registry of Helm charts or of Zarf
    // packages. The download names it with an empty filePath
    for kind in [PackType::Spectro, PackType::Helm, PackType::Oci] {
        let records = records(kind);
        let no_csi = |p: &Pack| (p.name != "csi-aws-ebs").then(|| values_file(p)).flatten();
        let c = renumber_chain(&records, &no_csi);
        let p = &c.pairs[0];
        assert_eq!(p.status, PairStatus::Renumber, "{kind:?}: {:?}", p.reasons);
        assert_eq!(p.archives_not_compared, ["csi/csi-aws-ebs"], "{kind:?}");
        assert!(p.archives_compared.is_empty(), "{kind:?}");
        let text = common::terminal().render_chain(&c).unwrap();
        let line = "               csi-aws-ebs  1.28.0-rc.1 -> 1.28.0 (pre-release to final; archive: not compared)";
        assert!(
            text.lines().any(|l| l == line),
            "{kind:?}: {line:?} in\n{text}"
        );
    }
}

/// A record that is no published version of a cluster profile has no download: a candidate, a
/// draft, or a cluster. Strata sends no request for it, and its renumbers keep `archive: not
/// compared`. Each version has one download, also when two pairs read it.
#[test]
fn strata_reads_one_download_for_each_published_version_and_none_for_another_record() {
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
    let records = vec![
        ("old.json".to_string(), old.clone()),
        ("new.json".to_string(), new.clone()),
        ("again.json".to_string(), old.clone()),
    ];
    let api = common::serve_downloads(MemoryApi::new(), &records, &values_file);
    let c = common::same_release_served(&records, renumber_allowed(), &api, "t1");
    assert!(c.same, "{:?}", c.pairs);
    for p in &c.pairs {
        assert_eq!(p.archives_compared, ["csi/csi-aws-ebs"]);
    }
    for (side, version) in [("old", UID_OLD), ("new", UID_NEW)] {
        assert_eq!(api.calls(&common::download_route(version)), 1, "{side}");
    }
    // a candidate of the old version: no uid, no download
    let mut candidate = old.clone();
    candidate.source.uid = String::new();
    let mut draft = old.clone();
    draft.source.template = Template::Draft.into();
    for side in [candidate, draft] {
        let api = common::serve_downloads(MemoryApi::new(), &records, &values_file);
        let c =
            common::same_release_served(&pair(side, new.clone()), renumber_allowed(), &api, "t1");
        assert_eq!(c.pairs[0].archives_not_compared, ["csi/csi-aws-ebs"]);
        assert_eq!(api.calls(&common::download_route(UID_NEW)), 0);
    }
}

/// Palette takes one download of a version in 10 seconds. The download of another client a moment
/// before gives HTTP 429 with the code `TooManyRequests`: the error names the limit.
#[test]
fn a_download_that_palette_refuses_for_its_limit_is_an_error_that_names_the_limit() {
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
    let records = pair(old, new);
    let body = r#"{"code":"TooManyRequests","message":"Too Many Requests, only 1 request in 10 seconds is allowed"}"#;
    let api = common::serve_downloads(MemoryApi::new(), &records, &values_file).failing(
        common::download_route(UID_NEW),
        429,
        body,
    );
    let err = ops::same_release_with(
        &records,
        renumber_allowed(),
        |_| Ok(strata::api::Palette::new(&api)),
        "t1",
        &strata::term::progress::Progress::silent(),
    )
    .unwrap_err();
    assert_eq!(err.kind(), "api");
    let message = err.to_string();
    assert!(message.contains("TooManyRequests"), "{message}");
    assert!(
        message.ends_with("Palette takes one download of a profile version in 10 seconds: run the command again later"),
        "{message}"
    );
    // a download that is not the file of a download
    let api = MemoryApi::new().with_file(
        common::download_route(UID_OLD),
        br#"{"Reader":{}}"#.to_vec(),
    );
    let err = ops::same_release_with(
        &records,
        renumber_allowed(),
        |_| Ok(strata::api::Palette::new(&api)),
        "t1",
        &strata::term::progress::Progress::silent(),
    )
    .unwrap_err();
    assert_eq!(err.kind(), "not-archive");
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
    // the download holds no archive of either build
    let records = pair(old, new);
    let c = renumber_chain(&records, &|_| None);
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
    // archives that hold the same files, and archives that differ in a file
    let to_json = |c: &Chain| -> Value {
        serde_json::from_str(&strata::render::Renderer::render_chain(&*json_renderer(), c).unwrap())
            .unwrap()
    };
    let json = to_json(&renumber_chain(&records, &values_file));
    assert_eq!(
        json["pairs"][0]["archivesCompared"],
        json!(["csi/csi-aws-ebs"])
    );
    assert!(json["pairs"][0].get("archivesNotCompared").is_none());
    let other = |p: &Pack| {
        Some(vec![(
            "values.yaml",
            format!("{}# {}\n", p.values, p.version),
        )])
    };
    let json = to_json(&renumber_chain(&records, &other));
    assert_eq!(json["pairs"][0]["status"], "different");
    assert_eq!(
        json["pairs"][0]["archivesChanged"],
        json!([{"key": "csi/csi-aws-ebs", "files": ["values.yaml"]}])
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
    let c = renumber_chain(&pair(old, new), &values_file);
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
