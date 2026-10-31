//! A candidate layer from a registry that Palette does not sync (#154). Palette lists no pack of
//! such a registry, and makes the pack when it creates the profile: no uid, no digest, the layer
//! addon, and the tag as the version.

use std::fs;
use std::path::Path;

use serde_json::{Value, json};
use strata::api::{MemoryApi, Palette};
use strata::closed_set::Open;
use strata::error::Error;
use strata::ops::{self, PairStatus, Policy};
use strata::snapshot::UpstreamSource;
use strata::term::progress::Progress;

use crate::common;
use common::{
    OCI_REGISTRY_SUMMARY, REGISTRY_LISTING, candidate_snapshot, no_variables, profile_document,
    serve_pack_listing, serve_registry_pack, snapshot_of, template, write_yaml,
};

const CHART: &str = "strata-live-chart";
const PRIVATE: &str = "reg-charts-auth";
const PUBLIC: &str = "reg-charts";
const ZARF: &str = "reg-zarf";

/// A tenant with a private Helm registry, a Helm registry that syncs, and an OCI registry of the
/// provider type zarf with sync off. The listing has no version of the chart.
fn tenant() -> MemoryApi {
    let api = MemoryApi::new()
        .with(
            REGISTRY_LISTING,
            json!({"items": [
                {"name": "charts-auth", "uid": PRIVATE, "kind": "helm", "isPrivate": true},
                {"name": "charts", "uid": PUBLIC, "kind": "helm", "isPrivate": false},
                {"name": "zarf", "uid": ZARF, "kind": "oci", "isPrivate": true},
            ]}),
        )
        .with(
            OCI_REGISTRY_SUMMARY,
            json!({"items": [{"metadata": {"uid": ZARF, "name": "zarf"},
                "spec": {"providerType": "zarf"},
                "status": {"sync": {"isSyncSupported": false}}}]}),
        );
    serve_pack_listing(api, CHART, json!([]))
}

fn candidate(dir: &Path, registry: &str, layer: Value) -> std::path::PathBuf {
    let mut l = json!({"name": CHART, "version": "1.0.0", "registry": registry,
        "values": "replicas: 2\n"});
    for (k, v) in layer.as_object().unwrap() {
        l[k] = v.clone();
    }
    let doc = json!({"name": "charts-probe", "version": "1.0.0", "type": "add-on",
        "cloudType": "all", "layers": [l]});
    let file = dir.join(format!("{registry}.yaml"));
    write_yaml(&file, &doc);
    file
}

#[test]
fn a_layer_from_a_private_helm_registry_gives_the_pack_that_palette_makes_at_the_create() {
    let dir = tempfile::tempdir().unwrap();
    let file = candidate(dir.path(), "charts-auth", json!({}));
    let snap = candidate_snapshot(&file, &tenant()).unwrap();
    let p = &snap.packs[0];
    assert_eq!(p.hash.upstream_source, UpstreamSource::NoDigest);
    assert_eq!(p.pack_uid, "");
    assert_eq!(p.version, "1.0.0");
    assert_eq!(p.tag, "1.0.0");
    assert_eq!(p.kind, "helm");
    assert_eq!(p.layer, "addon");
    assert_eq!(p.registry_uid, PRIVATE);
    assert_eq!(p.values, "replicas: 2\n");
}

#[test]
fn a_layer_from_an_oci_registry_with_sync_off_names_its_provider_type() {
    let dir = tempfile::tempdir().unwrap();
    let file = candidate(dir.path(), "zarf", json!({}));
    let snap = candidate_snapshot(&file, &tenant()).unwrap();
    let p = &snap.packs[0];
    assert_eq!((p.kind.as_str(), p.pack_uid.as_str()), ("oci", ""));
    assert_eq!(
        p.registry_provider_type.as_ref().map(Open::as_str),
        Some("zarf")
    );
    assert_eq!(p.hash.upstream_source, UpstreamSource::NoDigest);
}

#[test]
fn a_layer_from_a_registry_that_syncs_is_still_pack_not_found() {
    let dir = tempfile::tempdir().unwrap();
    let file = candidate(dir.path(), "charts", json!({}));
    let err = candidate_snapshot(&file, &tenant()).unwrap_err();
    assert!(matches!(err, Error::PackNotFound { .. }), "{err}");

    // an OCI registry of the provider type pack syncs, whatever the flag says
    let api = tenant()
        .with(
            REGISTRY_LISTING,
            json!({"items": [{"name": "zarf", "uid": ZARF, "kind": "oci"}]}),
        )
        .with(
            OCI_REGISTRY_SUMMARY,
            json!({"items": [{"metadata": {"uid": ZARF}, "spec": {"providerType": "pack"},
                "status": {"sync": {"isSyncSupported": false}}}]}),
        );
    let file = candidate(dir.path(), "zarf", json!({}));
    let err = candidate_snapshot(&file, &api).unwrap_err();
    assert!(matches!(err, Error::PackNotFound { .. }), "{err}");
}

