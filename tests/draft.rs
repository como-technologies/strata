//! `<ref>@draft`: the draft template of a profile version, with an in-memory Palette API that
//! answers as Palette does for a published version with a draft.

use serde_json::{Value, json};
use strata::api::{MemoryApi, Palette};
use strata::candidate::Candidate;
use strata::color::Paint;
use strata::diff::Status;
use strata::diff::VerifyStatus;
use strata::error::Error;
use strata::ops;
use strata::progress::Progress;
use strata::render::{self, Format};
use strata::snapshot::{DRAFT_TEMPLATE, PUBLISHED_TEMPLATE, Snapshot};

use crate::common;
use common::{
    PROFILE_LISTING, manifest_content, manifest_entity, manifest_route, no_template, no_variables,
    profile_document, serve_profile, snapshot_of,
};

const UID: &str = "64f0000000000000000000d1";
const RULES: &str = "64f00000000000000000a0e1";
const ADDED: &str = "64f00000000000000000a0e2";
const PUBLISHED_K8S: &str =
    "kubeadmconfig:\n  apiServer:\n    extraArgs:\n      audit-log-maxage: \"30\"";
const DRAFT_K8S: &str =
    "kubeadmconfig:\n  apiServer:\n    extraArgs:\n      audit-log-maxage: \"90\"";
const PUBLISHED_RULES: &str =
    "apiVersion: v1\nkind: ConfigMap\nmetadata:\n  name: rules\ndata:\n  level: warn";
const DRAFT_RULES: &str =
    "apiVersion: v1\nkind: ConfigMap\nmetadata:\n  name: rules\ndata:\n  level: error";
const ADDED_QUOTA: &str = "apiVersion: v1\nkind: ResourceQuota\nmetadata:\n  name: quota";

/// A pack ref of a template: the Kubernetes pack with `k8s` as its values, and the manifest pack
/// `team-rules` with the manifests `refs`.
fn template(k8s: &str, refs: Value) -> Value {
    common::template(
        "aws",
        "cluster",
        json!([
            {"name": "kubernetes", "layer": "k8s", "type": "spectro", "tag": "1.30.6",
             "version": "1.30.6", "packUid": "pk-k8s-1306", "registryUid": "reg-public",
             "digest": "sha256:k8s1306", "values": k8s, "manifests": []},
            {"name": "team-rules", "layer": "addon", "type": "manifest", "tag": "1.0.0",
             "version": "1.0.0", "packUid": "pk-rules", "values": "", "manifests": refs},
        ]),
    )
}

/// The digest that Palette gives a manifest content: empty for no content.
fn digest(content: &str) -> String {
    if content.is_empty() {
        String::new()
    } else {
        format!("sha1:{}", content.len())
    }
}

/// The answers of Palette for version 1.0.0 of `lab-profile`. The draft changes one value of the
/// Kubernetes pack, edits the manifest `rules`, and adds the manifest `quota`. `packs/manifests`
/// lists the packs of the draft, with the published content of each manifest, as Palette does.
fn palette_with_draft() -> MemoryApi {
    let published = template(
        PUBLISHED_K8S,
        json!([{"name": "rules", "uid": RULES, "kind": "manifest"}]),
    );
    let draft = template(
        DRAFT_K8S,
        json!([{"name": "rules", "uid": RULES, "kind": "manifest"},
               {"name": "quota", "uid": ADDED, "kind": "manifest"}]),
    );
    serve(published, draft, |name| match name {
        "rules" => PUBLISHED_RULES,
        _ => "",
    })
    .with(
        manifest_route(UID, "team-rules", RULES),
        manifest_entity(RULES, "rules", PUBLISHED_RULES, DRAFT_RULES),
    )
    .with(
        manifest_route(UID, "team-rules", ADDED),
        manifest_entity(ADDED, "quota", "", ADDED_QUOTA),
    )
}

/// The three documents of the profile, with `published` and `draft` as its templates, and the
/// answer of `packs/manifests` for the packs of the draft, or of the published template when the
/// draft is empty. `content` gives the published content of a manifest by name.
fn serve(published: Value, draft: Value, content: impl Fn(&str) -> &'static str) -> MemoryApi {
    let listed = if draft["packs"].as_array().is_some_and(|p| !p.is_empty()) {
        &draft
    } else {
        &published
    };
    let packs: Vec<Value> = listed["packs"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| {
            let manifests: Vec<Value> = p["manifests"]
                .as_array()
                .unwrap()
                .iter()
                .map(|m| {
                    let name = m["name"].as_str().unwrap();
                    json!({"name": name, "uid": m["uid"], "content": content(name),
                           "digest": digest(content(name))})
                })
                .collect();
            json!({"metadata": {"name": p["name"], "uid": p["packUid"]},
                   "spec": {"name": p["name"], "layer": p["layer"], "version": p["version"],
                            "values": p["values"], "digest": p["digest"],
                            "manifests": manifests}})
        })
        .collect();
    let document = profile_document(UID, "lab-profile", "1.0.0", published, Some(draft));
    let manifests =
        json!({"metadata": {"name": "lab-profile", "uid": UID}, "spec": {"packs": packs}});
    serve_profile(MemoryApi::new(), UID, document, manifests, no_variables()).with(
        PROFILE_LISTING,
        json!({"items": [
            {"metadata": {"name": "lab-profile", "uid": UID}, "spec": {"version": "1.0.0"}}
        ]}),
    )
}

