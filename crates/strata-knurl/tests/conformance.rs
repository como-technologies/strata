//! The adapter against the rules that knurl gives for the read half of its platform port.
//!
//! The tenant is in memory. It is made from the registry answers that this repository tracks
//! and from the profiles and the cluster that `just examples` builds from them. So the digests,
//! the tags, and the values are those of a real registry, and the test needs no network.

use std::fs;
use std::future::Future;
use std::path::{Path, PathBuf};

use knurl_core::{
    ContentDigest, FailureClass, Health, InstanceName, Level, PendingChange, Pins, UnitName,
    UnitRef, Version,
};
use knurl_ports::conformance::{World, read_half};
use knurl_ports::{PlatformRead, PlatformReadExt};
use serde_json::{Value, json};
use strata::api::{MemoryApi, Palette};
use strata_knurl::{StrataPlatform, definition_of};

const PACKS: [&str; 5] = [
    "ubuntu-aws",
    "kubernetes",
    "cni-cilium-oss",
    "csi-aws-ebs",
    "cert-manager",
];
const VERSIONS: [&str; 3] = ["1.0.0", "1.1.0", "1.1.1"];

/// Runs a future on a runtime of this thread. The adapter moves each read to a blocking task,
/// and the future waits for it.
fn block_on<F: Future>(future: F) -> F::Output {
    tokio::runtime::Builder::new_current_thread()
        .build()
        .expect("a runtime of this thread")
        .block_on(future)
}

fn examples() -> PathBuf {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples");
    assert!(
        dir.join("profiles/raw").is_dir(),
        "no built examples: run `just examples` first, or use `just adapter-test`"
    );
    dir
}

fn read(path: &Path) -> Value {
    serde_json::from_str(&fs::read_to_string(path).unwrap_or_else(|e| panic!("{path:?}: {e}")))
        .unwrap()
}

/// Gives each pack uid and each registry uid of a document another value. Palette does this
/// when a team copies the packs to another registry and publishes the profile from there.
fn move_ids(doc: &mut Value) {
    match doc {
        Value::Object(map) => {
            for (key, value) in map.iter_mut() {
                match value {
                    Value::String(id)
                        if (key == "packUid" || key == "registryUid") && !id.is_empty() =>
                    {
                        *id = format!("moved-{id}");
                    }
                    other => move_ids(other),
                }
            }
        }
        Value::Array(items) => items.iter_mut().for_each(move_ids),
        _ => {}
    }
}

const SECOND_REGISTRY: &str = "reg-second";
const SECOND_PACK_UID: &str = "second-pack-uid";
const OTHER_ARCHIVE: &str =
    "sha256:a746e14689085b14531566f9c9cfb2bc7716e96a394547ef50a1c52eab510c9e";

/// Blanks every `digest` in a document, as a registry that Palette does not sync lists none.
fn blank_digests(doc: &mut Value) {
    match doc {
        Value::Object(map) => {
            for (key, value) in map.iter_mut() {
                match value {
                    Value::String(digest) if key == "digest" => digest.clear(),
                    other => blank_digests(other),
                }
            }
        }
        Value::Array(items) => items.iter_mut().for_each(blank_digests),
        _ => {}
    }
}

/// What a test changes in the example tenant.
#[derive(Clone, Copy, Default)]
struct Setup {
    /// Each profile names the ids of another registry.
    moved: bool,
    /// A second registry holds csi-aws-ebs 1.65.0 too, with this digest. `None` is the same
    /// digest: a copy by digest. A real tenant has a pack in two registries.
    second_registry: Option<Option<&'static str>>,
    /// The registry lists no digest for csi-aws-ebs, as one that Palette does not sync.
    no_digest: bool,
    /// The cluster summary gives this health state. `None` serves no summary, as a Palette
    /// without the dashboard endpoint.
    health: Option<&'static str>,
    /// The cluster summary answers this HTTP error. A Palette without the route answers 404 or
    /// 405.
    summary_error: Option<u16>,
    /// The profile the cluster runs does not list the cluster among its consumers, as Palette
    /// answers on an error, or from a project the key cannot read.
    unlisted_consumer: bool,
    /// The notifications of the cluster: `Some(true)` holds an open update of the profile the
    /// cluster runs, `Some(false)` holds none, and `None` serves no notifications.
    waiting: Option<bool>,
}

