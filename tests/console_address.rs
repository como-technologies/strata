//! The packs of virtual machines hold the address that their console forwards to in one value,
//! `charts.<pack>.appConfig.clusterInfo.consoleBaseAddress`. Palette serves it in clear. A record
//! of a cluster, of a profile, and of a draft, and each answer that `--raw-dir` saves, hold
//! `********` in its place (#741). A question to Palette sends the address of the cluster in place
//! of the mask. So the question does not move the console to the mask.

use serde_json::{Value, json};

use strata::api::{MASKED, MemoryApi, Palette, RawCluster, RawProfile, ReadErrors};
use strata::diff::ImpactLevel;
use strata::ops;
use strata::snapshot::Snapshot;

use crate::common;
use common::{
    CLUSTER_UID, PackFixture, UID_A, assert_no_saved_file_holds, cluster_route, cluster_template,
    cluster_validate_repave_route, console_address_field, console_address_line,
    console_pack_values, manifests_answer, no_repave, no_template, no_variables, post,
    profile_document, template, template_validate_repave_route,
};

/// The made-up address of the console of the orchestrator of virtual machines.
const CONSOLE: &str = "https://vmo.lab-7.invalid";
/// The made-up address of the console of the move of virtual machines.
const MIGRATION: &str = "https://vma.lab-7.invalid";
/// Each made-up console address.
const ADDRESSES: [&str; 2] = [CONSOLE, MIGRATION];

// The names of the packs, as Palette names them. The test pins them on purpose, because the type
// of strata that holds them is private.
const ORCHESTRATOR: &str = "virtual-machine-orchestrator";
const DASHBOARD: &str = "spectro-vm-dashboard";
const MIGRATION_PACK: &str = "vm-migration-assistant";
/// A pack with no console: its value at the same path is no console address.
const OTHER_PACK: &str = "cert-manager";
/// The version of [`OTHER_PACK`].
const OTHER_VERSION: &str = "1.9.1";

/// The name of the profile.
const PROFILE: &str = "vm-stack";
/// The name of the record of the profile version.
const RECORD: &str = "vm-stack@1.0.0";

/// The pack refs of a template, and the answer of `packs/manifests` for the same packs.
struct Packs {
    refs: Value,
    manifests: Value,
}

/// The packs `packs`, each with its name, its version, and its values.
fn packs_of(packs: &[(&str, &str, String)]) -> Packs {
    let uids: Vec<String> = packs
        .iter()
        .map(|(name, ..)| format!("uid-{name}"))
        .collect();
    let fixtures: Vec<PackFixture> = packs
        .iter()
        .zip(&uids)
        .map(|((name, version, values), uid)| {
            PackFixture::public(name, "addon", version, uid, "", values)
        })
        .collect();
    Packs {
        refs: fixtures.iter().map(|p| p.pack_ref(&[])).collect(),
        manifests: manifests_answer(
            fixtures
                .iter()
                .map(|p| p.pack_manifests(Vec::new()))
                .collect(),
        ),
    }
}

/// The packs of a template: the orchestrator under `orchestrator` with `replicas`, the move of
/// virtual machines, and [`OTHER_PACK`]. The value of [`OTHER_PACK`] at the same path stays in
/// clear. Each address has a comment.
fn vm_packs(orchestrator: &str, replicas: u8) -> Packs {
    let values = |pack: &str, address: &str, replicas: u8| {
        console_pack_values(pack, &format!("{address} # the console"), replicas)
    };
    packs_of(&[
        (
            orchestrator,
            "4.6.0",
            values(orchestrator, CONSOLE, replicas),
        ),
        (
            MIGRATION_PACK,
            "1.2.0",
            values(MIGRATION_PACK, MIGRATION, 1),
        ),
        (
            OTHER_PACK,
            OTHER_VERSION,
            values(OTHER_PACK, "https://cert-manager.invalid", 1),
        ),
    ])
}

/// The template of the add-on packs `packs`.
fn add_ons(packs: &Packs) -> Value {
    template("all", "add-on", packs.refs.clone())
}

/// The document of the profile version, with `published` and `draft` as its templates.
fn profile(published: Value, draft: Option<Value>) -> Value {
    profile_document(UID_A, PROFILE, "1.0.0", published, draft)
}

/// The responses of the profile version, with `published` and `draft` as its templates, and the
/// packs of `served` in `packs/manifests`.
fn profile_raw(published: Value, draft: Option<Value>, served: &Packs) -> RawProfile {
    RawProfile {
        profile: profile(published, draft),
        manifests: served.manifests.clone(),
        variables: no_variables(),
        variables_unavailable: false,
        project_uid: None,
        fetched_at: None,
        manifest_entities: None,
        resolved_values: None,
    }
}

