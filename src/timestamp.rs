//! The schema of a time in a document.
//!
//! A document holds each time as text. The JSON Schema of a field says that the text is a time in
//! RFC 3339, with `format: date-time`. A field takes the schema with
//! `#[schemars(with = "Rfc3339")]`, and the field stays a `String` in the code.

use schemars::{JsonSchema, Schema, SchemaGenerator, json_schema};
use std::borrow::Cow;

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