/// The example tenant: one registry with five packs, three versions of `aws-stack`, and the
/// cluster `workload-a`, which runs 1.1.0.
fn tenant(setup: Setup) -> MemoryApi {
    let moved = setup.moved;
    let dir = examples();
    let registry = registry();
    let registry = registry.as_str();
    let mut api = MemoryApi::new().with(
        "/v1/registries/metadata",
        json!({"items": [{"name": "Public Repo", "uid": registry, "kind": "pack"}]}),
    );
    if setup.second_registry.is_some() {
        api = api.with(
            "/v1/registries/metadata",
            json!({"items": [
                {"name": "Public Repo", "uid": registry, "kind": "pack"},
                {"name": "Second Repo", "uid": SECOND_REGISTRY, "kind": "pack"},
            ]}),
        );
    }
    for name in PACKS {
        let mut summaries = read(&dir.join("packs").join(name).join("_summaries.json"));
        if let (Some(digest), "csi-aws-ebs") = (setup.second_registry, name) {
            // The same version in the second registry, as its own pack with its own uid.
            let items = summaries.as_array_mut().unwrap();
            let mut copy = items
                .iter()
                .find(|item| item["spec"]["version"] == "1.65.0")
                .unwrap()
                .clone();
            let first_uid = copy["metadata"]["uid"].as_str().unwrap().to_string();
            copy["metadata"]["uid"] = json!(SECOND_PACK_UID);
            copy["spec"]["registryUid"] = json!(SECOND_REGISTRY);
            if let Some(digest) = digest {
                copy["spec"]["digest"] = json!(digest);
            }
            items.push(copy);
            let entity =
                fs::read_to_string(dir.join("packs").join(name).join("_entity.json")).unwrap();
            let entity = entity
                .replace(&first_uid, SECOND_PACK_UID)
                .replace(registry, SECOND_REGISTRY);
            api = api.with(
                format!("/v1/packs/{name}/registries/{SECOND_REGISTRY}"),
                serde_json::from_str(&entity).unwrap(),
            );
        }
        let count = summaries.as_array().map(Vec::len).unwrap_or(0);
        let mut entity = read(&dir.join("packs").join(name).join("_entity.json"));
        if setup.no_digest && name == "csi-aws-ebs" {
            blank_digests(&mut summaries);
            blank_digests(&mut entity);
        }
        api = api
            .with(
                format!("/v1/packs?filters=spec.name={name}&limit=100&offset=0"),
                json!({"items": summaries, "listmeta": {"count": count}}),
            )
            .with(format!("/v1/packs/{name}/registries/{registry}"), entity);
    }

    let cluster = read(&dir.join("clusters/raw/workload-a/cluster.json"));
    let cluster_uid = cluster["metadata"]["uid"].as_str().unwrap().to_string();
    let mut listing = Vec::new();
    for version in VERSIONS {
        let raw = dir
            .join("profiles/raw")
            .join(format!("aws-stack@{version}"));
        let mut profile = read(&raw.join("profile.json"));
        let uid = profile["metadata"]["uid"].as_str().unwrap().to_string();
        if version == "1.1.0" && !setup.unlisted_consumer {
            profile["status"]["inUseClusters"] =
                json!([{"uid": cluster_uid, "name": "workload-a", "projectUid": ""}]);
        }
        if moved {
            move_ids(&mut profile);
        }
        listing.push(
            json!({"metadata": {"name": "aws-stack", "uid": uid}, "spec": {"version": version}}),
        );
        api = api
            .with(format!("/v1/clusterprofiles/{uid}"), profile)
            .with(
                format!("/v1/clusterprofiles/{uid}/packs/manifests"),
                read(&raw.join("manifests.json")),
            )
            .with(
                format!("/v1/clusterprofiles/{uid}/variables"),
                read(&raw.join("variables.json")),
            );
    }
    let raw = dir.join("clusters/raw/workload-a");
    if let Some(state) = setup.health {
        // The health is on the cluster summary, and not on the cluster document.
        let conditions = if state == "UnHealthy" {
            json!([{"type": "HeartbeatFailure", "message": "no heartbeat for 10m"}])
        } else {
            json!([])
        };
        api = api.with(
            format!("/v1/dashboard/spectroclusters/{cluster_uid}"),
            json!({"status": {"health": {"state": state, "conditions": conditions}}}),
        );
    }
    if let Some(status) = setup.summary_error {
        api = api.failing(
            format!("/v1/dashboard/spectroclusters/{cluster_uid}"),
            status,
            "{}",
        );
    }
    if let Some(open) = setup.waiting {
        let items = if open {
            let profile = &cluster["spec"]["clusterProfileTemplates"][0]["uid"];
            json!([{
                "metadata": {"uid": "n1", "creationTimestamp": "2026-09-24T23:55:02.083Z"},
                "action": {"actionType": "NotifyActionClusterProfileUpdate", "isDone": false,
                    "events": {"e1": {"profileUid": profile, "message": "Manifest 'm' is updated"}}},
            }])
        } else {
            Value::Null // Palette gives null, not [], when a cluster has none
        };
        api = api.with(
            format!("/v1/notifications/spectrocluster/{cluster_uid}"),
            json!({"items": items, "listmeta": {"continue": ""}}),
        );
    }
    api.with("/v1/dashboard/clusterprofiles/metadata", json!({"items": listing}))
        .with(
            "/v1/dashboard/spectroclusters/meta",
            json!([{"metadata": {"name": "workload-a", "uid": cluster_uid}, "status": {"state": "Running"}}]),
        )
        .with(format!("/v1/spectroclusters/{cluster_uid}"), cluster)
        .with(
            format!("/v1/spectroclusters/{cluster_uid}/profiles/packs/manifests"),
            read(&raw.join("manifests.json")),
        )
        .with(format!("/v1/spectroclusters/{cluster_uid}/variables"), read(&raw.join("variables.json")))
}

