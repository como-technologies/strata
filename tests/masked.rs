//! A published pack value whose schema format is `password`: Palette serves `********` in place
//! of the secret. A pack gives each masked value from its values, and `same-release`, `compare`,
//! and `verify` say that an equal root does not show an equal secret.

use serde_json::{Value, json};

use strata::api::{MemoryApi, Palette};
use strata::diff::{self, MaskedPack, VerifyStatus};
use strata::ops::{self, PairStatus, Policy};
use strata::snapshot::Snapshot;
use strata::term::progress::Progress;

use crate::common;
use common::{
    no_manifests, no_variables, profile_document, serve_profile, snapshot_of, template,
    write_record,
};

const PROFILE: &str = "64f0000000000000000000d2";
const GRAFANA: &str = "64f0000000000000000000e2";
const CERTS: &str = "64f0000000000000000000e3";
const PATH: &str = "charts.grafana.adminPassword";
const NOTE: &str = "pack addon/grafana has a masked value at charts.grafana.adminPassword: an \
                    equal root does not show an equal secret";

/// Palette with one published version: `grafana`, whose values hold a secret that Palette masks,
/// and `cert-manager`, whose values hold none.
fn published() -> MemoryApi {
    published_for("fred")
}

/// [`published`], with `user` as the admin user of grafana.
fn published_for(user: &str) -> MemoryApi {
    let grafana =
        format!("charts:\n  grafana:\n    adminUser: {user}\n    adminPassword: '********'\n");
    let certs = "charts:\n  cert-manager:\n    replicas: 2\n";
    let packs = json!([
        {"name": "grafana", "layer": "addon", "type": "spectro", "tag": "1.0.0",
         "version": "1.0.0", "packUid": GRAFANA, "registryUid": "r1",
         "digest": "sha256:aa", "values": grafana},
        {"name": "cert-manager", "layer": "addon", "type": "spectro",
         "tag": "1.0.0", "version": "1.0.0", "packUid": CERTS,
         "registryUid": "r1", "digest": "sha256:bb", "values": certs}]);
    let document = profile_document(
        PROFILE,
        "barney",
        "1.0.0",
        template("all", "add-on", packs),
        None,
    );
    serve_profile(
        MemoryApi::new(),
        PROFILE,
        document,
        no_manifests(),
        no_variables(),
    )
}

fn record() -> Snapshot {
    snapshot_of(&published(), PROFILE).unwrap()
}

fn pack<'a>(snap: &'a Snapshot, name: &str) -> &'a strata::snapshot::Pack {
    snap.packs.iter().find(|p| p.name == name).unwrap()
}

/// Write `doc`, a record, to a file and read it with `Snapshot::load`.
fn load(doc: &Value) -> Snapshot {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("record.json");
    write_record(&path, doc);
    Snapshot::load(&path).unwrap()
}

#[test]
fn a_pack_gives_the_masked_paths_of_its_values_and_a_new_record_holds_no_masked_field() {
    let snap = record();
    assert_eq!(pack(&snap, "grafana").masked_paths(), [PATH]);
    assert!(pack(&snap, "cert-manager").masked_paths().is_empty());
    let doc: Value = serde_json::from_str(&snap.to_json(false).unwrap()).unwrap();
    for p in doc["packs"].as_array().unwrap() {
        assert!(p.get("masked").is_none(), "{p}");
    }
    let grafana = doc["packs"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["name"] == "grafana")
        .unwrap();
    assert!(grafana["values"].as_str().unwrap().contains("********"));
}

#[test]
fn a_record_of_release_0_25_0_with_masked_reads_and_gives_the_paths_of_its_values() {
    let snap = record();
    let mut doc: Value = serde_json::from_str(&snap.to_json(false).unwrap()).unwrap();
    for p in doc["packs"].as_array_mut().unwrap() {
        // release 0.25.0 wrote the list only for a pack with a mask
        if p["name"] == "grafana" {
            p["masked"] = json!([PATH]);
        }
    }
    let older = load(&doc);
    assert_eq!(pack(&older, "grafana").masked_paths(), [PATH]);
    assert!(pack(&older, "cert-manager").masked_paths().is_empty());
    assert_eq!(older.root, snap.root);
    let again: Value = serde_json::from_str(&older.to_json(false).unwrap()).unwrap();
    for p in again["packs"].as_array().unwrap() {
        assert!(p.get("masked").is_none(), "{p}");
    }
}

#[test]
fn the_values_decide_the_masked_paths_and_not_the_masked_field_of_an_older_record() {
    let snap = record();
    let mut doc: Value = serde_json::from_str(&snap.to_json(false).unwrap()).unwrap();
    for p in doc["packs"].as_array_mut().unwrap() {
        p["masked"] = json!(["not.in.the.values"]);
    }
    let older = load(&doc);
    assert_eq!(pack(&older, "grafana").masked_paths(), [PATH]);
    assert!(pack(&older, "cert-manager").masked_paths().is_empty());
}

#[test]
fn compare_of_a_record_of_release_0_25_0_and_a_new_record_gives_the_same_masked_paths() {
    let new = record();
    let mut doc: Value = serde_json::from_str(&new.to_json(false).unwrap()).unwrap();
    for p in doc["packs"].as_array_mut().unwrap() {
        if p["name"] == "grafana" {
            p["masked"] = json!([PATH]);
        }
    }
    let older = load(&doc);
    let d = diff::compute(&older, &new, "t1", None);
    assert!(d.same);
    let grafana = d.packs.iter().find(|p| p.name == "grafana").unwrap();
    assert_eq!(grafana.masked, [PATH]);
}