#[test]
fn a_version_that_the_listing_has_goes_through_the_listing_also_in_a_private_registry() {
    // a pack that Palette made for an earlier profile is in the listing, with its uid
    let dir = tempfile::tempdir().unwrap();
    let api = serve_registry_pack(
        tenant(),
        CHART,
        json!([{"metadata": {"uid": "pk-minted"}, "spec": {"name": CHART, "version": "1.0.0",
            "registryUid": PRIVATE, "layer": "addon", "type": "helm"}}]),
        json!({"tags": [{"tag": "1.0.0", "packUid": "pk-minted", "version": "1.0.0"}]}),
        PRIVATE,
    );
    let file = candidate(dir.path(), "charts-auth", json!({}));
    let snap = candidate_snapshot(&file, &api).unwrap();
    assert_eq!(snap.packs[0].pack_uid, "pk-minted");
}

#[test]
fn a_layer_that_palette_would_make_otherwise_is_candidate_invalid() {
    let dir = tempfile::tempdir().unwrap();
    for (layer, says) in [
        (json!({"layer": "csi"}), "layer addon, not csi"),
        (json!({"tag": "1.0.1"}), "set version to 1.0.1"),
        (
            json!({"version": "stable", "tag": "stable"}),
            "the tag stable is not a semantic version",
        ),
    ] {
        let file = candidate(dir.path(), "charts-auth", layer);
        let err = candidate_snapshot(&file, &tenant()).unwrap_err();
        assert!(matches!(err, Error::CandidateInvalid { .. }), "{err}");
        let text = err.to_string();
        assert!(
            text.contains("charts-auth, which Palette does not sync") && text.contains(says),
            "{text}"
        );
    }
    // the layer addon, and a tag equal to the version, are what Palette makes
    let file = candidate(
        dir.path(),
        "charts-auth",
        json!({"layer": "addon", "tag": "1.0.0"}),
    );
    candidate_snapshot(&file, &tenant()).unwrap();
}

/// The record of a published version that pins the chart with the pack uid `uid` and, from a
/// registry that syncs, the digest `digest`. With no digest, it is the profile that Palette
/// created from the candidate, with the pack uid that it made.
fn published(dir: &Path, uid: &str, digest: &str) -> strata::snapshot::Snapshot {
    let raw = dir.join(format!("published-{uid}"));
    fs::create_dir_all(&raw).unwrap();
    for (name, doc) in [
        (
            "profile.json",
            profile_document(
                "p1",
                "charts-probe",
                "1.0.0",
                template(
                    "all",
                    "add-on",
                    json!([{"name": CHART, "layer": "addon",
                    "type": "helm", "tag": "1.0.0", "version": "1.0.0", "packUid": uid,
                    "registryUid": PRIVATE, "digest": digest, "values": "replicas: 2\n"}]),
                ),
                None,
            ),
        ),
        (
            "manifests.json",
            json!({"metadata": {"name": "charts-probe", "uid": "p1"},
                "spec": {"packs": [{"metadata": {"name": CHART, "uid": uid},
                    "spec": {"name": CHART, "layer": "addon", "version": "1.0.0",
                        "registryUid": PRIVATE, "digest": digest, "values": "replicas: 2\n"}}]}}),
        ),
        ("variables.json", no_variables()),
    ] {
        fs::write(raw.join(name), doc.to_string()).unwrap();
    }
    ops::snapshot_from_dir(&raw, "charts-probe@1.0.0", "t1").unwrap()
}