/// The registry uid that the example profiles name.
fn registry() -> String {
    let profile = read(&examples().join("profiles/raw/aws-stack@1.1.0/profile.json"));
    profile["spec"]["published"]["packs"][0]["registryUid"]
        .as_str()
        .unwrap()
        .to_string()
}

/// `MemoryApi` is not `Sync`, so each call makes its own. A real caller gives an `HttpApi`.
type Connect = fn(Option<&str>) -> strata::Result<Palette<MemoryApi>>;

fn platform() -> StrataPlatform<MemoryApi, Connect> {
    StrataPlatform::new(|_project| Ok(Palette::new(tenant(Setup::default()))))
}

fn moved_platform() -> StrataPlatform<MemoryApi, Connect> {
    StrataPlatform::new(|_project| {
        Ok(Palette::new(tenant(Setup {
            moved: true,
            ..Setup::default()
        })))
    })
}

/// csi-aws-ebs 1.65.0 is in two registries, with one digest: a copy by digest.
fn copied_platform() -> StrataPlatform<MemoryApi, Connect> {
    StrataPlatform::new(|_project| {
        Ok(Palette::new(tenant(Setup {
            second_registry: Some(None),
            ..Setup::default()
        })))
    })
}

/// csi-aws-ebs 1.65.0 is in two registries, with two digests: two archives under one version.
fn two_archives_platform() -> StrataPlatform<MemoryApi, Connect> {
    StrataPlatform::new(|_project| {
        Ok(Palette::new(tenant(Setup {
            second_registry: Some(Some(OTHER_ARCHIVE)),
            ..Setup::default()
        })))
    })
}

fn unit(level: Level, name: &str, version: &str) -> UnitRef {
    UnitRef {
        level,
        name: UnitName::new(name).unwrap(),
        version: Version::new(version).unwrap(),
        digest: None,
    }
}

