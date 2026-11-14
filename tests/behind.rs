//! The profile versions of `strata behind` (#707): the newer published versions of each profile
//! that a record holds. The document of a version gives the published versions of its profile in
//! `spec.versions[]`, in no order, and strata orders them. With the impact, `behind` also compares
//! each profile version that is behind with the newest version (#708). For a pin that is behind,
//! it names the newer profile version that carries a newer version of the pack (#709). On a cluster
//! with many profiles, each row names the profile that holds its pack, the counts of the heading
//! add up, `--behind-only` omits the rows that are current, and a pin names the newest patch of
//! its own minor version (#622). The move of a profile version names the required variables that
//! it adds, and a move of Kubernetes that skips a minor version (#623).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde_json::{Value, json};
use strata::api::MemoryApi;
use strata::api::codes::{INVALID_VARIABLE_CODE, K8S_UPGRADE_NOT_SUPPORTED_CODE};
use strata::closed_set::ClosedSet;
use strata::diff::{ChangeFlag, Status};
use strata::model::scope::ProfileScope;
use strata::ops;
use strata::render::RowState;
use strata::snapshot::{AppliedProfile, ClusterSource, ClusterTier, Snapshot, Tier};

use crate::common::{
    self, PROFILE_LISTING, PUBLIC_REGISTRY, PackFixture, REGISTRY_LISTING, WITH_IMPACT, behind_of,
    manifests_answer, manifests_route, no_manifests, no_variables, profile_document, profile_route,
    serve_profile, serve_registry_pack, snapshot_of, template,
};

/// The uid of `barney-addons@1.3.1`, the version that the records hold.
const HELD: &str = "64f0000000000000000000b1";

/// The time that Palette gives the create of each version of the tests.
const CREATED: &str = "2026-11-09T08:00:00.000Z";

/// The document of the profile version `uid`, `name` at `version`, with one manifest pack and so
/// no pin. `versions` are the published versions of its profile, `(uid, version)`, as
/// `spec.versions[]` gives them. The scope is `project`.
fn version_document(uid: &str, name: &str, version: &str, versions: &[(&str, &str)]) -> Value {
    let pack = json!({"name": "namespaces", "layer": "addon", "type": "manifest", "tag": "1.0.0",
        "version": "1.0.0", "packUid": format!("{uid}p"), "manifests": []});
    let mut doc = profile_document(
        uid,
        name,
        version,
        template("all", "add-on", json!([pack])),
        None,
    );
    doc["metadata"]["annotations"] = json!({"scope": ProfileScope::Project});
    doc["metadata"]["creationTimestamp"] = json!(CREATED);
    doc["spec"]["versions"] = versions
        .iter()
        .map(|(uid, version)| json!({"uid": uid, "version": version}))
        .collect();
    doc
}

/// Serve each version of `versions` of the profile `name` with its document. Each document lists
/// all of `versions`, in the order given.
fn serve_versions(api: MemoryApi, name: &str, versions: &[(&str, &str)]) -> MemoryApi {
    versions.iter().fold(api, |api, (uid, version)| {
        serve_profile(
            api,
            uid,
            version_document(uid, name, version, versions),
            no_manifests(),
            no_variables(),
        )
    })
}

/// The pipeline of the team published 1.4.0 and a pre-release of 1.5.0 after `barney-addons@1.3.1`.
/// The list of Palette has no order.
const BARNEY: &[(&str, &str)] = &[
    ("64f0000000000000000000b2", "1.4.0"),
    ("64f0000000000000000000b0", "1.2.0"),
    ("64f0000000000000000000b3", "1.5.0-rc.1"),
    (HELD, "1.3.1"),
];

/// Write the record of the profile version `uid` from `api` to `dir/name`, and give its path.
fn record_of(api: &MemoryApi, uid: &str, dir: &Path, name: &str) -> PathBuf {
    let path = dir.join(name);
    common::write_snapshot(&path, &snapshot_of(api, uid).unwrap(), false);
    path
}

/// The newer versions of the first profile version of `r`, newest first.
fn newer_of(r: &ops::BehindReport) -> Vec<&str> {
    r.profiles[0]
        .newer
        .iter()
        .map(|n| n.version.as_str())
        .collect()
}

#[test]
fn behind_names_the_newer_published_version_of_a_profile_and_a_pre_release_only_when_the_option_asks()
 {
    let dir = tempfile::tempdir().unwrap();
    let api = serve_versions(MemoryApi::new(), "barney-addons", BARNEY);
    let record = record_of(&api, HELD, dir.path(), "barney.json");

    let r = behind_of(
        &api,
        std::slice::from_ref(&record),
        &ops::BehindOptions::default(),
    );
    assert_eq!(newer_of(&r), ["1.4.0"]);
    let held = &r.profiles[0];
    assert!(held.behind && !held.gone && held.error.is_none());
    assert_eq!(held.scope, Some(ProfileScope::Project));
    assert_eq!(held.newer[0].uid, BARNEY[0].0);
    assert_eq!(held.newer[0].created, CREATED);
    assert_eq!(
        (
            r.summary.profiles,
            r.summary.profiles_behind,
            r.summary.profile_errors
        ),
        (1, 1, 0)
    );
    // a profile version that is behind gives the exit of a pin that is behind: the record pins
    // no pack
    assert!(r.pins.is_empty());
    assert_eq!(r.exit_code(), 1);
    let text = common::terminal().render_behind(&r, false).unwrap();
    assert!(
        text.starts_with("no pins from 1 record: 0 behind; 1 profile version: 1 behind\n"),
        "{text}"
    );
    assert!(
        text.contains("~ [profile] barney-addons  1.3.1  project  behind: 1.4.0\n"),
        "{text}"
    );
    assert!(
        text.contains(&format!("      1.4.0 created {CREATED}\n")),
        "{text}"
    );

    let options = ops::BehindOptions {
        pre_release: true,
        ..Default::default()
    };
    let r = behind_of(&api, &[record], &options);
    assert_eq!(newer_of(&r), ["1.5.0-rc.1", "1.4.0"]);
    assert_eq!(r.profiles[0].newer[0].created, CREATED);
    assert!(r.profiles[0].newer[1].created.is_empty(), "only the newest");
}

/// Issue #616. A version that does not parse reads as its first three numbers. A tail of numbers
/// is above them, and a tail of text is a pre-release.
#[test]
fn behind_orders_the_versions_of_a_profile_by_the_rule_of_the_pins() {
    let dir = tempfile::tempdir().unwrap();
    let versions = [
        ("64f0000000000000000000c1", "1.4.0.develop"),
        ("64f0000000000000000000c2", "1.3.1.1"),
        (HELD, "1.3.1"),
        ("64f0000000000000000000c3", "1.10.0"),
        ("64f0000000000000000000c4", "1.9.0"),
    ];
    let api = serve_versions(MemoryApi::new(), "barney-addons", &versions);
    let record = record_of(&api, HELD, dir.path(), "barney.json");
    let r = behind_of(
        &api,
        std::slice::from_ref(&record),
        &ops::BehindOptions::default(),
    );
    assert_eq!(newer_of(&r), ["1.10.0", "1.9.0", "1.3.1.1"]);
    let options = ops::BehindOptions {
        pre_release: true,
        ..Default::default()
    };
    let r = behind_of(&api, &[record], &options);
    assert_eq!(
        newer_of(&r),
        ["1.10.0", "1.9.0", "1.4.0.develop", "1.3.1.1"]
    );
}

