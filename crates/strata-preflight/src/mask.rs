//! The mask of the value of each variable of the entity in the messages of a report.
//!
//! Palette can quote the value of a cluster variable in a refusal, after it puts the value in place
//! of a `{{.spectro.var.NAME}}` reference. `K8SCidrOverlap` quotes a pod or service CIDR,
//! `CidrParseError` quotes a value that is not a CIDR, and `InvalidDropDownVariable` quotes a label
//! that is not an option. A cloud tool can quote a value in its output too. A job keeps the report
//! in its artifact, its log, and a comment. So each message gets a mark that names the variable in
//! place of each value, such as `[value of pod_cidr]`.
//!
//! The report puts the mask over each message. The check of Palette also puts it over the body of
//! an answer that failed, before strata cuts the body in the message. So the cut leaves no part of
//! a value. The mask matches whole values only: see [`Mask::apply`].

use std::collections::{BTreeMap, BTreeSet};

use serde_json::Value;
use strata::json::{arr, str_at, value_at};

/// The documents of the world that hold the variables of each profile: the entity, and the entity
/// before a change.
const ENTITIES: [&str; 2] = ["/entity", "/before"];

/// The characters that continue a value, such as the `.` of `10.0.0.3` and the `-` of
/// `us-east-1a`, when a letter or a digit is on their other side.
const JOINS: &str = "-_./:@";

/// The characters that a JSON encoder can write as an escape, beside the escapes of
/// `serde_json`. The JSON encoder of Go writes them so by default.
const HTML_ESCAPES: [(char, &str); 5] = [
    ('<', "\\u003c"),
    ('>', "\\u003e"),
    ('&', "\\u0026"),
    ('\u{2028}', "\\u2028"),
    ('\u{2029}', "\\u2029"),
];

/// The texts to find in a message, each with the mark that takes its place.
pub(crate) struct Mask {
    /// Each text and its mark, the longest text first, so that a value that holds a shorter value
    /// gets its own mark.
    texts: Vec<(String, String)>,
    /// Each mark of `texts`. A second mask over a text keeps each mark as it is.
    marks: BTreeSet<String>,
}

impl Mask {
    /// The mask of each value of a variable of the entity in `world`, and of the entity before a
    /// change.
    ///
    /// Each value comes in three forms: as it is, as the text of a JSON string, and as that text
    /// with the escapes of [`HTML_ESCAPES`]. The body of an answer of Palette is JSON text. A value
    /// with no letter and no digit, such as the mask `********` of a sensitive value, gets no mark.
    /// When two variables have one value, the mark names both: `[value of a or b]`.
    pub(crate) fn of(world: &Value) -> Mask {
        let mut names: BTreeMap<String, BTreeSet<&str>> = BTreeMap::new();
        for entity in ENTITIES {
            for profile in arr(world, &format!("{entity}/spec/profiles")) {
                for variable in arr(profile, "/variables") {
                    if let Some(value) = scalar_text(value_at(variable, "/value"))
                        && value.chars().any(char::is_alphanumeric)
                    {
                        names
                            .entry(value)
                            .or_default()
                            .insert(str_at(variable, "/name"));
                    }
                }
            }
        }
        let mut texts: Vec<(String, String)> = names
            .into_iter()
            .flat_map(|(value, names)| {
                let names: Vec<&str> = names.into_iter().collect();
                let mark = format!("[value of {}]", names.join(" or "));
                forms(&value)
                    .into_iter()
                    .map(move |form| (form, mark.clone()))
            })
            .collect();
        texts.sort_by(|a, b| b.0.len().cmp(&a.0.len()).then_with(|| a.0.cmp(&b.0)));
        let marks = texts.iter().map(|(_, mark)| mark.clone()).collect();
        Mask { texts, marks }
    }

    /// `text` with the mark in place of each whole value.
    ///
    /// A value is whole when no letter or digit touches it on either side. A character of
    /// [`JOINS`] that touches it counts as a letter when a letter or a digit is on its other side.
    /// So `3` masks `size 3` and `'3'`, but not `t3.xlarge`, `HTTP 403`, or `10.0.0.3`. `us-east-1`
    /// masks `us-east-1.` at the end of a sentence, but not `us-east-1a`. The scan goes one time
    /// from left to right, and it keeps each mark of the mask as it is. So a second mask over the
    /// text changes nothing.
    pub(crate) fn apply(&self, text: &str) -> String {
        let mut out = String::with_capacity(text.len());
        let mut rest = text;
        // the two characters of `text` before `rest`, the nearest first
        let mut before: [Option<char>; 2] = [None, None];
        while let Some(next) = rest.chars().next() {
            let (taken, written) = self
                .marks
                .iter()
                .find(|mark| rest.starts_with(mark.as_str()))
                .map(|mark| (mark.as_str(), mark.as_str()))
                .or_else(|| self.whole_value_at(rest, before))
                .unwrap_or((&rest[..next.len_utf8()], &rest[..next.len_utf8()]));
            out.push_str(written);
            for c in taken.chars() {
                before = [Some(c), before[0]];
            }
            rest = &rest[taken.len()..];
        }
        out
    }

    /// The text of a whole value at the start of `rest`, and its mark, or none. `before` holds the
    /// two characters before `rest`, the nearest first.
    fn whole_value_at(&self, rest: &str, before: [Option<char>; 2]) -> Option<(&str, &str)> {
        if joins(before[0], before[1]) {
            return None;
        }
        self.texts
            .iter()
            .find(|(form, _)| {
                rest.strip_prefix(form.as_str()).is_some_and(|after| {
                    let mut after = after.chars();
                    !joins(after.next(), after.next())
                })
            })
            .map(|(form, mark)| (form.as_str(), mark.as_str()))
    }
}