#[test]
fn same_release_passes_the_candidate_and_the_published_version_with_the_minted_uid_only() {
    let dir = tempfile::tempdir().unwrap();
    let file = candidate(dir.path(), "charts-auth", json!({}));
    let snap = candidate_snapshot(&file, &tenant()).unwrap();
    let published = published(dir.path(), "pk-minted", "");
    assert_ne!(snap.root, published.root, "the candidate has no pack uid");

    let records = [("c.json".into(), snap), ("p.json".into(), published)];
    let chain = ops::same_release_with(&records, Policy::Exact, true, "t2");
    assert!(chain.same, "{:?}", chain.pairs[0].reasons);
    assert_eq!(chain.pairs[0].status, PairStatus::SameContent);
    assert_eq!(chain.pairs[0].minted_uid_only, ["addon/strata-live-chart"]);

    // the pack has no digest, so without --allow no-digest the pair does not pass
    let chain = ops::same_release(&records, Policy::Exact, "t2");
    assert_eq!(chain.pairs[0].status, PairStatus::UnknownUpstream);
    assert_eq!(chain.pairs[0].minted_uid_only, ["addon/strata-live-chart"]);
}

#[test]
fn the_pack_uids_of_a_pack_with_a_digest_are_not_minted_uid_only() {
    // a pack uid of a registry that syncs names a build: a change of it is IDENTITY
    let dir = tempfile::tempdir().unwrap();
    let a = published(dir.path(), "pk-a", "sha256:aa");
    let b = published(dir.path(), "pk-b", "sha256:aa");
    let d = strata::diff::compute(&a, &b, "t3", None);
    assert!(d.packs[0].identity && !d.packs[0].minted_uid_only);
    assert!(!d.same_content);
}

#[test]
fn the_pack_that_palette_makes_has_the_layer_addon_for_the_rules_of_the_profile_type() {
    // the type rule of Palette reads the layer of that pack: an infra profile does not take addon
    let dir = tempfile::tempdir().unwrap();
    let file = candidate(dir.path(), "charts-auth", json!({}));
    let text = fs::read_to_string(&file).unwrap();
    fs::write(&file, text.replace("type: add-on", "type: infra")).unwrap();
    let err = candidate_snapshot(&file, &tenant()).unwrap_err();
    assert!(matches!(err, Error::CandidateInvalid { .. }), "{err}");
    assert!(
        err.to_string()
            .contains(&format!("layer 1 ({CHART}) has the layer addon")),
        "{err}"
    );
}

#[test]
fn packs_and_behind_name_the_registries_that_palette_does_not_sync_when_none_has_the_pack() {
    let api = tenant();
    let packs = |registry: Option<&str>| {
        ops::registry_packs(
            CHART,
            &ops::PackOptions {
                registry: registry.map(str::to_string),
                ..Default::default()
            },
            |_| Ok(Palette::new(api.clone())),
            "t0",
            &Progress::silent(),
        )
    };
    // no registry has the chart: the error names the private Helm registry and the OCI registry
    // with sync off, and not the Helm registry that syncs
    let err = packs(None).unwrap_err();
    assert_eq!(err.kind(), "pack-name-not-found");
    assert_eq!(err.to_json()["unsynced"], json!(["charts-auth", "zarf"]));
    assert!(
        err.to_string().contains(&format!(
            "; Palette does not sync the registries 'charts-auth' and 'zarf', and {}. ",
            strata::error::UNSYNCED_LISTS
        )),
        "{err}"
    );

    // one registry that Palette does not sync: the error names it, with no blank version
    let err = packs(Some("charts-auth")).unwrap_err();
    assert_eq!(err.kind(), "pack-not-found");
    assert_eq!(
        err.to_string(),
        format!(
            "no pack 'strata-live-chart' in registry 'charts-auth'; Palette does not sync the \
             registry 'charts-auth', and {}",
            strata::error::UNSYNCED_LISTS
        )
    );
    // asked by its uid, the error still names the registry
    let err = packs(Some(PRIVATE)).unwrap_err();
    assert_eq!(err.to_json()["registry"], "charts-auth");
    assert_eq!(err.to_json()["unsynced"], json!(["charts-auth"]));
    // a registry that syncs gets no reason
    let err = packs(Some("charts")).unwrap_err();
    assert_eq!(
        err.to_string(),
        "no pack 'strata-live-chart' in registry 'charts'"
    );
    assert!(err.to_json().get("unsynced").is_none(), "{err}");

    // a pack ref of that registry, and one with no registry
    let err = snapshot_of(&api, "pack:charts-auth/strata-live-chart@1.0.0").unwrap_err();
    assert_eq!(err.kind(), "pack-not-found");
    assert_eq!(err.to_json()["unsynced"], json!(["charts-auth"]));
    assert!(
        err.to_string().starts_with(
            "no pack 'strata-live-chart' at version '1.0.0' in registry 'charts-auth'; Palette does not sync the registry 'charts-auth'"
        ),
        "{err}"
    );
    let err = snapshot_of(&api, "pack:strata-live-chart@1.0.0").unwrap_err();
    assert_eq!(err.to_json()["unsynced"], json!(["charts-auth", "zarf"]));

    // behind cannot check the pin of a candidate record from that registry, and says why
    let dir = tempfile::tempdir().unwrap();
    let file = candidate(dir.path(), "charts-auth", json!({}));
    let snap = candidate_snapshot(&file, &api).unwrap();
    let record = dir.path().join("candidate.json");
    common::write_snapshot(&record, &snap, false);
    let r = ops::behind(
        &[record],
        &ops::BehindOptions::default(),
        |_| Ok(Palette::new(api.clone())),
        "t0",
        &Progress::silent(),
    )
    .unwrap();
    assert_eq!((r.exit_code(), r.pins.len(), r.summary.errors), (2, 1, 1));
    let error = r.pins[0].error.as_ref().unwrap();
    assert_eq!(error["kind"], "pack-not-found");
    assert_eq!(error["unsynced"], json!(["charts-auth"]));
    let text = common::terminal().render_behind(&r).unwrap();
    assert!(text.contains("not checked"), "{text}");
    assert!(
        text.contains(&format!(
            "(no pack 'strata-live-chart' in registry 'charts-auth'; Palette does not sync the \
             registry 'charts-auth', and {})",
            strata::error::UNSYNCED_LISTS
        )),
        "{text}"
    );
}