fn values<'a>(snap: &'a Snapshot, pack: &str) -> &'a str {
    &snap.packs.iter().find(|p| p.name == pack).unwrap().values
}

#[test]
fn a_draft_records_apart_from_its_version_and_compare_names_the_layer_that_the_publish_changes() {
    let api = palette_with_draft();
    let version = snapshot_of(&api, UID).unwrap();
    let draft = snapshot_of(&api, &format!("{UID}@draft")).unwrap();

    assert_eq!(version.source.template, PUBLISHED_TEMPLATE);
    assert_eq!(draft.source.template, DRAFT_TEMPLATE);
    assert_eq!(draft.source.reference, format!("{UID}@draft"));
    assert_eq!(draft.source.uid, UID);
    assert_ne!(version.root, draft.root, "the draft is another content");

    // each record holds the content of its own template
    assert_eq!(values(&version, "kubernetes"), PUBLISHED_K8S);
    assert_eq!(values(&draft, "kubernetes"), DRAFT_K8S);
    assert_eq!(manifest_content(&version, "rules"), Some(PUBLISHED_RULES));
    assert_eq!(manifest_content(&draft, "rules"), Some(DRAFT_RULES));
    assert_eq!(
        manifest_content(&version, "quota"),
        None,
        "the draft adds it"
    );
    assert_eq!(manifest_content(&draft, "quota"), Some(ADDED_QUOTA));

    let d = ops::compare(&version, &draft, "t1");
    assert!(!d.same);
    let changed: Vec<&str> = d
        .packs
        .iter()
        .filter(|p| p.status == Status::Changed)
        .map(|p| p.key.as_str())
        .collect();
    assert_eq!(changed, ["k8s/kubernetes", "addon/team-rules"]);
}

#[test]
fn a_draft_by_name_and_version_reads_the_same_draft() {
    let api = palette_with_draft();
    let by_uid = snapshot_of(&api, &format!("{UID}@draft")).unwrap();
    let by_name = snapshot_of(&api, "lab-profile@1.0.0@draft").unwrap();
    assert_eq!(by_name.root, by_uid.root);
    assert_eq!(by_name.source.reference, "lab-profile@1.0.0@draft");
    assert_eq!(by_name.source.template, DRAFT_TEMPLATE);
}

#[test]
fn a_version_with_no_draft_is_the_error_no_draft_and_reads_no_manifest_by_uid() {
    let published = template(
        PUBLISHED_K8S,
        json!([{"name": "rules", "uid": RULES, "kind": "manifest"}]),
    );
    let api = serve(published, no_template(), |_| PUBLISHED_RULES);
    let err = snapshot_of(&api, &format!("{UID}@draft")).unwrap_err();
    assert!(
        matches!(&err, Error::NoDraft { uid, .. } if uid == UID),
        "{err}"
    );
    assert_eq!(err.kind(), "no-draft");
    assert!(err.to_string().contains("has no draft"), "{err}");

    // the record of the version needs no more requests than before
    let version = snapshot_of(&api, UID).unwrap();
    assert_eq!(manifest_content(&version, "rules"), Some(PUBLISHED_RULES));
    let path = manifest_route(UID, "team-rules", RULES);
    assert_eq!(api.calls(&path), 0);
}

#[test]
fn a_version_that_was_never_published_records_the_manifest_content_of_its_draft() {
    // packs/manifests gives the published content of each manifest, and a manifest of a version
    // that was never published has none
    let draft = template(
        DRAFT_K8S,
        json!([{"name": "rules", "uid": RULES, "kind": "manifest"}]),
    );
    let api = serve(no_template(), draft, |_| "").with(
        manifest_route(UID, "team-rules", RULES),
        manifest_entity(RULES, "rules", "", DRAFT_RULES),
    );
    let snap = snapshot_of(&api, UID).unwrap();
    assert_eq!(snap.source.template, DRAFT_TEMPLATE);
    assert_eq!(manifest_content(&snap, "rules"), Some(DRAFT_RULES));
    let draft = snapshot_of(&api, &format!("{UID}@draft")).unwrap();
    assert_eq!(draft.root, snap.root, "the draft is the only template");
}