#[test]
fn same_release_of_two_records_with_a_masked_value_names_the_pack_and_passes() {
    let (a, b) = (record(), record());
    let chain = ops::same_release(
        &[("a.json".to_string(), a), ("b.json".to_string(), b)],
        Policy::Exact,
        "t1",
    );
    assert!(chain.same);
    let pair = &chain.pairs[0];
    assert_eq!(pair.status, PairStatus::Identical);
    assert!(pair.reasons.is_empty(), "{:?}", pair.reasons);
    assert_eq!(
        pair.masked,
        [MaskedPack {
            key: "addon/grafana".to_string(),
            paths: vec![PATH.to_string()],
        }]
    );
    let text = common::terminal().render_chain(&chain).unwrap();
    assert!(
        text.contains(&format!("\n               {NOTE}\n")),
        "{text}"
    );
    assert!(text.contains("SAME RELEASE: 1 pair passed"), "{text}");
    let md = common::markdown().render_chain(&chain).unwrap();
    assert!(md.contains(&format!("<br>{NOTE}")), "{md}");
    let json: Value = serde_json::to_value(&chain).unwrap();
    assert_eq!(
        json["pairs"][0]["masked"],
        json!([{"key": "addon/grafana", "paths": [PATH]}])
    );
}

#[test]
fn compare_of_two_records_with_a_masked_value_prints_the_note() {
    let (a, b) = (record(), record());
    let d = diff::compute(&a, &b, "t1", None);
    assert!(d.same);
    let text = common::terminal().render(&d, &a, &b, false).unwrap();
    assert!(text.contains(&format!("note       {NOTE}\n")), "{text}");
    let md = common::markdown().render(&d, &a, &b, false).unwrap();
    assert!(md.contains(&format!("**Note:** {NOTE}")), "{md}");
    let grafana = d.packs.iter().find(|p| p.name == "grafana").unwrap();
    assert_eq!(grafana.masked, [PATH]);
    let certs = d.packs.iter().find(|p| p.name == "cert-manager").unwrap();
    assert!(certs.masked.is_empty());
}

#[test]
fn verify_names_the_pack_with_a_masked_value_beside_its_line() {
    let recorded = record();
    let palette = Palette::new(published());
    let v = ops::verify(&recorded, &palette, "t1", &Progress::silent()).unwrap();
    assert!(v.unchanged);
    let ver = v.diff.verify.as_ref().unwrap();
    assert_eq!(ver.status, VerifyStatus::Unchanged);
    let text = common::terminal()
        .render(&v.diff, &recorded, &v.live, false)
        .unwrap();
    let lines: Vec<&str> = text.lines().collect();
    let at = lines
        .iter()
        .position(|l| l.contains("grafana") && l.contains("unchanged"))
        .unwrap();
    assert_eq!(
        lines[at + 1],
        "      masked: charts.grafana.adminPassword: Palette serves ******** in place of the \
         secret",
        "{text}"
    );
    assert!(text.contains(&format!("note       {NOTE}\n")), "{text}");
    assert!(!text.contains("masked: charts.cert-manager"), "{text}");
}

#[test]
fn the_unified_text_of_a_changed_value_keeps_the_placeholder_of_a_masked_value() {
    let a = record();
    let b = snapshot_of(&published_for("wilma"), PROFILE).unwrap();
    let d = diff::compute(&a, &b, "t1", None);
    let json = strata::render::renderer(
        strata::render::Format::Json,
        strata::term::color::Paint::off(),
        true,
    )
    .render(&d, &a, &b, true)
    .unwrap();
    let doc: Value = serde_json::from_str(&json).unwrap();
    let grafana = doc["packs"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["name"] == "grafana")
        .unwrap();
    let text = grafana["values"]["unified"].as_str().unwrap();
    assert!(
        text.contains("-    adminUser: fred\n+    adminUser: wilma\n"),
        "{text}"
    );
    assert!(
        text.contains("     adminPassword: '********'\n"),
        "the mask stays: {text}"
    );
}

#[test]
fn verify_of_many_records_with_full_gives_a_drifted_row_the_unified_text_and_without_it_none() {
    let recorded = record();
    let live = published_for("wilma");
    let grafana = |s: &ops::VerifySummary| {
        let diff = s.records[0].diff.as_ref().unwrap();
        diff.packs
            .iter()
            .find(|p| p.name == "grafana")
            .unwrap()
            .clone()
    };
    let plain = common::verify_summary_with(&recorded, &live, &ops::VerifyOptions::default());
    assert!(grafana(&plain).values.unified.is_none());

    let full = common::verify_summary_with(
        &recorded,
        &live,
        &ops::VerifyOptions {
            full: true,
            ..Default::default()
        },
    );
    let text = grafana(&full).values.unified.unwrap();
    assert!(
        text.contains("-    adminUser: fred\n+    adminUser: wilma\n"),
        "{text}"
    );
    assert!(text.contains("     adminPassword: '********'\n"), "{text}");
}