/// Palette selects the versions of a profile: the same name in the scope of the version. A
/// listing of a project also holds the versions of the tenant, and strata does not read it.
#[test]
fn behind_names_only_the_versions_that_the_document_gives_for_the_scope_of_the_version() {
    let dir = tempfile::tempdir().unwrap();
    // the tenant holds barney-addons 1.6.0, and the project listing holds it beside the project
    // versions. The document of the project version lists the project versions alone
    let tenant = ("64f0000000000000000000d1", "1.6.0");
    let listed: Vec<Value> = BARNEY
        .iter()
        .chain([&tenant])
        .map(|(uid, version)| {
            json!({"metadata": {"name": "barney-addons", "uid": uid},
            "spec": {"version": version, "cloudType": "all"}})
        })
        .collect();
    let mut tenant_doc = version_document(tenant.0, "barney-addons", tenant.1, &[tenant]);
    tenant_doc["metadata"]["annotations"] = json!({"scope": ProfileScope::Tenant});
    let api = serve_profile(
        serve_versions(MemoryApi::new(), "barney-addons", BARNEY),
        tenant.0,
        tenant_doc,
        no_manifests(),
        no_variables(),
    )
    .with(PROFILE_LISTING, json!({"items": listed}));
    let project = record_of(&api, HELD, dir.path(), "project.json");
    let r = behind_of(&api, &[project], &ops::BehindOptions::default());
    assert_eq!(newer_of(&r), ["1.4.0"]);
    assert_eq!(api.calls(PROFILE_LISTING), 0);

    // the tenant version is the newest of its own scope: the project versions are not in its list
    let record = record_of(&api, tenant.0, dir.path(), "tenant.json");
    let r = behind_of(&api, &[record], &ops::BehindOptions::default());
    assert!(!r.profiles[0].behind, "{:?}", r.profiles[0]);
    assert_eq!(r.profiles[0].scope, Some(ProfileScope::Tenant));
    assert_eq!(r.exit_code(), 0);
}

/// The record of the cluster `payments` from the record `record`: it runs `profiles`, `(uid,
/// name, version)`.
fn cluster_record(record: &Path, profiles: &[(&str, &str, &str)], dir: &Path) -> PathBuf {
    let mut snap = Snapshot::load(record).unwrap();
    snap.source.cluster = Some(ClusterSource {
        uid: "c1".to_string(),
        name: "payments".to_string(),
        profiles: profiles
            .iter()
            .map(|(uid, name, version)| AppliedProfile {
                uid: uid.to_string(),
                name: name.to_string(),
                version: version.to_string(),
                kind: "add-on".to_string(),
            })
            .collect(),
        ..Default::default()
    });
    let path = dir.join("payments.json");
    common::write_snapshot(&path, &snap, false);
    path
}

#[test]
fn behind_on_a_record_of_a_cluster_names_the_newer_versions_of_each_profile_that_the_cluster_runs()
{
    let dir = tempfile::tempdir().unwrap();
    let base = [
        ("64f0000000000000000000e0", "2.0.0"),
        ("64f0000000000000000000e1", "2.1.0"),
    ];
    let api = serve_versions(
        serve_versions(MemoryApi::new(), "barney-addons", BARNEY),
        "barney-base",
        &base,
    );
    let record = record_of(&api, HELD, dir.path(), "barney.json");
    let cluster = cluster_record(
        &record,
        &[
            (base[0].0, "barney-base", "2.0.0"),
            (HELD, "barney-addons", "1.3.1"),
        ],
        dir.path(),
    );
    let r = behind_of(
        &api,
        std::slice::from_ref(&cluster),
        &ops::BehindOptions::default(),
    );
    let held: Vec<(&str, &str, Vec<&str>)> = r
        .profiles
        .iter()
        .map(|p| {
            let newer = p.newer.iter().map(|n| n.version.as_str()).collect();
            (p.name.as_str(), p.version.as_str(), newer)
        })
        .collect();
    assert_eq!(
        held,
        [
            ("barney-addons", "1.3.1", vec!["1.4.0"]),
            ("barney-base", "2.0.0", vec!["2.1.0"]),
        ]
    );
    // the record covers two profiles, so its title gives their count (#624)
    for p in &r.profiles {
        let subjects: Vec<&str> = p.pinned_by.iter().map(|b| b.subject.as_str()).collect();
        assert_eq!(subjects, ["2 profiles on cluster payments"]);
    }

    // the record of the version and the record of the cluster hold one version: one entry, with
    // both records, and one read of its document
    let before = api.calls(&profile_route(HELD));
    let r = behind_of(&api, &[record, cluster], &ops::BehindOptions::default());
    assert_eq!(r.profiles.len(), 2);
    let barney = &r.profiles[0];
    assert_eq!(barney.pinned_by.len(), 2, "{:?}", barney.pinned_by);
    assert_eq!(api.calls(&profile_route(HELD)) - before, 1);
}

#[test]
fn a_version_that_palette_no_longer_holds_is_gone_and_a_failed_read_is_not_checked() {
    let dir = tempfile::tempdir().unwrap();
    let api = serve_versions(MemoryApi::new(), "barney-addons", BARNEY);
    let record = record_of(&api, HELD, dir.path(), "barney.json");

    // the version is gone: no error, and the exit of a version that is current
    let gone = MemoryApi::new().failing(profile_route(HELD), 404, "not found");
    let r = behind_of(
        &gone,
        std::slice::from_ref(&record),
        &ops::BehindOptions::default(),
    );
    assert!(r.profiles[0].gone && !r.profiles[0].behind && r.profiles[0].error.is_none());
    assert_eq!(r.exit_code(), 0);
    let text = common::terminal().render_behind(&r, false).unwrap();
    assert!(
        text.contains("  [profile] barney-addons  1.3.1  -  gone\n"),
        "{text}"
    );

    // another error: the version is not checked, and the exit says so
    let failing = MemoryApi::new().failing(profile_route(HELD), 500, "boom");
    let r = behind_of(&failing, &[record], &ops::BehindOptions::default());
    assert!(r.profiles[0].error.is_some() && r.profiles[0].newer.is_empty());
    assert_eq!(r.summary.profile_errors, 1);
    assert_eq!(r.exit_code(), 2);
    let text = common::terminal().render_behind(&r, false).unwrap();
    assert!(
        text.starts_with(
            "no pins from 1 record: 0 behind; 1 profile version: 0 behind, 1 not checked\n"
        ),
        "{text}"
    );
    assert!(
        text.contains("  [profile] barney-addons  1.3.1  -  not checked\n"),
        "{text}"
    );
}

