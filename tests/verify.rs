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
        "variable values are run-time input, never recorded"
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