/// The digest of csi-aws-ebs 1.65.0 in the registry answers that this repository tracks.
fn tracked_archive() -> ContentDigest {
    block_on(platform().observe_unit(&world().leaf))
        .unwrap()
        .content
}

fn other_archive() -> ContentDigest {
    OTHER_ARCHIVE.parse().unwrap()
}

fn world() -> World {
    let platform = platform();
    let published = platform
        .snapshot(&unit(Level::Intermediate, "aws-stack", "1.1.0"))
        .unwrap();
    World {
        leaf: unit(Level::Leaf, "csi-aws-ebs", "1.65.0"),
        intermediate: unit(Level::Intermediate, "aws-stack", "1.1.0"),
        unheld: unit(Level::Intermediate, "aws-stack", "1.0.0"),
        unpublished: unit(Level::Intermediate, "aws-stack", "9.9.9"),
        instance: InstanceName::new("workload-a").unwrap(),
        no_instance: InstanceName::new("no-such-cluster").unwrap(),
        definition: definition_of(&published).unwrap(),
    }
}

#[test]
fn the_adapter_obeys_the_read_half_of_the_platform_port() {
    let failures = block_on(read_half(&platform(), &world()));
    assert!(
        failures.is_empty(),
        "the adapter broke a rule of knurl: {failures:#?}"
    );
}

#[test]
fn a_copy_of_the_packs_in_another_registry_is_another_object_and_the_same_unit() {
    // The same profile, with each pack uid and the registry uid of another registry: what a
    // raise gives when it copies each pack by digest.
    let here = block_on(platform().observe_unit(&world().intermediate)).unwrap();
    let there = block_on(moved_platform().observe_unit(&world().intermediate)).unwrap();
    assert_ne!(
        here.exact, there.exact,
        "Palette gives other ids, so the object differs"
    );
    assert!(here.same_unit(&there), "the content digest has no such id");
}

#[test]
fn a_leaf_has_the_digest_that_the_registry_gives() {
    // The digest that strata prints is the digest that knurl pins. This one is the digest of
    // csi-aws-ebs 1.65.0 in the registry answers that this repository tracks.
    let platform = platform();
    let leaf = block_on(platform.observe_unit(&world().leaf)).unwrap();
    assert!(
        leaf.content.to_string().starts_with("sha256:bd09e8b4187b"),
        "{}",
        leaf.content
    );
    assert!(
        leaf.pins.is_empty(),
        "a leaf pins nothing that Palette holds"
    );

    let profile = block_on(platform.observe_unit(&world().intermediate)).unwrap();
    assert_eq!(
        profile.pins.len(),
        5,
        "aws-stack 1.1.0 has five packs from a registry"
    );
    assert_eq!(profile.pins["csi-aws-ebs"].digest, leaf.content);
}

#[test]
fn a_leaf_in_two_registries_with_one_digest_is_one_leaf() {
    // A raise copies a pack to another registry by digest. The content is the same there, so
    // the adapter can answer with no registry in the address.
    let alone = block_on(platform().observe_unit(&world().leaf)).unwrap();
    let copied = block_on(copied_platform().observe_unit(&world().leaf)).unwrap();
    assert_eq!(copied.content, alone.content);
    assert_ne!(
        copied.exact, alone.exact,
        "two objects hold the leaf now, and not one"
    );
}

#[test]
fn a_leaf_with_two_archives_and_no_digest_is_a_terminal_failure_that_names_each_one() {
    // Two kinds of registry can hold two archives under one name and version. An address with
    // no digest cannot say which one it means. The failure gives what the next address needs.
    let error = block_on(two_archives_platform().observe_unit(&world().leaf)).unwrap_err();
    assert_eq!(error.class(), FailureClass::Terminal);
    let message = error.to_string();
    for part in [
        "Public Repo",
        "Second Repo",
        "sha256:bd09e8b4187b",
        OTHER_ARCHIVE,
    ] {
        assert!(
            message.contains(part),
            "the failure does not name {part}: {message}"
        );
    }
}