/// True when `touch`, the character beside a value, makes the value part of a longer one. `beyond`
/// is the character on the other side of `touch`.
fn joins(touch: Option<char>, beyond: Option<char>) -> bool {
    match touch {
        Some(c) if c.is_alphanumeric() => true,
        Some(c) if JOINS.contains(c) => beyond.is_some_and(char::is_alphanumeric),
        _ => false,
    }
}

/// The text of a value of a variable: a string as it is, and a number or a flag as JSON writes it.
/// A file can give a number or a flag in YAML. Another value has no text.
fn scalar_text(value: &Value) -> Option<String> {
    match value {
        Value::String(s) => Some(s.clone()),
        Value::Number(_) | Value::Bool(_) => Some(value.to_string()),
        _ => None,
    }
}

/// `value` as it is, as the text of a JSON string, and as that text with [`HTML_ESCAPES`].
fn forms(value: &str) -> BTreeSet<String> {
    let quoted = Value::String(value.to_string()).to_string();
    let json = quoted
        .strip_prefix('"')
        .and_then(|text| text.strip_suffix('"'))
        .unwrap_or(&quoted)
        .to_string();
    let escaped = HTML_ESCAPES
        .iter()
        .fold(json.clone(), |text, (c, escape)| text.replace(*c, escape));
    [value.to_string(), json, escaped].into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// A world whose entity has one profile with `variables`, as pairs of a name and a value.
    fn world(variables: &[(&str, Value)]) -> Value {
        let variables: Vec<Value> = variables
            .iter()
            .map(|(name, value)| json!({"name": name, "value": value}))
            .collect();
        json!({"entity": {"spec": {"profiles": [{"uid": "p1", "variables": variables}]}}})
    }

    #[test]
    fn a_short_or_common_value_masks_only_a_whole_value_and_no_other_text() {
        let mask = Mask::of(&world(&[
            ("replicas", json!("3")),
            ("enabled", json!(true)),
            ("region", json!("us-east-1")),
            ("prefix", json!("10")),
        ]));
        for (text, masked) in [
            ("pool size 3", "pool size [value of replicas]"),
            ("label '3' is not", "label '[value of replicas]' is not"),
            (
                "t3.xlarge, HTTP 403, 10.0.0.3, 3a, a3",
                "t3.xlarge, HTTP 403, 10.0.0.3, 3a, a3",
            ),
            (
                "x=true; untrue, trueish, true_x",
                "x=[value of enabled]; untrue, trueish, true_x",
            ),
            ("in us-east-1.", "in [value of region]."),
            (
                "us-east-1a, us-east-10, us-east-1-b",
                "us-east-1a, us-east-10, us-east-1-b",
            ),
            (
                "vpc 10.0.0.0/16, 10.",
                "vpc 10.0.0.0/16, [value of prefix].",
            ),
        ] {
            assert_eq!(mask.apply(text), masked, "{text}");
        }
    }

    #[test]
    fn a_value_that_holds_a_shorter_value_gets_its_own_mark_and_a_second_mask_keeps_each_mark() {
        let mask = Mask::of(&world(&[
            ("pod_cidr", json!("10.244.0.0/16")),
            ("network", json!("10.244.0.0")),
            ("word", json!("value")),
        ]));
        let masked = mask.apply("CIDR '10.244.0.0/16' and '10.244.0.0', a value");
        assert_eq!(
            masked,
            "CIDR '[value of pod_cidr]' and '[value of network]', a [value of word]"
        );
        assert_eq!(mask.apply(&masked), masked, "a second mask keeps each mark");
        assert_eq!(mask.apply("x[value of word]"), "x[value of word]");
    }

    #[test]
    fn a_value_in_the_json_text_of_an_answer_is_masked_in_its_escaped_form() {
        let mask = Mask::of(&world(&[("owner", json!(r#"a "b" & <c>"#))]));
        for (text, masked) in [
            (r#"is a "b" & <c>"#, "is [value of owner]"),
            (
                r#"{"message":"a \"b\" & <c>"}"#,
                r#"{"message":"[value of owner]"}"#,
            ),
            (
                r#"{"message":"a \"b\" & <c>"}"#,
                r#"{"message":"[value of owner]"}"#,
            ),
        ] {
            assert_eq!(mask.apply(text), masked, "{text}");
        }
    }

    #[test]
    fn a_value_with_no_letter_and_no_digit_and_a_value_that_is_no_text_get_no_mark() {
        let mask = Mask::of(&world(&[
            ("secret", json!("********")),
            ("dash", json!("-")),
            ("empty", json!("")),
            ("none", Value::Null),
            ("list", json!(["a"])),
        ]));
        assert!(mask.texts.is_empty(), "{:?}", mask.texts);
        assert_eq!(mask.apply("a - b ******** [a]"), "a - b ******** [a]");
    }

    #[test]
    fn the_mark_names_each_variable_with_the_value_and_the_entity_before_a_change_counts_too() {
        let mut w = world(&[("pod_cidr", json!("10.244.0.0/16"))]);
        w["before"] = json!({"spec": {"profiles": [
            {"uid": "p2", "variables": [{"name": "cidr", "value": "10.244.0.0/16"}]},
            {"uid": "p3", "variables": [{"name": "old_label", "value": "silver"}]},
        ]}});
        let mask = Mask::of(&w);
        assert_eq!(
            mask.apply("'10.244.0.0/16' and 'silver'"),
            "'[value of cidr or pod_cidr]' and '[value of old_label]'"
        );
    }
}
