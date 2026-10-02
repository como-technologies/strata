//! A candidate layer from a registry that Palette does not sync (#154). Palette lists no pack of
//! such a registry, and makes the pack when it creates the profile: no uid, no digest, the layer
//! addon, and the tag as the version.

use std::fs;
use std::path::Path;

use serde_json::{Value, json};
use strata::api::{MemoryApi, Palette};
use strata::error::Error;
use strata::ops::{self, PairStatus, Policy};
use strata::progress::Progress;

const CHART: &str = "strata-live-chart";
const PRIVATE: &str = "reg-charts-auth";
const PUBLIC: &str = "reg-charts";
const ZARF: &str = "reg-zarf";

fn listing(path_name: &str, items: Value) -> (String, Value) {
    (
        format!("/v1/packs?filters=spec.name={path_name}&limit=100&offset=0"),
        json!({"items": items, "listmeta": {"count": items.as_array().map_or(0, Vec::len)}}),
    )
}

/// A tenant with a private Helm registry, a Helm registry that syncs, and an OCI registry of the
/// provider type zarf with sync off. The listing has no version of the chart.
fn tenant() -> MemoryApi {
    let (path, empty) = listing(CHART, json!([]));
    MemoryApi::new()
        .with(
            "/v1/registries/metadata",
            json!({"items": [
                {"name": "charts-auth", "uid": PRIVATE, "kind": "helm", "isPrivate": true},
                {"name": "charts", "uid": PUBLIC, "kind": "helm", "isPrivate": false},
                {"name": "zarf", "uid": ZARF, "kind": "oci", "isPrivate": true},
            ]}),
        )
        .with(
            "/v1/registries/oci/summary",
            json!({"items": [{"metadata": {"uid": ZARF, "name": "zarf"},
                "spec": {"providerType": "zarf"},
                "status": {"sync": {"isSyncSupported": false}}}]}),
        )
        .with(path, empty)
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
    fs::write(&file, serde_yaml::to_string(&doc).unwrap()).unwrap();
    file
}

fn snapshot(file: &Path, api: MemoryApi) -> strata::Result<strata::snapshot::Snapshot> {
    ops::snapshot_from_candidate(
        file,
        || Ok(Palette::new(api.clone())),
        "t0",
        &Progress::silent(),
    )
}

#[test]
fn a_layer_from_a_private_helm_registry_gives_the_pack_that_palette_makes_at_the_create() {
    let dir = tempfile::tempdir().unwrap();
    let file = candidate(dir.path(), "charts-auth", json!({}));
    let snap = snapshot(&file, tenant()).unwrap();
    let p = &snap.packs[0];
    assert_eq!(p.hash.upstream_source, "none");
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
    let snap = snapshot(&file, tenant()).unwrap();
    let p = &snap.packs[0];
    assert_eq!((p.kind.as_str(), p.pack_uid.as_str()), ("oci", ""));
    assert_eq!(p.registry_provider_type.as_deref(), Some("zarf"));
    assert_eq!(p.hash.upstream_source, "none");
}

#[test]
fn a_layer_from_a_registry_that_syncs_is_still_pack_not_found() {
    let dir = tempfile::tempdir().unwrap();
    let file = candidate(dir.path(), "charts", json!({}));
    let err = snapshot(&file, tenant()).unwrap_err();
    assert!(matches!(err, Error::PackNotFound { .. }), "{err}");

    // an OCI registry of the provider type pack syncs, whatever the flag says
    let api = tenant()
        .with(
            "/v1/registries/metadata",
            json!({"items": [{"name": "zarf", "uid": ZARF, "kind": "oci"}]}),
        )
        .with(
            "/v1/registries/oci/summary",
            json!({"items": [{"metadata": {"uid": ZARF}, "spec": {"providerType": "pack"},
                "status": {"sync": {"isSyncSupported": false}}}]}),
        );
    let file = candidate(dir.path(), "zarf", json!({}));
    let err = snapshot(&file, api).unwrap_err();
    assert!(matches!(err, Error::PackNotFound { .. }), "{err}");
}

#[test]
fn a_version_that_the_listing_has_goes_through_the_listing_also_in_a_private_registry() {
    // a pack that Palette made for an earlier profile is in the listing, with its uid
    let dir = tempfile::tempdir().unwrap();
    let (path, items) = listing(
        CHART,
        json!([{"metadata": {"uid": "pk-minted"}, "spec": {"name": CHART, "version": "1.0.0",
            "registryUid": PRIVATE, "layer": "addon", "type": "helm"}}]),
    );
    let api = tenant().with(path, items).with(
        format!("/v1/packs/{CHART}/registries/{PRIVATE}"),
        json!({"tags": [{"tag": "1.0.0", "packUid": "pk-minted", "version": "1.0.0"}]}),
    );
    let file = candidate(dir.path(), "charts-auth", json!({}));
    let snap = snapshot(&file, api).unwrap();
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
        let err = snapshot(&file, tenant()).unwrap_err();
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
    snapshot(&file, tenant()).unwrap();
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
            json!({"metadata": {"name": "charts-probe", "uid": "p1"},
                "spec": {"version": "1.0.0", "published": {"cloudType": "all", "type": "add-on",
                    "packs": [{"name": CHART, "layer": "addon", "type": "helm", "tag": "1.0.0",
                        "version": "1.0.0", "packUid": uid, "registryUid": PRIVATE,
                        "digest": digest, "values": "replicas: 2\n"}]}}}),
        ),
        (
            "manifests.json",
            json!({"metadata": {"name": "charts-probe", "uid": "p1"},
                "spec": {"packs": [{"metadata": {"name": CHART, "uid": uid},
                    "spec": {"name": CHART, "layer": "addon", "version": "1.0.0",
                        "registryUid": PRIVATE, "digest": digest, "values": "replicas: 2\n"}}]}}),
        ),
        ("variables.json", json!({"variables": []})),
    ] {
        fs::write(raw.join(name), doc.to_string()).unwrap();
    }
    ops::snapshot_from_dir(&raw, "charts-probe@1.0.0", "t1").unwrap()
}

#[test]
fn same_release_passes_the_candidate_and_the_published_version_with_the_minted_uid_only() {
    let dir = tempfile::tempdir().unwrap();
    let file = candidate(dir.path(), "charts-auth", json!({}));
    let snap = snapshot(&file, tenant()).unwrap();
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
    let err = snapshot(&file, tenant()).unwrap_err();
    assert!(matches!(err, Error::CandidateInvalid { .. }), "{err}");
    assert!(
        err.to_string()
            .contains(&format!("layer 1 ({CHART}) has the layer addon")),
        "{err}"
    );
}