#[test]
fn the_json_and_the_markdown_of_behind_hold_the_profile_versions() {
    let dir = tempfile::tempdir().unwrap();
    let api = serve_versions(MemoryApi::new(), "barney-addons", BARNEY);
    let record = record_of(&api, HELD, dir.path(), "barney.json");
    let r = behind_of(
        &api,
        std::slice::from_ref(&record),
        &ops::BehindOptions::default(),
    );
    let doc = serde_json::to_value(&r).unwrap();
    assert_eq!(
        doc["profiles"],
        json!([{"name": "barney-addons", "version": "1.3.1", "uid": HELD, "scope": ProfileScope::Project,
            "pinnedBy": [{"path": record.display().to_string(), "subject": "barney-addons@1.3.1"}],
            "newer": [{"version": "1.4.0", "uid": BARNEY[0].0, "created": CREATED}],
            "behind": true}])
    );
    assert_eq!(doc["summary"]["profiles"], 1);
    assert_eq!(doc["summary"]["profilesBehind"], 1);
    assert!(
        doc["summary"].get("profileErrors").is_none(),
        "a count of 0 is absent"
    );
    // `--since` reads the document again
    let back: ops::BehindReport = serde_json::from_value(doc).unwrap();
    assert_eq!(back.profiles[0].newer[0].version, "1.4.0");

    let markdown = common::markdown().render_behind(&r, false).unwrap();
    assert!(
        markdown.contains(&format!(
            "| profile | version | scope | status | bump | pinned by |\n|---|---|---|---|---|---|\n\
             | `barney-addons` | `1.3.1` | project | behind: 1.4.0; 1.4.0 created {CREATED} | - | `{}` (barney-addons@1.3.1) |\n",
            record.display()
        )),
        "{markdown}"
    );
    assert!(
        !markdown.contains("| pack |"),
        "no pin, so no table of pins: {markdown}"
    );
}

// ---- the move to the newest version (#708) ------------------------------------------------------

/// The uid of `barney-addons@1.4.0`, the newest version of [`BARNEY`] that is no pre-release.
const NEWEST: &str = BARNEY[0].0;

/// The packs of `barney-addons@1.3.1`, from no registry, so that the record has no pin.
fn packs_held() -> [PackFixture<'static>; 3] {
    [
        PackFixture::new(
            "fred-compositions",
            "addon",
            "0.1.0",
            "fc010",
            "",
            "sha256:f010",
            "compositions: []\n",
        ),
        PackFixture::new(
            "wilma-values",
            "addon",
            "1.0.0",
            "wv100",
            "",
            "sha256:w100",
            "replicas: 1\n",
        ),
        PackFixture::new(
            "dino-logs",
            "addon",
            "2.0.0",
            "dl200",
            "",
            "sha256:d200",
            "level: info\n",
        ),
    ]
}

/// The packs of `barney-addons@1.4.0`: fred-compositions moves to 0.2.0, a value of wilma-values
/// changes, the move adds pebbles-dns, and it removes dino-logs.
fn packs_newest() -> [PackFixture<'static>; 3] {
    [
        PackFixture::new(
            "fred-compositions",
            "addon",
            "0.2.0",
            "fc020",
            "",
            "sha256:f020",
            "compositions: []\n",
        ),
        PackFixture::new(
            "wilma-values",
            "addon",
            "1.0.0",
            "wv100",
            "",
            "sha256:w100",
            "replicas: 2\n",
        ),
        PackFixture::new(
            "pebbles-dns",
            "addon",
            "1.0.0",
            "pd100",
            "",
            "sha256:p100",
            "zone: quarry\n",
        ),
    ]
}

/// Serve the version `uid` of `barney-addons` at `version` with the packs `packs`. `versions` are
/// the published versions of the profile, as [`version_document`] takes them.
fn serve_packs(
    api: MemoryApi,
    (uid, version): (&str, &str),
    versions: &[(&str, &str)],
    packs: &[PackFixture],
) -> MemoryApi {
    serve_packs_with(api, (uid, version), versions, packs, no_variables())
}

/// [`serve_packs`], where the answer of the variables of the version is `variables`.
fn serve_packs_with(
    api: MemoryApi,
    (uid, version): (&str, &str),
    versions: &[(&str, &str)],
    packs: &[PackFixture],
    variables: Value,
) -> MemoryApi {
    let mut doc = version_document(uid, "barney-addons", version, versions);
    doc["spec"]["published"]["packs"] = packs.iter().map(|p| p.pack_ref(&[])).collect();
    let manifests = packs.iter().map(|p| p.pack_manifests(Vec::new())).collect();
    serve_profile(api, uid, doc, manifests_answer(manifests), variables)
}

/// [`serve_versions`] of [`BARNEY`], where 1.3.1 holds [`packs_held`] and 1.4.0 holds
/// [`packs_newest`].
fn serve_move() -> MemoryApi {
    let api = serve_versions(MemoryApi::new(), "barney-addons", BARNEY);
    let api = serve_packs(api, (HELD, "1.3.1"), BARNEY, &packs_held());
    serve_packs(api, (NEWEST, "1.4.0"), BARNEY, &packs_newest())
}

#[test]
fn behind_names_what_the_move_to_the_newest_profile_version_does_as_compare_gives_it() {
    let dir = tempfile::tempdir().unwrap();
    let api = serve_move();
    let record = record_of(&api, HELD, dir.path(), "barney.json");
    let reads = |uid: &str| api.calls(&profile_route(uid));
    let before = (reads(HELD), reads(NEWEST));
    let r = behind_of(&api, std::slice::from_ref(&record), &WITH_IMPACT);
    let held = &r.profiles[0];
    let bump = held.bump.as_ref().expect("the version is behind 1.4.0");
    // one read of the document of each of the two versions: for the newer versions, the time of
    // the create, and the move
    assert_eq!((reads(HELD), reads(NEWEST)), (before.0 + 1, before.1 + 1));

    // `compare barney-addons@1.3.1 barney-addons@1.4.0` names the same impact and the same flags
    let d = ops::compare(
        &snapshot_of(&api, HELD).unwrap(),
        &snapshot_of(&api, NEWEST).unwrap(),
        "t0",
    );
    assert_eq!(bump.to, "1.4.0");
    assert_eq!(bump.impact, d.summary.max_impact);
    let mut flags: Vec<ChangeFlag> = d.packs.iter().flat_map(|p| p.flags()).collect();
    flags.sort_by_key(|f| ChangeFlag::ALL.iter().position(|x| x == f));
    flags.dedup();
    let named: Vec<&str> = bump.flags.iter().map(|f| f.as_str()).collect();
    let flag_texts: Vec<&str> = flags.iter().map(|f| f.as_str()).collect();
    assert_eq!(named, flag_texts);
    assert_eq!(named, ["IDENTITY", "UPSTREAM", "AUTHORED"]);
    assert!(bump.palette_validation.is_none() && bump.palette_validation_error.is_none());
    assert!(
        api.posted().is_empty(),
        "strata asks Palette nothing about the move of a profile"
    );

    // the packs of the move are the packs that compare changes, adds, or removes, in its order
    let compared: Vec<&str> = d
        .packs
        .iter()
        .filter(|p| p.status != Status::Unchanged)
        .map(|p| p.key.as_str())
        .collect();
    let keys: Vec<&str> = bump.packs.iter().map(|p| p.key.as_str()).collect();
    assert_eq!(keys, compared);

    let text = common::terminal().render_behind(&r, false).unwrap();
    let line = format!(
        "      bump to 1.4.0: {} IDENTITY UPSTREAM AUTHORED\n",
        bump.impact
    );
    assert!(
        text.contains(&format!("      1.4.0 created {CREATED}\n{line}")),
        "the line comes under the time of the create: {text}"
    );
    let markdown = common::markdown().render_behind(&r, false).unwrap();
    assert!(
        markdown.contains(&format!(
            "| bump to 1.4.0: {} IDENTITY UPSTREAM AUTHORED |",
            bump.impact
        )),
        "{markdown}"
    );

    // the JSON holds the move: the version changes, the packs that it adds or removes, and the
    // values that change. `--since` reads it again
    let doc = serde_json::to_value(&r).unwrap();
    assert_eq!(
        doc["profiles"][0]["bump"]["packs"],
        json!([
            {"key": "addon/dino-logs", "status": "removed", "from": "2.0.0"},
            {"key": "addon/fred-compositions", "status": "changed", "from": "0.1.0", "to": "0.2.0",
                "flags": ["IDENTITY", "UPSTREAM"]},
            {"key": "addon/pebbles-dns", "status": "added", "to": "1.0.0"},
            {"key": "addon/wilma-values", "status": "changed", "from": "1.0.0", "to": "1.0.0",
                "flags": ["AUTHORED"], "valuesChanged": true},
        ])
    );
    let back: ops::BehindReport = serde_json::from_value(doc).unwrap();
    assert_eq!(back.profiles[0].bump.as_ref().unwrap().packs.len(), 4);
}

