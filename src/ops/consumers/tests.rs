//! The unit tests of `consumers`.

use super::*;
use crate::error::Error;

#[test]
fn a_consumer_kind_is_named_in_the_documents_and_in_the_reports() {
    for (kind, name, noun) in [
        (ConsumerKind::Cluster, "cluster", "cluster"),
        (ConsumerKind::EdgeHost, "edgehost", "edge host"),
        (ConsumerKind::Template, "template", "cluster template"),
        (ConsumerKind::App, "app", "app deployment"),
    ] {
        assert_eq!(serde_json::to_value(kind).unwrap(), Value::from(name));
        assert_eq!(kind.noun(), noun);
    }
    // a consumer with no kind is a cluster, as each older document names one
    let c: Consumer = serde_json::from_value(serde_json::json!({
        "uid": "c1", "name": "lab", "projectUid": "p1", "state": "Running"
    }))
    .unwrap();
    assert_eq!(c.kind, ConsumerKind::Cluster);
}

/// Read the consumer `c` in the home scope. `read` gives its document.
fn read_with(c: &mut Consumer, read: Result<Value>) {
    let home = Palette::new(crate::api::MemoryApi::new());
    let mut clients = HashMap::new();
    let mut connect = |_: Option<&str>| Ok(Palette::new(crate::api::MemoryApi::new()));
    c.read(
        &mut clients,
        &home,
        &mut connect,
        |_, _| read,
        |d| text(d, "/spec/profile/metadata/version"),
    );
}

#[test]
fn a_consumer_that_is_read_takes_the_fields_of_its_document_and_keeps_those_it_lacks() {
    let mut c = Consumer::unread(ConsumerKind::App, "a1".into(), "shop".into(), "p1".into());
    read_with(
        &mut c,
        Ok(serde_json::json!({
            "status": {"state": "Running"},
            "spec": {"profile": {"metadata": {"version": "1.2.0"}}},
        })),
    );
    assert_eq!(
        (c.name.as_str(), c.project_uid.as_str(), c.state.as_str()),
        ("shop", "p1", "Running"),
        "a field that the document does not give keeps the value of the list"
    );
    assert_eq!(c.version, "1.2.0");
    assert!(c.error.is_none());
}

#[test]
fn a_consumer_whose_document_cannot_be_read_keeps_the_list_and_records_the_error() {
    let mut c = Consumer::unread(ConsumerKind::Cluster, "c1".into(), "lab".into(), "".into());
    read_with(
        &mut c,
        Err(Error::Usage {
            message: "no such cluster".to_string(),
        }),
    );
    assert_eq!(c.state, UNKNOWN_STATE);
    assert_eq!(c.version, "");
    assert_eq!(c.name, "lab");
    let message = c.error.as_ref().and_then(|e| e.get("message")?.as_str());
    assert_eq!(message, Some("no such cluster"));
}
