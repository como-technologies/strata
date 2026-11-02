//! The stand-in for the endpoint of the report.
//!
//! It is no route of Palette, so it is no file of the tenant, and no fact names it. The server of
//! [`super`] answers it before it reads a file.

/// The path of the stand-in for the endpoint of the report. The endpoint answers 202 for each
/// request, as the real endpoint does for a report that it takes. A test compares the answer with
/// the answer of the real endpoint.
pub const REPORT_ENDPOINT: &str = "/strata-report";

/// The status of the answer of the stand-in for the endpoint of the report.
pub(super) const REPORT_TAKEN_STATUS: u16 = 202;

/// The body of the answer of the stand-in for the endpoint of the report: one issue opened, none
/// changed.
pub(super) const REPORT_TAKEN: &[u8] = br#"{"ok":true,"opens":1,"updates":0}"#;

#[cfg(test)]
mod tests {
    use std::path::Path;
    use std::time::Instant;

    use serde_json::{Value, json};
    use strata_report_endpoint::server::{Request as Post, Service};
    use strata_report_endpoint::tracker::{Issue, Tracker, TrackerError};

    use super::REPORT_ENDPOINT;
    use crate::palette::{Request, served};

    /// A tracker with no issue: a report opens one issue and changes none.
    struct NoIssue;

    impl Tracker for NoIssue {
        fn issues(&mut self) -> Result<Vec<Issue>, TrackerError> {
            Ok(Vec::new())
        }

        fn open(&mut self, _: &str, _: &str) -> Result<u64, TrackerError> {
            Ok(1)
        }

        fn update(&mut self, _: u64, _: &str) -> Result<(), TrackerError> {
            unreachable!("a report of a new value changes no issue")
        }
    }

    #[test]
    fn the_stand_in_for_the_endpoint_of_the_report_answers_as_the_real_endpoint_does() {
        let report = json!({"schema": "strata-report/1", "unknown": [{
            "strata": "0.26.0", "set": "ProfileType", "value": "later",
            "route": "/v1/clusterprofiles/{uid}", "count": 1}]})
        .to_string()
        .into_bytes();
        let real = Service::new(NoIssue).handle(
            &Post {
                method: "POST",
                path: "/",
                content_type: Some("application/json"),
                key: None,
                sender: "127.0.0.1",
                body: &report,
            },
            Instant::now(),
        );
        let request = Request {
            target: REPORT_ENDPOINT.to_string(),
            project: None,
            accept: None,
            body: report.clone(),
        };
        let (status, body) = served(Path::new("/r"), &request).unwrap();
        assert_eq!(status, real.status);
        assert_eq!(serde_json::from_slice::<Value>(&body).unwrap(), real.body);
    }
}
