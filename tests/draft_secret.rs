//! A profile with a draft only: Palette serves its packs with each secret in clear. A record of
//! it, and the answers that `--raw-dir` saves, hold `********` in place of each value whose
//! schema format is `password`, as Palette masks a published pack.

use serde_json::{Value, json};

use strata::api::{MemoryApi, Palette};
use strata::error::Error;
use strata::progress::Progress;
use strata::snapshot::Template;

use crate::common;
use common::{
    manifest_content, manifest_entity, manifest_route, no_manifests, no_template, no_variables,
    profile_document, profile_route, serve_profile, snapshot_of, template,
};

const PROFILE: &str = "64f0000000000000000000d1";
const PACK: &str = "64f0000000000000000000e1";
const SECRET: &str = "s3cret-barney";
const VALUES: &str = "charts:\n  grafana:\n    adminUser: fred # the login\n    \
                      adminPassword: s3cret-barney\n    replicas: 2\n";

/// The template with one pack, `grafana`, with `values`.
fn grafana_template(values: &str) -> Value {
    template(
        "all",
        "add-on",
        json!([
        {"name": "grafana", "layer": "addon", "type": "spectro", "tag": "1.0.0",
         "version": "1.0.0", "packUid": PACK, "registryUid": "r1", "values": values}]),
    )
}

/// The document of version 1.0.0 of `barney`, with `published` and `draft` as its templates.
fn profile_doc(published: Value, draft: Option<Value>) -> Value {
    profile_document(PROFILE, "barney", "1.0.0", published, draft)
}

/// The answer of `packs/manifests` for the pack `grafana` with the uid `uid`: its values in
/// clear, as Palette serves a draft.
fn grafana_manifests(uid: &str) -> Value {
    json!({"spec": {"packs": [{"metadata": {"name": "grafana", "uid": uid},
        "spec": {"name": "grafana", "version": "1.0.0", "values": VALUES}}]}})
}

/// The document of the pack `uid`, `GET /v1/packs/{uid}`.
fn pack_route(uid: &str) -> String {
    format!("/v1/packs/{uid}")
}

/// The answer of `GET /v1/packs/{uid}`: the schema marks `adminPassword` as a password.
fn pack_doc() -> Value {
    json!({"name": "grafana", "packValues": [{"packUid": PACK, "values": "", "schema": [
        {"name": "charts.grafana.adminPassword", "format": "${password}"},
        {"name": "charts.grafana.adminUser", "format": "${string}"}]}]})
}

/// Palette with a draft-only profile, as it answers: the packs and `packs/manifests` in clear.
fn draft_only() -> MemoryApi {
    serve_profile(
        MemoryApi::new(),
        PROFILE,
        profile_doc(no_template(), Some(grafana_template(VALUES))),
        grafana_manifests(PACK),
        no_variables(),
    )
}

#[test]
fn a_record_of_a_draft_holds_no_clear_secret() {
    let api = draft_only().with(pack_route(PACK), pack_doc());
    let snap = snapshot_of(&api, PROFILE).unwrap();
    assert_eq!(snap.source.template, Template::Draft);
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
    assert_eq!(api.calls(&pack_route(PACK)), 1);
}

#[test]
fn the_saved_answers_of_a_draft_hold_no_clear_secret() {
    let api = draft_only().with(pack_route(PACK), pack_doc());
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
    let draft = draft_only().with(pack_route(PACK), pack_doc());
    // Palette writes the published values again with its YAML library: other text, same YAML
    let masked = "charts:\n    grafana:\n        adminUser: fred\n        \
                  adminPassword: '********'\n        replicas: 2\n";
    let published = serve_profile(
        MemoryApi::new(),
        PROFILE,
        profile_doc(grafana_template(masked), None),
        no_manifests(),
        no_variables(),
    );
    let a = snapshot_of(&draft, PROFILE).unwrap();
    let b = snapshot_of(&published, PROFILE).unwrap();
    assert_eq!(a.packs[0].values_hash, b.packs[0].values_hash);
    assert_eq!(a.content_digest(), b.content_digest());
    // Palette masks a published profile: strata reads no schema for it
    assert_eq!(published.calls(&pack_route(PACK)), 0);
}

