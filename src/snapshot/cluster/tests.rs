//! The tests of the cluster record: the older read errors, the applied profiles, and the tiers.

use serde_json::json;

use super::*;

#[test]
fn a_document_applies_the_profile_of_a_uid_with_the_version_that_its_template_names() {
    let doc = json!({"spec": {"clusterProfileTemplates": [
        {"uid": "p1", "name": "lab", "profileVersion": "1.2.0", "type": "cluster",
         "version": 3},
        {"uid": "p2", "name": "addons", "profileVersion": "2.0.0", "type": "add-on",
         "version": 1},
    ]}});
    let applied = AppliedProfile::applied(&doc, "p2").expect("the document applies p2");
    assert_eq!(
        (
            applied.uid.as_str(),
            applied.name.as_str(),
            applied.version.as_str(),
            applied.kind.as_str()
        ),
        ("p2", "addons", "2.0.0", "add-on"),
        "the version is the profile version, not the version of the template"
    );
    assert_eq!(
        AppliedProfile::template_of(&doc, "p1").map(|t| t["version"].clone()),
        Some(json!(3))
    );
    assert!(AppliedProfile::applied(&doc, "p3").is_none());
    assert!(AppliedProfile::applied(&json!({}), "p1").is_none());
}

#[test]
fn a_pack_of_a_template_is_an_entry_of_pack_values_with_empty_text_for_a_missing_field() {
    let doc = json!({"spec": {"clusterProfileTemplates": [
        {"uid": "p1", "packs": [
            {"name": "kubernetes", "tag": "1.32", "type": "spectro", "values": "a: 1\n",
             "manifests": [{"name": "m"}]},
            {"name": "manifest-pack", "type": "manifest"},
        ]},
    ]}});
    let packs = AppliedProfile::template_packs(&doc, "p1");
    assert_eq!(packs.len(), 2);
    assert_eq!(
        AppliedProfile::template_pack_value(&packs[0]),
        json!({"name": "kubernetes", "tag": "1.32", "type": "spectro", "values": "a: 1\n"})
    );
    assert_eq!(
        AppliedProfile::template_pack_value(&packs[1]),
        json!({"name": "manifest-pack", "tag": "", "type": "manifest", "values": ""})
    );
    assert!(AppliedProfile::template_packs(&doc, "p2").is_empty());
}

#[test]
fn a_tier_leaves_a_field_that_strata_could_not_read_out_of_its_fields_and_its_hash() {
    let errors = BTreeMap::from([("rbacs".to_string(), "HTTP 403: forbidden".to_string())]);
    let unread = ClusterTier::with_errors(
        json!({"timezone": "Etc/UTC", "rbacs": null}),
        errors.clone(),
    );
    let read = ClusterTier::of(json!({"timezone": "Etc/UTC"}));
    assert_eq!(unread.fields, read.fields);
    assert_eq!(unread.hash, read.hash);
    assert_eq!(unread.errors, errors);
}

/// The field `cluster` of a `Source`, as serde reads it from `record`.
fn try_read_cluster(record: Value) -> Result<Option<ClusterSource>, serde_json::Error> {
    #[derive(Deserialize)]
    struct Holder {
        #[serde(deserialize_with = "deserialize_cluster")]
        cluster: Option<ClusterSource>,
    }
    serde_json::from_value::<Holder>(json!({"cluster": record})).map(|h| h.cluster)
}

fn read_cluster(record: Value) -> Option<ClusterSource> {
    try_read_cluster(record).unwrap()
}

/// A cluster object with the fields that a record must hold, and `more`.
fn cluster_object(more: Value) -> Value {
    let mut cluster = json!({"uid": "c1", "name": "payments", "profiles": []});
    cluster
        .as_object_mut()
        .unwrap()
        .extend(more.as_object().unwrap().clone());
    cluster
}

#[test]
fn the_read_errors_of_an_older_record_move_into_errors_and_an_entry_of_errors_stays() {
    let older = read_cluster(cluster_object(json!({
        "updatesPendingError": "HTTP 404: not found",
        "pendingRepaveError": "HTTP 403: forbidden",
    })))
    .unwrap();
    assert_eq!(
        serde_json::to_value(&older.errors).unwrap(),
        json!({
            "updatesPending": "HTTP 404: not found",
            "pendingRepave": "HTTP 403: forbidden",
        })
    );
    assert_eq!(older.uid, "c1");

    // a record that holds both shapes keeps the entry of the new one
    let both = read_cluster(cluster_object(json!({
        "pendingRepaveError": "HTTP 500: older",
        "errors": {"pendingRepave": "HTTP 403: newer", "updatesPending": "HTTP 404: x"},
    })))
    .unwrap();
    assert_eq!(
        serde_json::to_value(&both.errors).unwrap(),
        json!({"pendingRepave": "HTTP 403: newer", "updatesPending": "HTTP 404: x"})
    );
}

