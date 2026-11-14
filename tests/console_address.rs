//! The packs of virtual machines hold the address that their console forwards to in one value,
//! `charts.<pack>.appConfig.clusterInfo.consoleBaseAddress`. Palette serves it in clear. A record
//! of a cluster, of a profile, and of a draft, and each answer that `--raw-dir` saves, hold
//! `********` in its place (#741). A question to Palette sends the address of the cluster in place
//! of the mask. So the question does not move the console to the mask.

use std::path::Path;

use serde_json::{Value, json};

use strata::api::{MemoryApi, Palette, RawCluster, RawProfile, ReadErrors};
use strata::diff::ImpactLevel;
use strata::ops;
use strata::snapshot::Snapshot;

use crate::common;
use common::{
    cluster_route, cluster_template, manifests_answer, no_template, no_variables, post,
    profile_document, template,
};

/// The made-up address of the console of the orchestrator of virtual machines.
const CONSOLE: &str = "https://vmo.lab-7.invalid";
/// The made-up address of the console of the move of virtual machines.
const MIGRATION: &str = "https://vma.lab-7.invalid";
/// The uid of the profile version, and of the cluster.
const PROFILE_UID: &str = "64f0000000000000000000f1";
const CLUSTER_UID: &str = "64f0000000000000000000f2";

/// The values of the pack `pack` with `address` as the console address, and `replicas`.
fn values(pack: &str, address: &str, replicas: u8) -> String {
    format!(
        "charts:\n  {pack}:\n    appConfig:\n      clusterInfo:\n        \
         consoleBaseAddress: {address} # the console\n    replicas: {replicas}\n"
    )
}

/// The field of the console address of the pack `pack`.
fn field(pack: &str) -> String {
    format!("charts.{pack}.appConfig.clusterInfo.consoleBaseAddress")
}

/// One pack of a template.
fn pack(name: &str, tag: &str, values: &str) -> Value {
    json!({"name": name, "layer": "addon", "type": "spectro", "tag": tag, "version": tag,
           "packUid": format!("uid-{name}"), "registryUid": "r1", "values": values})
}

/// The packs of a template: the orchestrator under `orchestrator`, its name, the move of virtual
/// machines, and `cert-manager`. The value of `cert-manager` at the same path is no console
/// address, so it stays in clear.
fn vm_packs(orchestrator: &str, replicas: u8) -> Value {
    json!([
        pack(
            orchestrator,
            "4.6.0",
            &values(orchestrator, CONSOLE, replicas)
        ),
        pack(
            "vm-migration-assistant",
            "1.2.0",
            &values("vm-migration-assistant", MIGRATION, 1)
        ),
        pack(
            "cert-manager",
            "1.9.1",
            &values("cert-manager", "https://cert-manager.invalid", 1)
        ),
    ])
}

/// The answer of `packs/manifests` for `packs`: each pack with its values, as Palette serves it.
fn manifests_of(packs: &Value) -> Value {
    let entries = packs
        .as_array()
        .unwrap()
        .iter()
        .map(|p| {
            json!({"metadata": {"name": p["name"], "uid": p["packUid"]},
                   "spec": {"name": p["name"], "version": p["tag"], "values": p["values"]}})
        })
        .collect();
    manifests_answer(entries)
}

/// The responses of the profile version, with `published` and `draft` as its templates, and the
/// packs of `served` in `packs/manifests`.
fn profile_raw(published: Value, draft: Option<Value>, served: &Value) -> RawProfile {
    RawProfile {
        profile: profile_document(PROFILE_UID, "vm-stack", "1.0.0", published, draft),
        manifests: manifests_of(served),
        variables: no_variables(),
        variables_unavailable: false,
        project_uid: None,
        fetched_at: None,
        manifest_entities: None,
        resolved_values: None,
    }
}

/// The document of the cluster that runs the profile version with `packs`.
fn cluster_doc(packs: &Value) -> Value {
    let profile = profile_document(
        PROFILE_UID,
        "vm-stack",
        "1.0.0",
        template("all", "add-on", packs.clone()),
        None,
    );
    json!({"metadata": {"uid": CLUSTER_UID, "name": "vm-lab"},
           "spec": {"cloudType": "edge-native", "clusterProfileTemplates": [cluster_template(&profile)]}})
}

