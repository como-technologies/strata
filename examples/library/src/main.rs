//! Uses the strata library the way another Rust program would: no command line, no network.
//! Serves two versions of a profile from an in-memory Palette API, records both, compares them,
//! and prints the `strata-diff/1` summary. Then it publishes the second version again from
//! another registry, and reads the content digest, the pins, and the class of a failure. Exits
//! 1 if the library does not report what each step makes.

use serde_json::{Value, json};
use strata::api::{MemoryApi, Palette};
use strata::color::Paint;
use strata::diff::ImpactLevel;
use strata::error::FailureClass;
use strata::ops;
use strata::progress::Progress;
use strata::reference::Ref;
use strata::render::{self, Format};

/// One published profile version with a Kubernetes layer, in the shapes Palette returns.
fn serve(
    api: MemoryApi,
    uid: &str,
    version: &str,
    k8s_version: &str,
    digest: &str,
    registry: &str,
) -> MemoryApi {
    let pack = json!({
        "name": "kubernetes", "layer": "k8s", "type": "spectro",
        "tag": k8s_version, "version": k8s_version,
        "packUid": format!("pk-{registry}-{k8s_version}"),
        "registryUid": registry, "digest": digest, "values": "pack:\n  k8sHardening: true",
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
    let api = serve(
        MemoryApi::new(),
        "uid-1",
        "1.0.0",
        "1.35.6",
        "sha256:aaa",
        "reg-dev",
    );
    let api = serve(api, "uid-2", "1.1.0", "1.36.2", "sha256:bbb", "reg-dev");
    // The same version, with the pack copied by digest to another registry.
    let api = serve(
        api,
        "uid-3",
        "1.1.0",
        "1.36.2",
        "sha256:bbb",
        "reg-released",
    );
    let quiet = Progress::silent();
    let at = "2026-09-16T00:00:00Z";

    let snapshot = |uid: &str| {
        ops::snapshot_for(
            &Ref::Uid(uid.to_string()),
            &ops::SnapshotOptions::default(),
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

    // Two objects, one version: the roots differ, and the content digest does not.
    let c = snapshot("uid-3")?;
    let pins = c.pins();
    let one_version = b.root != c.root
        && b.content_digest() == c.content_digest()
        && a.content_digest() != b.content_digest()
        && pins.len() == 1
        && pins[0].digest == "sha256:bbb";
    // A version that is not there is a failure that stays until something changes.
    let missing = snapshot("uid-404").expect_err("no such profile").class();
    if !one_version || missing != FailureClass::Terminal {
        eprintln!("unexpected digests, pins, or failure class from the strata library");
        std::process::exit(1);
    }
    println!("{} {} {}", pins[0].name, pins[0].version, pins[0].digest);
    eprintln!("ok: a copy to another registry is another root and the same content digest");
    Ok(())
}
