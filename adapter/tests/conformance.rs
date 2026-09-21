//! The adapter against the rules that knurl gives for the read half of its platform port.
//!
//! The tenant is in memory. It is made from the registry answers that this repository tracks
//! and from the profiles and the cluster that `just examples` builds from them. So the digests,
//! the tags, and the values are those of a real registry, and the test needs no network.

use std::fs;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::pin;
use std::task::{Context, Poll, Waker};

use knurl_core::{FailureClass, InstanceName, Level, UnitName, UnitRef, Version};
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

/// Runs a future that is ready at its first poll. The adapter does not wait: strata blocks.
fn block_on<F: Future>(future: F) -> F::Output {
    let mut context = Context::from_waker(Waker::noop());
    let mut future = pin!(future);
    loop {
        if let Poll::Ready(output) = future.as_mut().poll(&mut context) {
            return output;
        }
    }
}

fn examples() -> PathBuf {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../examples");
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

/// What a test changes in the example tenant.
#[derive(Clone, Copy, Default)]
struct Setup {
    /// Each profile names the ids of another registry.
    moved: bool,
    /// A second registry holds csi-aws-ebs 1.65.0 too, with this digest. `None` is the same
    /// digest: a copy by digest. A real tenant has a pack in two registries.
    second_registry: Option<Option<&'static str>>,
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
        api = api
            .with(
                format!("/v1/packs?filters=spec.name={name}&limit=100&offset=0"),
                json!({"items": summaries, "listmeta": {"count": count}}),
            )
            .with(
                format!("/v1/packs/{name}/registries/{registry}"),
                read(&dir.join("packs").join(name).join("_entity.json")),
            );
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
        if version == "1.1.0" {
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
    }
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
fn a_leaf_with_two_archives_is_a_terminal_failure_that_names_each_one() {
    // Two kinds of registry can hold two archives under one name and version. An address has
    // no registry, so knurl cannot say which one it means. The failure gives what it needs.
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
fn an_instance_gives_the_state_and_the_copy_that_it_holds() {
    let platform = platform();
    let seen = block_on(platform.observe_instance(&world().instance)).unwrap();
    assert_eq!(seen.state.0, "Running");
    assert_eq!(
        seen.health,
        knurl_core::Health::Unknown,
        "strata reads no health"
    );
    assert_eq!(seen.pins["aws-stack"].version.as_str(), "1.1.0");
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