#[test]
fn a_profile_version_with_no_newer_version_and_a_run_with_no_impact_show_no_move() {
    let dir = tempfile::tempdir().unwrap();
    let api = serve_move();
    let record = record_of(&api, HELD, dir.path(), "barney.json");
    let newest = record_of(&api, NEWEST, dir.path(), "newest.json");
    let manifests = |uid: &str| api.calls(&manifests_route(uid));
    let before = (manifests(HELD), manifests(NEWEST));

    // the newest version is current, so it has no move, and strata compares nothing
    let r = behind_of(&api, std::slice::from_ref(&newest), &WITH_IMPACT);
    assert!(!r.profiles[0].behind && r.profiles[0].bump.is_none());
    let text = common::terminal().render_behind(&r, false).unwrap();
    assert!(!text.contains("bump to"), "{text}");

    // --no-impact names the newer version, and compares nothing
    let r = behind_of(
        &api,
        std::slice::from_ref(&record),
        &ops::BehindOptions::default(),
    );
    assert!(r.profiles[0].behind && r.profiles[0].bump.is_none());
    let text = common::terminal().render_behind(&r, false).unwrap();
    assert!(!text.contains("bump to"), "{text}");
    assert_eq!((manifests(HELD), manifests(NEWEST)), before);
}

/// The pack of the `k8s` layer of `barney-addons` at `version`, from no registry, so that the
/// record has no pin of it.
fn kubernetes_at(version: &str) -> PackFixture<'_> {
    PackFixture::new("kubernetes", "k8s", version, "k8s", "", "", "k: 1\n")
}

#[test]
fn the_move_to_the_newest_profile_version_names_the_required_variables_that_it_adds_and_a_skip() {
    let dir = tempfile::tempdir().unwrap();
    let api = serve_versions(MemoryApi::new(), "barney-addons", BARNEY);
    // 1.3.1 runs Kubernetes 1.34.9 and has the variable region. 1.4.0 moves Kubernetes to 1.36.2,
    // and adds three variables: owner needs a value, tier has a default value, and note is not
    // required
    let held = [packs_held()[0], kubernetes_at("1.34.9")];
    let newest = [packs_newest()[0], kubernetes_at("1.36.2")];
    let region = common::variable("region", "Region", "", false);
    let api = serve_packs_with(
        api,
        (HELD, "1.3.1"),
        BARNEY,
        &held,
        json!({"variables": [region]}),
    );
    let mut note = common::variable("note", "Note", "", false);
    note["required"] = json!(false);
    let variables = json!({"variables": [
        region,
        common::variable("owner", "Owner", "", false),
        common::variable("tier", "Tier", "gold", false),
        note,
    ]});
    let api = serve_packs_with(api, (NEWEST, "1.4.0"), BARNEY, &newest, variables);
    let record = record_of(&api, HELD, dir.path(), "barney.json");
    let r = behind_of(&api, &[record], &WITH_IMPACT);
    let bump = r.profiles[0].bump.as_ref().expect("1.3.1 is behind 1.4.0");
    assert_eq!(
        bump.update_checks
            .required_variables
            .as_ref()
            .unwrap()
            .names,
        ["owner"]
    );
    let skip = bump.update_checks.skipped_minor.as_ref().unwrap();
    assert_eq!(
        (
            skip.from.as_str(),
            skip.to.as_str(),
            skip.next_step.as_deref()
        ),
        ("1.34.9", "1.36.2", None),
        "strata reads no registry for the move of a profile version"
    );
    assert!(
        api.posted().is_empty(),
        "strata asks Palette nothing about the move of a profile"
    );

    // the two lines come under the bump line, the variables first
    let text = common::terminal().render_behind(&r, false).unwrap();
    let variables_line = format!(
        "the move adds the required variable owner with no default value: give it a value at the \
         update, or Palette refuses the update with {INVALID_VARIABLE_CODE}"
    );
    let skip_line = format!(
        "kubernetes 1.34.9 -> 1.36.2 skips a minor version: Palette refuses the update with \
         {K8S_UPGRADE_NOT_SUPPORTED_CODE}; move to a version of 1.35 first"
    );
    assert!(
        text.contains(&format!(
            "{} IDENTITY UPSTREAM\n      {variables_line}\n      {skip_line}\n",
            bump.impact
        )),
        "{text}"
    );
    // the markdown writes the `->` of the skip as `-&gt;`
    let markdown = common::markdown().render_behind(&r, false).unwrap();
    assert!(
        markdown.contains(&format!(
            "<br>{variables_line}<br>{} |",
            strata::text::html_text(&skip_line)
        )),
        "{markdown}"
    );
    let doc = serde_json::to_value(&r).unwrap();
    assert_eq!(
        doc["profiles"][0]["bump"]["updateChecks"]["requiredVariables"]["names"],
        json!(["owner"])
    );
    assert_eq!(
        doc["profiles"][0]["bump"]["updateChecks"]["skippedMinor"]["nextMinor"],
        "1.35"
    );
}

