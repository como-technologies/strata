//! The content digest, the pins, a snapshot for each applied profile, and the failure class.
//! These are what a caller needs to pin a profile version by digest. They need no network.

mod common;

use std::path::Path;

use strata::api::{RawCluster, RawProfile};
use strata::error::{Error, FailureClass};
use strata::snapshot::Snapshot;

fn fixtures() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    common::make_fixtures(dir.path());
    dir
}

fn profile(root: &Path, name: &str) -> Snapshot {
    let raw = RawProfile::load_dir(&root.join(name)).unwrap();
    Snapshot::from_raw(&raw, name, "t").unwrap()
}

#[test]
fn one_profile_has_one_content_digest() {
    let dir = fixtures();
    let first = profile(dir.path(), "a");
    let second = profile(dir.path(), "a");
    assert_eq!(first.content_digest(), second.content_digest());
    assert_eq!(first.content_digest().len(), 64);
    assert_ne!(
        first.content_digest(),
        first.root,
        "the two digests answer different questions"
    );
}

#[test]
fn a_rewrite_of_the_values_keeps_the_content_digest() {
    let dir = fixtures();
    // a2 is a with the keys of the kubernetes values in another order and with another indent.
    // Palette rewrites values in this way when it returns them. The digest reads the canonical
    // form, so it does not move.
    let (a, a2) = (profile(dir.path(), "a"), profile(dir.path(), "a2"));
    assert_eq!(a.content_digest(), a2.content_digest());
}

#[test]
fn an_edit_of_the_values_changes_both_digests() {
    let dir = fixtures();
    // a4 is a3 with another pod subnet in the kubernetes values.
    let (a3, a4) = (profile(dir.path(), "a3"), profile(dir.path(), "a4"));
    assert_ne!(a3.root, a4.root);
    assert_ne!(a3.content_digest(), a4.content_digest());
}

#[test]
fn another_tag_name_is_another_root_and_the_same_content() {
    let dir = fixtures();
    // a7 is a with the OS pack pinned under another tag name for the same version. The tag is
    // a name a registry gives a build, and not the build.
    let (a, a7) = (profile(dir.path(), "a"), profile(dir.path(), "a7"));
    assert_ne!(a.root, a7.root);
    assert_eq!(a.content_digest(), a7.content_digest());
}

#[test]
fn another_pack_type_is_another_root_and_the_same_content() {
    let dir = fixtures();
    // a5 is a3 with the pack type `oci` in place of `spectro`. The type is identity, not content.
    let (a3, a5) = (profile(dir.path(), "a3"), profile(dir.path(), "a5"));
    assert_ne!(a3.root, a5.root);
    assert_eq!(a3.content_digest(), a5.content_digest());
}

#[test]
fn a_pack_with_no_registry_is_another_root_and_the_same_content() {
    let dir = fixtures();
    // a6 is a with the type `manifest` and no registry uid: only ids that Palette gives differ.
    let (a, a6) = (profile(dir.path(), "a"), profile(dir.path(), "a6"));
    assert_ne!(a.root, a6.root);
    assert_eq!(a.content_digest(), a6.content_digest());
}

#[test]
fn another_archive_is_another_content_digest() {
    let dir = fixtures();
    // a3 has each pack digest rewritten: the registry holds other bytes for the same version.
    assert_ne!(
        profile(dir.path(), "a").content_digest(),
        profile(dir.path(), "a3").content_digest()
    );
}

/// The content digest is a contract with every tool that pins a version by it. This test holds
/// the digest of one fixture. It fails when the encoding changes, whether by its tag or by the
/// fields it covers, so the change is a decision and not an accident. A new encoding is a new
/// tag, `content/2`, and a new value here; an edit to the fixture `a` is a new value here too.
#[test]
fn the_content_digest_encoding_is_pinned() {
    assert_eq!(strata::snapshot::CONTENT_DIGEST, "content/1");
    let dir = fixtures();
    assert_eq!(
        profile(dir.path(), "a").content_digest(),
        "44680519d410b0a10413a10c3668323d9b30e991529eae3d4b0a0464b50a15ea"
    );
}

