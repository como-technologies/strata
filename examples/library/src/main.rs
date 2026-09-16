//! Uses the strata library the way another Rust program would: no command line, no network.
//! Serves two versions of a profile from an in-memory Palette API, records both, compares them,
//! and prints the `strata-diff/1` summary. Exits 1 if the library does not report the one
//! change the second version makes.

use serde_json::{json, Value};
use strata::api::{MemoryApi, Palette};
use strata::color::Paint;
use strata::diff::ImpactLevel;
use strata::ops;
use strata::progress::Progress;
use strata::reference::Ref;
use strata::render::{self, Format};

/// One published profile version with a Kubernetes layer, in the shapes Palette returns.
fn serve(api: MemoryApi, uid: &str, version: &str, k8s_version: &str, digest: &str) -> MemoryApi {
    let pack = json!({
        "name": "kubernetes", "layer": "k8s", "type": "spectro",
        "tag": k8s_version, "version": k8s_version, "packUid": format!("pk-{k8s_version}"),
        "registryUid": "reg", "digest": digest, "values": "pack:\n  k8sHardening: true",
    });
    api.with(
        format!("/v1/clusterprofiles/{uid}"),
        json!({
            "metadata": {"uid": uid, "name": "example"},
            "spec": {"version": version, "published": {"cloudType": "aws", "type": "infra", "packs": [pack]}},
        }),
    )
    .with(
        format!("/v1/clusterprofiles/{uid}/packs/manifests"),
        json!({"spec": {"packs": []}}),
    )
    .with(
        format!("/v1/clusterprofiles/{uid}/variables"),
        json!({"variables": []}),
    )
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let api = serve(MemoryApi::new(), "uid-1", "1.0.0", "1.35.6", "sha256:aaa");
    let api = serve(api, "uid-2", "1.1.0", "1.36.2", "sha256:bbb");
    let quiet = Progress::silent();
    let at = "2026-09-16T00:00:00Z";

    let snapshot = |uid: &str| {
        ops::snapshot_for(
            &Ref::Uid(uid.to_string()),
            || Ok(Palette::new(api.clone())),
            at,
            &quiet,
        )
    };
    let (a, b) = (snapshot("uid-1")?, snapshot("uid-2")?);
    let diff = ops::compare(&a, &b, at);

    let text = render::renderer(Format::Json, Paint::off(), true).render(&diff, &a, &b, false)?;
    let doc: Value = serde_json::from_str(&text)?;
    println!("{}", serde_json::to_string_pretty(&doc["summary"])?);

    let expected = !diff.same
        && diff.summary.packs_changed == 1
        && diff.summary.max_impact == ImpactLevel::Repave
        && doc["schema"] == "strata-diff/1";
    if !expected {
        eprintln!("unexpected result from the strata library");
        std::process::exit(1);
    }
    eprintln!("ok: a Kubernetes version change is a repave");
    Ok(())
}
