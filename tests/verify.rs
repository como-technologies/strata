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

const UID_B: &str = "64f0000000000000000000b2";

#[test]
fn name_at_latest_records_the_resolved_version_and_keeps_latest_in_the_ref() {
    let dir = tempfile::tempdir().unwrap();
    common::make_fixtures(dir.path());
    let listing = serde_json::json!({"items": [
        {"metadata": {"name": "lab-profile", "uid": UID_B}, "spec": {"version": "1.3.0"}},
        {"metadata": {"name": "lab-profile", "uid": UID_A}, "spec": {"version": "1.2.0"}},
    ]});
    let api = serve(MemoryApi::new(), dir.path(), "a", UID_A);
    let api =
        serve(api, dir.path(), "b", UID_B).with("/v1/dashboard/clusterprofiles/metadata", listing);
    let snap = ops::snapshot_for(
        &Ref::parse_with("lab-profile@latest", |_| false),
        || Ok(Palette::new(api.clone())),
        "t0",
        &Progress::silent(),
    )
    .unwrap();
    assert_eq!(snap.source.reference, "lab-profile@latest");
    assert_eq!(snap.source.version, "1.3.0");
    assert_eq!(snap.source.uid, UID_B);
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

// ---- retries ---------------------------------------------------------------------------------

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use strata::api::{PaletteApi, Retry, RetryPolicy};
use strata::snapshot::Snapshot;

/// A retrying wrapper over a borrowed API, with a recorder in place of `sleep`.
fn retrying(api: &MemoryApi, retries: u32) -> (Retry<&MemoryApi>, Rc<RefCell<Vec<Duration>>>) {
    let slept = Rc::new(RefCell::new(Vec::new()));
    let s = slept.clone();
    let retry = Retry::with_sleep(
        api,
        RetryPolicy::new(retries),
        Progress::silent(),
        move |d| s.borrow_mut().push(d),
    );
    (retry, slept)
}

fn profile_path() -> String {
    format!("/v1/clusterprofiles/{UID_A}")
}

#[test]
fn a_503_twice_then_success_yields_a_snapshot() {
    let dir = tempfile::tempdir().unwrap();
    common::make_fixtures(dir.path());
    let api = serve(MemoryApi::new(), dir.path(), "a", UID_A).failing_n_times(
        profile_path(),
        2,
        503,
        "busy",
    );
    let (retry, slept) = retrying(&api, 3);
    let snap = ops::snapshot_for(
        &Ref::Uid(UID_A.into()),
        || Ok(Palette::new(retry)),
        "t0",
        &Progress::silent(),
    )
    .unwrap();
    assert_eq!(snap.source.uid, UID_A);
    assert_eq!(api.calls(&profile_path()), 3);
    assert_eq!(
        *slept.borrow(),
        vec![Duration::from_secs(1), Duration::from_secs(2)],
        "exponential backoff from one second"
    );
}

#[test]
fn a_permanent_500_fails_with_the_last_error_after_the_configured_attempts() {
    let dir = tempfile::tempdir().unwrap();
    common::make_fixtures(dir.path());
    let api = serve(MemoryApi::new(), dir.path(), "a", UID_A).failing_n_times(
        profile_path(),
        99,
        500,
        "broken",
    );
    let (retry, slept) = retrying(&api, 3);
    let palette = Palette::new(retry);
    let err = palette
        .fetch_profile(UID_A, &Progress::silent())
        .unwrap_err();
    assert!(matches!(err, Error::Api { status: 500, .. }), "{err}");
    assert_eq!(err.exit_code(), 2);
    assert_eq!(
        api.calls(&profile_path()),
        4,
        "one attempt plus three retries"
    );
    assert_eq!(slept.borrow().len(), 3);
}

#[test]
fn retry_after_is_honored_on_429() {
    let dir = tempfile::tempdir().unwrap();
    common::make_fixtures(dir.path());
    let api = serve(MemoryApi::new(), dir.path(), "a", UID_A)
        .failing_n_times(profile_path(), 1, 429, "slow down")
        .with_retry_after(profile_path(), 2);
    let (retry, slept) = retrying(&api, 3);
    let raw = Palette::new(retry)
        .fetch_profile(UID_A, &Progress::silent())
        .unwrap();
    assert!(Snapshot::from_raw(&raw, UID_A, "t0").is_ok());
    assert_eq!(*slept.borrow(), vec![Duration::from_secs(2)]);
}

#[test]
fn other_4xx_are_not_retried() {
    let dir = tempfile::tempdir().unwrap();
    common::make_fixtures(dir.path());
    let api = serve(MemoryApi::new(), dir.path(), "a", UID_A).failing_n_times(
        profile_path(),
        5,
        404,
        "gone",
    );
    let (retry, slept) = retrying(&api, 3);
    let err = Palette::new(retry)
        .fetch_profile(UID_A, &Progress::silent())
        .unwrap_err();
    assert!(matches!(err, Error::Api { status: 404, .. }), "{err}");
    assert_eq!(api.calls(&profile_path()), 1);
    assert!(slept.borrow().is_empty());
}

/// Drops the connection `n` times, then delegates. Transport errors come from the network layer,
/// so the in-memory API cannot produce them; any [`PaletteApi`] can sit under [`Retry`].
struct Flaky<'a> {
    inner: &'a MemoryApi,
    drops: RefCell<u32>,
}

impl PaletteApi for Flaky<'_> {
    fn get_json(&self, path: &str) -> strata::Result<Value> {
        let mut left = self.drops.borrow_mut();
        if *left > 0 {
            *left -= 1;
            return Err(Error::Transport {
                path: path.to_string(),
                message: "connection reset".into(),
            });
        }
        self.inner.get_json(path)
    }
}

#[test]
fn transport_errors_are_retried() {
    let dir = tempfile::tempdir().unwrap();
    common::make_fixtures(dir.path());
    let api = serve(MemoryApi::new(), dir.path(), "a", UID_A);
    let flaky = Flaky {
        inner: &api,
        drops: RefCell::new(2),
    };
    let retry = Retry::with_sleep(flaky, RetryPolicy::new(2), Progress::silent(), |_| {});
    assert!(Palette::new(retry)
        .fetch_profile(UID_A, &Progress::silent())
        .is_ok());
}

#[test]
fn zero_retries_means_one_attempt() {
    let dir = tempfile::tempdir().unwrap();
    common::make_fixtures(dir.path());
    let api = serve(MemoryApi::new(), dir.path(), "a", UID_A).failing_n_times(
        profile_path(),
        1,
        503,
        "busy",
    );
    let (retry, slept) = retrying(&api, 0);
    assert!(Palette::new(retry)
        .fetch_profile(UID_A, &Progress::silent())
        .is_err());
    assert_eq!(api.calls(&profile_path()), 1);
    assert!(slept.borrow().is_empty());
}
