//! A candidate whose values select an option of the pack template: Palette can write the presets
//! of that option into the values, and no route serves them, so strata records the selection.

use std::fs;
use std::path::Path;

use serde_json::{Value, json};
use strata::api::MemoryApi;
use strata::snapshot::{PresetOption, Snapshot};

use crate::common;
use common::{candidate_snapshot, pack_listing_route};

const REGISTRY: &str = "reg-public";

/// The registry routes for the pack `barney` 1.0.0, with `template` in its listing entry.
fn registry(template: Option<Value>) -> MemoryApi {
    let mut summary = json!({
        "metadata": {"uid": "pack-barney-1"},
        "spec": {"name": "barney", "version": "1.0.0", "registryUid": REGISTRY,
                 "digest": "sha256:b1", "layer": "addon", "type": "spectro",
                 "values": "a:\n  b: x\n"},
    });
    if let Some(t) = template {
        summary["spec"]["template"] = t;
    }
    MemoryApi::new()
        .with(
            pack_listing_route("barney"),
            json!({"items": [summary], "listmeta": {"count": 1}}),
        )
        .with(
            format!("/v1/packs/barney/registries/{REGISTRY}"),
            json!({"tags": [{"tag": "1.0.0", "packUid": "pack-barney-1"}]}),
        )
}

/// One input parameter `access` at `a.b`, with the options `x` and `y`, in the shape of
/// `v1PackTemplate`: each option has a label and no presets, as the listing gives it.
fn template() -> Value {
    json!({"parameters": {"inputParameters": [{
        "name": "access", "displayName": "Access", "targetKey": "a.b",
        "options": {
            "x": {"label": "Ex", "dependencies": []},
            "y": {"label": "Why", "dependencies": []},
        },
    }]}})
}

fn snapshot_with(dir: &Path, values: &str, api: &MemoryApi) -> Snapshot {
    let file = dir.join("fred.yaml");
    let candidate = json!({
        "name": "fred", "version": "1.0.0", "type": "add-on", "registryUid": REGISTRY,
        "layers": [{"name": "barney", "version": "1.0.0", "values": values}],
    });
    fs::write(&file, serde_yaml::to_string(&candidate).unwrap()).unwrap();
    candidate_snapshot(&file, api).unwrap()
}

#[test]
fn values_that_select_a_template_option_record_the_selection_and_keep_the_root() {
    let dir = tempfile::tempdir().unwrap();
    let with = registry(Some(template()));
    let selects = snapshot_with(dir.path(), "a:\n  b: x\n", &with);
    assert_eq!(
        selects.packs[0].preset_options,
        vec![PresetOption {
            parameter: "access".into(),
            target_key: "a.b".into(),
            option: "x".into(),
        }]
    );
    let json = serde_json::to_value(&selects).unwrap();
    assert_eq!(
        json["packs"][0]["presetOptions"],
        json!([{"parameter": "access", "targetKey": "a.b", "option": "x"}])
    );

    // the selection is outside the root: the same values with no template give the same root
    let without = snapshot_with(dir.path(), "a:\n  b: x\n", &registry(None));
    assert!(without.packs[0].preset_options.is_empty());
    assert_eq!(selects.root, without.root);
    assert_eq!(selects.content_digest(), without.content_digest());
}

#[test]
fn values_that_name_no_option_or_have_no_target_key_record_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let with = registry(Some(template()));
    for values in ["a:\n  b: z\n", "c: 1\n"] {
        let snap = snapshot_with(dir.path(), values, &with);
        assert!(
            snap.packs[0].preset_options.is_empty(),
            "{values:?} selects no option"
        );
        let json = serde_json::to_value(&snap).unwrap();
        assert!(
            json["packs"][0].get("presetOptions").is_none(),
            "{values:?}: the record has no presetOptions field"
        );
    }
}

#[test]
fn a_number_or_a_list_item_at_the_target_key_is_read_as_text() {
    let dir = tempfile::tempdir().unwrap();
    let template = json!({"parameters": {"inputParameters": [
        {"name": "replicas", "targetKey": "a.n", "options": {"3": {"label": "Three"}}},
        {"name": "first", "targetKey": "a.list.0.mode", "options": {"open": {"label": "Open"}}},
        {"name": "flag", "targetKey": "a.flag", "options": {"true": {"label": "On"}}},
    ]}});
    let snap = snapshot_with(
        dir.path(),
        "a:\n  n: 3\n  flag: true\n  list:\n    - mode: open\n",
        &registry(Some(template)),
    );
    let got: Vec<(&str, &str)> = snap.packs[0]
        .preset_options
        .iter()
        .map(|o| (o.parameter.as_str(), o.option.as_str()))
        .collect();
    assert_eq!(
        got,
        vec![("first", "open"), ("flag", "true"), ("replicas", "3")]
    );
}
