//! The target of the endpoint of the report: the check of the body of a request.

use strata_report_endpoint::accept::parse;

/// The content type that a sender of a report gives.
const JSON: Option<&str> = Some("application/json");

/// The statuses that the check gives for a body that it refuses.
const REFUSALS: [u16; 4] = [400, 413, 415, 422];

/// Check `bytes` as the body of a request to the endpoint. Return true when the body is a report.
///
/// A body that the check takes is a report that passes the check of the library, and it writes as
/// a body that the check takes again and reads as the same report. A body that the check refuses
/// gets one of the four statuses of a refusal.
pub(crate) fn body(bytes: &[u8]) -> bool {
    let report = match parse(JSON, bytes) {
        Ok(report) => report,
        Err(refusal) => {
            assert!(
                REFUSALS.contains(&refusal.status),
                "the status {} is no refusal of the check",
                refusal.status
            );
            return false;
        }
    };
    assert_eq!(report.fault(), None, "a taken report has a fault");
    let text = serde_json::to_vec(&report).expect("a report is JSON");
    let again = parse(JSON, &text)
        .unwrap_or_else(|r| panic!("a taken report is refused when it is written again: {r:?}"));
    assert_eq!(again, report, "a report and its text are two reports");
    true
}

#[test]
fn a_request_body_is_refused_with_a_status_or_is_a_report_that_writes_as_itself() {
    bolero::check!().for_each(|bytes| {
        body(bytes);
    });
}