#[test]
fn the_digest_of_an_address_chooses_one_of_two_archives() {
    // A pin holds the content digest, so what pins the leaf can say which archive it means.
    let platform = two_archives_platform();
    let first = world().leaf.with_digest(tracked_archive());
    let first = block_on(platform.observe_unit(&first)).unwrap();
    assert_eq!(first.content, tracked_archive());

    let second = world().leaf.with_digest(other_archive());
    let second = block_on(platform.observe_unit(&second)).unwrap();
    assert_eq!(second.content, other_archive());
    assert_ne!(first.exact, second.exact, "two objects hold two contents");
}

#[test]
fn a_digest_that_no_archive_has_is_a_terminal_failure_that_names_each_one() {
    let none: ContentDigest = format!("sha256:{}", "0".repeat(64)).parse().unwrap();
    let address = world().leaf.with_digest(none);
    let error = block_on(two_archives_platform().observe_unit(&address)).unwrap_err();
    assert_eq!(error.class(), FailureClass::Terminal);
    let message = error.to_string();
    for part in [
        "none is sha256:0000",
        "Public Repo",
        "Second Repo",
        "sha256:bd09e8b4187b",
        OTHER_ARCHIVE,
    ] {
        assert!(
            message.contains(part),
            "the failure does not name {part}: {message}"
        );
    }
}

#[test]
fn the_digest_of_an_address_never_refuses_the_one_archive() {
    // One registry holds the leaf. A read by another digest gives it, and the caller compares.
    let address = world().leaf.with_digest(other_archive());
    let given = block_on(platform().observe_unit(&address)).unwrap();
    assert_eq!(given.content, tracked_archive());
    assert_ne!(given.content, other_archive());
}

#[test]
fn the_pins_of_a_profile_are_in_the_order_of_its_layers() {
    // Palette stores the layers in the order given and keeps it, so the order is content, and
    // knurl keeps it.
    let platform = platform();
    let published = platform.snapshot(&world().intermediate).unwrap();
    let profile = block_on(platform.observe_unit(&world().intermediate)).unwrap();
    let packs: Vec<String> = published.pins().into_iter().map(|p| p.name).collect();
    let pins: Vec<&str> = profile.pins.names().map(UnitName::as_str).collect();
    assert_eq!(pins, packs);
    assert_eq!(
        pins[0], "ubuntu-aws",
        "the operating system is the first layer"
    );
}

#[test]
fn the_order_of_the_pins_is_content() {
    // The pins of a definition own the order of the layers. Two orders are two units.
    let platform = platform();
    let published = platform.snapshot(&world().intermediate).unwrap();
    let definition = definition_of(&published).unwrap();
    let forward = block_on(platform.identity_of(&definition)).unwrap();

    // The last two layers, the other way round.
    let count = definition.pins.len();
    let mut order: Vec<(UnitName, Version)> = definition
        .pins
        .iter()
        .map(|(name, version)| (name.clone(), version.clone()))
        .collect();
    order.swap(count - 2, count - 1);
    let swapped = knurl_core::UnitDefinition {
        pins: order.into_iter().collect::<Pins<Version>>(),
        ..definition
    };
    let backward = block_on(platform.identity_of(&swapped)).unwrap();
    assert_ne!(forward, backward);
}

#[test]
fn an_instance_gives_the_state_and_the_copy_that_it_holds() {
    let platform = platform();
    let seen = block_on(platform.observe_instance(&world().instance)).unwrap();
    assert_eq!(seen.state.0, "Running");
    assert_eq!(
        seen.health,
        Health::Unknown,
        "this tenant serves no cluster summary, so the health is unknown"
    );
    assert_eq!(seen.pins["aws-stack"].version.as_str(), "1.1.0");
}

