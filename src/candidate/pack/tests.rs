//! The tests of the pack versions that a ref picks from.

use serde_json::json;

use super::*;
use crate::api::MemoryApi;

/// The route of the tags of `nginx` in the registry `reg-public`.
const TAGS: &str = "/v1/packs/nginx/registries/reg-public";

/// The listing entry of `nginx` at `version` in `registry`, in `state` when it is not empty.
fn entry(registry: &str, version: &str, state: &str) -> Value {
    let mut entry = json!({
        "metadata": {"uid": format!("pk-{version}")},
        "spec": {"name": "nginx", "version": version, "registryUid": registry},
    });
    if !state.is_empty() {
        entry["spec"]["annotations"] = json!({"system_state": state});
    }
    entry
}

/// An API whose registry answer holds a tag for each of `tagged`.
fn tags_for(tagged: &[&str]) -> MemoryApi {
    let tags: Vec<Value> = tagged
        .iter()
        .map(|v| json!({"packUid": format!("pk-{v}"), "version": v, "tag": v}))
        .collect();
    MemoryApi::new().with(TAGS, json!({"layer": "addon", "tags": tags}))
}

fn picked(api: &MemoryApi, entries: &[Value], version: &str) -> Option<String> {
    picked_in(&Palette::new(api), entries, "nginx", "reg-public", version).unwrap()
}

#[test]
fn latest_is_picked_from_the_versions_that_the_registry_serves_and_an_exact_version_from_the_listing()
 {
    // the registry gives no tag to 1.30.0, as a FIPS tenant does for a version that is not FIPS
    let api = tags_for(&["1.29.0"]);
    let entries = [
        entry("reg-public", "1.30.0", ""),
        entry("reg-public", "1.29.0", ""),
    ];
    assert_eq!(picked(&api, &entries, LATEST).as_deref(), Some("1.29.0"));
    assert_eq!(picked(&api, &entries, "1.30.0").as_deref(), Some("1.30.0"));
    assert_eq!(picked(&api, &entries, "1.31.0"), None);
}

#[test]
fn latest_keeps_a_version_that_spectro_cloud_retired_and_picks_none_when_the_registry_answers_404()
{
    // the plain answer has no disabled version, and the registry answers 404 to the states query
    let entries = [
        entry("reg-public", "1.30.0", "disabled"),
        entry("reg-public", "1.29.0", ""),
    ];
    let api = tags_for(&["1.29.0"]);
    assert_eq!(picked(&api, &entries, LATEST).as_deref(), Some("1.30.0"));

    // the registry serves no version: only the retired one remains
    let api = MemoryApi::new();
    assert_eq!(picked(&api, &entries, LATEST).as_deref(), Some("1.30.0"));
    assert_eq!(picked(&api, &entries[1..], LATEST), None);
}

#[test]
fn latest_asks_no_registry_that_has_no_version_of_the_pack() {
    let api = tags_for(&["1.29.0"]);
    let entries = [entry("reg-other", "1.29.0", "")];
    assert_eq!(picked(&api, &entries, LATEST), None);
    assert_eq!(api.calls(TAGS), 0);
}
