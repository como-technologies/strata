//! The time in a document: its text, and its schema.
//!
//! A document holds each time as text. The JSON Schema of a field says that the text is a time in
//! RFC 3339, with `format: date-time`. A field takes the schema with
//! `#[schemars(with = "Rfc3339")]`, and the field stays a `String` in the code.

use chrono::{DateTime, SecondsFormat, Utc};
use schemars::{JsonSchema, Schema, SchemaGenerator, json_schema};
use std::borrow::Cow;

/// The time now in RFC 3339 UTC, with whole seconds: `2026-09-17T09:30:00Z`.
///
/// This is the text of `fetched_at`, and of each other time that a command writes.
pub fn now_utc() -> String {
    utc_text(Utc::now().timestamp())
}

/// The time `seconds` after the Unix epoch, as [`now_utc`] writes it.
///
/// A caller that needs the seconds too reads the clock one time and calls this function. A
/// number of seconds out of the range of a date gives the epoch: no clock gives such a number.
pub fn utc_text(seconds: i64) -> String {
    DateTime::from_timestamp(seconds, 0)
        .unwrap_or_default()
        .to_rfc3339_opts(SecondsFormat::Secs, true)
}

/// A time in RFC 3339, such as `2026-09-17T09:30:00Z`.
pub(crate) struct Rfc3339;

impl JsonSchema for Rfc3339 {
    fn inline_schema() -> bool {
        true
    }

    fn schema_name() -> Cow<'static, str> {
        "Rfc3339".into()
    }

    fn json_schema(_: &mut SchemaGenerator) -> Schema {
        json_schema!({"type": "string", "format": "date-time"})
    }
}

/// A time in RFC 3339, or empty text. A field of this kind holds a time that Palette gives, and
/// holds empty text when Palette gives none.
pub(crate) struct Rfc3339OrEmpty;

impl JsonSchema for Rfc3339OrEmpty {
    fn inline_schema() -> bool {
        true
    }

    fn schema_name() -> Cow<'static, str> {
        "Rfc3339OrEmpty".into()
    }

    fn json_schema(generator: &mut SchemaGenerator) -> Schema {
        json_schema!({
            "anyOf": [
                generator.subschema_for::<Rfc3339>(),
                {"type": "string", "maxLength": 0},
            ]
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_time_is_written_in_rfc_3339_utc_with_whole_seconds() {
        assert_eq!(utc_text(0), "1970-01-01T00:00:00Z");
        assert_eq!(utc_text(1_789_637_400), "2026-09-17T09:30:00Z");
    }

    #[test]
    fn a_number_of_seconds_out_of_range_gives_the_epoch() {
        assert_eq!(utc_text(i64::MAX), "1970-01-01T00:00:00Z");
    }

    #[test]
    fn the_time_now_is_a_time_in_the_format_of_a_document() {
        let now = now_utc();
        let parsed = DateTime::parse_from_rfc3339(&now).unwrap();
        assert_eq!(
            parsed.to_utc().to_rfc3339_opts(SecondsFormat::Secs, true),
            now
        );
        assert!(now.ends_with('Z'), "{now}");
    }
}
