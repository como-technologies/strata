//! The target of the answers of Palette that the reads of strata take.
//!
//! One document answers each request, so a document of the shape of a profile, a cluster, a
//! listing, or a search is read by every route that fits it. The target reads the document by
//! each ref and by each listing, and checks the record that a read gives.

use serde_json::Value;
use strata::api::Palette;
use strata::ops::{self, ListOptions, SnapshotOptions};
use strata::reference::Ref;
use strata::snapshot::SubjectKind;
use strata::term::progress::Progress;

use crate::record::check_record;
use crate::support::{AT, Everywhere, UID, answer};

/// The refs that the target reads. One of each kind that names an object of Palette.
fn refs() -> Vec<Ref> {
    let uid = UID.to_string();
    vec![
        Ref::Uid(uid.clone()),
        Ref::NameVersion {
            name: "demo".to_string(),
            version: "1.0.0".to_string(),
        },
        Ref::Draft {
            id: uid.clone(),
            version: None,
        },
        Ref::Cluster {
            uid: uid.clone(),
            profile: None,
            pending: false,
        },
        Ref::Cluster {
            uid: uid.clone(),
            profile: Some("demo".to_string()),
            pending: true,
        },
        Ref::EdgeHost {
            uid: uid.clone(),
            profile: None,
        },
        Ref::ClusterGroup { uid: uid.clone() },
        Ref::AppProfile {
            id: uid.clone(),
            version: None,
        },
        Ref::Pack {
            registry: None,
            name: "demo".to_string(),
            version: "1.0.0".to_string(),
        },
        Ref::ClusterSet {
            tags: vec!["env=prod".to_string()],
            filter: None,
        },
    ]
}

/// Read the document `answer` by each ref, each listing, and each read of a single object. Return
/// true when a read gave a record.
fn read_all(document: Value) -> bool {
    let api = Everywhere::new(document);
    let quiet = Progress::silent();
    let palette = Palette::new(&api);
    let mut recorded = false;

    for reference in refs() {
        let read = ops::snapshot_for(
            &reference,
            &SnapshotOptions::default(),
            || Ok(Palette::new(&api)),
            AT,
            &quiet,
        );
        if let Some(record) = answer(read) {
            check_record(&record);
            recorded = true;
        }
        answer(ops::consumers(
            &reference,
            |_| Ok(Palette::new(&api)),
            AT,
            &quiet,
        ));
    }

    let options = ListOptions {
        packs: true,
        applied: true,
        ..ListOptions::default()
    };
    answer(ops::list(&options, |_| Ok(Palette::new(&api)), AT, &quiet));

    answer(palette.profile_listing());
    answer(palette.cluster_listing());
    answer(palette.project_listing());
    answer(palette.edge_hosts());
    answer(palette.cluster_groups());
    answer(palette.project_uid_named("demo"));
    answer(palette.cluster_uid_named("demo"));
    answer(palette.resolve("demo", "1.0.0"));
    answer(palette.resolve_latest("demo"));
    answer(palette.resolve_app_profile("demo", "1.0.0"));
    answer(palette.cluster_events(UID));
    answer(palette.cluster_notifications(UID));
    answer(palette.cluster_group_members(UID));
    answer(palette.audits(None, UID, AT));
    for kind in [
        SubjectKind::Cluster,
        SubjectKind::EdgeHost,
        SubjectKind::ClusterGroup,
    ] {
        answer(palette.fetch_subject(kind, UID, &quiet));
    }
    recorded
}

/// Read the JSON document of `bytes` as the answer of Palette. Return true when a read gave a
/// record.
pub(crate) fn answers(bytes: &[u8]) -> bool {
    serde_json::from_slice(bytes).is_ok_and(read_all)
}

#[test]
fn an_answer_of_palette_of_any_shape_gives_a_record_or_an_error_with_a_kind() {
    bolero::check!().for_each(|bytes| {
        answers(bytes);
    });
}
