//! A candidate whose values select an option of the pack template: Palette can write the presets
//! of that option into the values, and no route serves them, so strata records the selection.

use std::fs;
use std::path::Path;

use serde_json::{Value, json};
use strata::api::{MemoryApi, Palette};
use strata::candidate::{ResolvedCandidate, load, resolve};
use strata::snapshot::{PresetOption, Snapshot};
use strata::term::progress::Progress;

use crate::common;
use common::{PUBLIC_REGISTRY, PackFixture, candidate_snapshot, serve_registry_pack, write_yaml};

/// `api` with the registry routes for the pack `name` 1.0.0, with `template` in its listing
/// entry.
fn with_pack(api: MemoryApi, name: &str, template: Option<Value>) -> MemoryApi {
    let uid = format!("pack-{name}-1");
    let mut summary =
        PackFixture::public(name, "addon", "1.0.0", &uid, "sha256:b1", "a:\n  b: x\n").summary(&[]);
    if let Some(t) = template {
        summary["spec"]["template"] = t;
    }
    serve_registry_pack(
        api,
        name,
        json!([summary]),
        json!({"tags": [{"tag": "1.0.0", "packUid": uid}]}),
        PUBLIC_REGISTRY,
    )
}

/// The registry routes for the pack `barney` 1.0.0, with `template` in its listing entry.
fn registry(template: Option<Value>) -> MemoryApi {
    with_pack(MemoryApi::new(), "barney", template)
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

/// Write the candidate `fred` 1.0.0 with `layers` to `dir`, and give its file.
fn write_candidate(dir: &Path, layers: Value) -> std::path::PathBuf {
    let file = dir.join("fred.yaml");
    let candidate = json!({
        "name": "fred", "version": "1.0.0", "type": "add-on", "registryUid": PUBLIC_REGISTRY,
        "layers": layers,
    });
    write_yaml(&file, &candidate);
    file
}

/// The one layer `barney` 1.0.0 with `values`.
fn barney(values: &str) -> Value {
    json!([{"name": "barney", "version": "1.0.0", "values": values}])
}

fn snapshot_with(dir: &Path, values: &str, api: &MemoryApi) -> Snapshot {
    candidate_snapshot(&write_candidate(dir, barney(values)), api).unwrap()
}

/// The resolve of the candidate `fred` with `layers` against `api`.
fn resolved(dir: &Path, layers: Value, api: &MemoryApi) -> ResolvedCandidate {
    let loaded = load(&write_candidate(dir, layers)).unwrap();
    resolve(&loaded, &Palette::new(api), &Progress::silent()).unwrap()
}

/// The selection of the option `x` of the parameter `access`.
fn access_x() -> PresetOption {
    PresetOption {
        parameter: "access".into(),
        target_key: "a.b".into(),
        option: "x".into(),
    }
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

/// The keys of a pack reference that strata builds, as a published profile of Palette gives
/// them, sorted.
const PACK_REF_KEYS: [&str; 12] = [
    "digest",
    "layer",
    "manifests",
    "name",
    "packUid",
    "params",
    "presets",
    "registryUid",
    "tag",
    "type",
    "values",
    "version",
];

#[test]
fn the_documents_of_a_resolve_hold_no_key_of_strata_and_the_result_holds_the_selection() {
    let dir = tempfile::tempdir().unwrap();
    let resolved = resolved(
        dir.path(),
        barney("a:\n  b: x\n"),
        &registry(Some(template())),
    );
    assert_eq!(resolved.preset_options, vec![vec![access_x()]]);

    let pack_ref = &resolved.raw.profile["spec"]["published"]["packs"][0];
    let mut keys: Vec<&str> = pack_ref
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    keys.sort_unstable();
    assert_eq!(keys, PACK_REF_KEYS);
    for doc in [
        &resolved.raw.profile,
        &resolved.raw.manifests,
        &resolved.raw.variables,
    ] {
        assert!(!doc.to_string().contains("presetOptions"), "{doc}");
    }

    // the record of the resolve has the selection, outside the documents
    let snap = resolved.into_snapshot("fred", "t0").unwrap();
    assert_eq!(snap.packs[0].preset_options, vec![access_x()]);
}

#[test]
fn a_raw_directory_saved_from_a_candidate_holds_no_preset_options_key() {
    let dir = tempfile::tempdir().unwrap();
    let resolved = resolved(
        dir.path(),
        barney("a:\n  b: x\n"),
        &registry(Some(template())),
    );
    let saved = dir.path().join("raw");
    resolved.raw.save_dir(&saved, "t0").unwrap();
    let mut files = 0;
    for entry in fs::read_dir(&saved).unwrap() {
        let path = entry.unwrap().path();
        let text = fs::read_to_string(&path).unwrap();
        assert!(
            !text.contains("presetOptions"),
            "{}: {text}",
            path.display()
        );
        files += 1;
    }
    assert!(
        files >= 3,
        "{files} files: the profile, the manifests, the variables"
    );
    // the directory holds only the documents of Palette, so a record built from it has no
    // selection. The record of the resolve has it.
    let from_dir = strata::ops::snapshot_from_dir(&saved, "fred", "t0").unwrap();
    assert!(from_dir.packs[0].preset_options.is_empty());
    assert_eq!(
        from_dir.root,
        resolved.into_snapshot("fred", "t0").unwrap().root,
        "the selection is outside the root"
    );
}

#[test]
fn the_selection_of_each_layer_is_on_its_own_pack_of_the_record() {
    let dir = tempfile::tempdir().unwrap();
    // barney has no template; wilma has one, and its values select the option x
    let api = with_pack(registry(None), "wilma", Some(template()));
    let layers = json!([
        {"name": "barney", "version": "1.0.0", "values": "a:\n  b: x\n"},
        {"name": "wilma", "version": "1.0.0", "values": "a:\n  b: x\n"},
    ]);
    let resolved = resolved(dir.path(), layers.clone(), &api);
    assert_eq!(resolved.preset_options, vec![vec![], vec![access_x()]]);
    let snap = candidate_snapshot(&write_candidate(dir.path(), layers), &api).unwrap();
    let names: Vec<&str> = snap.packs.iter().map(|p| p.name.as_str()).collect();
    assert_eq!(names, ["barney", "wilma"]);
    assert!(snap.packs[0].preset_options.is_empty());
    assert_eq!(snap.packs[1].preset_options, vec![access_x()]);
}