#[test]
fn the_candidate_of_a_draft_holds_the_content_of_the_draft() {
    let api = palette_with_draft();
    let draft = snapshot_of(&api, &format!("{UID}@draft")).unwrap();
    let c = Candidate::from_snapshot(&draft).unwrap();
    let text = strata::candidate::to_yaml(&c).unwrap();
    assert!(text.contains("audit-log-maxage: \"90\""), "{text}");
    assert!(text.contains("level: error"), "{text}");
    assert!(text.contains("kind: ResourceQuota"), "{text}");
    assert!(!text.contains("level: warn"), "{text}");
}

#[test]
fn a_record_of_the_draft_verifies_against_the_version_once_the_draft_is_published() {
    let api = palette_with_draft();
    let version = snapshot_of(&api, UID).unwrap();
    let draft = snapshot_of(&api, &format!("{UID}@draft")).unwrap();
    let r = render::renderer(Format::Terminal, Paint::off(), false);

    // a version and its draft: not an edit in place
    let d = ops::compare(&version, &draft, "t1");
    assert!(!d.edited_in_place);
    let text = r.render(&d, &version, &draft, false).unwrap();
    assert!(
        text.contains(&format!(
            "{UID}@draft is the draft of lab-profile@1.0.0 (uid {UID}): its publish makes this change"
        )),
        "{text}"
    );
    assert!(!text.contains("edited in place"), "{text}");

    // while the draft waits, the version holds its content, and the draft differs from it
    let palette = Palette::new(&api);
    let v = ops::verify(&version, &palette, "t1", &Progress::silent()).unwrap();
    assert!(v.unchanged);
    let v = ops::verify(&draft, &palette, "t1", &Progress::silent()).unwrap();
    let ver = v.diff.verify.as_ref().unwrap();
    assert_eq!(ver.status, VerifyStatus::Drifted);
    let verdict = r.verdict(ver, &draft.source).unwrap();
    assert!(
        verdict.contains("published version lab-profile@1.0.0 differs from the draft recorded t0"),
        "{verdict}"
    );

    // the publish makes the draft the published template, and empties the draft
    let published = template(
        DRAFT_K8S,
        json!([{"name": "rules", "uid": RULES, "kind": "manifest"},
               {"name": "quota", "uid": ADDED, "kind": "manifest"}]),
    );
    let after = serve(published, no_template(), |name| match name {
        "rules" => DRAFT_RULES,
        "quota" => ADDED_QUOTA,
        _ => "",
    });
    let palette = Palette::new(&after);
    let v = ops::verify(&draft, &palette, "t2", &Progress::silent()).unwrap();
    assert!(v.unchanged, "{:?}", v.diff.packs);
    let verdict = r
        .verdict(v.diff.verify.as_ref().unwrap(), &draft.source)
        .unwrap();
    assert!(
        verdict.contains("published version lab-profile@1.0.0 holds the draft recorded t0"),
        "{verdict}"
    );
    let v = ops::verify(&version, &palette, "t2", &Progress::silent()).unwrap();
    assert!(
        !v.unchanged,
        "the record of the version from before the publish drifts"
    );
    assert!(matches!(
        snapshot_of(&after, &format!("{UID}@draft")),
        Err(Error::NoDraft { .. })
    ));
}

#[test]
fn a_record_of_the_version_takes_nothing_from_a_pack_that_only_the_draft_has() {
    // the published Kubernetes pack has no digest on its ref; the draft moves it to 1.30.7
    let mut published = template(PUBLISHED_K8S, json!([]));
    published["packs"][0]
        .as_object_mut()
        .unwrap()
        .remove("digest");
    let mut draft = template(PUBLISHED_K8S, json!([]));
    draft["packs"][0]["packUid"] = json!("pk-k8s-1307");
    draft["packs"][0]["version"] = json!("1.30.7");
    draft["packs"][0]["tag"] = json!("1.30.7");
    draft["packs"][0]["digest"] = json!("sha256:k8s1307");
    let api = serve(published, draft, |_| "");
    let version = snapshot_of(&api, UID).unwrap();
    let k8s = version
        .packs
        .iter()
        .find(|p| p.name == "kubernetes")
        .unwrap();
    assert_eq!(k8s.version, "1.30.6");
    assert_eq!(
        k8s.hash.upstream_source, "none",
        "packs/manifests lists the 1.30.7 of the draft, and its digest is not the digest of 1.30.6"
    );
    let draft = snapshot_of(&api, &format!("{UID}@draft")).unwrap();
    let k8s = draft.packs.iter().find(|p| p.name == "kubernetes").unwrap();
    assert_eq!(k8s.version, "1.30.7");
    assert_eq!(k8s.hash.upstream_source, "packRef.digest");
}
