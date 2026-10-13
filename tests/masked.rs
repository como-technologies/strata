//! A published pack value whose schema format is `password`: Palette serves `********` in place
//! of the secret. A record names each masked value, and `same-release`, `compare`, and `verify`
//! say that an equal root does not show an equal secret.

use serde_json::{Value, json};

use strata::api::{MemoryApi, Palette};
use strata::diff::{self, MaskedPack, VerifyStatus};
use strata::ops::{self, PairStatus, Policy};
use strata::render::{self, Format};
use strata::snapshot::Snapshot;
use strata::term::color::Paint;
use strata::term::progress::Progress;

use crate::common;
use common::{no_manifests, no_variables, profile_document, serve_profile, snapshot_of, template};

const PROFILE: &str = "64f0000000000000000000d2";
const GRAFANA: &str = "64f0000000000000000000e2";
const CERTS: &str = "64f0000000000000000000e3";
const PATH: &str = "charts.grafana.adminPassword";
const NOTE: &str = "pack addon/grafana has a masked value at charts.grafana.adminPassword: an \
                    equal root does not show an equal secret";

/// Palette with one published version: `grafana`, whose values hold a secret that Palette masks,
/// and `cert-manager`, whose values hold none.
fn published() -> MemoryApi {
    let grafana = "charts:\n  grafana:\n    adminUser: fred\n    adminPassword: '********'\n";
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

#[test]
fn a_record_names_each_masked_value_of_each_pack() {
    let snap = record();
    assert_eq!(pack(&snap, "grafana").masked, [PATH]);
    assert!(pack(&snap, "cert-manager").masked.is_empty());
    let doc: Value = serde_json::from_str(&snap.to_json(false).unwrap()).unwrap();
    let packs = doc["packs"].as_array().unwrap();
    let grafana = packs.iter().find(|p| p["name"] == "grafana").unwrap();
    let certs = packs.iter().find(|p| p["name"] == "cert-manager").unwrap();
    assert_eq!(grafana["masked"], json!([PATH]));
    assert!(certs.get("masked").is_none(), "{certs}");
}

#[test]
fn the_root_does_not_cover_the_masked_paths() {
    let snap = record();
    let mut doc: Value = serde_json::from_str(&snap.to_json(false).unwrap()).unwrap();
    for p in doc["packs"].as_array_mut().unwrap() {
        p.as_object_mut().unwrap().remove("masked");
    }
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("older.json");
    std::fs::write(&path, doc.to_string()).unwrap();
    // a record written before the field gets it from its values when it is read
    let older = Snapshot::load(&path).unwrap();
    assert_eq!(pack(&older, "grafana").masked, [PATH]);
    assert_eq!(older.root, snap.root);
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
    let text = render::renderer(Format::Terminal, Paint::off(), false)
        .render_chain(&chain)
        .unwrap();
    assert!(
        text.contains(&format!("\n               {NOTE}\n")),
        "{text}"
    );
    assert!(text.contains("SAME RELEASE: 1 pair passed"), "{text}");
    let md = render::renderer(Format::Markdown, Paint::off(), false)
        .render_chain(&chain)
        .unwrap();
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
    let text = render::renderer(Format::Terminal, Paint::off(), false)
        .render(&d, &a, &b, false)
        .unwrap();
    assert!(text.contains(&format!("note       {NOTE}\n")), "{text}");
    let md = render::renderer(Format::Markdown, Paint::off(), false)
        .render(&d, &a, &b, false)
        .unwrap();
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
    let text = render::renderer(Format::Terminal, Paint::off(), false)
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