/// The responses of the cluster that runs the packs of [`vm_packs`].
fn cluster_raw() -> RawCluster {
    let packs = vm_packs("virtual-machine-orchestrator", 2);
    let mut served = manifests_of(&packs);
    served["metadata"] = json!({"uid": PROFILE_UID, "name": "vm-stack"});
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

/// Make sure that `text`, which `what` names, holds no console address in clear.
fn assert_no_address(what: &str, text: &str) {
    for address in [CONSOLE, MIGRATION] {
        assert!(!text.contains(address), "{what} holds {address}: {text}");
    }
}

/// Make sure that `snap` holds no console address, and that the console address is the one masked
/// path of each pack of virtual machines. The value of `cert-manager` stays in clear.
fn assert_masked(what: &str, snap: &Snapshot, orchestrator: &str) {
    assert_no_address(what, &serde_json::to_string(snap).unwrap());
    for name in [orchestrator, "vm-migration-assistant"] {
        let p = snap.packs.iter().find(|p| p.name == name).unwrap();
        assert_eq!(p.masked_paths(), [field(name)], "{what}: {}", p.values);
        assert!(
            p.values.contains("consoleBaseAddress: '********'"),
            "{what}: {}",
            p.values
        );
        assert!(p.values.contains("replicas:"), "{what}: {}", p.values);
    }
    let other = snap
        .packs
        .iter()
        .find(|p| p.name == "cert-manager")
        .unwrap();
    assert!(other.masked_paths().is_empty(), "{what}: {}", other.values);
}

/// Make sure that no file in `dir` holds a console address in clear.
fn assert_no_saved_file_holds_an_address(dir: &Path) {
    for file in std::fs::read_dir(dir).unwrap() {
        let path = file.unwrap().path();
        let text = std::fs::read_to_string(&path).unwrap();
        assert_no_address(&path.display().to_string(), &text);
    }
}

#[test]
fn a_record_of_a_cluster_with_the_packs_of_virtual_machines_holds_no_console_address() {
    let snap = Snapshot::from_cluster(&cluster_raw(), None, "cluster:vm-lab", "t0").unwrap();
    assert_masked(
        "the record of the cluster",
        &snap,
        "virtual-machine-orchestrator",
    );
}

#[test]
fn a_record_of_a_profile_and_of_a_draft_holds_no_console_address_also_under_the_old_name() {
    let old = vm_packs("spectro-vm-dashboard", 2);
    let new = vm_packs("virtual-machine-orchestrator", 3);
    let published = Snapshot::from_raw(
        &profile_raw(template("all", "add-on", old.clone()), None, &old),
        "vm-stack@1.0.0",
        "t0",
    )
    .unwrap();
    assert_masked(
        "the record of the profile",
        &published,
        "spectro-vm-dashboard",
    );
    // a profile with a draft: packs/manifests serves the packs of the draft
    let edited = profile_raw(
        template("all", "add-on", old.clone()),
        Some(template("all", "add-on", new.clone())),
        &new,
    );
    let draft = Snapshot::from_raw_draft(&edited, "vm-stack@1.0.0", "t0").unwrap();
    assert_masked(
        "the record of the draft",
        &draft,
        "virtual-machine-orchestrator",
    );
    // a version that nobody published has its packs in the draft alone
    let unpublished = profile_raw(
        no_template(),
        Some(template("all", "add-on", new.clone())),
        &new,
    );
    let snap = Snapshot::from_raw(&unpublished, "vm-stack@1.0.0", "t0").unwrap();
    assert_masked(
        "the record of a draft only",
        &snap,
        "virtual-machine-orchestrator",
    );
}

#[test]
fn a_saved_directory_holds_no_console_address_and_gives_the_record_of_the_fetch() {
    let dir = tempfile::tempdir().unwrap();
    let raw = cluster_raw();
    raw.save_dir(dir.path(), "t0").unwrap();
    assert_no_saved_file_holds_an_address(dir.path());
    let fetched = Snapshot::from_cluster(&raw, None, "cluster:vm-lab", "t0").unwrap();
    let saved = Snapshot::from_cluster(
        &RawCluster::load_dir(dir.path()).unwrap(),
        None,
        "cluster:vm-lab",
        "t0",
    )
    .unwrap();
    assert_eq!(saved.root, fetched.root);

    let dir = tempfile::tempdir().unwrap();
    let packs = vm_packs("virtual-machine-orchestrator", 2);
    let raw = profile_raw(template("all", "add-on", packs.clone()), None, &packs);
    raw.save_dir(dir.path(), "t0").unwrap();
    assert_no_saved_file_holds_an_address(dir.path());
    let fetched = Snapshot::from_raw(&raw, "vm-stack@1.0.0", "t0").unwrap();
    let saved = Snapshot::from_raw(
        &RawProfile::load_dir(dir.path()).unwrap(),
        "vm-stack@1.0.0",
        "t0",
    )
    .unwrap();
    assert_eq!(saved.root, fetched.root);
}

#[test]
fn a_question_to_palette_sends_the_console_address_of_the_cluster_in_place_of_the_mask() {
    let raw = cluster_raw();
    let cluster = Snapshot::from_cluster(&raw, None, "cluster:vm-lab", "t0").unwrap();
    let route = format!("{}/validate/repave", cluster_route(CLUSTER_UID));
    let api = MemoryApi::new()
        .with(cluster_route(CLUSTER_UID), raw.cluster.clone())
        .with(post(&route), json!({"isRepaveRequired": false}));
    let palette = Palette::new(&api);
    // the values of the pack that the question sends, from the last body
    let sent = |name: &str| -> String {
        let (_, body) = api.posted().pop().unwrap();
        let packs = body["profiles"][0]["packValues"]
            .as_array()
            .unwrap()
            .clone();
        let p = packs.iter().find(|p| p["name"] == name).unwrap();
        p["values"].as_str().unwrap().to_string()
    };
    let held = |name: &str| -> String {
        let packs = &raw.cluster["spec"]["clusterProfileTemplates"][0]["packs"];
        let p = packs
            .as_array()
            .unwrap()
            .iter()
            .find(|p| p["name"] == name)
            .unwrap();
        p["values"].as_str().unwrap().to_string()
    };

    // the version that the cluster runs, with another number of replicas of the orchestrator
    let packs = vm_packs("virtual-machine-orchestrator", 3);
    let target = Snapshot::from_raw(
        &profile_raw(template("all", "add-on", packs.clone()), None, &packs),
        "vm-stack@1.0.0",
        "t0",
    )
    .unwrap();
    ops::palette_repave(&palette, &cluster, &target, ImpactLevel::InPlace).unwrap();
    let orchestrator = sent("virtual-machine-orchestrator");
    assert!(
        orchestrator.contains(&format!("consoleBaseAddress: '{CONSOLE}'")),
        "{orchestrator}"
    );
    assert!(orchestrator.contains("replicas: 3"), "{orchestrator}");
    assert!(!orchestrator.contains("********"), "{orchestrator}");
    // a pack with no other change goes with the values that the cluster holds
    assert_eq!(
        sent("vm-migration-assistant"),
        held("vm-migration-assistant")
    );
}

#[test]
fn a_question_for_a_cluster_that_does_not_run_the_pack_sends_an_empty_console_address() {
    // the cluster runs cert-manager alone
    let packs = json!([pack("cert-manager", "1.9.1", "replicas: 1\n")]);
    let doc = cluster_doc(&packs);
    let raw = RawCluster {
        cluster: doc.clone(),
        ..cluster_raw()
    };
    let cluster = Snapshot::from_cluster(&raw, None, "cluster:vm-lab", "t0").unwrap();
    let route = format!("{}/validate/repave", cluster_route(CLUSTER_UID));
    let api = MemoryApi::new()
        .with(cluster_route(CLUSTER_UID), doc)
        .with(post(&route), json!({"isRepaveRequired": false}));
    let added = vm_packs("virtual-machine-orchestrator", 2);
    let target = Snapshot::from_raw(
        &profile_raw(template("all", "add-on", added.clone()), None, &added),
        "vm-stack@1.0.0",
        "t0",
    )
    .unwrap();
    ops::palette_repave(&Palette::new(&api), &cluster, &target, ImpactLevel::InPlace).unwrap();
    let (_, body) = api.posted().pop().unwrap();
    let text = body.to_string();
    assert!(text.contains("consoleBaseAddress: ''"), "{text}");
    assert!(!text.contains("********"), "{text}");
}

#[test]
fn an_update_that_keeps_the_pack_of_a_cluster_of_a_template_sends_no_values_of_the_pack() {
    // a cluster template governs the cluster, and the route of the template takes no pack values
    let mut raw = cluster_raw();
    raw.cluster["spec"]["clusterTemplate"] = json!({"uid": "tpl-vm"});
    let cluster = Snapshot::from_cluster(&raw, None, "cluster:vm-lab", "t0").unwrap();
    let pending = cluster.clone().mark_pending();
    let route = "/v1/spectroclusters/clusterTemplates/tpl-vm/validate/repave";
    let api = MemoryApi::new()
        .with(cluster_route(CLUSTER_UID), raw.cluster.clone())
        .with(post(route), json!({"isRepaveRequired": false}));
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
    assert_eq!(body["profiles"], json!([{"uid": PROFILE_UID}]));
}