#[test]
fn the_move_names_a_required_dropdown_variable_with_no_default_option_and_not_one_with_a_default_option()
 {
    let dir = tempfile::tempdir().unwrap();
    let api = serve_versions(MemoryApi::new(), "barney-addons", BARNEY);
    let api = serve_packs_with(
        api,
        (HELD, "1.3.1"),
        BARNEY,
        &packs_held(),
        json!({"variables": []}),
    );
    // 1.4.0 adds two required dropdown variables with no default value. Palette gives the mark
    // `default` on each option, and only zone has an option with the mark
    let dropdown = |name: &str, default: bool| {
        let mut v = common::variable(name, name, "", false);
        v["inputType"] = json!("dropdown");
        v["options"] = json!([
            {"label": "a", "value": "1", "description": "", "default": false},
            {"label": "b", "value": "2", "description": "", "default": default},
        ]);
        v
    };
    let variables = json!({"variables": [dropdown("size", false), dropdown("zone", true)]});
    let api = serve_packs_with(api, (NEWEST, "1.4.0"), BARNEY, &packs_newest(), variables);
    let record = record_of(&api, HELD, dir.path(), "barney.json");
    let r = behind_of(&api, &[record], &WITH_IMPACT);
    let bump = r.profiles[0].bump.as_ref().expect("1.3.1 is behind 1.4.0");
    let required = bump.update_checks.required_variables.as_ref();
    assert_eq!(required.map(|r| r.names.clone()), Some(vec!["size".into()]));

    // a record of 1.4.0 keeps the mark of the default option, and no mark of another option
    let newest = record_of(&api, NEWEST, dir.path(), "newest.json");
    let doc: Value = serde_json::from_str(&std::fs::read_to_string(newest).unwrap()).unwrap();
    let marks = |name: &str| -> Vec<Value> {
        let v = doc["variables"]
            .as_array()
            .unwrap()
            .iter()
            .find(|v| v["name"] == name)
            .unwrap();
        v["options"]
            .as_array()
            .unwrap()
            .iter()
            .map(|o| o["default"].clone())
            .collect()
    };
    assert_eq!(marks("zone"), [Value::Null, json!(true)]);
    assert_eq!(marks("size"), [Value::Null, Value::Null]);
}

#[test]
fn a_move_that_strata_cannot_compare_makes_the_profile_version_not_checked() {
    let dir = tempfile::tempdir().unwrap();
    let api = serve_move();
    let record = record_of(&api, HELD, dir.path(), "barney.json");
    let api = api.failing(manifests_route(NEWEST), 500, "boom");
    let r = behind_of(&api, &[record], &WITH_IMPACT);
    let held = &r.profiles[0];
    assert!(
        held.behind && held.bump.is_none() && held.error.is_some(),
        "{held:?}"
    );
    assert_eq!(r.exit_code(), 2);
    let text = common::terminal().render_behind(&r, false).unwrap();
    assert!(
        text.contains("~ [profile] barney-addons  1.3.1  project  not checked\n"),
        "{text}"
    );
}

// ---- the newer profile version that carries a newer pack (#709) ---------------------------------

/// The versions of fred-compositions in the Public Repo, `(version, pack uid)`.
const FRED: &[(&str, &str)] = &[("0.1.0", "fc010"), ("0.1.5", "fc015"), ("0.2.0", "fc020")];

/// The uids of the two versions that the pipeline published after `barney-addons@1.3.1`.
const V132: &str = "64f0000000000000000000c2";
const V140: &str = "64f0000000000000000000c4";

/// The versions of `barney-addons` of the chain, in no order: 1.3.2 and 1.4.0 are newer than the
/// held 1.3.1.
const CHAIN: &[(&str, &str)] = &[(V140, "1.4.0"), (HELD, "1.3.1"), (V132, "1.3.2")];

/// fred-compositions at `version` from the Public Repo.
fn fred(version: &str) -> PackFixture<'_> {
    let uid = FRED
        .iter()
        .find(|(v, _)| *v == version)
        .map_or("fc999", |(_, u)| *u);
    PackFixture::public(
        "fred-compositions",
        "addon",
        version,
        uid,
        "",
        "compositions: []\n",
    )
}

/// The registry listing and the Public Repo with each version of [`FRED`]. A record of 0.1.0 is
/// behind 0.2.0 and 0.1.5.
fn serve_fred(api: MemoryApi) -> MemoryApi {
    let api = api.with(
        REGISTRY_LISTING,
        json!({"items": [{"name": "Public Repo", "uid": PUBLIC_REGISTRY, "kind": "pack"}]}),
    );
    let summaries = FRED.iter().map(|(v, _)| fred(v).summary(&[])).collect();
    let tags: Vec<Value> = FRED
        .iter()
        .map(|(v, uid)| json!({"tag": v, "packUid": uid}))
        .collect();
    serve_registry_pack(
        api,
        "fred-compositions",
        Value::Array(summaries),
        json!({"layer": "addon", "tags": tags}),
        PUBLIC_REGISTRY,
    )
}

/// The tenant of the chain: `barney-addons@1.3.1` pins fred-compositions 0.1.0, 1.3.2 pins
/// `at_132`, and 1.4.0 pins `at_140`.
fn serve_chain(at_132: &str, at_140: &str) -> MemoryApi {
    let api = serve_fred(MemoryApi::new());
    let api = serve_packs(api, (HELD, "1.3.1"), CHAIN, &[fred("0.1.0")]);
    let api = serve_packs(api, (V132, "1.3.2"), CHAIN, &[fred(at_132)]);
    serve_packs(api, (V140, "1.4.0"), CHAIN, &[fred(at_140)])
}

/// The pin of fred-compositions in `r`.
fn fred_pin(r: &ops::BehindReport) -> &ops::Pin {
    r.pins
        .iter()
        .find(|p| p.name == "fred-compositions")
        .expect("the records pin fred-compositions")
}

#[test]
fn behind_names_the_newer_profile_version_that_carries_the_newer_pack() {
    let dir = tempfile::tempdir().unwrap();
    let api = serve_chain("0.1.0", "0.2.0");
    let record = record_of(&api, HELD, dir.path(), "barney.json");
    let r = behind_of(
        &api,
        std::slice::from_ref(&record),
        &ops::BehindOptions::default(),
    );
    let pin = fred_pin(&r);
    let newer: Vec<&str> = pin.newer.iter().map(|n| n.version.as_str()).collect();
    assert_eq!(
        (pin.version.as_str(), newer),
        ("0.1.0", vec!["0.2.0", "0.1.5"])
    );
    assert_eq!(pin.carriers.len(), 1, "{:?}", pin.carriers);
    let held = &pin.carriers[0];
    assert_eq!(
        (
            held.profile.as_str(),
            held.version.as_str(),
            held.uid.as_str()
        ),
        ("barney-addons", "1.3.1", HELD)
    );
    let carrier = held.carrier.as_ref().expect("1.4.0 pins 0.2.0");
    assert_eq!(
        (
            carrier.version.as_str(),
            carrier.uid.as_str(),
            carrier.pack_version.as_str()
        ),
        ("1.4.0", V140, "0.2.0")
    );
    assert!(held.error.is_none());
    // the carrier is a fact beside the pin: the pin and the version stay behind, with exit 1
    assert_eq!(r.exit_code(), 1);

    let text = common::terminal().render_behind(&r, false).unwrap();
    assert!(
        text.contains(
            "      barney-addons@1.4.0 carries fred-compositions@0.2.0\n      pinned by "
        ),
        "the line comes before the records that pin the pack: {text}"
    );
    let markdown = common::markdown().render_behind(&r, false).unwrap();
    assert!(
        markdown.contains("; barney-addons@1.4.0 carries fred-compositions@0.2.0 |"),
        "{markdown}"
    );
    let doc = serde_json::to_value(&r).unwrap();
    assert_eq!(
        doc["pins"][0]["carriers"],
        json!([{"profile": "barney-addons", "version": "1.3.1", "uid": HELD,
            "carrier": {"version": "1.4.0", "uid": V140, "packVersion": "0.2.0"}}])
    );
    let back: ops::BehindReport = serde_json::from_value(doc).unwrap();
    assert_eq!(
        fred_pin(&back).carriers[0]
            .carrier
            .as_ref()
            .unwrap()
            .version,
        "1.4.0"
    );
}

