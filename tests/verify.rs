//! Library-level tests for the operations, with an in-memory Palette API.

mod common;

use std::fs;
use std::path::Path;

use serde_json::Value;
use strata::api::{MemoryApi, Palette, RawProfile};
use strata::diff::VerifyStatus;
use strata::error::Error;
use strata::ops;
use strata::progress::Progress;
use strata::reference::Ref;

fn read(path: &Path) -> Value {
    serde_json::from_str(&fs::read_to_string(path).unwrap()).unwrap()
}

/// An API that serves the raw files of fixture `name` under profile `uid`.
fn serve(api: MemoryApi, raw_root: &Path, name: &str, uid: &str) -> MemoryApi {
    let d = raw_root.join(name);
    api.with(
        format!("/v1/clusterprofiles/{uid}"),
        read(&d.join("profile.json")),
    )
    .with(
        format!("/v1/clusterprofiles/{uid}/packs/manifests"),
        read(&d.join("manifests.json")),
    )
    .with(
        format!("/v1/clusterprofiles/{uid}/variables"),
        read(&d.join("variables.json")),
    )
}

const UID_A: &str = "64f0000000000000000000a1";

#[test]
fn verify_reports_unchanged_when_the_live_profile_matches_the_record() {
    let dir = tempfile::tempdir().unwrap();
    common::make_fixtures(dir.path());
    let recorded = ops::snapshot_from_dir(&dir.path().join("a"), "a", "t0").unwrap();
    let palette = Palette::new(serve(MemoryApi::new(), dir.path(), "a", UID_A));
    let v = ops::verify(&recorded, &palette, "t1", &Progress::silent()).unwrap();
    assert!(v.unchanged);
    assert!(v.diff.same);
    let ver = v.diff.verify.unwrap();
    assert_eq!(ver.status, VerifyStatus::Unchanged);
    assert_eq!(
        (
            ver.recorded_at.as_str(),
            ver.checked_at.as_str(),
            ver.uid.as_str()
        ),
        ("t0", "t1", UID_A)
    );
}

#[test]
fn verify_reports_drift_when_the_published_version_was_edited() {
    let dir = tempfile::tempdir().unwrap();
    common::make_fixtures(dir.path());
    let recorded = ops::snapshot_from_dir(&dir.path().join("a"), "a", "t0").unwrap();
    // the live object under A's uid now has B's content: an in-place edit
    let palette = Palette::new(serve(MemoryApi::new(), dir.path(), "b", UID_A));
    let v = ops::verify(&recorded, &palette, "t1", &Progress::silent()).unwrap();
    assert!(!v.unchanged);
    assert_eq!(
        v.diff.verify.as_ref().unwrap().status,
        VerifyStatus::Drifted
    );
    assert!(v
        .diff
        .packs
        .iter()
        .any(|p| p.name == "kubernetes" && p.identity && p.authored));
    assert_eq!(v.diff.summary.max_impact, strata::diff::ImpactLevel::Repave);
}

#[test]
fn verify_needs_a_uid_and_surfaces_api_errors() {
    let dir = tempfile::tempdir().unwrap();
    common::make_fixtures(dir.path());
    let mut recorded = ops::snapshot_from_dir(&dir.path().join("a"), "a", "t0").unwrap();
    let palette = Palette::new(MemoryApi::new());
    let err = ops::verify(&recorded, &palette, "t1", &Progress::silent()).unwrap_err();
    assert!(matches!(err, Error::Api { status: 404, .. }), "{err}");
    recorded.source.uid.clear();
    let err = ops::verify(&recorded, &palette, "t1", &Progress::silent()).unwrap_err();
    assert!(matches!(err, Error::NoUid));
}

#[test]
fn snapshot_for_resolves_names_and_never_connects_for_files() {
    let dir = tempfile::tempdir().unwrap();
    common::make_fixtures(dir.path());
    let listing = serde_json::json!({"items": [
        {"metadata": {"name": "lab-profile", "uid": UID_A}, "spec": {"version": "1.2.0"}}
    ]});
    let api = serve(MemoryApi::new(), dir.path(), "a", UID_A)
        .with("/v1/dashboard/clusterprofiles/metadata", listing);
    let by_name = ops::snapshot_for(
        &Ref::NameVersion {
            name: "lab-profile".into(),
            version: "1.2.0".into(),
        },
        || Ok(Palette::new(api.clone())),
        "t0",
        &Progress::silent(),
    )
    .unwrap();
    let by_uid = ops::snapshot_for(
        &Ref::Uid(UID_A.into()),
        || Ok(Palette::new(api.clone())),
        "t0",
        &Progress::silent(),
    )
    .unwrap();
    assert_eq!(by_name.root, by_uid.root);
    assert_eq!(by_name.source.reference, "lab-profile@1.2.0");

    let file = dir.path().join("a.json");
    fs::write(&file, by_uid.to_json(true).unwrap()).unwrap();
    let from_file = ops::snapshot_for(
        &Ref::Snapshot(file),
        || -> strata::Result<Palette<MemoryApi>> { panic!("must not connect for a snapshot file") },
        "t9",
        &Progress::silent(),
    )
    .unwrap();
    assert_eq!(from_file.root, by_uid.root);

    let missing = ops::snapshot_for(
        &Ref::NameVersion {
            name: "lab-profile".into(),
            version: "9.9.9".into(),
        },
        || Ok(Palette::new(api.clone())),
        "t0",
        &Progress::silent(),
    )
    .unwrap_err();
    assert!(matches!(missing, Error::ProfileNotFound { .. }));
}

#[test]
fn raw_profile_from_directory_matches_the_api_path() {
    let dir = tempfile::tempdir().unwrap();
    common::make_fixtures(dir.path());
    let raw = RawProfile::load_dir(&dir.path().join("b")).unwrap();
    let from_raw = strata::snapshot::Snapshot::from_raw(&raw, "b", "t").unwrap();
    let palette = Palette::new(serve(MemoryApi::new(), dir.path(), "b", "x"));
    let fetched = palette.fetch_profile("x", &Progress::silent()).unwrap();
    let from_api = strata::snapshot::Snapshot::from_raw(&fetched, "b", "t").unwrap();
    assert_eq!(from_raw.root, from_api.root);
    assert_eq!(from_api.source.consumers.count, 2);
}