#[test]
fn a_record_carries_the_content_digest_and_an_old_record_still_loads() {
    let dir = fixtures();
    let snapshot = profile(dir.path(), "a");
    let mut json = serde_json::to_value(&snapshot).unwrap();
    assert_eq!(json["contentDigest"], snapshot.content_digest());

    // A record from before this field has no `contentDigest`. It loads, and the digest is
    // calculated from the pack hashes that every record holds.
    json.as_object_mut().unwrap().remove("contentDigest");
    let old: Snapshot = serde_json::from_value(json).unwrap();
    assert_eq!(old.content_digest(), snapshot.content_digest());
}

#[test]
fn the_pins_are_the_packs_that_come_from_a_registry() {
    let dir = fixtures();
    let pins = profile(dir.path(), "a").pins();
    let kubernetes = pins
        .iter()
        .find(|pin| pin.name == "kubernetes")
        .expect("a pins kubernetes");
    assert_eq!(kubernetes.version, "1.29.4");
    assert!(
        kubernetes.digest.starts_with("sha256:"),
        "the digest is the one the registry gives"
    );

    // In a6 every pack has the type `manifest`, so it comes from no registry and is not a pin.
    assert!(profile(dir.path(), "a6").pins().is_empty());
}

#[test]
fn one_read_of_a_cluster_gives_a_snapshot_for_each_profile_it_applies() {
    let dir = fixtures();
    let raw = RawCluster::load_dir(&dir.path().join("cluster-merged")).unwrap();
    let each = Snapshot::each_from_cluster(&raw, "cluster:lab-cluster", "t").unwrap();
    assert_eq!(each.len(), 2, "cluster-merged applies b and addons");

    // Each one is the snapshot that `cluster:<x>/<profile>` gives.
    for snapshot in &each {
        let name = &snapshot.source.name;
        let alone = Snapshot::from_cluster(&raw, Some(name), "cluster:lab-cluster", "t").unwrap();
        assert_eq!(snapshot.root, alone.root, "{name}");
    }
    // A cluster that runs a recorded version has the digests of that record.
    let b = each
        .iter()
        .find(|s| s.source.name == profile(dir.path(), "b").source.name)
        .unwrap();
    assert_eq!(
        b.content_digest(),
        profile(dir.path(), "b").content_digest()
    );
}

#[test]
fn a_merged_record_has_one_root_whatever_order_palette_lists_the_profiles() {
    let dir = fixtures();
    // cluster-merged-reversed is cluster-merged with the two templates the other way round, as
    // Palette lists them for another cluster built from the same definition.
    let listed = RawCluster::load_dir(&dir.path().join("cluster-merged")).unwrap();
    let reversed = RawCluster::load_dir(&dir.path().join("cluster-merged-reversed")).unwrap();
    let one = Snapshot::from_cluster(&listed, None, "cluster:lab-cluster", "t").unwrap();
    let other = Snapshot::from_cluster(&reversed, None, "cluster:lab-cluster", "t").unwrap();
    assert_eq!(one.root, other.root);
    assert_eq!(one.content_digest(), other.content_digest());

    // The cluster profile comes first, then the add-on profile, in both records.
    assert_eq!(one.source.name, "lab-profile+lab-addons");
    assert_eq!(other.source.name, "lab-profile+lab-addons");
    let names = |snapshot: &Snapshot| -> Vec<String> {
        let cluster = snapshot.source.cluster.as_ref().unwrap();
        cluster.profiles.iter().map(|p| p.name.clone()).collect()
    };
    assert_eq!(names(&one), ["lab-profile", "lab-addons"]);
    assert_eq!(names(&other), ["lab-profile", "lab-addons"]);

    // One snapshot for each profile comes in the same order.
    let each = Snapshot::each_from_cluster(&reversed, "cluster:lab-cluster", "t").unwrap();
    let each: Vec<&str> = each.iter().map(|s| s.source.name.as_str()).collect();
    assert_eq!(each, ["lab-profile", "lab-addons"]);
}

#[test]
fn an_error_has_a_failure_class() {
    let api = |status| Error::Api {
        method: "GET".into(),
        status,
        path: "/v1/x".into(),
        body: String::new(),
        retry_after: None,
    };
    assert_eq!(api(503).class(), FailureClass::Transient);
    assert_eq!(api(429).class(), FailureClass::Transient);
    assert_eq!(api(403).class(), FailureClass::Terminal);
    assert_eq!(api(404).class(), FailureClass::Terminal);
    let missing = Error::ClusterNotFound {
        cluster: "x".into(),
        available: vec![],
    };
    assert_eq!(missing.class(), FailureClass::Terminal);
    assert_eq!(Error::NoUid.class(), FailureClass::Unknown);
}