/// The responses of the profile version that publishes `packs`.
fn published_raw(packs: &Packs) -> RawProfile {
    profile_raw(add_ons(packs), None, packs)
}

/// The record of the profile version of `raw`.
fn profile_record(raw: &RawProfile) -> Snapshot {
    Snapshot::from_raw(raw, RECORD, "t0").unwrap()
}

/// The document of the cluster that runs the profile version with `packs`.
fn cluster_doc(packs: &Packs) -> Value {
    json!({"metadata": {"uid": CLUSTER_UID, "name": "vm-lab"},
           "spec": {"cloudType": "edge-native",
                    "clusterProfileTemplates": [cluster_template(&profile(add_ons(packs), None))]}})
}

/// The responses of the cluster that runs the packs of [`vm_packs`].
fn cluster_raw() -> RawCluster {
    let packs = vm_packs(ORCHESTRATOR, 2);
    let mut served = packs.manifests.clone();
    served["metadata"] = json!({"uid": UID_A, "name": PROFILE});
    RawCluster {
        cluster: cluster_doc(&packs),
        manifests: json!({"profiles": [served]}),
        variables: json!([]),
        variables_unavailable: false,
        project_uid: None,
        fetched_at: None,
        notifications: None,
        repave: None,
        resolved_values: None,
        cloud_config: None,
        rbacs: None,
        namespaces: None,
        errors: ReadErrors::default(),
    }
}

/// The record of the cluster of `raw`.
fn cluster_record(raw: &RawCluster) -> Snapshot {
    Snapshot::from_cluster(raw, None, "cluster:vm-lab", "t0").unwrap()
}

/// A Palette that serves `cluster` as the document of the cluster, and that answers the question
/// at `route` with no repave.
fn repave_api(cluster: Value, route: &str) -> MemoryApi {
    MemoryApi::new()
        .with(cluster_route(CLUSTER_UID), cluster)
        .with(post(route), no_repave())
}

/// The values of the pack `name` in `packs`, a list of packs of a template or of a question.
fn values_named(packs: &Value, name: &str) -> String {
    let p = packs
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["name"] == name)
        .unwrap();
    p["values"].as_str().unwrap().to_string()
}

/// Make sure that `text`, which `what` names, holds no console address in clear.
fn assert_no_address(what: &str, text: &str) {
    for address in ADDRESSES {
        assert!(!text.contains(address), "{what} holds {address}: {text}");
    }
}

/// Make sure that `snap` holds no console address, and that the console address is the one masked
/// path of each pack of virtual machines. The value of [`OTHER_PACK`] stays in clear.
fn assert_masked(what: &str, snap: &Snapshot, orchestrator: &str) {
    assert_no_address(what, &serde_json::to_string(snap).unwrap());
    let pack = |name: &str| snap.packs.iter().find(|p| p.name == name).unwrap();
    for name in [orchestrator, MIGRATION_PACK] {
        let p = pack(name);
        assert_eq!(
            p.masked_paths(),
            [console_address_field(name)],
            "{what}: {}",
            p.values
        );
        assert!(
            p.values.contains(&console_address_line(MASKED)),
            "{what}: {}",
            p.values
        );
        assert!(p.values.contains("replicas:"), "{what}: {}", p.values);
    }
    let other = pack(OTHER_PACK);
    assert!(other.masked_paths().is_empty(), "{what}: {}", other.values);
}

#[test]
fn a_record_of_a_cluster_with_the_packs_of_virtual_machines_holds_no_console_address() {
    assert_masked(
        "the record of the cluster",
        &cluster_record(&cluster_raw()),
        ORCHESTRATOR,
    );
}

#[test]
fn a_record_of_a_profile_and_of_a_draft_holds_no_console_address_also_under_the_old_name() {
    let old = vm_packs(DASHBOARD, 2);
    let new = vm_packs(ORCHESTRATOR, 3);
    assert_masked(
        "the record of the profile",
        &profile_record(&published_raw(&old)),
        DASHBOARD,
    );
    // a profile with a draft: packs/manifests serves the packs of the draft
    let edited = profile_raw(add_ons(&old), Some(add_ons(&new)), &new);
    let draft = Snapshot::from_raw_draft(&edited, RECORD, "t0").unwrap();
    assert_masked("the record of the draft", &draft, ORCHESTRATOR);
    // a version that nobody published has its packs in the draft alone
    let unpublished = profile_raw(no_template(), Some(add_ons(&new)), &new);
    assert_masked(
        "the record of a draft only",
        &profile_record(&unpublished),
        ORCHESTRATOR,
    );
}