#[test]
fn a_published_profile_with_a_draft_saves_no_clear_secret_of_the_draft() {
    let doc = profile_doc(
        grafana_template("charts: {}\n"),
        Some(grafana_template(VALUES)),
    );
    let api = draft_only()
        .with(profile_route(PROFILE), doc)
        .with(pack_route(PACK), pack_doc());
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
    let api = draft_only().failing(pack_route(PACK), 403, "forbidden");
    let err = snapshot_of(&api, PROFILE).unwrap_err();
    assert!(matches!(err, Error::Api { status: 403, .. }), "{err}");
}

#[test]
fn a_pack_uid_with_a_character_that_is_not_alphanumeric_is_masked() {
    let uid = "64f0-barney+e1";
    let mut profile = profile_doc(no_template(), Some(grafana_template(VALUES)));
    profile["spec"]["draft"]["packs"][0]["packUid"] = json!(uid);
    let mut pack = pack_doc();
    pack["packValues"][0]["packUid"] = json!(uid);
    let api = serve_profile(
        MemoryApi::new(),
        PROFILE,
        profile,
        grafana_manifests(uid),
        no_variables(),
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
    template(
        "all",
        "add-on",
        json!([
        {"name": "grafana", "layer": "addon", "type": "spectro", "tag": "1.0.0",
         "version": "1.0.0", "packUid": PACK, "registryUid": "r1", "values": values,
         "manifests": [{"name": "dash", "uid": DASH, "kind": "manifest"}]},
        {"name": "team-rules", "layer": "addon", "type": "manifest", "tag": "1.0.0",
         "version": "1.0.0", "packUid": RULES_PACK, "values": "",
         "manifests": [{"name": "rules", "uid": RULES, "kind": "manifest"}]}]),
    )
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
    let document = profile_doc(
        template_with_manifests(PUBLISHED_VALUES),
        Some(template_with_manifests(DRAFT_VALUES)),
    );
    let manifests = json!({"spec": {"packs": [
        served("grafana", PACK, DRAFT_VALUES,
               json!({"name": "dash", "uid": DASH, "content": PUBLISHED_DASH})),
        served("team-rules", RULES_PACK, "",
               json!({"name": "rules", "uid": RULES, "content": PUBLISHED_RULES}))]}});
    serve_profile(
        MemoryApi::new(),
        PROFILE,
        document,
        manifests,
        no_variables(),
    )
    .with(pack_route(PACK), pack_doc())
    .with(
        manifest_route(PROFILE, "grafana", DASH),
        manifest_entity(DASH, "dash", PUBLISHED_DASH, DRAFT_DASH),
    )
    .with(
        manifest_route(PROFILE, "team-rules", RULES),
        manifest_entity(RULES, "rules", PUBLISHED_RULES, DRAFT_RULES),
    )
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
    assert_eq!(draft.source.template, Template::Draft);
    let record = draft.to_json(false).unwrap();
    assert!(!record.contains(SECRET), "{record}");
    let grafana = draft.packs.iter().find(|p| p.name == "grafana").unwrap();
    assert_eq!(grafana.values, MASKED_DRAFT_VALUES);
    assert_eq!(
        grafana.values_canon["charts"]["grafana"]["adminPassword"],
        "********"
    );
    assert_eq!(grafana.values_canon["charts"]["grafana"]["replicas"], 3);
    assert_eq!(manifest_content(&draft, "dash"), Some(DRAFT_DASH));
    assert_eq!(manifest_content(&draft, "rules"), Some(DRAFT_RULES));

    let version = snapshot_of(&api, PROFILE).unwrap();
    assert_eq!(version.source.template, Template::Published);
    let record = version.to_json(false).unwrap();
    assert!(!record.contains(SECRET), "{record}");
    assert_eq!(manifest_content(&version, "dash"), Some(PUBLISHED_DASH));
    assert_eq!(manifest_content(&version, "rules"), Some(PUBLISHED_RULES));
    assert_ne!(draft.root, version.root);

    // strata reads no schema for a manifest pack: it has no values to mask
    assert_eq!(api.calls(&pack_route(RULES_PACK)), 0);
}
