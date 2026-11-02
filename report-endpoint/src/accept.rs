//! The check of a request: the endpoint takes `strata-report/1` alone.
//!
//! The type [`Report`] has the fixed list of fields and refuses each other field. [`Report::fault`]
//! refuses the rest: another schema, too many entries, and a value that is not a short word. A
//! request that fails a check gets an error status and a message, and changes nothing.

use strata::closed_set::unknown::Report;

/// The most bytes that the body of a request has. A report has 100 entries at most, and an entry is
/// a few hundred bytes.
pub const MOST_BYTES: usize = 64 * 1024;

/// A refusal: the status that the endpoint answers, and why.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Refusal {
    /// The HTTP status.
    pub status: u16,
    /// Why the endpoint refuses the request. It never repeats the body of the request.
    pub message: String,
}

impl Refusal {
    fn new(status: u16, message: impl Into<String>) -> Refusal {
        Refusal {
            status,
            message: message.into(),
        }
    }
}

/// The report in a request with the content type `content_type` and the body `body`.
///
/// # Errors
///
/// A [`Refusal`] with the status 415 when the content type is not JSON, 413 when the body is
/// longer than [`MOST_BYTES`], 400 when the body is not JSON, and 422 when it is JSON that is not
/// a valid report: a field outside the list, another schema, or a field that is not a short word.
pub fn parse(content_type: Option<&str>, body: &[u8]) -> Result<Report, Refusal> {
    let json = content_type.is_some_and(|text| {
        text.split(';')
            .next()
            .is_some_and(|kind| kind.trim().eq_ignore_ascii_case("application/json"))
    });
    if !json {
        return Err(Refusal::new(415, "the body must be application/json"));
    }
    if body.len() > MOST_BYTES {
        return Err(Refusal::new(
            413,
            format!("the body has more than {MOST_BYTES} bytes"),
        ));
    }
    let report: Report = serde_json::from_slice(body).map_err(|e| match e.classify() {
        serde_json::error::Category::Data => {
            Refusal::new(422, format!("the report is not valid: {e}"))
        }
        _ => Refusal::new(400, "the body is not JSON"),
    })?;
    match report.fault() {
        Some(fault) => Err(Refusal::new(
            422,
            format!("the report is not valid: {fault}"),
        )),
        None => Ok(report),
    }
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};

    use super::*;

    const JSON: Option<&str> = Some("application/json");

    fn entry() -> Value {
        json!({"strata": "0.26.0", "palette": "4.7.1", "set": "ProfileType", "value": "later",
            "route": "/v1/clusterprofiles/{uid}", "count": 2})
    }

    fn report(entries: Vec<Value>) -> Vec<u8> {
        json!({"schema": "strata-report/1", "unknown": entries})
            .to_string()
            .into_bytes()
    }

    fn refusal(content_type: Option<&str>, body: &[u8]) -> Refusal {
        parse(content_type, body).unwrap_err()
    }

    #[test]
    fn a_report_of_the_schema_with_the_listed_fields_is_taken() {
        let taken = parse(
            Some("Application/JSON; charset=utf-8"),
            &report(vec![entry()]),
        )
        .unwrap();
        assert_eq!(taken.unknown[0].value, "later");
        assert_eq!(taken.unknown[0].count, 2);
        let mut bare = entry();
        bare.as_object_mut().unwrap().remove("palette");
        assert!(
            parse(JSON, &report(vec![bare])).is_ok(),
            "palette, status, and errorCode are optional"
        );
    }

    #[test]
    fn a_report_with_another_field_is_refused_at_the_top_and_in_an_entry() {
        for field in ["uid", "name", "host", "tenant", "time", "message", "tool"] {
            let mut with = entry();
            with[field] = json!("x");
            let refused = refusal(JSON, &report(vec![with]));
            assert_eq!(refused.status, 422, "{field}");
            assert!(
                refused
                    .message
                    .contains(&format!("unknown field `{field}`")),
                "{refused:?}"
            );
        }
        let top = json!({"schema": "strata-report/1", "unknown": [entry()], "host": "h"});
        let refused = refusal(JSON, top.to_string().as_bytes());
        assert_eq!(refused.status, 422);
        assert!(
            refused.message.contains("unknown field `host`"),
            "{refused:?}"
        );
    }

    #[test]
    fn a_report_that_is_not_a_valid_report_is_refused_with_422() {
        let long = json!({"strata": "0.26.0", "set": "S", "value": "x".repeat(65),
            "route": "/v1/packs", "count": 1});
        let uid = json!({"strata": "0.26.0", "set": "S", "value": "v",
            "route": "/v1/clusterprofiles/64f0a1b2c3d4e5f6a7b8c9d0", "count": 1});
        let bodies = [
            report(vec![]),
            report(vec![long]),
            report(vec![uid]),
            json!({"schema": "strata-report/2", "unknown": [entry()]})
                .to_string()
                .into_bytes(),
            report((0..101).map(|_| entry()).collect()),
            br#"{"schema":"strata-report/1"}"#.to_vec(),
            br#"{"schema":"strata-report/1","unknown":[{"strata":1}]}"#.to_vec(),
        ];
        for body in bodies {
            assert_eq!(
                refusal(JSON, &body).status,
                422,
                "{}",
                String::from_utf8_lossy(&body)
            );
        }
    }

    #[test]
    fn a_body_that_is_not_json_or_too_long_or_has_another_content_type_is_refused() {
        assert_eq!(refusal(JSON, b"not json").status, 400);
        assert_eq!(refusal(JSON, b"").status, 400);
        assert_eq!(refusal(JSON, &vec![b' '; MOST_BYTES + 1]).status, 413);
        assert_eq!(
            refusal(Some("text/plain"), &report(vec![entry()])).status,
            415
        );
        assert_eq!(refusal(None, &report(vec![entry()])).status, 415);
    }

    #[test]
    fn a_refusal_never_repeats_a_value_of_the_body() {
        let mut with = entry();
        with["secret-field"] = json!("hunter2");
        let refused = refusal(JSON, &report(vec![with]));
        assert!(!refused.message.contains("hunter2"), "{refused:?}");
    }
}
