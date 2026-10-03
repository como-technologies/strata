//! A profile with a draft only: Palette serves its packs with each secret in clear. A record of
//! it, and the answers that `--raw-dir` saves, hold `********` in place of each value whose
//! schema format is `password`, as Palette masks a published pack.

use serde_json::{Value, json};

use strata::api::{MemoryApi, Palette};
use strata::error::Error;
use strata::progress::Progress;
use strata::snapshot::{DRAFT_TEMPLATE, PUBLISHED_TEMPLATE};

mod common;
use common::snapshot_of;

const PROFILE: &str = "64f0000000000000000000d1";
const PACK: &str = "64f0000000000000000000e1";
const SECRET: &str = "s3cret-barney";
const VALUES: &str = "charts:\n  grafana:\n    adminUser: fred # the login\n    \
                      adminPassword: s3cret-barney\n    replicas: 2\n";

/// One pack, `grafana`, in the template at `template`, with `values`.
fn profile_doc(template: &str, values: &str) -> Value {
    json!({
        "metadata": {"uid": PROFILE, "name": "barney"},
        "spec": {"version": "1.0.0", template: {"cloudType": "all", "type": "add-on", "packs": [
            {"name": "grafana", "layer": "addon", "type": "spectro", "tag": "1.0.0",
             "version": "1.0.0", "packUid": PACK, "registryUid": "r1", "values": values}]}},
        "status": {"isPublished": template == "published", "inUseClusterUids": []}
    })
}

/// The answer of `GET /v1/packs/{uid}`: the schema marks `adminPassword` as a password.
fn pack_doc() -> Value {
    json!({"name": "grafana", "packValues": [{"packUid": PACK, "values": "", "schema": [
        {"name": "charts.grafana.adminPassword", "format": "${password}"},
        {"name": "charts.grafana.adminUser", "format": "${string}"}]}]})
}

/// Palette with a draft-only profile, as it answers: the packs and `packs/manifests` in clear.
fn draft_only() -> MemoryApi {
    MemoryApi::new()
        .with(
            format!("/v1/clusterprofiles/{PROFILE}"),
            profile_doc("draft", VALUES),
        )
        .with(
            format!("/v1/clusterprofiles/{PROFILE}/packs/manifests"),
            json!({"spec": {"packs": [{"metadata": {"name": "grafana", "uid": PACK},
                "spec": {"name": "grafana", "version": "1.0.0", "values": VALUES}}]}}),
        )
        .with(
            format!("/v1/clusterprofiles/{PROFILE}/variables"),
            json!({"variables": []}),
        )
}

#[test]
fn a_record_of_a_draft_holds_no_clear_secret() {
    let api = draft_only().with(format!("/v1/packs/{PACK}"), pack_doc());
    let snap = snapshot_of(&api, PROFILE).unwrap();
    assert_eq!(snap.source.template, DRAFT_TEMPLATE);
    let record = snap.to_json(false).unwrap();
    assert!(!record.contains(SECRET), "{record}");
    let pack = &snap.packs[0];
    assert_eq!(
        pack.values,
        "charts:\n  grafana:\n    adminUser: fred # the login\n    \
         adminPassword: '********'\n    replicas: 2\n"
    );
    assert_eq!(
        pack.values_canon["charts"]["grafana"]["adminPassword"],
        "********"
    );
    assert_eq!(api.calls(&format!("/v1/packs/{PACK}")), 1);
}

#[test]
fn the_saved_answers_of_a_draft_hold_no_clear_secret() {
    let api = draft_only().with(format!("/v1/packs/{PACK}"), pack_doc());
    let raw = Palette::new(&api)
        .fetch_profile(PROFILE, &Progress::silent())
        .unwrap();
    let dir = tempfile::tempdir().unwrap();
    raw.save_dir(dir.path(), "t0").unwrap();
    for file in std::fs::read_dir(dir.path()).unwrap() {
        let path = file.unwrap().path();
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(!text.contains(SECRET), "{}: {text}", path.display());
    }
    assert!(raw.manifests.to_string().contains("'********'"));
}

#[test]
fn a_masked_draft_has_the_values_digest_of_the_version_that_palette_masks() {
    let draft = draft_only().with(format!("/v1/packs/{PACK}"), pack_doc());
    // Palette writes the published values again with its YAML library: other text, same YAML
    let masked = "charts:\n    grafana:\n        adminUser: fred\n        \
                  adminPassword: '********'\n        replicas: 2\n";
    let published = MemoryApi::new()
        .with(
            format!("/v1/clusterprofiles/{PROFILE}"),
            profile_doc("published", masked),
        )
        .with(
            format!("/v1/clusterprofiles/{PROFILE}/packs/manifests"),
            json!({"spec": {"packs": []}}),
        )
        .with(
            format!("/v1/clusterprofiles/{PROFILE}/variables"),
            json!({"variables": []}),
        );
    let a = snapshot_of(&draft, PROFILE).unwrap();
    let b = snapshot_of(&published, PROFILE).unwrap();
    assert_eq!(a.packs[0].values_hash, b.packs[0].values_hash);
    assert_eq!(a.content_digest(), b.content_digest());
    // Palette masks a published profile: strata reads no schema for it
    assert_eq!(published.calls(&format!("/v1/packs/{PACK}")), 0);
}

