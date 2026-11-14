//! The targets of the answers of Palette that the reads of strata take: a document, and an error.
//!
//! One answer serves each request. Every route that fits it reads a document of the shape of a
//! profile, a cluster, a listing, or a search. Every route reads an error, with a status and a
//! body, too. The targets read the answer by each ref and by each listing, and check the record
//! that a read gives.

use serde_json::{Value, json};
use strata::Error;
use strata::api::codes::{ValidateRoute, is_refusal_code};
use strata::api::{Method, Palette};
use strata::error::FailureClass;
use strata::model::validation::PaletteRefusal;
use strata::ops::{self, ListOptions, SnapshotOptions};
use strata::reference::Ref;
use strata::snapshot::{Snapshot, SubjectKind};
use strata::term::progress::Progress;

use crate::record::check_record;
use crate::support::{AT, Everywhere, UID, answer, raw_profile, utf8};

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

/// Read the answers of `api` by each ref, each listing, and each read of a single object. Return
/// true when a read gave a record.
fn read_all(api: &Everywhere) -> bool {
    let quiet = Progress::silent();
    let palette = Palette::new(api);
    let mut recorded = false;

    for reference in refs() {
        let read = ops::snapshot_for(
            &reference,
            &SnapshotOptions::default(),
            || Ok(Palette::new(api)),
            AT,
            &quiet,
        );
        if let Some(record) = answer(read) {
            check_record(&record);
            recorded = true;
        }
        answer(ops::consumers(
            &reference,
            |_| Ok(Palette::new(api)),
            AT,
            &quiet,
        ));
    }

    let options = ListOptions {
        packs: true,
        applied: true,
        ..ListOptions::default()
    };
    answer(ops::list(&options, |_| Ok(Palette::new(api)), AT, &quiet));

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
    serde_json::from_slice(bytes).is_ok_and(|document| read_all(&Everywhere::new(document)))
}

#[test]
fn an_answer_of_palette_of_any_shape_gives_a_record_or_an_error_with_a_kind() {
    bolero::check!().for_each(|bytes| {
        answers(bytes);
    });
}

/// The status and the body of the error that `bytes` hold. The first two bytes are the status, in
/// big-endian order. The rest is the text of the body. `None` when the bytes are shorter, or the
/// body is not UTF-8.
fn error_of(bytes: &[u8]) -> Option<(u16, &str)> {
    let (status, body) = bytes.split_first_chunk::<2>()?;
    Some((u16::from_be_bytes(*status), utf8(body)?))
}

/// Make sure that each validate route reads the body of an error the same way as the reader of
/// refusals. A route that refuses the body finds a code in it. With HTTP 500, a route refuses
/// exactly the codes of its checks.
#[track_caller]
fn check_routes(status: u16, body: &str, refusal: Option<&PaletteRefusal>) {
    let error: Value = serde_json::from_str(body).unwrap_or(Value::Null);
    for route in ValidateRoute::ALL {
        let refuses = route.refuses_answer(status, &error);
        assert!(
            !refuses || refusal.is_some(),
            "{route:?} refuses {status} {body:?}, which holds no code"
        );
        if status == 500 {
            let in_checks = refusal.is_some_and(|r| route.refusal_codes().any(|c| c == r.code));
            assert_eq!(refuses, in_checks, "{route:?} reads {body:?} with HTTP 500");
        }
    }
}

/// Make sure that the class of the error `status` `body` follows the rule of the retry. A 5xx with
/// the code of an intended refusal stays. Another 5xx or a 429 passes. Each other status stays.
#[track_caller]
fn check_class(error: &Error, status: u16, refusal: Option<&PaletteRefusal>) {
    let server = (500..600).contains(&status);
    let by_design = refusal.is_some_and(|r| is_refusal_code(&r.code));
    let expected = if server && by_design {
        FailureClass::Terminal
    } else if server || status == 429 {
        FailureClass::Transient
    } else {
        FailureClass::Terminal
    };
    assert_eq!(error.class(), expected, "the class of {error}");
}

/// Ask Palette about a profile version that has a uid, and about a candidate that has none. Make
/// sure that a Palette that refuses each request never accepts. The record of the candidate goes
/// to the three routes that check a create. The record of a version goes to the route of its
/// packs.
fn validate(api: &Everywhere) {
    let palette = Palette::new(api);
    let version = crate::canon::record("a: 1\n", "").expect("a profile of one pack builds");
    let mut raw = raw_profile("a: 1\n", "");
    raw.profile["metadata"]["uid"] = json!("");
    let candidate = Snapshot::from_raw(&raw, "candidate", AT).expect("a candidate builds");
    for record in [&version, &candidate] {
        if let Some(validation) = answer(ops::profile_validation(&palette, record)) {
            assert!(
                !validation.accepts(),
                "a Palette that refuses each request accepts a profile"
            );
        }
    }
}

/// Read the first two bytes of `bytes` as the status of an HTTP error and the rest as its body.
/// Palette answers each request with that error. Return true when the body holds the `code` of a
/// refusal.
pub(crate) fn errors(bytes: &[u8]) -> bool {
    let Some((status, body)) = error_of(bytes) else {
        return false;
    };
    let refusal = PaletteRefusal::from_error_body(body);
    if let Some(refusal) = &refusal {
        assert!(!refusal.line().is_empty(), "a refusal has a line");
    }
    check_routes(status, body, refusal.as_ref());

    let error = Error::Api {
        method: Method::Get,
        path: "/v1/clusterprofiles/uid".to_string(),
        status,
        body: body.to_string(),
        retry_after: None,
        key_proved: false,
    };
    check_class(&error, status, refusal.as_ref());
    assert!(!error.to_string().is_empty(), "an error has a message");
    answer(Err::<(), _>(error));

    let api = Everywhere::failing(status, body);
    assert!(
        !read_all(&api),
        "a Palette that refuses each request gave a record"
    );
    validate(&api);
    refusal.is_some()
}

#[test]
fn an_error_of_palette_is_read_as_a_refusal_or_gives_an_error_with_a_kind() {
    bolero::check!().for_each(|bytes| {
        errors(bytes);
    });
}