#[test]
fn a_cluster_that_a_record_writes_now_reads_with_the_errors_it_holds_and_writes_the_same() {
    let current = cluster_object(json!({
        "updatesPending": true,
        "errors": {"network": "HTTP 500: later", "pendingRepave": "HTTP 403: forbidden"},
    }));
    let read = read_cluster(current.clone()).unwrap();
    assert_eq!(read.errors.len(), 2);
    assert_eq!(serde_json::to_value(&read).unwrap(), current);

    let none = read_cluster(cluster_object(json!({}))).unwrap();
    assert!(none.errors.is_empty());
    assert_eq!(read_cluster(Value::Null).map(|c| c.uid), None);
}

#[test]
fn an_older_field_that_holds_null_is_no_error() {
    let read = read_cluster(cluster_object(json!({"updatesPendingError": null}))).unwrap();
    assert!(read.errors.is_empty());
}

#[test]
fn an_older_field_that_is_not_text_is_an_error_of_the_record() {
    let err = try_read_cluster(cluster_object(json!({"pendingRepaveError": 7}))).unwrap_err();
    assert!(err.to_string().contains("invalid type: integer"), "{err}");
}

#[test]
fn the_errors_of_a_cluster_name_the_field_that_strata_could_not_read() {
    let reasons = [
        (
            Open::Known(ClusterRead::UpdatesPending),
            &Some("HTTP 404: not found".to_string()),
        ),
        (Open::Known(ClusterRead::PendingRepave), &None),
    ];
    assert_eq!(
        unread(reasons),
        BTreeMap::from([(
            Open::Known(ClusterRead::UpdatesPending),
            "HTTP 404: not found".to_string()
        )])
    );
}

#[test]
fn the_errors_of_a_record_keep_a_part_that_a_later_release_names() {
    let errors: BTreeMap<Open<ClusterRead>, String> = serde_json::from_value(json!({
        "network": "HTTP 500: later",
        "pendingRepave": "HTTP 403: forbidden",
    }))
    .unwrap();
    let known: Vec<Option<ClusterRead>> = errors.keys().map(Open::known).collect();
    assert_eq!(known, [None, Some(ClusterRead::PendingRepave)]);
    assert_eq!(
        serde_json::to_value(&errors).unwrap(),
        json!({"network": "HTTP 500: later", "pendingRepave": "HTTP 403: forbidden"})
    );
}

/// `take_record` on a record whose cluster holds `more`, with a path for the file.
fn take_with(more: Value) -> crate::error::Result<Option<Value>> {
    let mut document = json!({"source": {"cluster": cluster_object(more)}});
    take_record(Path::new("older.json"), &mut document)
}

/// Review of Wave 14, A11. The move renames an older field, so the error of a value that is not
/// text named an entry that the file does not hold.
#[test]
fn an_older_field_that_is_not_text_gives_an_error_that_names_the_key_of_the_file() {
    for older in ["updatesPendingError", "pendingRepaveError"] {
        for wrong in [json!(7), json!(true), json!({"code": 403}), json!(["x"])] {
            let err = take_with(json!({ older: wrong })).unwrap_err();
            let crate::error::Error::SnapshotSchema { field, .. } = &err else {
                panic!("{err}");
            };
            assert_eq!(field, &format!("source.cluster.{older}"));
            let message = err.to_string();
            assert!(
                message.contains(&format!("source.cluster.{older}: invalid type: ")),
                "{message}"
            );
            assert!(message.ends_with("Take the record again."), "{message}");
        }
    }
    // the older field is wrong also when `errors` holds the entry that the move would keep
    let both = json!({
        "pendingRepaveError": 7,
        "errors": {"pendingRepave": "HTTP 403: newer"},
    });
    assert!(take_with(both).is_err());
}

#[test]
fn an_older_field_that_is_text_or_null_moves_into_errors_when_the_record_is_taken() {
    let taken = take_with(json!({
        "updatesPendingError": null,
        "pendingRepaveError": "HTTP 403: forbidden",
    }))
    .unwrap()
    .unwrap();
    assert_eq!(
        taken["errors"],
        json!({"pendingRepave": "HTTP 403: forbidden"})
    );
    assert!(taken.get("pendingRepaveError").is_none());
    let mut no_cluster = json!({"source": {"cluster": null}});
    assert!(
        take_record(Path::new("none.json"), &mut no_cluster)
            .unwrap()
            .is_none()
    );
}
