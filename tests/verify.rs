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
use strata::config::Config;
use strata::snapshot::Snapshot;

/// A retrying wrapper over a borrowed API, with a recorder in place of `sleep`.
fn retrying(api: &MemoryApi, retries: u32) -> (Retry<&MemoryApi>, Rc<RefCell<Vec<Duration>>>) {
    retrying_with(api, RetryPolicy::new(retries))
}

/// [`retrying`] with a policy of the caller's.
fn retrying_with(
    api: &MemoryApi,
    policy: RetryPolicy,
) -> (Retry<&MemoryApi>, Rc<RefCell<Vec<Duration>>>) {
    let slept = Rc::new(RefCell::new(Vec::new()));
    let s = slept.clone();
    let retry = Retry::with_sleep(api, policy, Progress::silent(), move |d| {
        s.borrow_mut().push(d)
    });
    (retry, slept)
}

/// Fixture `a` served under [`UID_A`], with one 429 carrying `Retry-After: seconds` first.
fn throttled_once(raw_root: &Path, seconds: u64) -> MemoryApi {
    serve(MemoryApi::new(), raw_root, "a", UID_A)
        .failing_n_times(profile_path(), 1, 429, "slow down")
        .with_retry_after(profile_path(), seconds)
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
fn a_retry_after_above_the_ceiling_fails_at_once_without_waiting() {
    let dir = tempfile::tempdir().unwrap();
    common::make_fixtures(dir.path());
    let api = throttled_once(dir.path(), 900);
    let (retry, slept) = retrying(&api, 3);
    let err = Palette::new(retry)
        .fetch_profile(UID_A, &Progress::silent())
        .unwrap_err();
    assert!(
        matches!(
            err,
            Error::RetryAfterTooLong {
                status: 429,
                retry_after: 900,
                limit: 300,
                ..
            }
        ),
        "{err}"
    );
    assert_eq!(err.kind(), "retry-after-too-long");
    assert_eq!(err.exit_code(), 2);
    let message = err.to_string();
    assert!(
        message.contains("900") && message.contains("300"),
        "names what the server asked for and the ceiling: {message}"
    );
    assert_eq!(api.calls(&profile_path()), 1, "no second call");
    assert!(slept.borrow().is_empty(), "nothing slept");
}

#[test]
fn a_retry_after_at_the_ceiling_is_honored_as_sent() {
    let dir = tempfile::tempdir().unwrap();
    common::make_fixtures(dir.path());
    let api = throttled_once(dir.path(), 300);
    let (retry, slept) = retrying(&api, 3);
    Palette::new(retry)
        .fetch_profile(UID_A, &Progress::silent())
        .unwrap();
    assert_eq!(*slept.borrow(), vec![Duration::from_secs(300)]);
}

#[test]
fn the_retry_after_ceiling_comes_from_the_config() {
    let config = Config::from_lookup(|key| match key {
        "PALETTE_API_KEY" => Some("k".to_string()),
        "STRATA_RETRIES" => Some("2".to_string()),
        "STRATA_MAX_RETRY_AFTER" => Some("900".to_string()),
        _ => None,
    })
    .unwrap();
    let policy = RetryPolicy::from_config(&config);
    assert_eq!(policy.retries, 2);
    assert_eq!(policy.max_retry_after, Duration::from_secs(900));

    let dir = tempfile::tempdir().unwrap();
    common::make_fixtures(dir.path());
    let api = throttled_once(dir.path(), 900);
    let (retry, slept) = retrying_with(&api, policy);
    Palette::new(retry)
        .fetch_profile(UID_A, &Progress::silent())
        .unwrap();
    assert_eq!(
        *slept.borrow(),
        vec![Duration::from_secs(900)],
        "fifteen minutes is within the raised ceiling"
    );
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

// ---- project scope and bulk verify ------------------------------------------------------------

use std::path::PathBuf;

use strata::ops::{RecordStatus, SummaryStatus};

#[test]
fn snapshot_for_records_the_project_scope_of_the_client() {
    let dir = tempfile::tempdir().unwrap();
    common::make_fixtures(dir.path());
    let api = serve(MemoryApi::new(), dir.path(), "a", UID_A);
    let scoped = ops::snapshot_for(
        &Ref::Uid(UID_A.into()),
        || Ok(Palette::new(api.clone().in_project("p1"))),
        "t0",
        &Progress::silent(),
    )
    .unwrap();
    assert_eq!(scoped.source.project_uid, "p1");
    let tenant = ops::snapshot_for(
        &Ref::Uid(UID_A.into()),
        || Ok(Palette::new(api.clone())),
        "t0",
        &Progress::silent(),
    )
    .unwrap();
    assert_eq!(tenant.source.project_uid, "");
    assert_eq!(
        scoped.root, tenant.root,
        "the scope is metadata, not part of the root"
    );

    // a record written before the field existed loads with an empty scope
    let mut v: Value = serde_json::from_str(&scoped.to_json(true).unwrap()).unwrap();
    v["source"]
        .as_object_mut()
        .unwrap()
        .remove("projectUid")
        .expect("the field is written");
    let old = dir.path().join("old.json");
    fs::write(&old, v.to_string()).unwrap();
    assert_eq!(Snapshot::load(&old).unwrap().source.project_uid, "");
}

#[test]
fn verify_many_checks_each_record_in_its_own_scope_and_counts_the_outcomes() {
    let dir = tempfile::tempdir().unwrap();
    common::make_fixtures(dir.path());
    let base = ops::snapshot_from_dir(&dir.path().join("a"), "a", "t0").unwrap();
    let write = |name: &str, snap: Snapshot| -> PathBuf {
        let p = dir.path().join(name);
        fs::write(&p, snap.to_json(true).unwrap()).unwrap();
        p
    };
    let rec_p1 = write("p1.json", base.clone().with_project(Some("p1")));
    let rec_p2 = write("p2.json", base.clone().with_project(Some("p2")));
    let not_a_record = dir.path().join("a/profile.json");

    // project p1 still serves A's content; in p2 the live object has B's content, an in-place edit
    let live_p1 = serve(MemoryApi::new(), dir.path(), "a", UID_A);
    let live_p2 = serve(MemoryApi::new(), dir.path(), "b", UID_A);
    let scopes: RefCell<Vec<Option<String>>> = RefCell::new(Vec::new());
    let connect = |project: Option<&str>| {
        scopes.borrow_mut().push(project.map(str::to_string));
        Ok(Palette::new(match project {
            Some("p1") => live_p1.clone(),
            _ => live_p2.clone(),
        }))
    };

    let paths = [rec_p1.clone(), rec_p2.clone(), not_a_record.clone()];
    let s = ops::verify_many(&paths, None, connect, "t1", &Progress::silent());
    assert_eq!(s.schema, "strata-verify-summary/1");
    let statuses: Vec<RecordStatus> = s.records.iter().map(|r| r.status).collect();
    assert_eq!(
        statuses,
        vec![
            RecordStatus::Unchanged,
            RecordStatus::Drifted,
            RecordStatus::Error
        ]
    );
    assert_eq!(
        (
            s.summary.records,
            s.summary.unchanged,
            s.summary.drifted,
            s.summary.errors
        ),
        (3, 1, 1, 1)
    );
    assert_eq!(s.summary.status, SummaryStatus::Error);
    assert_eq!(s.exit_code(), 2, "an error outranks a drift");
    assert_eq!(
        *scopes.borrow(),
        vec![Some("p1".to_string()), Some("p2".to_string())],
        "one client per scope, and a file that is not a record never connects"
    );
    let drifted = &s.records[1];
    assert_eq!(drifted.path, rec_p2.display().to_string());
    assert_eq!(drifted.project_uid, "p2");
    assert_eq!(drifted.max_impact, Some(strata::diff::ImpactLevel::Repave));
    assert!(drifted
        .diff
        .as_ref()
        .expect("a drifted record carries its diff")
        .packs
        .iter()
        .any(|p| p.name == "kubernetes" && p.identity));
    assert!(
        s.records[0].diff.is_none(),
        "an unchanged record carries no diff"
    );
    let failed = &s.records[2];
    assert_eq!(failed.error.as_ref().unwrap()["kind"], "not-snapshot");
    assert!(failed.name.is_empty());

    // every record unchanged is exit 0
    let s = ops::verify_many(&paths[..1], None, connect, "t1", &Progress::silent());
    assert_eq!(s.summary.status, SummaryStatus::Unchanged);
    assert_eq!(s.exit_code(), 0);

    // an explicit project overrides every recorded scope, so one client serves every record
    scopes.borrow_mut().clear();
    let s = ops::verify_many(&paths[..2], Some("p9"), connect, "t1", &Progress::silent());
    assert_eq!(*scopes.borrow(), vec![Some("p9".to_string())]);
    assert_eq!(s.summary.status, SummaryStatus::Drifted);
    assert_eq!(s.exit_code(), 1);
}

#[test]
fn a_raw_directory_keeps_the_project_a_profile_or_cluster_was_fetched_in() {
    let dir = tempfile::tempdir().unwrap();
    common::make_fixtures(dir.path());
    let silent = Progress::silent();
    let project_of = |saved: &Path, reference: &str| {
        ops::snapshot_from_dir(saved, reference, "t1")
            .unwrap()
            .source
            .project_uid
    };

    let profile = Palette::new(serve(MemoryApi::new(), dir.path(), "a", UID_A).in_project("p1"));
    let saved = dir.path().join("raw-profile");
    profile
        .fetch_profile(UID_A, &silent)
        .unwrap()
        .save_dir(&saved)
        .unwrap();
    assert_eq!(project_of(&saved, UID_A), "p1");

    let cluster =
        Palette::new(serve_cluster(MemoryApi::new(), dir.path(), "cluster-a").in_project("p1"));
    let saved = dir.path().join("raw-cluster");
    cluster
        .fetch_cluster(CLUSTER_UID, &silent)
        .unwrap()
        .save_dir(&saved)
        .unwrap();
    assert_eq!(project_of(&saved, &format!("cluster:{CLUSTER_UID}")), "p1");

    let tenant = Palette::new(serve(MemoryApi::new(), dir.path(), "a", UID_A));
    let saved = dir.path().join("raw-tenant");
    tenant
        .fetch_profile(UID_A, &silent)
        .unwrap()
        .save_dir(&saved)
        .unwrap();
    assert_eq!(project_of(&saved, UID_A), "", "fetched at tenant scope");
    assert_eq!(
        project_of(&dir.path().join("a"), "a"),
        "",
        "a raw directory written before the scope was saved still loads, at tenant scope"
    );
}

#[test]
fn a_record_rebuilt_from_a_raw_directory_is_verified_in_its_own_project() {
    let dir = tempfile::tempdir().unwrap();
    common::make_fixtures(dir.path());
    // saved in project p1 before the version was edited in place
    let before = Palette::new(serve(MemoryApi::new(), dir.path(), "a", UID_A).in_project("p1"));
    let saved = dir.path().join("raw");
    before
        .fetch_profile(UID_A, &Progress::silent())
        .unwrap()
        .save_dir(&saved)
        .unwrap();
    let record = dir.path().join("rebuilt.json");
    let rebuilt = ops::snapshot_from_dir(&saved, UID_A, "t1").unwrap();
    fs::write(&record, rebuilt.to_json(true).unwrap()).unwrap();

    // after the edit, p1 serves B's content under the same uid; tenant scope does not see it
    let edited = serve(MemoryApi::new(), dir.path(), "b", UID_A).in_project("p1");
    let scopes: RefCell<Vec<Option<String>>> = RefCell::new(Vec::new());
    let connect = |project: Option<&str>| {
        scopes.borrow_mut().push(project.map(str::to_string));
        Ok(Palette::new(match project {
            Some("p1") => edited.clone(),
            _ => MemoryApi::new(),
        }))
    };
    let s = ops::verify_many(&[record], None, connect, "t2", &Progress::silent());
    assert_eq!(
        *scopes.borrow(),
        vec![Some("p1".to_string())],
        "checked in the saved project, with none given"
    );
    assert_eq!(
        s.records[0].status,
        RecordStatus::Drifted,
        "{:?}",
        s.records[0].error
    );
}

// ---- same release ----------------------------------------------------------------------------

use strata::ops::{PairStatus, Policy};

fn chain_input(dir: &Path, names: &[&str]) -> Vec<(String, Snapshot)> {
    names
        .iter()
        .map(|n| {
            (
                format!("{n}.json"),
                ops::snapshot_from_dir(&dir.join(n), n, "t0").unwrap(),
            )
        })
        .collect()
}

#[test]
fn same_release_passes_when_every_root_is_equal() {
    let dir = tempfile::tempdir().unwrap();
    common::make_fixtures(dir.path());
    let c = ops::same_release(
        &chain_input(dir.path(), &["a", "a2", "a"]),
        Policy::Exact,
        "t1",
    );
    assert_eq!(c.schema, "strata-chain/1");
    assert!(c.same);
    assert_eq!(c.exit_code(), 0);
    assert_eq!(c.pairs.len(), 2, "consecutive pairs in the order given");
    assert!(c.pairs.iter().all(|p| p.status == PairStatus::Identical));
    assert_eq!(
        (c.pairs[0].a.as_str(), c.pairs[0].b.as_str()),
        ("a.json", "a2.json")
    );
    assert_eq!(c.records.len(), 3);
    assert_eq!(c.records[0].label, "a.json");
    assert_eq!(c.summary.passed, 2);
}

#[test]
fn a_root_difference_fails_and_names_the_two_records() {
    let dir = tempfile::tempdir().unwrap();
    common::make_fixtures(dir.path());
    let c = ops::same_release(&chain_input(dir.path(), &["a", "b"]), Policy::Exact, "t1");
    assert!(!c.same);
    assert_eq!(c.exit_code(), 1);
    let p = &c.pairs[0];
    assert_eq!(p.status, PairStatus::Different);
    assert_eq!((p.a.as_str(), p.b.as_str()), ("a.json", "b.json"));
    assert!(
        p.reasons.iter().any(|r| r.contains("variables changed")),
        "{:?}",
        p.reasons
    );
    assert!(
        p.reasons
            .iter()
            .any(|r| r.contains("addon/cert-manager added")),
        "{:?}",
        p.reasons
    );
    assert!(
        p.reasons
            .iter()
            .any(|r| r.contains("addon/external-dns removed")),
        "{:?}",
        p.reasons
    );
    assert!(
        p.reasons
            .iter()
            .any(|r| r.contains("k8s/kubernetes") && r.contains("1.29.4 -> 1.30.2")),
        "{:?}",
        p.reasons
    );
}

#[test]
fn a_registry_move_passes_only_when_allowed() {
    let dir = tempfile::tempdir().unwrap();
    common::make_fixtures(dir.path());
    // a3 is a re-pinned from another registry: every pack has IDENTITY UPSTREAM and nothing else
    let exact = ops::same_release(&chain_input(dir.path(), &["a", "a3"]), Policy::Exact, "t1");
    assert_eq!(exact.pairs[0].status, PairStatus::Different);
    assert!(
        exact.pairs[0]
            .reasons
            .iter()
            .any(|r| r.contains("os/ubuntu-aws")
                && r.contains("moved registry reg-public -> reg-prod")),
        "{:?}",
        exact.pairs[0].reasons
    );
    let allowed = ops::same_release(
        &chain_input(dir.path(), &["a", "a3"]),
        Policy::RegistryMove,
        "t1",
    );
    assert!(allowed.same);
    assert_eq!(allowed.exit_code(), 0);
    assert_eq!(allowed.pairs[0].status, PairStatus::RegistryMove);
    assert_eq!(allowed.pairs[0].packs_repinned, 7);
    assert!(allowed.pairs[0].reasons.is_empty());
    // roots still differ, and the document says so
    assert_ne!(allowed.records[0].root, allowed.records[1].root);
}

#[test]
fn an_authored_change_during_a_registry_move_fails_and_names_the_pack_and_the_leaf() {
    let dir = tempfile::tempdir().unwrap();
    common::make_fixtures(dir.path());
    // a4 is a3 with one value edited on the kubernetes pack
    let c = ops::same_release(
        &chain_input(dir.path(), &["a", "a4"]),
        Policy::RegistryMove,
        "t1",
    );
    assert!(!c.same);
    assert_eq!(c.exit_code(), 1);
    let p = &c.pairs[0];
    assert_eq!(p.status, PairStatus::Different);
    assert_eq!(
        p.reasons.len(),
        1,
        "only the violation is a reason: {:?}",
        p.reasons
    );
    assert!(p.reasons[0].contains("k8s/kubernetes"), "{}", p.reasons[0]);
    assert!(p.reasons[0].contains("AUTHORED"), "{}", p.reasons[0]);
    assert!(
        p.reasons[0].contains("kubeadmconfig.clusterConfiguration.networking.podSubnet"),
        "{}",
        p.reasons[0]
    );
    // the other direction of the same edit, and a plain content change, are not registry moves either
    let c = ops::same_release(
        &chain_input(dir.path(), &["a", "b"]),
        Policy::RegistryMove,
        "t1",
    );
    assert_eq!(c.pairs[0].status, PairStatus::Different);
}

// ---- clusters --------------------------------------------------------------------------------

use common::{CLUSTER_UID, SECRET_VALUE};

/// An API that serves the cluster fixture `dir` under CLUSTER_UID.
fn serve_cluster(api: MemoryApi, raw_root: &Path, dir: &str) -> MemoryApi {
    let d = raw_root.join(dir);
    api.with(
        format!("/v1/spectroclusters/{CLUSTER_UID}"),
        read(&d.join("cluster.json")),
    )
    .with(
        format!("/v1/spectroclusters/{CLUSTER_UID}/profiles/packs/manifests"),
        read(&d.join("manifests.json")),
    )
    .with(
        format!("/v1/spectroclusters/{CLUSTER_UID}/variables"),
        read(&d.join("variables.json")),
    )
}

fn cluster_ref(profile: Option<&str>) -> Ref {
    Ref::Cluster {
        uid: CLUSTER_UID.into(),
        profile: profile.map(str::to_string),
    }
}

fn snapshot_cluster(api: &MemoryApi, profile: Option<&str>) -> strata::Result<Snapshot> {
    ops::snapshot_for(
        &cluster_ref(profile),
        || Ok(Palette::new(api.clone())),
        "t0",
        &Progress::silent(),
    )
}

#[test]
fn a_cluster_running_the_recorded_version_is_identical_to_the_record() {
    let dir = tempfile::tempdir().unwrap();
    common::make_fixtures(dir.path());
    let recorded_b = ops::snapshot_from_dir(&dir.path().join("b"), "b", "t0").unwrap();
    let recorded_a = ops::snapshot_from_dir(&dir.path().join("a"), "a", "t0").unwrap();

    let on_b = snapshot_cluster(
        &serve_cluster(MemoryApi::new(), dir.path(), "cluster-b"),
        None,
    )
    .unwrap();
    assert_eq!(on_b.root, recorded_b.root, "same release, same root");
    assert!(ops::compare(&recorded_b, &on_b, "t1").same);
    let d = ops::compare(&recorded_a, &on_b, "t1");
    assert!(!d.same);
    assert!(d.packs.iter().any(|p| p.name == "kubernetes" && p.identity));

    let c = on_b
        .source
        .cluster
        .as_ref()
        .expect("a cluster snapshot says so");
    assert_eq!(
        (c.uid.as_str(), c.name.as_str()),
        (CLUSTER_UID, "lab-cluster")
    );
    assert_eq!(c.profiles.len(), 1);
    assert_eq!(
        (c.profiles[0].name.as_str(), c.profiles[0].version.as_str()),
        ("lab-profile", "1.3.0")
    );
    assert_eq!(c.profile, None);
    assert_eq!(on_b.source.template, "applied");
    assert_eq!(on_b.source.reference, format!("cluster:{CLUSTER_UID}"));
    assert_eq!(
        (on_b.source.name.as_str(), on_b.source.version.as_str()),
        ("lab-profile", "1.3.0")
    );
    assert_eq!(
        on_b.source.title(),
        "lab-profile@1.3.0 on cluster lab-cluster"
    );
    assert!(
        !on_b.to_json(true).unwrap().contains(SECRET_VALUE),
        "a sensitive variable value is never recorded"
    );
    assert!(recorded_b.source.cluster.is_none());
    assert_eq!(recorded_b.source.title(), "lab-profile@1.3.0");
}

#[test]
fn a_profile_on_the_cluster_is_selected_by_name_and_the_rest_are_merged() {
    let dir = tempfile::tempdir().unwrap();
    common::make_fixtures(dir.path());
    let recorded_b = ops::snapshot_from_dir(&dir.path().join("b"), "b", "t0").unwrap();
    let addons = ops::snapshot_from_dir(&dir.path().join("addons"), "addons", "t0").unwrap();
    let api = serve_cluster(MemoryApi::new(), dir.path(), "cluster-merged");

    let selected = snapshot_cluster(&api, Some("lab-profile")).unwrap();
    assert_eq!(selected.root, recorded_b.root);
    assert_eq!(
        selected.source.cluster.as_ref().unwrap().profile.as_deref(),
        Some("lab-profile")
    );
    assert_eq!(
        selected.source.reference,
        format!("cluster:{CLUSTER_UID}/lab-profile")
    );
    let other = snapshot_cluster(&api, Some("lab-addons")).unwrap();
    assert_eq!(other.root, addons.root);

    let merged = snapshot_cluster(&api, None).unwrap();
    assert_eq!(
        merged.packs.len(),
        recorded_b.packs.len() + addons.packs.len(),
        "every applied profile, in order"
    );
    assert_eq!(merged.packs.last().unwrap().name, "nginx-ingress");
    assert_eq!(merged.variables.len(), recorded_b.variables.len() + 1);
    let c = merged.source.cluster.as_ref().unwrap();
    assert_eq!(c.profiles.len(), 2);
    assert_eq!(
        merged.source.uid, "",
        "no single profile uid for a merged snapshot"
    );
    assert_eq!(merged.source.name, "lab-profile+lab-addons");
    assert_eq!(merged.source.version, "1.3.0+2.0.0");

    let err = snapshot_cluster(&api, Some("nope")).unwrap_err();
    match &err {
        Error::ClusterProfileNotFound {
            cluster,
            profile,
            applied,
        } => {
            assert_eq!((cluster.as_str(), profile.as_str()), (CLUSTER_UID, "nope"));
            assert_eq!(
                applied,
                &vec!["lab-profile".to_string(), "lab-addons".to_string()]
            );
        }
        other => panic!("unexpected error {other}"),
    }
    assert_eq!(err.exit_code(), 2);
    assert!(
        err.to_string().contains("applied: lab-profile, lab-addons"),
        "{err}"
    );
}

#[test]
fn verify_refetches_the_cluster_for_a_cluster_record() {
    let dir = tempfile::tempdir().unwrap();
    common::make_fixtures(dir.path());
    let recorded = snapshot_cluster(
        &serve_cluster(MemoryApi::new(), dir.path(), "cluster-b"),
        None,
    )
    .unwrap();

    let still_b = Palette::new(serve_cluster(MemoryApi::new(), dir.path(), "cluster-b"));
    let v = ops::verify(&recorded, &still_b, "t1", &Progress::silent()).unwrap();
    assert!(v.unchanged);
    assert_eq!(v.diff.verify.as_ref().unwrap().uid, CLUSTER_UID);

    // the same cluster was moved back to a: the run no longer matches its record
    let now_a = Palette::new(serve_cluster(MemoryApi::new(), dir.path(), "cluster-a"));
    let v = ops::verify(&recorded, &now_a, "t1", &Progress::silent()).unwrap();
    assert!(!v.unchanged);
    assert_eq!(
        v.diff.verify.as_ref().unwrap().status,
        VerifyStatus::Drifted
    );
    assert!(v.live.source.cluster.is_some());

    // a cluster without the variables endpoint still snapshots, with no variables
    let d = dir.path().join("cluster-b");
    let no_vars = MemoryApi::new()
        .with(
            format!("/v1/spectroclusters/{CLUSTER_UID}"),
            read(&d.join("cluster.json")),
        )
        .with(
            format!("/v1/spectroclusters/{CLUSTER_UID}/profiles/packs/manifests"),
            read(&d.join("manifests.json")),
        );
    assert!(snapshot_cluster(&no_vars, None)
        .unwrap()
        .variables
        .is_empty());
}

// ---- consumers -------------------------------------------------------------------------------

use strata::color::Paint;
use strata::render::{self, Format};

/// Profile b's API, with `inUseClusters` naming one of its two consumers the way Palette does.
fn serve_b_with_named_consumer(dir: &Path) -> MemoryApi {
    let mut profile = read(&dir.join("b").join("profile.json"));
    profile["status"]["inUseClusters"] = serde_json::json!([
        {"uid": "cluster-aaa", "name": "alpha", "projectUid": "p1", "kind": "spectrocluster"}
    ]);
    serve(MemoryApi::new(), dir, "b", UID_B).with(format!("/v1/clusterprofiles/{UID_B}"), profile)
}

fn cluster_doc(uid: &str, name: &str, project: &str, state: &str) -> Value {
    serde_json::json!({
        "metadata": {"uid": uid, "name": name, "annotations": {"projectUid": project}},
        "status": {"state": state},
    })
}

#[test]
fn consumers_lists_each_cluster_with_name_project_and_state_from_its_own_project() {
    let dir = tempfile::tempdir().unwrap();
    common::make_fixtures(dir.path());
    let tenant = serve_b_with_named_consumer(dir.path());
    let in_p1 = MemoryApi::new().with(
        "/v1/spectroclusters/cluster-aaa",
        cluster_doc("cluster-aaa", "alpha", "p1", "Running"),
    );
    let scopes: RefCell<Vec<Option<String>>> = RefCell::new(Vec::new());
    let connect = |scope: Option<&str>| {
        scopes.borrow_mut().push(scope.map(str::to_string));
        Ok(Palette::new(match scope {
            Some("p1") => in_p1.clone(),
            _ => tenant.clone(),
        }))
    };
    let r = ops::consumers(&Ref::Uid(UID_B.into()), connect, "t1", &Progress::silent()).unwrap();
    assert_eq!(r.schema, "strata-consumers/1");
    assert_eq!(
        (
            r.profile.uid.as_str(),
            r.profile.name.as_str(),
            r.profile.version.as_str()
        ),
        (UID_B, "lab-profile", "1.3.0")
    );
    assert_eq!(r.count, 2);
    assert_eq!(
        r.exit_code(),
        1,
        "consumers exist: a delete step must not proceed"
    );
    let alpha = &r.consumers[0];
    assert_eq!(
        (
            alpha.uid.as_str(),
            alpha.name.as_str(),
            alpha.project_uid.as_str(),
            alpha.state.as_str()
        ),
        ("cluster-aaa", "alpha", "p1", "Running")
    );
    assert!(alpha.error.is_none());
    // cluster-bbb is only a uid on the profile, and it cannot be read in the tenant scope
    let unknown = &r.consumers[1];
    assert_eq!(
        (unknown.uid.as_str(), unknown.state.as_str()),
        ("cluster-bbb", "unknown")
    );
    assert_eq!(unknown.error.as_ref().unwrap()["kind"], "api");
    assert_eq!(
        *scopes.borrow(),
        vec![None, Some("p1".to_string())],
        "one client per project; the profile and unscoped clusters share one"
    );

    let text = render::renderer(Format::Terminal, Paint::off(), false)
        .render_consumers(&r)
        .unwrap();
    assert!(
        text.starts_with("lab-profile@1.3.0 (uid 64f0000000000000000000b2): 2 consumers\n"),
        "{text}"
    );
    assert!(text.contains("alpha"), "{text}");
    assert!(text.contains("p1") && text.contains("Running"), "{text}");
    assert!(
        text.contains("cluster-bbb") && text.contains("unknown"),
        "{text}"
    );
    let md = render::renderer(Format::Markdown, Paint::off(), false)
        .render_consumers(&r)
        .unwrap();
    assert!(
        md.contains("| `alpha` | `cluster-aaa` | `p1` | Running |"),
        "{md}"
    );
    let json: Value = serde_json::from_str(
        &render::renderer(Format::Json, Paint::off(), true)
            .render_consumers(&r)
            .unwrap(),
    )
    .unwrap();
    assert_eq!(json["consumers"][0]["projectUid"], "p1");
    assert_eq!(json["count"], 2);
}

#[test]
fn a_profile_without_consumers_exits_zero_and_fetches_no_cluster() {
    let dir = tempfile::tempdir().unwrap();
    common::make_fixtures(dir.path());
    let api = serve(MemoryApi::new(), dir.path(), "a", UID_A).with(
        "/v1/dashboard/clusterprofiles/metadata",
        serde_json::json!({"items": [
            {"metadata": {"name": "lab-profile", "uid": UID_A}, "spec": {"version": "1.2.0"}}
        ]}),
    );
    let r = ops::consumers(
        &Ref::NameVersion {
            name: "lab-profile".into(),
            version: "1.2.0".into(),
        },
        |_| Ok(Palette::new(api.clone())),
        "t1",
        &Progress::silent(),
    )
    .unwrap();
    assert_eq!((r.count, r.exit_code()), (0, 0));
    assert!(r.consumers.is_empty());
    let text = render::renderer(Format::Terminal, Paint::off(), false)
        .render_consumers(&r)
        .unwrap();
    assert_eq!(
        text,
        "lab-profile@1.2.0 (uid 64f0000000000000000000a1): no consumers\n"
    );
}

#[test]
fn consumers_takes_a_profile_and_refuses_a_cluster() {
    let dir = tempfile::tempdir().unwrap();
    common::make_fixtures(dir.path());
    let never = |_: Option<&str>| -> strata::Result<Palette<MemoryApi>> {
        panic!("a cluster ref is refused before connecting")
    };
    let err = ops::consumers(&cluster_ref(None), never, "t1", &Progress::silent()).unwrap_err();
    assert!(matches!(err, Error::ProfileRefRequired { .. }), "{err}");
    assert_eq!(err.kind(), "profile-ref-required");

    // a record of a cluster is refused the same way; a profile record uses its uid, fetched live
    let on_b = snapshot_cluster(
        &serve_cluster(MemoryApi::new(), dir.path(), "cluster-b"),
        None,
    )
    .unwrap();
    let file = dir.path().join("on-b.json");
    fs::write(&file, on_b.to_json(true).unwrap()).unwrap();
    let err = ops::consumers(&Ref::Snapshot(file), never, "t1", &Progress::silent()).unwrap_err();
    assert!(matches!(err, Error::ProfileRefRequired { .. }), "{err}");

    let recorded = ops::snapshot_from_dir(&dir.path().join("a"), "a", "t0").unwrap();
    let file = dir.path().join("a.json");
    fs::write(&file, recorded.to_json(true).unwrap()).unwrap();
    let api = serve(MemoryApi::new(), dir.path(), "a", UID_A);
    let r = ops::consumers(
        &Ref::Snapshot(file),
        |_| Ok(Palette::new(api.clone())),
        "t1",
        &Progress::silent(),
    )
    .unwrap();
    assert_eq!(r.profile.uid, UID_A);
}

// ---- list ------------------------------------------------------------------------------------

use strata::ops::{ListKind, ListOptions};

/// A profile listing entry, as `GET /v1/dashboard/clusterprofiles/metadata` returns it.
fn listed_profile(name: &str, version: &str, uid: &str) -> Value {
    serde_json::json!({"metadata": {"name": name, "uid": uid}, "spec": {"cloudType": "aws", "version": version}})
}

/// A cluster listing entry, as `GET /v1/dashboard/spectroclusters/meta` returns it.
fn listed_cluster(name: &str, uid: &str, state: &str) -> Value {
    serde_json::json!({
        "metadata": {"name": name, "uid": uid, "annotations": {"projectUid": "p1", "scope": "project"}},
        "spec": {"cloudType": "aws", "cloudRegion": "us-east-2"},
        "status": {"state": state},
    })
}

fn projects(names: &[(&str, &str)]) -> Value {
    let items: Vec<Value> = names
        .iter()
        .map(|(name, uid)| serde_json::json!({"metadata": {"name": name, "uid": uid}}))
        .collect();
    serde_json::json!({ "items": items })
}

#[test]
fn list_names_each_profile_and_cluster_as_a_ref_in_sorted_order() {
    let api = MemoryApi::new()
        .in_project("p1")
        .with(
            "/v1/dashboard/clusterprofiles/metadata",
            serde_json::json!({"items": [
                listed_profile("lab-profile", "1.10.0", "u3"),
                listed_profile("addons", "2.0.0", "u9"),
                listed_profile("lab-profile", "1.9.0", "u2"),
            ]}),
        )
        .with(
            "/v1/dashboard/spectroclusters/meta",
            serde_json::json!([
                listed_cluster("workload-b", "c2", "Pending"),
                listed_cluster("workload-a", "c1", "Running"),
            ]),
        )
        .with(
            "/v1/dashboard/projects/metadata",
            projects(&[("lab", "p1")]),
        );
    let scopes: RefCell<Vec<Option<String>>> = RefCell::new(Vec::new());
    let connect = |scope: Option<&str>| {
        scopes.borrow_mut().push(scope.map(str::to_string));
        Ok(Palette::new(api.clone()))
    };
    let l = ops::list(&ListOptions::default(), connect, "t1", &Progress::silent()).unwrap();
    assert_eq!(l.schema, "strata-list/1");
    assert_eq!(
        (l.project_uid.as_str(), l.project_name.as_str()),
        ("p1", "lab")
    );
    let refs = |v: &[ops::ListedProfile]| v.iter().map(|p| p.reference.clone()).collect::<Vec<_>>();
    let profiles = l.profiles.as_ref().expect("profiles are listed by default");
    assert_eq!(
        refs(profiles),
        ["addons@2.0.0", "lab-profile@1.9.0", "lab-profile@1.10.0"],
        "by name, then natural version order"
    );
    assert_eq!(
        (profiles[1].uid.as_str(), profiles[1].cloud_type.as_str()),
        ("u2", "aws")
    );
    assert!(profiles
        .iter()
        .all(|p| p.packs.is_none() && p.error.is_none()));
    let clusters = l.clusters.as_ref().expect("clusters are listed by default");
    assert_eq!(
        clusters
            .iter()
            .map(|c| (c.reference.as_str(), c.name.as_str(), c.state.as_str()))
            .collect::<Vec<_>>(),
        [
            ("cluster:c1", "workload-a", "Running"),
            ("cluster:c2", "workload-b", "Pending")
        ]
    );
    assert!(l.projects.is_none(), "projects only when asked");
    assert_eq!(
        *scopes.borrow(),
        vec![None],
        "one client, in the configured scope"
    );
    assert_eq!(
        api.calls("/v1/clusterprofiles/u2"),
        0,
        "no profile document without --packs"
    );

    let text = render::renderer(Format::Terminal, Paint::off(), false)
        .render_list(&l)
        .unwrap();
    assert!(text.starts_with("project lab (uid p1)\n"), "{text}");
    assert!(text.contains("\n3 profiles\n"), "{text}");
    assert!(text.contains("  lab-profile@1.10.0  u3  aws\n"), "{text}");
    assert!(
        text.contains("  cluster:c1  workload-a  Running  aws\n"),
        "{text}"
    );
    let md = render::renderer(Format::Markdown, Paint::off(), false)
        .render_list(&l)
        .unwrap();
    assert!(md.contains("| `lab-profile@1.9.0` | `u2` | aws |"), "{md}");
    assert!(
        md.contains("| `cluster:c2` | workload-b | Pending | aws |"),
        "{md}"
    );
    let json: Value = serde_json::from_str(
        &render::renderer(Format::Json, Paint::off(), true)
            .render_list(&l)
            .unwrap(),
    )
    .unwrap();
    assert_eq!(json["profiles"][0]["ref"], "addons@2.0.0");
    assert_eq!(json["clusters"][1]["cloudType"], "aws");
    assert!(json.get("projects").is_none(), "{json}");
}

#[test]
fn list_with_a_pack_reads_each_profile_and_keeps_those_that_carry_it() {
    let dir = tempfile::tempdir().unwrap();
    common::make_fixtures(dir.path());
    let api = serve(
        serve(MemoryApi::new(), dir.path(), "a", UID_A),
        dir.path(),
        "b",
        UID_B,
    )
    .with(
        "/v1/dashboard/clusterprofiles/metadata",
        serde_json::json!({"items": [
            listed_profile("lab-profile", "1.3.0", UID_B),
            listed_profile("lab-profile", "1.2.0", UID_A),
            listed_profile("hidden", "1.0.0", "u-denied"),
        ]}),
    )
    .failing("/v1/clusterprofiles/u-denied", 403, "forbidden");
    let options = ListOptions {
        pack: Some("CERT-man".into()),
        ..ListOptions::default()
    };
    let l = ops::list(
        &options,
        |_| Ok(Palette::new(api.clone())),
        "t1",
        &Progress::silent(),
    )
    .unwrap();
    assert!(
        l.clusters.is_none(),
        "--pack lists profiles only unless kinds are named"
    );
    assert_eq!(api.calls("/v1/dashboard/spectroclusters/meta"), 0);
    assert_eq!(
        (l.project_uid.as_str(), l.project_name.as_str()),
        ("", ""),
        "tenant scope has no project"
    );
    let profiles = l.profiles.as_ref().unwrap();
    assert_eq!(
        profiles
            .iter()
            .map(|p| p.reference.as_str())
            .collect::<Vec<_>>(),
        ["hidden@1.0.0", "lab-profile@1.3.0"],
        "1.2.0 has no cert-manager; a profile that cannot be read stays, with its error"
    );
    let denied = &profiles[0];
    assert_eq!(denied.error.as_ref().unwrap()["kind"], "api");
    assert!(denied.packs.is_none());
    let b = &profiles[1];
    assert_eq!(
        (b.profile_type.as_deref(), b.consumers),
        (Some("cluster"), Some(2))
    );
    let packs = b.packs.as_ref().unwrap();
    assert_eq!(packs.len(), 7, "every layer is shown, not only the match");
    let cm = packs.iter().find(|p| p.name == "cert-manager").unwrap();
    assert_eq!(
        (cm.layer.as_str(), cm.version.as_str(), cm.tag.as_str()),
        ("addon", "1.14.0", "1.14.0")
    );

    let text = render::renderer(Format::Terminal, Paint::off(), false)
        .render_list(&l)
        .unwrap();
    assert!(text.starts_with("tenant scope\n"), "{text}");
    assert!(
        text.contains("\n2 profiles with a pack matching 'CERT-man'\n"),
        "{text}"
    );
    assert!(
        text.contains(&format!(
            "  lab-profile@1.3.0  {UID_B}  aws  cluster  2 consumers\n"
        )),
        "{text}"
    );
    assert!(
        text.contains("    addon  cert-manager         1.14.0\n"),
        "{text}"
    );
    assert!(text.contains("forbidden"), "the error is shown: {text}");
    let md = render::renderer(Format::Markdown, Paint::off(), false)
        .render_list(&l)
        .unwrap();
    assert!(md.contains("addon/cert-manager 1.14.0"), "{md}");
}

#[test]
fn list_projects_reads_at_tenant_scope_and_a_project_override_scopes_the_rest() {
    let in_p2 = MemoryApi::new()
        .in_project("p2")
        .with(
            "/v1/dashboard/clusterprofiles/metadata",
            serde_json::json!({"items": [listed_profile("edge", "1.0.0", "u1")]}),
        )
        .with(
            "/v1/dashboard/projects/metadata",
            projects(&[("prod", "p2")]),
        );
    let tenant = MemoryApi::new().with(
        "/v1/dashboard/projects/metadata",
        projects(&[("prod", "p2"), ("dev", "p1")]),
    );
    let scopes: RefCell<Vec<Option<String>>> = RefCell::new(Vec::new());
    let connect = |scope: Option<&str>| {
        scopes.borrow_mut().push(scope.map(str::to_string));
        Ok(Palette::new(match scope {
            Some(ops::TENANT_SCOPE) => tenant.clone(),
            _ => in_p2.clone(),
        }))
    };
    let options = ListOptions {
        kinds: vec![ListKind::Projects, ListKind::Profiles],
        project: Some("p2".into()),
        ..ListOptions::default()
    };
    let l = ops::list(&options, connect, "t1", &Progress::silent()).unwrap();
    assert_eq!(
        l.projects
            .as_ref()
            .unwrap()
            .iter()
            .map(|p| (p.name.as_str(), p.uid.as_str()))
            .collect::<Vec<_>>(),
        [("dev", "p1"), ("prod", "p2")],
        "every project the key can see, by name"
    );
    assert_eq!(l.profiles.as_ref().unwrap()[0].reference, "edge@1.0.0");
    assert!(l.clusters.is_none(), "named kinds only");
    assert_eq!(
        (l.project_uid.as_str(), l.project_name.as_str()),
        ("p2", "prod")
    );
    assert_eq!(
        *scopes.borrow(),
        vec![Some("p2".to_string()), Some(String::new())],
        "the override scopes profiles; projects are read at tenant scope"
    );
    assert_eq!(
        in_p2.calls("/v1/dashboard/projects/metadata"),
        0,
        "the tenant listing already names the project"
    );
    let text = render::renderer(Format::Terminal, Paint::off(), false)
        .render_list(&l)
        .unwrap();
    assert!(
        text.contains("\n2 projects\n  dev   p1\n  prod  p2\n"),
        "{text}"
    );
}

#[test]
fn list_at_tenant_scope_names_the_project_of_each_cluster() {
    let mut other = listed_cluster("edge-1", "c7", "Running");
    other["metadata"]["annotations"]["projectUid"] = serde_json::json!("p2");
    let api = MemoryApi::new().with(
        "/v1/dashboard/spectroclusters/meta",
        serde_json::json!([listed_cluster("workload-a", "c1", "Running"), other]),
    );
    let options = ListOptions {
        kinds: vec![ListKind::Clusters],
        ..ListOptions::default()
    };
    let l = ops::list(
        &options,
        |_| Ok(Palette::new(api.clone())),
        "t1",
        &Progress::silent(),
    )
    .unwrap();
    let clusters = l.clusters.as_ref().unwrap();
    assert_eq!(
        clusters
            .iter()
            .map(|c| c.project_uid.as_str())
            .collect::<Vec<_>>(),
        ["p2", "p1"],
        "a cluster ref works only in its own project, so each carries it"
    );
    let text = render::renderer(Format::Terminal, Paint::off(), false)
        .render_list(&l)
        .unwrap();
    assert!(
        text.contains("  cluster:c7  edge-1      Running  aws  project p2\n"),
        "{text}"
    );
    let md = render::renderer(Format::Markdown, Paint::off(), false)
        .render_list(&l)
        .unwrap();
    assert!(
        md.contains("| `cluster:c1` | workload-a | Running | aws | `p1` |"),
        "{md}"
    );
}

#[test]
fn list_fails_when_a_listing_cannot_be_read() {
    let api = MemoryApi::new().failing("/v1/dashboard/clusterprofiles/metadata", 401, "no key");
    let err = ops::list(
        &ListOptions::default(),
        |_| Ok(Palette::new(api.clone())),
        "t1",
        &Progress::silent(),
    )
    .unwrap_err();
    assert_eq!(err.kind(), "api", "{err}");
}

// ---- candidates ------------------------------------------------------------------------------

/// The registry endpoints strata reads for one pack, as the examples cached them from a live
/// tenant: the version listing filtered by name, and the pack entity with its tags.
fn serve_registry_pack(
    api: MemoryApi,
    name: &str,
    summaries: Value,
    entity: Value,
    registry: &str,
) -> MemoryApi {
    let count = summaries.as_array().map(Vec::len).unwrap_or(0);
    api.with(
        format!("/v1/packs?filters=spec.name={name}&limit=100&offset=0"),
        serde_json::json!({"items": summaries, "listmeta": {"count": count}}),
    )
    .with(format!("/v1/packs/{name}/registries/{registry}"), entity)
}

fn write_yaml(path: &Path, doc: &Value) {
    fs::write(path, serde_yaml::to_string(doc).unwrap()).unwrap();
}

fn never_connect() -> strata::Result<Palette<MemoryApi>> {
    panic!("local errors are found before connecting")
}

#[test]
fn a_candidate_built_from_real_registry_responses_has_the_root_of_the_published_profile() {
    let examples = Path::new(env!("CARGO_MANIFEST_DIR")).join("examples");
    let dir = tempfile::tempdir().unwrap();
    for version in ["1.0.0", "1.1.0", "1.1.1"] {
        let raw = examples
            .join("profiles/raw")
            .join(format!("aws-pxk@{version}"));
        let profile = read(&raw.join("profile.json"));
        let manifests = read(&raw.join("manifests.json"));
        let registry = profile["spec"]["published"]["packs"][0]["registryUid"]
            .as_str()
            .unwrap()
            .to_string();

        let mut api = MemoryApi::new().with(
            "/v1/registries/metadata",
            serde_json::json!({"items": [{"name": "Public Repo", "uid": registry, "kind": "pack"}]}),
        );
        let mut layers = Vec::new();
        for pack in profile["spec"]["published"]["packs"].as_array().unwrap() {
            let name = pack["name"].as_str().unwrap();
            let d = examples.join("packs").join(name);
            api = serve_registry_pack(
                api,
                name,
                read(&d.join("_summaries.json")),
                read(&d.join("_entity.json")),
                &registry,
            );
            // only the version is given; layer, tag, packUid, digest, and default values come
            // from the registry, and the manifest the example attaches is carried inline
            let mut layer = serde_json::json!({"name": name, "version": pack["version"]});
            let attached: Vec<Value> = manifests["spec"]["packs"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|m| m["spec"]["name"] == name)
                .flat_map(|m| {
                    m["spec"]["manifests"]
                        .as_array()
                        .cloned()
                        .unwrap_or_default()
                })
                .map(|m| serde_json::json!({"name": m["name"], "content": m["content"]}))
                .collect();
            if !attached.is_empty() {
                layer["manifests"] = Value::Array(attached);
            }
            layers.push(layer);
        }
        let candidate = serde_json::json!({
            "name": "aws-pxk", "version": version, "cloudType": "aws", "registry": "Public Repo",
            "layers": layers,
            "variables": read(&raw.join("variables.json"))["variables"],
        });
        let file = dir.path().join(format!("aws-pxk@{version}.yaml"));
        write_yaml(&file, &candidate);

        let snap = ops::snapshot_from_candidate(
            &file,
            || Ok(Palette::new(api.clone())),
            "t0",
            &Progress::silent(),
        )
        .unwrap();
        let published = read(
            &examples
                .join("profiles")
                .join(format!("aws-pxk@{version}.json")),
        );
        assert_eq!(
            snap.root,
            published["root"].as_str().unwrap(),
            "aws-pxk@{version}: the candidate is the release that gets published"
        );
        assert_eq!(snap.source.template, "candidate");
        assert_eq!(snap.source.uid, "");
        assert_eq!(
            (snap.source.name.as_str(), snap.source.version.as_str()),
            ("aws-pxk", version)
        );
        assert_eq!(snap.source.reference, file.display().to_string());
    }
}

/// Registry responses for every pack of fixture `b`, so a candidate that spells out b's values
/// and manifests resolves to b's packs.
fn serve_registry_for_b(dir: &Path) -> MemoryApi {
    let profile = read(&dir.join("b").join("profile.json"));
    let manifests = read(&dir.join("b").join("manifests.json"));
    let mut api = MemoryApi::new();
    for pack in profile["spec"]["published"]["packs"].as_array().unwrap() {
        let name = pack["name"].as_str().unwrap();
        let entry = manifests["spec"]["packs"]
            .as_array()
            .unwrap()
            .iter()
            .find(|m| m["spec"]["name"] == name)
            .unwrap();
        let summary = serde_json::json!({
            "metadata": {"uid": pack["packUid"]},
            "spec": {"name": name, "version": pack["version"], "registryUid": "reg-public",
                     "digest": entry["spec"]["digest"], "layer": pack["layer"], "type": "spectro",
                     "values": "registry: defaults"},
        });
        let entity = serde_json::json!({"tags": [
            {"tag": "floating.x", "packUid": pack["packUid"]},
            {"tag": pack["tag"], "packUid": pack["packUid"]},
        ]});
        api = serve_registry_pack(
            api,
            name,
            serde_json::json!([summary]),
            entity,
            "reg-public",
        );
    }
    api
}

#[test]
fn a_candidate_carries_values_and_manifests_inline_or_from_files_next_to_it() {
    let dir = tempfile::tempdir().unwrap();
    common::make_fixtures(dir.path());
    let recorded_b = ops::snapshot_from_dir(&dir.path().join("b"), "b", "t0").unwrap();
    let profile = read(&dir.path().join("b").join("profile.json"));
    let manifests = read(&dir.path().join("b").join("manifests.json"));

    let cand = dir.path().join("candidate");
    fs::create_dir_all(cand.join("values")).unwrap();
    fs::create_dir_all(cand.join("manifests")).unwrap();
    let mut layers = Vec::new();
    for pack in profile["spec"]["published"]["packs"].as_array().unwrap() {
        let name = pack["name"].as_str().unwrap();
        let mut layer =
            serde_json::json!({"name": name, "version": pack["version"], "layer": pack["layer"]});
        if name == "kubernetes" {
            fs::write(
                cand.join("values/kubernetes.yaml"),
                pack["values"].as_str().unwrap(),
            )
            .unwrap();
            layer["valuesFile"] = serde_json::json!("values/kubernetes.yaml");
        } else {
            layer["values"] = pack["values"].clone();
        }
        let entry = manifests["spec"]["packs"]
            .as_array()
            .unwrap()
            .iter()
            .find(|m| m["spec"]["name"] == name)
            .unwrap();
        let mut ms = Vec::new();
        for m in entry["spec"]["manifests"].as_array().unwrap() {
            let mname = m["name"].as_str().unwrap();
            let file = format!("manifests/{mname}.yaml");
            fs::write(cand.join(&file), m["content"].as_str().unwrap()).unwrap();
            ms.push(serde_json::json!({"name": mname, "contentFile": file}));
        }
        if !ms.is_empty() {
            layer["manifests"] = Value::Array(ms);
        }
        layers.push(layer);
    }
    let file = cand.join("profile.yaml");
    write_yaml(
        &file,
        &serde_json::json!({
            "name": "lab-profile", "version": "1.3.0", "type": "cluster", "cloudType": "aws",
            "registryUid": "reg-public", "layers": layers,
            "variables": read(&dir.path().join("b").join("variables.json"))["variables"],
        }),
    );
    let api = serve_registry_for_b(dir.path());
    let snap = ops::snapshot_from_candidate(
        &file,
        || Ok(Palette::new(api.clone())),
        "t0",
        &Progress::silent(),
    )
    .unwrap();
    assert_eq!(snap.root, recorded_b.root);
    assert!(ops::compare(&recorded_b, &snap, "t1").same);

    // a layer that leaves values out gets the registry defaults, which differ from b's
    let mut doc: Value = serde_yaml::from_str(&fs::read_to_string(&file).unwrap()).unwrap();
    doc["layers"][4].as_object_mut().unwrap().remove("values");
    let defaults = cand.join("defaults.yaml");
    write_yaml(&defaults, &doc);
    let snap = ops::snapshot_from_candidate(
        &defaults,
        || Ok(Palette::new(api.clone())),
        "t0",
        &Progress::silent(),
    )
    .unwrap();
    let d = ops::compare(&recorded_b, &snap, "t1");
    let changed: Vec<&str> = d
        .packs
        .iter()
        .filter(|p| p.authored)
        .map(|p| p.name.as_str())
        .collect();
    assert_eq!(changed, vec!["prometheus-operator"]);
}

#[test]
fn candidate_errors_name_what_is_wrong() {
    let dir = tempfile::tempdir().unwrap();
    common::make_fixtures(dir.path());
    let api = serve_registry_for_b(dir.path());
    let cand = dir.path().join("errors");
    fs::create_dir_all(&cand).unwrap();
    let write = |name: &str, doc: Value| -> std::path::PathBuf {
        let p = cand.join(name);
        write_yaml(&p, &doc);
        p
    };
    let k8s = |extra: Value| {
        let mut l = serde_json::json!({"name": "kubernetes", "version": "1.30.2"});
        for (k, v) in extra.as_object().unwrap() {
            l[k] = v.clone();
        }
        serde_json::json!({"name": "p", "version": "1", "registryUid": "reg-public", "layers": [l]})
    };

    // local problems are found before connecting
    let f = write(
        "missing-values.yaml",
        k8s(serde_json::json!({"valuesFile": "values/nope.yaml"})),
    );
    let err =
        ops::snapshot_from_candidate(&f, never_connect, "t0", &Progress::silent()).unwrap_err();
    assert!(
        matches!(&err, Error::Io { path, .. } if path.ends_with("values/nope.yaml")),
        "{err}"
    );
    assert_eq!(err.exit_code(), 2);

    let f = write(
        "both.yaml",
        k8s(serde_json::json!({"values": "a: 1", "valuesFile": "v.yaml"})),
    );
    let err =
        ops::snapshot_from_candidate(&f, never_connect, "t0", &Progress::silent()).unwrap_err();
    assert!(matches!(err, Error::CandidateInvalid { .. }), "{err}");
    assert!(err.to_string().contains("kubernetes"), "{err}");

    let f = write(
        "typo.yaml",
        k8s(serde_json::json!({"valuesfile": "v.yaml"})),
    );
    let err =
        ops::snapshot_from_candidate(&f, never_connect, "t0", &Progress::silent()).unwrap_err();
    assert!(matches!(err, Error::CandidateInvalid { .. }), "{err}");
    assert!(err.to_string().contains("valuesfile"), "{err}");

    let f = write(
        "no-registry.yaml",
        serde_json::json!({"name": "p", "version": "1", "layers": [{"name": "kubernetes", "version": "1.30.2"}]}),
    );
    let err =
        ops::snapshot_from_candidate(&f, never_connect, "t0", &Progress::silent()).unwrap_err();
    assert!(matches!(err, Error::CandidateInvalid { .. }), "{err}");

    // registry problems need the registry
    let f = write(
        "unknown-version.yaml",
        k8s(serde_json::json!({"version": "9.9.9"})),
    );
    let err = ops::snapshot_from_candidate(
        &f,
        || Ok(Palette::new(api.clone())),
        "t0",
        &Progress::silent(),
    )
    .unwrap_err();
    match &err {
        Error::PackNotFound {
            name,
            version,
            registry,
            ..
        } => {
            assert_eq!(
                (name.as_str(), version.as_str(), registry.as_str()),
                ("kubernetes", "9.9.9", "reg-public")
            );
        }
        other => panic!("unexpected {other}"),
    }
    assert_eq!(err.kind(), "pack-not-found");

    let f = write("unknown-tag.yaml", k8s(serde_json::json!({"tag": "2.x"})));
    let err = ops::snapshot_from_candidate(
        &f,
        || Ok(Palette::new(api.clone())),
        "t0",
        &Progress::silent(),
    )
    .unwrap_err();
    assert!(matches!(err, Error::PackNotFound { .. }), "{err}");
    assert!(err.to_string().contains("2.x"), "{err}");

    let named = api.clone().with(
        "/v1/registries/metadata",
        serde_json::json!({"items": [{"name": "Public Repo", "uid": "reg-public"}, {"name": "Internal", "uid": "reg-int"}]}),
    );
    let f = write(
        "unknown-registry.yaml",
        serde_json::json!({"name": "p", "version": "1", "registry": "Nope", "layers": [{"name": "kubernetes", "version": "1.30.2"}]}),
    );
    let err = ops::snapshot_from_candidate(
        &f,
        || Ok(Palette::new(named.clone())),
        "t0",
        &Progress::silent(),
    )
    .unwrap_err();
    assert!(matches!(err, Error::RegistryNotFound { .. }), "{err}");
    assert!(err.to_string().contains("Public Repo, Internal"), "{err}");

    // an explicit tag is used when the pack version has it
    let f = write(
        "floating.yaml",
        k8s(serde_json::json!({"tag": "floating.x"})),
    );
    let snap = ops::snapshot_from_candidate(
        &f,
        || Ok(Palette::new(api.clone())),
        "t0",
        &Progress::silent(),
    )
    .unwrap();
    assert_eq!(snap.packs[0].tag, "floating.x");
}

#[test]
fn an_explicit_tag_equal_to_the_version_is_accepted_as_palette_does() {
    // Palette stores the tag a profile was authored with. A profile built with Terraform pins the
    // version itself, `24.04`, although the registry names that version `LTS__24.x`; the live
    // example run of 2026-09-16 recorded exactly that.
    let summary = serde_json::json!([{
        "metadata": {"uid": "pk-ubuntu-2404"},
        "spec": {"name": "ubuntu-aws", "version": "24.04", "registryUid": "reg", "digest": "sha256:u",
                 "layer": "os", "type": "spectro", "values": ""},
    }]);
    let entity = serde_json::json!({"tags": [
        {"tag": "LTS__24.x", "packUid": "pk-ubuntu-2404"},
        {"tag": "LTS__24.04", "packUid": "pk-ubuntu-2404"},
    ]});
    let api = serve_registry_pack(MemoryApi::new(), "ubuntu-aws", summary, entity, "reg");
    let dir = tempfile::tempdir().unwrap();
    let candidate = |name: &str, tag: Option<&str>| {
        let mut layer = serde_json::json!({"name": "ubuntu-aws", "version": "24.04"});
        if let Some(t) = tag {
            layer["tag"] = serde_json::json!(t);
        }
        let file = dir.path().join(name);
        write_yaml(
            &file,
            &serde_json::json!({"name": "p", "version": "1", "registryUid": "reg", "layers": [layer]}),
        );
        ops::snapshot_from_candidate(
            &file,
            || Ok(Palette::new(api.clone())),
            "t0",
            &Progress::silent(),
        )
    };

    assert_eq!(
        candidate("version.yaml", Some("24.04")).unwrap().packs[0].tag,
        "24.04"
    );
    assert_eq!(
        candidate("registry.yaml", Some("LTS__24.04"))
            .unwrap()
            .packs[0]
            .tag,
        "LTS__24.04"
    );
    assert_eq!(
        candidate("default.yaml", None).unwrap().packs[0].tag,
        "LTS__24.x",
        "without a tag, the registry's first tag for the version"
    );
    let err = candidate("typo.yaml", Some("LTS__99")).unwrap_err();
    assert!(matches!(err, Error::PackNotFound { .. }), "{err}");
}

#[test]
fn a_record_from_another_snapshot_schema_is_refused_by_name_and_extra_fields_are_ignored() {
    let dir = tempfile::tempdir().unwrap();
    common::make_fixtures(dir.path());
    let recorded = ops::snapshot_from_dir(&dir.path().join("a"), "a", "t0").unwrap();
    let mut doc: Value = serde_json::from_str(&recorded.to_json(true).unwrap()).unwrap();

    // a field this strata does not know: additive changes keep the schema id and still load
    doc["source"]["addedLater"] = serde_json::json!("x");
    let extra = dir.path().join("extra.json");
    fs::write(&extra, doc.to_string()).unwrap();
    assert_eq!(Snapshot::load(&extra).unwrap().root, recorded.root);

    // a newer schema id is refused, naming both ids
    doc["schema"] = serde_json::json!("strata-snapshot/2");
    let newer = dir.path().join("newer.json");
    fs::write(&newer, doc.to_string()).unwrap();
    let err = Snapshot::load(&newer).unwrap_err();
    match &err {
        Error::UnsupportedSchema {
            found, supported, ..
        } => {
            assert_eq!(
                (found.as_str(), *supported),
                ("strata-snapshot/2", "strata-snapshot/1")
            );
        }
        other => panic!("unexpected {other}"),
    }
    assert_eq!(err.kind(), "unsupported-schema");
    assert!(
        err.to_string().contains("strata-snapshot/2")
            && err.to_string().contains("strata-snapshot/1"),
        "{err}"
    );

    // a file that is not a strata document at all stays not-snapshot
    let other = dir.path().join("other.json");
    fs::write(&other, r#"{"schema": "something-else/1"}"#).unwrap();
    assert!(matches!(
        Snapshot::load(&other).unwrap_err(),
        Error::NotSnapshot(_)
    ));
}

// ---- names for projects and clusters -----------------------------------------------------------

#[test]
fn a_project_name_resolves_to_its_uid_and_an_unknown_name_lists_the_projects() {
    let tenant = Palette::new(MemoryApi::new().with(
        "/v1/dashboard/projects/metadata",
        projects(&[("prod", "p2"), ("lab", "p1")]),
    ));
    assert_eq!(tenant.project_uid_named("prod").unwrap(), "p2");

    let err = tenant.project_uid_named("staging").unwrap_err();
    assert!(matches!(err, Error::ProjectNotFound { .. }), "{err}");
    assert_eq!((err.kind(), err.exit_code()), ("project-not-found", 2));
    assert!(
        err.to_string().contains("'staging'") && err.to_string().contains("lab, prod"),
        "names the visible projects in order: {err}"
    );
    assert_eq!(
        err.to_json()["available"],
        serde_json::json!(["lab", "prod"])
    );
}

/// Cluster fixture `cluster-a` under [`CLUSTER_UID`], and a scope listing of `clusters`.
fn clusters_named(raw_root: &Path, clusters: &[(&str, &str)]) -> MemoryApi {
    let listing: Vec<Value> = clusters
        .iter()
        .map(|(name, uid)| listed_cluster(name, uid, "Running"))
        .collect();
    serve_cluster(MemoryApi::new(), raw_root, "cluster-a")
        .with("/v1/dashboard/spectroclusters/meta", Value::Array(listing))
        .in_project("p1")
}

fn cluster_named(name: &str) -> Ref {
    Ref::Cluster {
        uid: name.into(),
        profile: None,
    }
}

#[test]
fn a_cluster_ref_takes_a_uid_or_a_name() {
    let dir = tempfile::tempdir().unwrap();
    common::make_fixtures(dir.path());
    let api = clusters_named(dir.path(), &[("lab-cluster", CLUSTER_UID), ("other", "c2")]);
    let by_uid = snapshot_cluster(&api, None).unwrap();
    assert_eq!(
        api.calls("/v1/dashboard/spectroclusters/meta"),
        0,
        "a uid needs no listing"
    );
    let by_name = ops::snapshot_for(
        &cluster_named("lab-cluster"),
        || Ok(Palette::new(api.clone())),
        "t0",
        &Progress::silent(),
    )
    .unwrap();
    assert_eq!(
        by_name.source.cluster.as_ref().unwrap().uid,
        CLUSTER_UID,
        "the record carries the uid"
    );
    assert_eq!(by_name.root, by_uid.root);

    let err = ops::snapshot_for(
        &cluster_named("nope"),
        || Ok(Palette::new(api.clone())),
        "t0",
        &Progress::silent(),
    )
    .unwrap_err();
    assert!(matches!(err, Error::ClusterNotFound { .. }), "{err}");
    assert_eq!((err.kind(), err.exit_code()), ("cluster-not-found", 2));
    assert!(
        err.to_string().contains("'nope'") && err.to_string().contains("lab-cluster, other"),
        "{err}"
    );
}

#[test]
fn a_cluster_name_that_matches_several_clusters_is_an_error_and_a_uid_wins_over_a_name() {
    let dir = tempfile::tempdir().unwrap();
    common::make_fixtures(dir.path());
    let twins = clusters_named(dir.path(), &[("twin", "c7"), ("twin", "c8")]);
    let err = ops::snapshot_for(
        &cluster_named("twin"),
        || Ok(Palette::new(twins.clone())),
        "t0",
        &Progress::silent(),
    )
    .unwrap_err();
    assert!(matches!(err, Error::ClusterNameAmbiguous { .. }), "{err}");
    assert_eq!(err.kind(), "cluster-name-ambiguous");
    assert!(
        err.to_string().contains("c7, c8"),
        "lists the candidates: {err}"
    );

    // another cluster is named after this cluster's uid; the uid is fetched first
    let api = clusters_named(
        dir.path(),
        &[("lab-cluster", CLUSTER_UID), (CLUSTER_UID, "c9")],
    );
    let snap = snapshot_cluster(&api, None).unwrap();
    assert_eq!(snap.source.cluster.unwrap().uid, CLUSTER_UID);
    assert_eq!(api.calls("/v1/dashboard/spectroclusters/meta"), 0);
}

#[test]
fn a_cluster_record_taken_by_name_verifies_by_uid_after_a_rename() {
    let dir = tempfile::tempdir().unwrap();
    common::make_fixtures(dir.path());
    let api = clusters_named(dir.path(), &[("lab-cluster", CLUSTER_UID)]);
    let recorded = ops::snapshot_for(
        &cluster_named("lab-cluster"),
        || Ok(Palette::new(api.clone())),
        "t0",
        &Progress::silent(),
    )
    .unwrap();

    let mut renamed_doc = read(&dir.path().join("cluster-a/cluster.json"));
    renamed_doc["metadata"]["name"] = Value::from("lab-cluster-renamed");
    let renamed = api
        .clone()
        .with(format!("/v1/spectroclusters/{CLUSTER_UID}"), renamed_doc)
        .with(
            "/v1/dashboard/spectroclusters/meta",
            Value::Array(vec![listed_cluster(
                "lab-cluster-renamed",
                CLUSTER_UID,
                "Running",
            )]),
        );
    let v = ops::verify(&recorded, &Palette::new(renamed), "t1", &Progress::silent()).unwrap();
    assert!(v.unchanged, "the old name is gone, the uid is not");
}

// ---- cluster variable values -------------------------------------------------------------------

fn cluster_snapshot(raw_root: &Path, dir: &str) -> Snapshot {
    ops::snapshot_from_dir(&raw_root.join(dir), &format!("cluster:{CLUSTER_UID}"), "t0").unwrap()
}

#[test]
fn a_cluster_snapshot_records_variable_values_outside_the_root_and_never_a_secret() {
    let dir = tempfile::tempdir().unwrap();
    common::make_fixtures(dir.path());
    let dev = cluster_snapshot(dir.path(), "cluster-a");
    let cluster = dev.source.cluster.as_ref().unwrap();
    let values: Vec<(&str, &str, Option<&str>, bool)> = cluster
        .variables
        .iter()
        .map(|v| {
            (
                v.profile.as_str(),
                v.name.as_str(),
                v.value.as_deref(),
                v.sensitive,
            )
        })
        .collect();
    assert_eq!(
        values,
        vec![
            ("lab-profile", "cluster_env", Some("dev"), false),
            ("lab-profile", "grafana_password", None, true),
        ]
    );
    assert_eq!(cluster.variables[0].profile_uid, UID_A);

    let json = dev.to_json(true).unwrap();
    assert!(
        !json.contains(SECRET_VALUE),
        "a sensitive value is in no document"
    );
    let staging = cluster_snapshot(dir.path(), "cluster-a-staging");
    assert_eq!(staging.root, dev.root, "values are outside the root");

    let path = dir.path().join("dev.json");
    fs::write(&path, json).unwrap();
    assert_eq!(
        Snapshot::load(&path)
            .unwrap()
            .source
            .cluster
            .unwrap()
            .variables,
        dev.source.cluster.unwrap().variables
    );
}

#[test]
fn two_clusters_on_one_release_with_different_values_compare_identical_and_show_the_values() {
    let dir = tempfile::tempdir().unwrap();
    common::make_fixtures(dir.path());
    let dev = cluster_snapshot(dir.path(), "cluster-a");
    let staging = cluster_snapshot(dir.path(), "cluster-a-staging");
    let d = ops::compare(&dev, &staging, "t1");
    assert!(d.same, "one release");
    let values = d
        .cluster_variables
        .as_ref()
        .expect("both sides are clusters");
    assert!(values.changed);
    assert_eq!(values.values.len(), 1, "{:?}", values.values);
    let c = &values.values[0];
    assert_eq!(
        (c.profile.as_str(), c.name.as_str(), c.op),
        ("lab-profile", "cluster_env", strata::diff::Op::Changed)
    );
    assert_eq!(
        (c.old.as_deref(), c.new.as_deref(), c.sensitive),
        (Some("dev"), Some("staging"), false)
    );

    let text = render::renderer(Format::Terminal, Paint::off(), false)
        .render(&d, &dev, &staging, false)
        .unwrap();
    assert!(
        text.contains(r#"~ lab-profile.cluster_env: "dev" -> "staging""#),
        "{text}"
    );
    let md = render::renderer(Format::Markdown, Paint::off(), false)
        .render(&d, &dev, &staging, false)
        .unwrap();
    assert!(
        md.contains("- lab-profile.cluster_env: \"dev\"\n+ lab-profile.cluster_env: \"staging\"\n"),
        "the markdown diff style: {md}"
    );
    let json = render::renderer(Format::Json, Paint::off(), true)
        .render(&d, &dev, &staging, false)
        .unwrap();
    assert!(json.contains("\"clusterVariables\""));
    assert!(!json.contains(SECRET_VALUE));

    let same = ops::compare(&dev, &dev, "t1");
    assert!(!same.cluster_variables.unwrap().changed);
    let profile = ops::snapshot_from_dir(&dir.path().join("a"), "a", "t0").unwrap();
    assert!(
        ops::compare(&profile, &dev, "t1")
            .cluster_variables
            .is_none(),
        "a profile has no cluster values to compare"
    );
}

#[test]
fn verify_names_a_cluster_value_changed_since_the_record_without_calling_it_drift() {
    let dir = tempfile::tempdir().unwrap();
    common::make_fixtures(dir.path());
    let recorded = snapshot_cluster(
        &serve_cluster(MemoryApi::new(), dir.path(), "cluster-a"),
        None,
    )
    .unwrap();
    let live = serve_cluster(MemoryApi::new(), dir.path(), "cluster-a-staging");

    let v = ops::verify(
        &recorded,
        &Palette::new(live.clone()),
        "t1",
        &Progress::silent(),
    )
    .unwrap();
    assert!(v.unchanged, "a value is not part of the release");
    assert!(v.diff.cluster_variables.as_ref().unwrap().changed);
    let renderer = render::renderer(Format::Terminal, Paint::off(), false);
    let verdict = renderer
        .verdict(v.diff.verify.as_ref().unwrap(), &recorded.source)
        .unwrap();
    assert!(
        verdict.starts_with("OK:") && verdict.contains("1 variable value changed"),
        "{verdict}"
    );

    let path = dir.path().join("cluster.json");
    fs::write(&path, recorded.to_json(true).unwrap()).unwrap();
    let s = ops::verify_many(
        &[path],
        None,
        |_| Ok(Palette::new(live.clone())),
        "t1",
        &Progress::silent(),
    );
    assert_eq!(
        (s.records[0].status, s.exit_code()),
        (RecordStatus::Unchanged, 0)
    );
    let named = s.records[0]
        .cluster_variables
        .as_ref()
        .expect("the summary names the changed value");
    assert_eq!(named.values[0].name, "cluster_env");
    let lines = renderer.render_verify_summary(&s).unwrap();
    assert!(lines.contains("1 variable value changed"), "{lines}");
    assert!(!serde_json::to_string(&s).unwrap().contains(SECRET_VALUE));
}
