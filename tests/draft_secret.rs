//! A profile with a draft only: Palette serves its packs with each secret in clear. A record of
//! it, and the answers that `--raw-dir` saves, hold `********` in place of each value whose
//! schema format is `password`, as Palette masks a published pack.

use serde_json::{Value, json};

use strata::api::{MemoryApi, Palette};
use strata::error::Error;
use strata::ops;
use strata::progress::Progress;
use strata::reference::Ref;

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

fn snapshot(api: &MemoryApi) -> strata::error::Result<strata::snapshot::Snapshot> {
    ops::snapshot_for(
        &Ref::Uid(PROFILE.to_string()),
        || Ok(Palette::new(api)),
        "t0",
        &Progress::silent(),
    )
}

#[test]
fn a_record_of_a_draft_holds_no_clear_secret() {
    let api = draft_only().with(format!("/v1/packs/{PACK}"), pack_doc());
    let snap = snapshot(&api).unwrap();
    assert_eq!(snap.source.template, "draft");
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
    let a = snapshot(&draft).unwrap();
    let b = snapshot(&published).unwrap();
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
    let snap = snapshot(&draft_only()).unwrap();
    assert!(snap.packs[0].values.contains(SECRET));
}

#[test]
fn a_failed_read_of_the_schema_stops_the_record() {
    let api = draft_only().failing(format!("/v1/packs/{PACK}"), 403, "forbidden");
    let err = snapshot(&api).unwrap_err();
    assert!(matches!(err, Error::Api { status: 403, .. }), "{err}");
}