#[test]
fn a_published_profile_with_a_draft_saves_no_clear_secret_of_the_draft() {
    let mut doc = profile_doc("published", "charts: {}\n");
    doc["spec"]["draft"] = profile_doc("draft", VALUES)["spec"]["draft"].clone();
    let api = draft_only()
        .with(format!("/v1/clusterprofiles/{PROFILE}"), doc)
        .with(format!("/v1/packs/{PACK}"), pack_doc());
    let raw = Palette::new(&api)
        .fetch_profile(PROFILE, &Progress::silent())
        .unwrap();
    assert!(!raw.profile.to_string().contains(SECRET));
    assert!(!raw.manifests.to_string().contains(SECRET));
}

#[test]
fn a_pack_that_palette_no_longer_has_is_not_masked_as_in_palette() {
    // with no pack, Palette has no schema and masks nothing; the memory client answers 404
    let snap = snapshot_of(&draft_only(), PROFILE).unwrap();
    assert!(snap.packs[0].values.contains(SECRET));
}

#[test]
fn a_failed_read_of_the_schema_stops_the_record() {
    let api = draft_only().failing(format!("/v1/packs/{PACK}"), 403, "forbidden");
    let err = snapshot_of(&api, PROFILE).unwrap_err();
    assert!(matches!(err, Error::Api { status: 403, .. }), "{err}");
}

#[test]
fn a_pack_uid_with_a_character_that_is_not_alphanumeric_is_masked() {
    let uid = "64f0-barney+e1";
    let mut profile = profile_doc("draft", VALUES);
    profile["spec"]["draft"]["packs"][0]["packUid"] = json!(uid);
    let mut pack = pack_doc();
    pack["packValues"][0]["packUid"] = json!(uid);
    let api = MemoryApi::new()
        .with(format!("/v1/clusterprofiles/{PROFILE}"), profile)
        .with(
            format!("/v1/clusterprofiles/{PROFILE}/packs/manifests"),
            json!({"spec": {"packs": [{"metadata": {"name": "grafana", "uid": uid},
                "spec": {"name": "grafana", "version": "1.0.0", "values": VALUES}}]}}),
        )
        .with(
            format!("/v1/clusterprofiles/{PROFILE}/variables"),
            json!({"variables": []}),
        )
        // strata escapes the uid in the path, as for each other uid
        .with("/v1/packs/64f0-barney%2Be1", pack);
    let raw = Palette::new(&api)
        .fetch_profile(PROFILE, &Progress::silent())
        .unwrap();
    assert!(!raw.profile.to_string().contains(SECRET));
    assert!(!raw.manifests.to_string().contains(SECRET));
    assert_eq!(api.calls("/v1/packs/64f0-barney%2Be1"), 1);
}

// ---- a draft with a secret and manifests ------------------------------------------------------

const RULES_PACK: &str = "64f0000000000000000000e2";
const DASH: &str = "64f00000000000000000a0d1";
const RULES: &str = "64f00000000000000000a0e1";
const PUBLISHED_VALUES: &str =
    "charts:\n  grafana:\n    adminPassword: '********'\n    replicas: 2\n";
const DRAFT_VALUES: &str =
    "charts:\n  grafana:\n    adminPassword: s3cret-barney\n    replicas: 3\n";
const MASKED_DRAFT_VALUES: &str =
    "charts:\n  grafana:\n    adminPassword: '********'\n    replicas: 3\n";
const PUBLISHED_DASH: &str =
    "apiVersion: v1\nkind: ConfigMap\nmetadata:\n  name: dash\ndata:\n  panels: \"1\"";
const DRAFT_DASH: &str =
    "apiVersion: v1\nkind: ConfigMap\nmetadata:\n  name: dash\ndata:\n  panels: \"2\"";
const PUBLISHED_RULES: &str =
    "apiVersion: v1\nkind: ConfigMap\nmetadata:\n  name: rules\ndata:\n  level: warn";
const DRAFT_RULES: &str =
    "apiVersion: v1\nkind: ConfigMap\nmetadata:\n  name: rules\ndata:\n  level: error";

/// A template with the pack `grafana`, which has `values` and the manifest `dash`, and the
/// manifest pack `team-rules`, which has the manifest `rules`.
fn template_with_manifests(values: &str) -> Value {
    json!({"cloudType": "all", "type": "add-on", "packs": [
        {"name": "grafana", "layer": "addon", "type": "spectro", "tag": "1.0.0",
         "version": "1.0.0", "packUid": PACK, "registryUid": "r1", "values": values,
         "manifests": [{"name": "dash", "uid": DASH, "kind": "manifest"}]},
        {"name": "team-rules", "layer": "addon", "type": "manifest", "tag": "1.0.0",
         "version": "1.0.0", "packUid": RULES_PACK, "values": "",
         "manifests": [{"name": "rules", "uid": RULES, "kind": "manifest"}]}]})
}

