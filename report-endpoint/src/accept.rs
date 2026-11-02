//! The check of a request: the endpoint takes `strata-report/1` alone.
//!
//! The type [`Report`] has the fixed list of fields and refuses each other field. [`Report::fault`]
//! refuses the rest: another schema, too many entries, and a value that is not a short word. A
//! request that fails a check gets an error status and a message, and changes nothing.

use strata::closed_set::unknown::Report;
use strata::closed_set::unknown::send::{KEY_HEADER, KEY_LENGTH};

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

/// The key of a send in the header `value`, or `None` when the request has no key.
///
/// The key is the hex of a SHA-256 digest: 64 characters of `0` to `9` and `a` to `f`. The same
/// lines of the log give the same key, so a second try of a send has the key of the first.
///
/// # Errors
///
/// A [`Refusal`] with the status 400 when the key has another form. The message never repeats it.
pub fn key(value: Option<&str>) -> Result<Option<&str>, Refusal> {
    match value {
        None => Ok(None),
        Some(text)
            if text.len() == KEY_LENGTH
                && text
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)) =>
        {
            Ok(Some(text))
        }
        Some(_) => Err(Refusal::new(
            400,
            format!("the {KEY_HEADER} must have {KEY_LENGTH} characters of 0 to 9 and a to f"),
        )),
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
        // the message of serde_json holds the value of the field, so the refusal gives only the place
        serde_json::error::Category::Data => Refusal::new(
            422,
            format!(
                "the report is not valid: a field is missing, is not in the list, or has the wrong type, at line {} column {}",
                e.line(),
                e.column()
            ),
        ),
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
    use strata::closed_set::unknown::report::SCHEMA;

    use super::*;

    const JSON: Option<&str> = Some("application/json");

    fn entry() -> Value {
        json!({"strata": "0.26.0", "palette": "4.7.1", "set": "ProfileType", "value": "later",
            "route": "/v1/clusterprofiles/{uid}", "count": 2})
    }

    fn report(entries: Vec<Value>) -> Vec<u8> {
        json!({"schema": SCHEMA, "unknown": entries})
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
                refused.message.contains("is not in the list") && !refused.message.contains(field),
                "{refused:?}"
            );
        }
        let top = json!({"schema": SCHEMA, "unknown": [entry()], "host": "h"});
        let refused = refusal(JSON, top.to_string().as_bytes());
        assert_eq!(refused.status, 422);
        assert!(
            refused.message.contains("is not in the list") && !refused.message.contains("host"),
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
            json!({"schema": SCHEMA}).to_string().into_bytes(),
            json!({"schema": SCHEMA, "unknown": [{"strata": 1}]})
                .to_string()
                .into_bytes(),
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
        assert!(!refused.message.contains("secret-field"), "{refused:?}");
    }

    #[test]
    fn a_field_of_the_wrong_type_gets_a_fixed_text_with_the_line_and_the_column_and_no_value() {
        // serde_json names the value of a field of the wrong type in its message
        let wrong = [
            ("count", json!("hunter2")),
            ("strata", json!(["my-secret-token"])),
            ("set", json!({"password": "hunter2"})),
        ];
        for (field, value) in wrong {
            let mut with = entry();
            with[field] = value;
            let refused = refusal(JSON, &report(vec![with]));
            assert_eq!(refused.status, 422, "{field}");
            assert!(
                refused
                    .message
                    .contains("has the wrong type, at line 1 column "),
                "{refused:?}"
            );
            for secret in ["hunter2", "my-secret-token", "password"] {
                assert!(!refused.message.contains(secret), "{refused:?}");
            }
        }
        let top = br#"{"schema":7,"unknown":[]}"#;
        let refused = refusal(JSON, top);
        assert_eq!(refused.status, 422);
        assert!(
            refused.message.ends_with("at line 1 column 11"),
            "{refused:?}"
        );
    }

    #[test]
    fn a_key_is_64_characters_of_lower_case_hex_and_no_key_is_none() {
        let good = "0123456789abcdef".repeat(4);
        assert_eq!(key(None), Ok(None));
        assert_eq!(key(Some(&good)), Ok(Some(good.as_str())));
        for bad in [
            "",
            "abc",
            &good[1..],
            &format!("{good}0"),
            &good.replace('0', "g"),
            &good.to_uppercase(),
            &format!("{}\n", &good[1..]),
        ] {
            let refused = key(Some(bad)).unwrap_err();
            assert_eq!(refused.status, 400, "{bad:?}");
            assert!(
                refused.message.starts_with("the Idempotency-Key must"),
                "{refused:?}"
            );
            if !bad.is_empty() {
                assert!(!refused.message.contains(bad), "{refused:?}");
            }
        }
    }
}