#[test]
fn behind_says_that_no_newer_profile_version_carries_a_newer_pack() {
    let dir = tempfile::tempdir().unwrap();
    let api = serve_chain("0.1.0", "0.1.0");
    let record = record_of(&api, HELD, dir.path(), "barney.json");
    let r = behind_of(&api, &[record], &ops::BehindOptions::default());
    let pin = fred_pin(&r);
    assert!(pin.behind);
    let held = &pin.carriers[0];
    assert!(
        held.carrier.is_none() && held.error.is_none(),
        "{:?}",
        pin.carriers
    );
    let text = common::terminal().render_behind(&r, false).unwrap();
    assert!(
        text.contains(
            "      no newer version of barney-addons@1.3.1 carries a newer fred-compositions\n"
        ),
        "{text}"
    );
    assert!(!text.contains(" carries fred-compositions@"), "{text}");
    let doc = serde_json::to_value(&r).unwrap();
    assert!(
        doc["pins"][0]["carriers"][0].get("carrier").is_none(),
        "{doc}"
    );
}

#[test]
fn a_newer_profile_version_that_pins_a_version_between_the_pin_and_the_newest_is_the_carrier() {
    let dir = tempfile::tempdir().unwrap();
    let record = |api: &MemoryApi| record_of(api, HELD, dir.path(), "barney.json");

    // the newest profile version pins 0.1.5: below the newest pack, and above the pin
    let api = serve_chain("0.1.0", "0.1.5");
    let r = behind_of(&api, &[record(&api)], &ops::BehindOptions::default());
    let carrier = fred_pin(&r).carriers[0].carrier.clone().unwrap();
    assert_eq!(
        (carrier.version.as_str(), carrier.pack_version.as_str()),
        ("1.4.0", "0.1.5")
    );
    let text = common::terminal().render_behind(&r, false).unwrap();
    assert!(
        text.contains("      barney-addons@1.4.0 carries fred-compositions@0.1.5\n"),
        "{text}"
    );

    // the newest profile version keeps 0.1.0, and an older newer version pins 0.1.5: the carrier
    // is the newest version that pins a newer pack
    let api = serve_chain("0.1.5", "0.1.0");
    let r = behind_of(&api, &[record(&api)], &ops::BehindOptions::default());
    let carrier = fred_pin(&r).carriers[0].carrier.clone().unwrap();
    assert_eq!(
        (
            carrier.version.as_str(),
            carrier.uid.as_str(),
            carrier.pack_version.as_str()
        ),
        ("1.3.2", V132, "0.1.5")
    );
}

#[test]
fn behind_reads_the_newer_versions_only_of_a_profile_version_that_holds_a_pin_that_is_behind() {
    let dir = tempfile::tempdir().unwrap();
    // barney-base holds no pin: its packs come from no registry. It has two newer versions
    let base = [
        ("64f0000000000000000000e0", "2.0.0"),
        ("64f0000000000000000000e1", "2.1.0"),
        ("64f0000000000000000000e5", "2.0.5"),
    ];
    let api = serve_versions(serve_chain("0.1.5", "0.1.0"), "barney-base", &base);
    let record = record_of(&api, HELD, dir.path(), "barney.json");
    let cluster = cluster_record(
        &record,
        &[
            (base[0].0, "barney-base", "2.0.0"),
            (HELD, "barney-addons", "1.3.1"),
        ],
        dir.path(),
    );
    let reads = |uid: &str| api.calls(&profile_route(uid));
    let uids = [HELD, V140, V132, base[0].0, base[1].0, base[2].0];
    let before: Vec<u32> = uids.iter().map(|u| reads(u)).collect();
    let r = behind_of(&api, &[record, cluster], &ops::BehindOptions::default());
    let after: Vec<u32> = uids.iter().map(|u| reads(u)).collect();
    let made: Vec<u32> = after.iter().zip(&before).map(|(a, b)| a - b).collect();
    // one read of each version of barney-addons: its newest version gives the time of the create,
    // and 1.3.2 carries the newer pack. barney-base holds the pin on no record, so strata reads its
    // newest version for the time of the create alone, and never 2.0.5
    assert_eq!(made, [1, 1, 1, 1, 1, 0]);

    // two records hold barney-addons@1.3.1, and one entry names it
    let pin = fred_pin(&r);
    assert_eq!(pin.pinned_by.len(), 2);
    let named: Vec<(&str, Option<&str>)> = pin
        .carriers
        .iter()
        .map(|c| {
            (
                c.profile.as_str(),
                c.carrier.as_ref().map(|by| by.version.as_str()),
            )
        })
        .collect();
    assert_eq!(named, [("barney-addons", Some("1.3.2"))]);
}

#[test]
fn a_newer_profile_version_that_strata_cannot_read_gives_its_error_and_keeps_the_exit() {
    let dir = tempfile::tempdir().unwrap();
    let api = serve_chain("0.1.5", "0.1.0");
    let record = record_of(&api, HELD, dir.path(), "barney.json");
    let api = api.failing(profile_route(V132), 500, "boom");
    let r = behind_of(&api, &[record], &ops::BehindOptions::default());
    let held = &fred_pin(&r).carriers[0];
    assert!(held.carrier.is_none() && held.error.is_some(), "{held:?}");
    assert_eq!(r.exit_code(), 1);
    let text = common::terminal().render_behind(&r, false).unwrap();
    assert!(
        text.contains(
            "      strata could not read which newer version of barney-addons@1.3.1 carries a newer fred-compositions ("
        ),
        "{text}"
    );
}

// ---- the rows of a cluster with many profiles (#622) ----------------------------------------------

/// The uid of `barney-base@2.0.0`, the other profile of the cluster of the owner tests.
const BASE: &str = "64f0000000000000000000e0";