/// One manifest of the two templates, as Palette gives it by manifest uid.
fn manifest_entity(uid: &str, name: &str, published: &str, draft: &str) -> Value {
    json!({"metadata": {"name": name, "uid": uid},
           "spec": {"published": {"content": published}, "draft": {"content": draft}}})
}

/// Palette with a published version and a draft. The draft changes `replicas` and the content of
/// both manifests. Palette masks the secret of the published template, but not of the draft, and
/// `packs/manifests` serves the draft packs with the published content of each manifest.
fn draft_with_manifests() -> MemoryApi {
    let served = |name: &str, uid: &str, values: &str, manifest: Value| {
        json!({"metadata": {"name": name, "uid": uid},
               "spec": {"name": name, "version": "1.0.0", "values": values,
                        "manifests": [manifest]}})
    };
    MemoryApi::new()
        .with(
            format!("/v1/clusterprofiles/{PROFILE}"),
            json!({"metadata": {"uid": PROFILE, "name": "barney"},
                   "spec": {"version": "1.0.0",
                            "published": template_with_manifests(PUBLISHED_VALUES),
                            "draft": template_with_manifests(DRAFT_VALUES)},
                   "status": {"isPublished": true, "inUseClusterUids": []}}),
        )
        .with(
            format!("/v1/clusterprofiles/{PROFILE}/packs/manifests"),
            json!({"spec": {"packs": [
                served("grafana", PACK, DRAFT_VALUES,
                       json!({"name": "dash", "uid": DASH, "content": PUBLISHED_DASH})),
                served("team-rules", RULES_PACK, "",
                       json!({"name": "rules", "uid": RULES, "content": PUBLISHED_RULES}))]}}),
        )
        .with(
            format!("/v1/clusterprofiles/{PROFILE}/variables"),
            json!({"variables": []}),
        )
        .with(format!("/v1/packs/{PACK}"), pack_doc())
        .with(
            format!("/v1/clusterprofiles/{PROFILE}/packs/grafana/manifests/{DASH}"),
            manifest_entity(DASH, "dash", PUBLISHED_DASH, DRAFT_DASH),
        )
        .with(
            format!("/v1/clusterprofiles/{PROFILE}/packs/team-rules/manifests/{RULES}"),
            manifest_entity(RULES, "rules", PUBLISHED_RULES, DRAFT_RULES),
        )
}

/// The content of the manifest `name` in a record.
fn manifest_content<'a>(snap: &'a strata::snapshot::Snapshot, name: &str) -> &'a str {
    snap.packs
        .iter()
        .flat_map(|p| &p.manifests)
        .find(|m| m.name == name)
        .map(|m| m.content.as_str())
        .unwrap_or_default()
}

#[test]
fn a_draft_with_a_secret_and_manifests_records_no_clear_secret_and_the_content_of_each_template() {
    let api = draft_with_manifests();

    let raw = Palette::new(&api)
        .fetch_profile(PROFILE, &Progress::silent())
        .unwrap();
    assert!(!raw.profile.to_string().contains(SECRET));
    assert!(!raw.manifests.to_string().contains(SECRET));
    let entities = raw.manifest_entities.as_ref().unwrap();
    assert!(
        entities.get(DASH).is_some() && entities.get(RULES).is_some(),
        "{entities}"
    );
    let dir = tempfile::tempdir().unwrap();
    raw.save_dir(dir.path(), "t0").unwrap();
    for file in std::fs::read_dir(dir.path()).unwrap() {
        let path = file.unwrap().path();
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(!text.contains(SECRET), "{}: {text}", path.display());
    }

    let draft = snapshot_of(&api, &format!("{PROFILE}@draft")).unwrap();
    assert_eq!(draft.source.template, DRAFT_TEMPLATE);
    let record = draft.to_json(false).unwrap();
    assert!(!record.contains(SECRET), "{record}");
    let grafana = draft.packs.iter().find(|p| p.name == "grafana").unwrap();
    assert_eq!(grafana.values, MASKED_DRAFT_VALUES);
    assert_eq!(
        grafana.values_canon["charts"]["grafana"]["adminPassword"],
        "********"
    );
    assert_eq!(grafana.values_canon["charts"]["grafana"]["replicas"], 3);
    assert_eq!(manifest_content(&draft, "dash"), DRAFT_DASH);
    assert_eq!(manifest_content(&draft, "rules"), DRAFT_RULES);

    let version = snapshot_of(&api, PROFILE).unwrap();
    assert_eq!(version.source.template, PUBLISHED_TEMPLATE);
    let record = version.to_json(false).unwrap();
    assert!(!record.contains(SECRET), "{record}");
    assert_eq!(manifest_content(&version, "dash"), PUBLISHED_DASH);
    assert_eq!(manifest_content(&version, "rules"), PUBLISHED_RULES);
    assert_ne!(draft.root, version.root);

    // strata reads no schema for a manifest pack: it has no values to mask
    assert_eq!(api.calls(&format!("/v1/packs/{RULES_PACK}")), 0);
}