#[test]
fn a_saved_directory_holds_no_console_address_and_gives_the_record_of_the_fetch() {
    let dir = tempfile::tempdir().unwrap();
    let raw = cluster_raw();
    raw.save_dir(dir.path(), "t0").unwrap();
    assert_no_saved_file_holds(dir.path(), &ADDRESSES);
    let saved = cluster_record(&RawCluster::load_dir(dir.path()).unwrap());
    assert_eq!(saved.root, cluster_record(&raw).root);

    let dir = tempfile::tempdir().unwrap();
    let raw = published_raw(&vm_packs(ORCHESTRATOR, 2));
    raw.save_dir(dir.path(), "t0").unwrap();
    assert_no_saved_file_holds(dir.path(), &ADDRESSES);
    let saved = profile_record(&RawProfile::load_dir(dir.path()).unwrap());
    assert_eq!(saved.root, profile_record(&raw).root);
}

#[test]
fn a_question_to_palette_sends_the_console_address_of_the_cluster_in_place_of_the_mask() {
    let raw = cluster_raw();
    let api = repave_api(
        raw.cluster.clone(),
        &cluster_validate_repave_route(CLUSTER_UID),
    );
    // the version that the cluster runs, with another number of replicas of the orchestrator
    let target = profile_record(&published_raw(&vm_packs(ORCHESTRATOR, 3)));
    ops::palette_repave(
        &Palette::new(&api),
        &cluster_record(&raw),
        &target,
        ImpactLevel::InPlace,
    )
    .unwrap();
    // the values of each pack that the question sends, from the last body
    let (_, body) = api.posted().pop().unwrap();
    let sent = &body["profiles"][0]["packValues"];
    let orchestrator = values_named(sent, ORCHESTRATOR);
    assert!(
        orchestrator.contains(&console_address_line(CONSOLE)),
        "{orchestrator}"
    );
    assert!(orchestrator.contains("replicas: 3"), "{orchestrator}");
    assert!(!orchestrator.contains(MASKED), "{orchestrator}");
    // a pack with no other change goes with the values that the cluster holds
    let held = &raw.cluster["spec"]["clusterProfileTemplates"][0]["packs"];
    assert_eq!(
        values_named(sent, MIGRATION_PACK),
        values_named(held, MIGRATION_PACK)
    );
}

#[test]
fn a_question_for_a_cluster_that_does_not_run_the_pack_sends_an_empty_console_address() {
    // the cluster runs the other pack alone
    let doc = cluster_doc(&packs_of(&[(
        OTHER_PACK,
        OTHER_VERSION,
        "replicas: 1\n".to_string(),
    )]));
    let raw = RawCluster {
        cluster: doc.clone(),
        ..cluster_raw()
    };
    let api = repave_api(doc, &cluster_validate_repave_route(CLUSTER_UID));
    let target = profile_record(&published_raw(&vm_packs(ORCHESTRATOR, 2)));
    ops::palette_repave(
        &Palette::new(&api),
        &cluster_record(&raw),
        &target,
        ImpactLevel::InPlace,
    )
    .unwrap();
    let (_, body) = api.posted().pop().unwrap();
    let text = body.to_string();
    assert!(text.contains(&console_address_line("")), "{text}");
    assert!(!text.contains(MASKED), "{text}");
}

#[test]
fn an_update_that_keeps_the_pack_of_a_cluster_of_a_template_sends_no_values_of_the_pack() {
    // a cluster template governs the cluster, and the route of the template takes no pack values
    let mut raw = cluster_raw();
    let governs = "tpl-vm";
    raw.cluster["spec"]["clusterTemplate"] = json!({"uid": governs});
    let cluster = cluster_record(&raw);
    let pending = cluster.clone().mark_pending();
    let route = template_validate_repave_route(governs);
    let api = repave_api(raw.cluster.clone(), &route);
    // the record holds the mask, and the cluster the address: the pack is the same
    ops::palette_repave(
        &Palette::new(&api),
        &cluster,
        &pending,
        ImpactLevel::InPlace,
    )
    .unwrap();
    let (path, body) = api.posted().pop().unwrap();
    assert_eq!(path, route);
    assert_eq!(body["profiles"], json!([{"uid": UID_A}]));
}
