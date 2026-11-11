//! The profile versions of `strata behind` (#707): the newer published versions of each profile
//! that a record holds. The document of a version gives the published versions of its profile in
//! `spec.versions[]`, in no order, and strata orders them. With the impact, `behind` also compares
//! each profile version that is behind with the newest version (#708).

use std::path::{Path, PathBuf};

use serde_json::{Value, json};
use strata::api::MemoryApi;
use strata::closed_set::ClosedSet;
use strata::diff::{ChangeFlag, Status};
use strata::model::scope::ProfileScope;
use strata::ops;
use strata::snapshot::{AppliedProfile, ClusterSource, Snapshot};

use crate::common::{
    self, PROFILE_LISTING, PackFixture, WITH_IMPACT, behind_of, manifests_answer, manifests_route,
    no_manifests, no_variables, profile_document, profile_route, serve_profile, snapshot_of,
    template,
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
    let text = common::terminal().render_behind(&r).unwrap();
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
    for p in &r.profiles {
        let subjects: Vec<&str> = p.pinned_by.iter().map(|b| b.subject.as_str()).collect();
        assert_eq!(subjects, ["barney-addons@1.3.1 on cluster payments"]);
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
    let text = common::terminal().render_behind(&r).unwrap();
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
    let text = common::terminal().render_behind(&r).unwrap();
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

    let markdown = common::markdown().render_behind(&r).unwrap();
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

/// [`serve_versions`] of [`BARNEY`], where 1.3.1 holds [`packs_held`] and 1.4.0 holds
/// [`packs_newest`].
fn serve_move() -> MemoryApi {
    let serve = |api: MemoryApi, uid: &str, version: &str, packs: &[PackFixture]| {
        let mut doc = version_document(uid, "barney-addons", version, BARNEY);
        doc["spec"]["published"]["packs"] = packs.iter().map(|p| p.pack_ref(&[])).collect();
        let manifests = packs.iter().map(|p| p.pack_manifests(Vec::new())).collect();
        serve_profile(api, uid, doc, manifests_answer(manifests), no_variables())
    };
    let api = serve_versions(MemoryApi::new(), "barney-addons", BARNEY);
    let api = serve(api, HELD, "1.3.1", &packs_held());
    serve(api, NEWEST, "1.4.0", &packs_newest())
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

    // the packs of the move, in the order of compare: the version changes, the packs that it adds
    // or removes, and the values that change
    let packs: Vec<(&str, &str, Option<&str>, Option<&str>, bool)> = bump
        .packs
        .iter()
        .map(|p| {
            let (from, to) = (p.from.as_deref(), p.to.as_deref());
            (
                p.key.as_str(),
                p.status.as_str(),
                from,
                to,
                p.values_changed,
            )
        })
        .collect();
    assert_eq!(
        packs,
        [
            ("addon/dino-logs", "removed", Some("2.0.0"), None, false),
            (
                "addon/fred-compositions",
                "changed",
                Some("0.1.0"),
                Some("0.2.0"),
                false
            ),
            ("addon/pebbles-dns", "added", None, Some("1.0.0"), false),
            (
                "addon/wilma-values",
                "changed",
                Some("1.0.0"),
                Some("1.0.0"),
                true
            ),
        ]
    );
    let compared: Vec<&str> = d
        .packs
        .iter()
        .filter(|p| p.status != Status::Unchanged)
        .map(|p| p.key.as_str())
        .collect();
    let keys: Vec<&str> = bump.packs.iter().map(|p| p.key.as_str()).collect();
    assert_eq!(keys, compared);


    let text = common::terminal().render_behind(&r).unwrap();
    let line = format!(
        "      bump to 1.4.0: {} IDENTITY UPSTREAM AUTHORED\n",
        bump.impact
    );
    assert!(
        text.contains(&format!("      1.4.0 created {CREATED}\n{line}")),
        "the line comes under the time of the create: {text}"
    );
    let markdown = common::markdown().render_behind(&r).unwrap();
    assert!(
        markdown.contains(&format!(
            "| bump to 1.4.0: {} IDENTITY UPSTREAM AUTHORED |",
            bump.impact
        )),
        "{markdown}"
    );

    // the JSON holds the move, and `--since` reads it again
    let doc = serde_json::to_value(&r).unwrap();
    assert_eq!(
        doc["profiles"][0]["bump"]["packs"][1],
        json!({"key": "addon/fred-compositions", "status": "changed", "from": "0.1.0", "to": "0.2.0",
            "flags": ["IDENTITY", "UPSTREAM"]})
    );
    assert_eq!(
        doc["profiles"][0]["bump"]["packs"][2],
        json!({"key": "addon/pebbles-dns", "status": "added", "to": "1.0.0"})
    );
    assert_eq!(
        doc["profiles"][0]["bump"]["packs"][3]["valuesChanged"],
        true
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
    let text = common::terminal().render_behind(&r).unwrap();
    assert!(!text.contains("bump to"), "{text}");

    // --no-impact names the newer version, and compares nothing
    let r = behind_of(
        &api,
        std::slice::from_ref(&record),
        &ops::BehindOptions::default(),
    );
    assert!(r.profiles[0].behind && r.profiles[0].bump.is_none());
    let text = common::terminal().render_behind(&r).unwrap();
    assert!(!text.contains("bump to"), "{text}");
    assert_eq!((manifests(HELD), manifests(NEWEST)), before);
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
    let text = common::terminal().render_behind(&r).unwrap();
    assert!(
        text.contains("~ [profile] barney-addons  1.3.1  project  not checked\n"),
        "{text}"
    );
}
