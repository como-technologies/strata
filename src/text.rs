//! The texts that a message builds:
//!
//! - A phrase from a list of names, and the name of an object.
//! - A long text cut to a length.
//! - A mark for an empty text, and a text in a cell of a markdown table.
//! - A JSON Schema as text, with its descriptions in plain words.
//!
//! `strata-preflight` calls the public ones, so both tools write each of these in one way.

use schemars::Schema;
use serde_json::Value;

/// The name of an object, or its uid when Palette gives no name.
pub fn name_or_uid<'a>(name: &'a str, uid: &'a str) -> &'a str {
    if name.is_empty() { uid } else { name }
}

/// `one` when `n` is 1, else `many`: the noun that agrees with a count, such as `cluster` or
/// `clusters`.
///
/// It also picks a verb, an ending, or a pair of words, such as `"was"` and `"were"`. Each message
/// of strata, and each report of `strata-preflight`, that agrees a word with a count calls it.
pub fn plural<T>(n: usize, one: T, many: T) -> T {
    if n == 1 { one } else { many }
}

/// `items` as one phrase: `a`, `a and b`, or `a, b, and c`. Empty text for no item.
///
/// Each message of strata that names a list of things builds it here, so each one puts the comma
/// before `and` in the same way.
pub(crate) fn and_list<S: AsRef<str>>(items: &[S]) -> String {
    match items {
        [] => String::new(),
        [one] => one.as_ref().to_string(),
        [a, b] => format!("{} and {}", a.as_ref(), b.as_ref()),
        [rest @ .., last] => {
            let rest: Vec<&str> = rest.iter().map(AsRef::as_ref).collect();
            format!("{}, and {}", rest.join(", "), last.as_ref())
        }
    }
}

/// `s`, cut to at most `n` characters: its first `n - 1` characters and `…` when it is longer.
///
/// Each message and each column of a report that shows the start of a long text cuts it here.
/// So each one marks the cut in the same way. An `n` of zero gives empty text.
pub fn cut(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        return s.to_string();
    }
    let Some(kept) = n.checked_sub(1) else {
        return String::new();
    };
    let mut part: String = s.chars().take(kept).collect();
    part.push('…');
    part
}

/// `text` with each rustdoc link written as a plain code span: ``[`Snapshot`]`` is `` `Snapshot` ``.
///
/// A doc comment links a type for the reader of rustdoc. A reader of a schema sees the markup of
/// the link and cannot follow it.
fn plain_doc(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find("[`") {
        // `span` starts at the opening backtick of the code span
        let span = &rest[start + 1..];
        let Some(end) = span[1..].find("`]") else {
            break;
        };
        out.push_str(&rest[..start]);
        out.push_str(&span[..end + 2]);
        rest = &span[end + 3..];
    }
    out.push_str(rest);
    out
}

/// Write each `description` of the schema `value` as [`plain_doc`] writes a doc comment.
fn plain_descriptions(value: &mut Value) {
    match value {
        Value::Object(map) => {
            for (key, item) in map.iter_mut() {
                match item {
                    Value::String(text) if key == "description" => *text = plain_doc(text),
                    _ => plain_descriptions(item),
                }
            }
        }
        Value::Array(items) => items.iter_mut().for_each(plain_descriptions),
        _ => {}
    }
}

/// The JSON Schema `schema` as indented JSON, with no rustdoc link in a description.
///
/// A description of a schema is the doc comment of a type or a field, and a person who writes a
/// document reads it. `strata schema` and `strata-preflight --schema` print the schema here, so
/// that both show a link as a plain code span.
///
/// # Errors
///
/// A [`serde_json::Error`] when the schema cannot be written as JSON.
pub fn schema_json(schema: &Schema) -> serde_json::Result<String> {
    let mut value = schema.as_value().clone();
    plain_descriptions(&mut value);
    serde_json::to_string_pretty(&value)
}

/// `s`, or `-` when `s` is empty.
///
/// Each report that shows a field with no value shows this one mark.
pub fn dash(s: &str) -> &str {
    if s.is_empty() { "-" } else { s }
}