/// The record of the cluster `payments`, which runs `barney-base@2.0.0` and the version of
/// `record`. Its title names the two profiles, as a record of a cluster does. With `running`, the
/// record holds the tier `running`, which names the profile of each pack.
fn two_profile_cluster(record: &Path, running: bool, dir: &Path) -> PathBuf {
    let profiles = [
        (BASE, "barney-base", "2.0.0"),
        (HELD, "barney-addons", "1.3.1"),
    ];
    let path = cluster_record(record, &profiles, dir);
    let mut snap = Snapshot::load(&path).unwrap();
    snap.source.name = "barney-base+barney-addons".to_string();
    snap.source.version = "2.0.0+1.3.1".to_string();
    if running {
        let fields = json!([
            {"name": "namespaces", "version": "1.0.0", "profileUid": BASE},
            {"name": "fred-compositions", "version": "0.1.0", "profileUid": HELD},
        ]);
        let tier = ClusterTier {
            hash: String::new(),
            fields,
            errors: BTreeMap::new(),
        };
        let cluster = snap.source.cluster.as_mut().unwrap();
        cluster.tiers.insert(Tier::Running.into(), tier);
    }
    common::write_snapshot(&path, &snap, false);
    path
}

/// The titles that the records of `by` show: [`ops::PinnedBy::owner`], else the subject.
fn titles(by: &[ops::PinnedBy]) -> Vec<&str> {
    by.iter()
        .map(|b| b.owner.as_deref().unwrap_or(&b.subject))
        .collect()
}

#[test]
fn each_row_of_a_cluster_with_many_profiles_names_the_profile_that_holds_the_pack() {
    let dir = tempfile::tempdir().unwrap();
    let api = serve_versions(
        serve_chain("0.1.0", "0.2.0"),
        "barney-base",
        &[(BASE, "2.0.0")],
    );
    let record = record_of(&api, HELD, dir.path(), "barney.json");
    let cluster = two_profile_cluster(&record, true, dir.path());
    let r = behind_of(
        &api,
        std::slice::from_ref(&cluster),
        &ops::BehindOptions::default(),
    );
    let by = &fred_pin(&r).pinned_by[0];
    assert_eq!(by.subject, "2 profiles on cluster payments");
    assert_eq!(
        by.owner.as_deref(),
        Some("barney-addons@1.3.1 on cluster payments"),
        "the tier running names the profile of the pack"
    );
    // each profile version that the cluster runs names itself
    let rows: Vec<(&str, Vec<&str>)> = r
        .profiles
        .iter()
        .map(|p| (p.name.as_str(), titles(&p.pinned_by)))
        .collect();
    assert_eq!(
        rows,
        [
            (
                "barney-addons",
                vec!["barney-addons@1.3.1 on cluster payments"]
            ),
            ("barney-base", vec!["barney-base@2.0.0 on cluster payments"]),
        ]
    );
    let text = common::terminal().render_behind(&r, false).unwrap();
    let path = cluster.display();
    assert!(
        text.contains(&format!(
            "      pinned by {path} (barney-addons@1.3.1 on cluster payments)\n"
        )) && text.contains(&format!(
            "      pinned by {path} (barney-base@2.0.0 on cluster payments)\n"
        )),
        "{text}"
    );
    assert!(!text.contains("2 profiles on cluster"), "{text}");
    let markdown = common::markdown().render_behind(&r, false).unwrap();
    assert!(
        markdown.contains(&format!(
            "| `{path}` (barney-addons@1.3.1 on cluster payments) |\n"
        )),
        "{markdown}"
    );
    // the JSON keeps the title of the record for a script, and adds the owner
    let doc = serde_json::to_value(&r).unwrap();
    assert_eq!(
        doc["pins"][0]["pinnedBy"],
        json!([{"path": path.to_string(),
            "subject": "2 profiles on cluster payments",
            "owner": "barney-addons@1.3.1 on cluster payments"}])
    );
    let back: ops::BehindReport = serde_json::from_value(doc).unwrap();
    assert_eq!(back.pins[0].pinned_by[0].owner, by.owner);

    // a record with no tier running does not say which profile holds the pack: the line keeps
    // the title of the record. The profile versions still name themselves
    let cluster = two_profile_cluster(&record, false, dir.path());
    let r = behind_of(&api, &[cluster], &ops::BehindOptions::default());
    assert_eq!(fred_pin(&r).pinned_by[0].owner, None);
    assert_eq!(
        titles(&r.profiles[0].pinned_by),
        ["barney-addons@1.3.1 on cluster payments"]
    );

    // a record of one profile names that profile in its title, and has no owner
    let r = behind_of(&api, &[record], &ops::BehindOptions::default());
    assert!(fred_pin(&r).pinned_by.iter().all(|b| b.owner.is_none()));
    assert!(r.profiles[0].pinned_by.iter().all(|b| b.owner.is_none()));
}

/// A pin of `name` at `version` from the Public Repo, with the newer versions `newer`, newest
/// first, as `strata-behind/1` holds it.
fn pin_doc(name: &str, version: &str, newer: &[&str]) -> Value {
    json!({"name": name, "layer": "addon", "registry": "Public Repo", "registryUid": PUBLIC_REGISTRY,
        "version": version, "tag": version,
        "pinnedBy": [{"path": "dev.json", "subject": "aws-stack@1.0.0"}],
        "newer": newer.iter().map(|v| json!({"version": v})).collect::<Vec<_>>(),
        "behind": !newer.is_empty()})
}

/// A profile version `aws-stack@<version>`, as `strata-behind/1` holds it, with the fields
/// `fields` over it.
fn profile_doc(version: &str, fields: Value) -> Value {
    let mut doc = json!({"name": "aws-stack", "version": version, "uid": format!("u{version}"),
        "pinnedBy": [{"path": "dev.json", "subject": format!("aws-stack@{version}")}],
        "newer": [], "behind": false});
    for (key, value) in fields.as_object().unwrap() {
        doc[key] = value.clone();
    }
    doc
}

/// The report of `behind` on the record `dev.json` with `pins` and `profiles`, and the counts of
/// its summary.
fn report_of(pins: Vec<Value>, profiles: Vec<Value>) -> ops::BehindReport {
    let count = |list: &[Value], field: &str| {
        list.iter()
            .filter(|p| p.get(field).is_some_and(|v| v != &json!(false)))
            .count()
    };
    let summary = json!({"pins": pins.len(), "behind": count(&pins, "behind"),
        "floating": count(&pins, "floating"), "errors": count(&pins, "error"), "arrived": 0,
        "profiles": profiles.len(), "profilesBehind": count(&profiles, "behind"),
        "profileErrors": count(&profiles, "error")});
    serde_json::from_value(json!({"schema": ops::BEHIND_SCHEMA,
        "tool": {"name": "strata", "version": "0.34.0"}, "generatedAt": "2026-11-13T09:30:00Z",
        "records": ["dev.json"], "pins": pins, "profiles": profiles, "summary": summary}))
    .unwrap()
}