#[test]
fn an_instance_says_whether_palette_has_an_update_waiting_for_it() {
    // Connect is a plain function, so each tenant gets its own closure.
    let seen = |platform: StrataPlatform<MemoryApi, Connect>| {
        block_on(platform.observe_instance(&world().instance))
            .unwrap()
            .pending
    };
    let waiting: StrataPlatform<MemoryApi, Connect> = StrataPlatform::new(|_project| {
        Ok(Palette::new(tenant(Setup {
            waiting: Some(true),
            ..Setup::default()
        })))
    });
    let clear: StrataPlatform<MemoryApi, Connect> = StrataPlatform::new(|_project| {
        Ok(Palette::new(tenant(Setup {
            waiting: Some(false),
            ..Setup::default()
        })))
    });
    assert_eq!(
        seen(waiting),
        PendingChange::Waiting(["aws-stack".parse::<UnitName>().unwrap()].into())
    );
    assert_eq!(seen(clear), PendingChange::Clear);
    assert_eq!(
        seen(platform()),
        PendingChange::Unknown,
        "a tenant that serves no notifications gives no answer"
    );
}

#[test]
fn an_instance_gives_its_health_from_the_cluster_summary() {
    // Palette keeps the health on the cluster summary, with two states. The adapter maps them,
    // and an unhealthy instance names its first condition.
    let healthy: StrataPlatform<MemoryApi, Connect> = StrataPlatform::new(|_project| {
        Ok(Palette::new(tenant(Setup {
            health: Some("Healthy"),
            ..Setup::default()
        })))
    });
    let seen = block_on(healthy.observe_instance(&world().instance)).unwrap();
    assert_eq!(seen.health, Health::Healthy);

    let unhealthy: StrataPlatform<MemoryApi, Connect> = StrataPlatform::new(|_project| {
        Ok(Palette::new(tenant(Setup {
            health: Some("UnHealthy"),
            ..Setup::default()
        })))
    });
    let seen = block_on(unhealthy.observe_instance(&world().instance)).unwrap();
    assert_eq!(
        seen.health,
        Health::Unhealthy("no heartbeat for 10m".into())
    );
}

#[test]
fn an_instance_has_no_health_when_palette_answers_405_for_the_cluster_summary() {
    let platform: StrataPlatform<MemoryApi, Connect> = StrataPlatform::new(|_project| {
        Ok(Palette::new(tenant(Setup {
            summary_error: Some(405),
            ..Setup::default()
        })))
    });
    let seen = block_on(platform.observe_instance(&world().instance)).unwrap();
    assert_eq!(seen.health, Health::Unknown);
}

#[test]
fn a_leaf_with_no_digest_is_a_terminal_failure() {
    // A registry that Palette does not sync lists no digest. Then there is no content to pin by.
    let platform: StrataPlatform<MemoryApi, Connect> = StrataPlatform::new(|_project| {
        Ok(Palette::new(tenant(Setup {
            no_digest: true,
            ..Setup::default()
        })))
    });
    let error = block_on(platform.observe_unit(&world().leaf)).unwrap_err();
    assert_eq!(error.class(), FailureClass::Terminal);
    assert!(error.to_string().contains("lists no digest"), "{error}");
}

#[test]
fn pinned_by_finds_an_instance_the_profile_does_not_list() {
    // Palette computes the consumer list of a profile when it is asked, and gives an empty list
    // on an error too. The adapter reads each cluster as well, so the precondition before a
    // retire does not trust an empty answer.
    let platform: StrataPlatform<MemoryApi, Connect> = StrataPlatform::new(|_project| {
        Ok(Palette::new(tenant(Setup {
            unlisted_consumer: true,
            ..Setup::default()
        })))
    });
    let holders = block_on(platform.pinned_by(&world().intermediate)).unwrap();
    assert!(holders.contains(&world().instance), "{holders:?}");
}

#[test]
fn a_new_record_of_a_unit_verifies() {
    let platform = platform();
    let recorded = block_on(platform.observe_unit(&world().intermediate)).unwrap();
    assert_eq!(
        block_on(platform.verify(&recorded)).unwrap(),
        knurl_core::Verification::Unchanged
    );
}

#[test]
fn this_platform_holds_no_root_unit() {
    let error =
        block_on(platform().observe_unit(&unit(Level::Root, "edge-cluster", "1"))).unwrap_err();
    assert_eq!(error.class(), FailureClass::Terminal);
}