/// `s` as the text of one cell of a markdown table.
///
/// A `|` ends a cell, and a line break ends the row. So the text escapes each `|`, and writes a
/// line break as a space.
pub fn markdown_cell(s: &str) -> String {
    s.replace('|', "\\|").replace('\n', " ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_noun_agrees_with_its_count_and_only_one_takes_the_singular() {
        assert_eq!(plural(0, "cluster", "clusters"), "clusters");
        assert_eq!(plural(1, "cluster", "clusters"), "cluster");
        assert_eq!(plural(2, "cluster", "clusters"), "clusters");
        assert_eq!(plural(1, "was", "were"), "was");
        assert_eq!(plural(3, String::from("a"), String::from("b")), "b");
    }

    #[test]
    fn an_object_shows_its_name_and_its_uid_only_when_it_has_no_name() {
        assert_eq!(name_or_uid("lab", "u1"), "lab");
        assert_eq!(name_or_uid("", "u1"), "u1");
        assert_eq!(name_or_uid("", ""), "");
    }

    #[test]
    fn a_cut_text_has_at_most_n_characters_and_ends_with_a_mark() {
        assert_eq!(cut("abc", 3), "abc");
        assert_eq!(cut("abcd", 3), "ab…");
        assert_eq!(cut("abcd", 3).chars().count(), 3);
        // a character of two bytes is one character, and no cut stops in it
        assert_eq!(cut("ééééé", 4), "ééé…");
        assert_eq!(cut("abcd", 1), "…");
        assert_eq!(cut("abcd", 0), "");
        assert_eq!(cut("", 0), "");
    }

    #[test]
    fn a_doc_comment_shows_each_link_as_a_plain_code_span_and_keeps_the_other_text() {
        assert_eq!(
            plain_doc("The digest, as [`Snapshot::content_digest`] gives it: see [`Template`]."),
            "The digest, as `Snapshot::content_digest` gives it: see `Template`."
        );
        assert_eq!(
            plain_doc("an [array] and `code` and [`open"),
            "an [array] and `code` and [`open"
        );
        assert_eq!(plain_doc("[`a`][`b`]"), "`a``b`");
        assert_eq!(plain_doc(""), "");
    }

    #[test]
    fn a_schema_prints_each_description_without_link_markup() {
        let schema = Schema::try_from(serde_json::json!({
            "description": "A [`Type`].",
            "properties": {
                "description": {"description": "see [`Field`]", "type": "string"},
                "list": {"items": [{"description": "[`First`]"}]}
            },
            "const": "[`kept`]"
        }))
        .unwrap();
        let printed: Value = serde_json::from_str(&schema_json(&schema).unwrap()).unwrap();
        assert_eq!(printed["description"], "A `Type`.");
        assert_eq!(
            printed["properties"]["description"]["description"],
            "see `Field`"
        );
        assert_eq!(
            printed["properties"]["list"]["items"][0]["description"],
            "`First`"
        );
        assert_eq!(printed["const"], "[`kept`]", "only a description changes");
    }

    #[test]
    fn an_empty_text_shows_a_dash_and_another_text_shows_as_it_is() {
        assert_eq!(dash(""), "-");
        assert_eq!(dash("lab"), "lab");
    }

    #[test]
    fn a_cell_of_a_markdown_table_holds_no_bar_and_no_line_break() {
        assert_eq!(markdown_cell("a|b"), "a\\|b");
        assert_eq!(markdown_cell("first\nsecond"), "first second");
        assert_eq!(markdown_cell("plain"), "plain");
    }

    #[test]
    fn a_list_of_names_has_a_comma_before_and_only_from_three_names() {
        assert_eq!(and_list::<&str>(&[]), "");
        assert_eq!(and_list(&["os"]), "os");
        assert_eq!(and_list(&["os", "k8s"]), "os and k8s");
        assert_eq!(and_list(&["os", "k8s", "cni"]), "os, k8s, and cni");
        assert_eq!(
            and_list(&[
                "a".to_string(),
                "b".to_string(),
                "c".to_string(),
                "d".to_string()
            ]),
            "a, b, c, and d"
        );
    }
}