/// A report with a pin and a profile version of each state. Two pins have a floating tag, and the
/// bump of `csi-aws-ebs` failed, so that pin is behind and not checked.
fn report_of_each_state() -> ops::BehindReport {
    let error = json!({"kind": "api", "message": "HTTP 500: boom"});
    let mut floating_behind = pin_doc("ubuntu-aws", "20.04", &["24.04", "22.04"]);
    floating_behind["tag"] = json!("LTS__20.4.x");
    floating_behind["floating"] = json!(true);
    let mut floating_current = pin_doc("ubuntu-aws", "24.04", &[]);
    floating_current["tag"] = json!("LTS__24.x");
    floating_current["floating"] = json!(true);
    let mut failed_bump = pin_doc("csi-aws-ebs", "1.63.1", &["1.65.0"]);
    failed_bump["error"] = error.clone();
    let mut failed_read = pin_doc("loki", "2.10.2", &[]);
    failed_read["error"] = error.clone();
    let pins = vec![
        pin_doc("cert-manager", "1.21.1", &[]),
        pin_doc("kubernetes", "1.36.2", &["1.37.0", "1.36.4", "1.36.3"]),
        floating_behind,
        floating_current,
        failed_bump,
        failed_read,
    ];
    let profiles = vec![
        profile_doc(
            "1.0.0",
            json!({"newer": [{"version": "1.1.0", "uid": "u1.1.0"}], "behind": true}),
        ),
        profile_doc("1.1.0", json!({})),
        profile_doc("0.9.0", json!({"gone": true})),
        profile_doc("0.8.0", json!({"error": error})),
    ];
    report_of(pins, profiles)
}

#[test]
fn the_counts_of_the_heading_of_behind_add_up_to_the_pins_and_to_the_profile_versions() {
    let r = report_of_each_state();
    let states: Vec<RowState> = r.pins.iter().map(RowState::of_pin).collect();
    assert_eq!(
        states,
        [
            RowState::Current,
            RowState::Behind,
            RowState::Behind,
            RowState::Current,
            RowState::NotChecked,
            RowState::NotChecked,
        ]
    );
    let heading = "6 pins (2 with a floating tag) from 1 record: 2 behind, 2 current, 2 not \
                   checked; 4 profile versions: 1 behind, 1 current, 1 gone, 1 not checked\n";
    let text = common::terminal().render_behind(&r, false).unwrap();
    assert!(text.starts_with(heading), "{text}");
    let markdown = common::markdown().render_behind(&r, false).unwrap();
    assert!(
        markdown.starts_with(&format!("### behind: {heading}")),
        "{markdown}"
    );
    // each state has its word in the status of the row
    for state in RowState::ALL {
        assert_eq!(RowState::from_text(state.as_str()), Some(*state));
    }
    assert!(
        text.contains("  [addon] loki          2.10.2  Public Repo  not checked\n"),
        "{text}"
    );
}

#[test]
fn behind_only_omits_the_rows_that_are_current_and_keeps_the_heading_and_the_json() {
    let r = report_of_each_state();
    let all = common::terminal().render_behind(&r, false).unwrap();
    let text = common::terminal().render_behind(&r, true).unwrap();
    let first = |t: &str| t.lines().next().unwrap_or_default().to_string();
    assert_eq!(first(&text), first(&all), "the heading counts each row");
    let rows: Vec<&str> = text
        .lines()
        .filter(|l| l.contains(" [") && !l.starts_with("      "))
        .collect();
    assert_eq!(
        rows,
        [
            "~ [addon] kubernetes   1.36.2  Public Repo  behind: 1.37.0, 1.36.4, 1.36.3",
            "~ [addon] ubuntu-aws   20.04   Public Repo  behind: 24.04, 22.04 (floating tag LTS__20.4.x)",
            "~ [addon] csi-aws-ebs  1.63.1  Public Repo  not checked",
            "  [addon] loki         2.10.2  Public Repo  not checked",
            "~ [profile] aws-stack  1.0.0  -  behind: 1.1.0",
            "  [profile] aws-stack  0.9.0  -  gone",
            "  [profile] aws-stack  0.8.0  -  not checked",
        ],
        "{text}"
    );
    let markdown = common::markdown().render_behind(&r, true).unwrap();
    assert!(
        !markdown.contains("| `cert-manager` |") && !markdown.contains("| `1.1.0` |"),
        "{markdown}"
    );
    assert!(
        markdown.contains("| `loki` |") && markdown.contains("| `0.9.0` |"),
        "{markdown}"
    );
    // the JSON holds each pin and each profile version: `--since` reads them all
    let json = strata::render::renderer(
        strata::render::Format::Json,
        strata::term::color::Paint::off(),
        false,
    );
    assert_eq!(
        json.render_behind(&r, true).unwrap(),
        json.render_behind(&r, false).unwrap()
    );

    // a report with no row that is behind shows its heading alone
    let r = report_of(vec![pin_doc("cert-manager", "1.21.1", &[])], Vec::new());
    assert_eq!(
        common::terminal().render_behind(&r, true).unwrap(),
        "1 pin from 1 record: 0 behind, 1 current\n"
    );
    assert_eq!(
        common::markdown().render_behind(&r, true).unwrap(),
        "### behind: 1 pin from 1 record: 0 behind, 1 current\n\n"
    );
}

#[test]
fn a_pin_behind_a_newer_minor_version_names_the_newest_patch_of_its_own_and_the_command() {
    let mut no_name = pin_doc("cert-manager", "1.21.1", &["1.22.0", "1.21.3"]);
    no_name["registry"] = json!("");
    let pins = vec![
        pin_doc("kubernetes", "1.36.2", &["1.37.0", "1.36.4", "1.36.3"]),
        // the newest version is of the minor version of the pin: the bump names it
        pin_doc("kubernetes", "1.36.3", &["1.36.4"]),
        // no newer version of the minor version of the pin
        pin_doc("csi-aws-ebs", "1.63.1", &["1.65.0", "1.64.0"]),
        // a version that does not parse has no minor version
        pin_doc("ubuntu-aws", "LTS__20.4.x", &["24.04"]),
        no_name,
    ];
    let r = report_of(pins, Vec::new());
    let text = common::terminal().render_behind(&r, false).unwrap();
    let lines: Vec<&str> = text
        .lines()
        .filter(|l| l.trim_start().starts_with("next patch"))
        .collect();
    assert_eq!(
        lines,
        [
            "      next patch 1.36.4: strata compare 'pack:Public Repo/kubernetes@1.36.2' \
             'pack:Public Repo/kubernetes@1.36.4'"
                .to_string(),
            // a registry with no name: the ref names its uid
            format!(
                "      next patch 1.21.3: strata compare pack:{PUBLIC_REGISTRY}/cert-manager@1.21.1 \
                 pack:{PUBLIC_REGISTRY}/cert-manager@1.21.3"
            ),
        ],
        "{text}"
    );
    assert!(
        text.contains(
            "behind: 1.37.0, 1.36.4, 1.36.3\n      next patch 1.36.4: strata compare 'pack:Public \
             Repo/kubernetes@1.36.2'"
        ),
        "the line comes under the line of the pin: {text}"
    );
    let markdown = common::markdown().render_behind(&r, false).unwrap();
    assert!(
        markdown.contains(
            "| behind: 1.37.0, 1.36.4, 1.36.3; next patch 1.36.4: strata compare 'pack:Public \
             Repo/kubernetes@1.36.2' 'pack:Public Repo/kubernetes@1.36.4' |"
        ),
        "{markdown}"
    );
    // the line is a hint of the report: the JSON holds the newer versions, and no other field
    let doc = serde_json::to_value(&r).unwrap();
    assert!(!doc.to_string().contains("next patch"), "{doc}");
}