#[test]
fn a_resolve_reads_the_registry_listing_and_the_oci_summary_one_time() {
    // three layers, each with a reason to read a listing. A listed chart of the OCI registry
    // needs its provider type. A chart that the listing does not have asks whether Palette syncs
    // its registry. One is from the same registry, one from the private Helm registry. Each layer
    // names its registry by name, so each needs its uid. One resolve reads each listing one time
    // (#327)
    let listed = json!({
        "metadata": {"uid": "pack-listed-1"},
        "spec": {"name": "listed-chart", "version": "1.0.0", "registryUid": ZARF,
                 "digest": "sha256:l1", "layer": "addon", "type": "oci", "values": "a: 1\n"},
    });
    let api = serve_registry_pack(
        tenant(),
        "listed-chart",
        json!([listed]),
        json!({"tags": [{"tag": "1.0.0", "packUid": "pack-listed-1"}]}),
        ZARF,
    );
    let api = serve_pack_listing(api, "other-chart", json!([]));
    let dir = tempfile::tempdir().unwrap();
    let doc = json!({"name": "charts-probe", "version": "1.0.0", "type": "add-on",
    "cloudType": "all", "layers": [
        {"name": "listed-chart", "version": "1.0.0", "registry": "zarf"},
        {"name": CHART, "version": "1.0.0", "registry": "zarf", "values": "replicas: 2\n"},
        {"name": "other-chart", "version": "1.0.0", "registry": "charts-auth",
         "values": "replicas: 1\n"},
    ]});
    let file = dir.path().join("three.yaml");
    write_yaml(&file, &doc);
    let snap = candidate_snapshot(&file, &api).unwrap();
    assert_eq!(snap.packs.len(), 3);
    assert_eq!(
        snap.packs[0]
            .registry_provider_type
            .as_ref()
            .map(Open::as_str),
        Some("zarf"),
        "the listed chart names the provider type of its registry"
    );
    assert_eq!(
        snap.packs[1]
            .registry_provider_type
            .as_ref()
            .map(Open::as_str),
        Some("zarf")
    );
    assert_eq!(snap.packs[2].kind, "helm");
    assert_eq!(
        api.calls(REGISTRY_LISTING),
        1,
        "one read of the registry listing"
    );
    assert_eq!(
        api.calls(OCI_REGISTRY_SUMMARY),
        1,
        "one read of the OCI summary"
    );
}

#[test]
fn packs_reads_the_registry_listing_and_the_oci_summary_one_time_when_no_registry_has_the_pack() {
    // the error names each registry that Palette does not sync. The rule reads the registry
    // listing and the OCI summary one time each, and not one time for each registry
    let api = tenant();
    let err = ops::registry_packs(
        CHART,
        &ops::PackOptions::default(),
        |_| Ok(Palette::new(&api)),
        "t0",
        &Progress::silent(),
    )
    .unwrap_err();
    assert_eq!(err.to_json()["unsynced"], json!(["charts-auth", "zarf"]));
    assert_eq!(api.calls(REGISTRY_LISTING), 1, "{err}");
    assert_eq!(api.calls(OCI_REGISTRY_SUMMARY), 1, "{err}");
}
