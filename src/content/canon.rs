//! Canonical form and hashing. YAML is parsed and re-serialized as JSON with sorted keys and no
//! whitespace before hashing, so Palette's whitespace, key order and template expansion on read
//! do not move the hashes.

use serde::Deserialize;
use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::text::cut;

/// Lowercase hex SHA-256 of `s`.
pub fn sha256_hex(s: &str) -> String {
    hex::encode(Sha256::digest(s.as_bytes()))
}

/// The canonical text of `v`: the text that each hash of strata covers.
///
/// The rule: an object writes its keys in sorted order, and the text holds no whitespace. A string
/// has the escapes of JSON. It also has `\uXXXX` for each character that a YAML parser does not
/// read back as itself: DEL, U+0080 to U+009F, U+2028, U+2029, U+FFFE, and U+FFFF. So the text is
/// YAML that means the same value. Strata sorts the keys here, so the result does not depend on
/// the map ordering feature of `serde_json`. A change to this rule moves the root of each record
/// that holds such a value.
pub fn canonical_json(v: &Value) -> String {
    let mut out = String::new();
    write_canon(v, &mut out);
    out
}

/// Write `s` as a JSON string, with `\uXXXX` for each character that [`needs_escape`] names.
fn write_string(s: &str, out: &mut String) {
    let json = serde_json::to_string(s).expect("string serializes");
    for c in json.chars() {
        if needs_escape(c) {
            out.push_str(&format!("\\u{:04x}", c as u32));
        } else {
            out.push(c);
        }
    }
}

/// True for a character that JSON writes as it is, and that a YAML parser does not read back as
/// itself in a double-quoted text.
///
/// The parser reads U+0085, U+2028, and U+2029 as a line break, and folds it. It refuses DEL, the
/// other control characters of U+0080 to U+009F, and the two characters U+FFFE and U+FFFF.
const fn needs_escape(c: char) -> bool {
    matches!(
        c,
        '\u{7f}'..='\u{9f}' | '\u{2028}' | '\u{2029}' | '\u{fffe}' | '\u{ffff}'
    )
}

fn write_canon(v: &Value, out: &mut String) {
    match v {
        Value::Null => out.push_str("null"),
        Value::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
        Value::Number(n) => out.push_str(&n.to_string()),
        Value::String(s) => write_string(s, out),
        Value::Array(a) => {
            out.push('[');
            for (i, x) in a.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                write_canon(x, out);
            }
            out.push(']');
        }
        Value::Object(m) => {
            let mut keys: Vec<&String> = m.keys().collect();
            keys.sort();
            out.push('{');
            for (i, k) in keys.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                write_string(k, out);
                out.push(':');
                write_canon(&m[*k], out);
            }
            out.push('}');
        }
    }
}

/// The outcome of parsing YAML text.
pub enum Canon {
    /// Parsed; one document is the document, several are an array, empty text is null.
    Parsed(Value),
    /// Not valid YAML for strata's purposes; the first line of the parser's message.
    Unparseable(String),
}

/// The documents of the YAML `text`, each one as the parser reads it. A document that the parser
/// cannot read gives an error in its place.
pub(crate) fn yaml_documents(
    text: &str,
) -> impl Iterator<Item = Result<serde_yaml::Value, serde_yaml::Error>> + '_ {
    serde_yaml::Deserializer::from_str(text).map(serde_yaml::Value::deserialize)
}

/// YAML text to a JSON value: one document is that document, several documents become an array,
/// empty text is null. Map keys are stringified.
pub fn canon_yaml(text: &str) -> Canon {
    if text.trim().is_empty() {
        return Canon::Parsed(Value::Null);
    }
    let mut docs = Vec::new();
    for doc in yaml_documents(text) {
        match doc {
            Ok(v) => match yaml_to_json(v) {
                Ok(j) => docs.push(j),
                Err(e) => return Canon::Unparseable(e),
            },
            Err(e) => return Canon::Unparseable(first_line(&e.to_string())),
        }
    }
    match docs.len() {
        0 => Canon::Parsed(Value::Null),
        1 => Canon::Parsed(docs.pop().expect("one document")),
        _ => Canon::Parsed(Value::Array(docs)),
    }
}

/// Hash YAML text canonically; unparseable text is hashed as trimmed text and the error is returned.
pub fn hash_yaml(text: &str) -> (String, Option<String>) {
    let (_, hash, error) = canon_yaml_hashed(text);
    (hash, error)
}

/// YAML text in canonical form, with the hash of that form.
///
/// The steps:
///
/// 1. Parse the text with [`canon_yaml`]: one document is itself, several are an array, and empty
///    text is null. A key of a map becomes text.
/// 2. Write the value with [`canonical_json`], so a comment, the whitespace, the order of the keys,
///    and the style of the YAML do not change the hash.
/// 3. Hash that text with SHA-256.
///
/// Unparseable text gives null, the hash of the trimmed text, and the error.
pub fn canon_yaml_hashed(text: &str) -> (Value, String, Option<String>) {
    match canon_yaml(text) {
        Canon::Parsed(v) => {
            let hash = sha256_hex(&canonical_json(&v));
            (v, hash, None)
        }
        Canon::Unparseable(e) => (Value::Null, sha256_hex(text.trim()), Some(e)),
    }
}

/// The first line of the message `s`, cut to 200 characters.
fn first_line(s: &str) -> String {
    cut(s.lines().next().unwrap_or(""), 200)
}

/// Mapping keys must be scalars, as in `PyYAML`: an unquoted Go template such as
/// `{{ .spectro.var.x }}` parses as a mapping-valued key and is reported as unparseable.
fn yaml_to_json(v: serde_yaml::Value) -> Result<Value, String> {
    use serde_yaml::Value as Y;
    Ok(match v {
        Y::Null => Value::Null,
        Y::Bool(b) => Value::Bool(b),
        Y::Number(n) => {
            if let Some(i) = n.as_i64() {
                Value::from(i)
            } else if let Some(u) = n.as_u64() {
                Value::from(u)
            } else {
                serde_json::Number::from_f64(n.as_f64().unwrap_or(0.0))
                    .map(Value::Number)
                    .unwrap_or_else(|| Value::String(n.to_string()))
            }
        }
        Y::String(s) => Value::String(s),
        Y::Sequence(s) => Value::Array(
            s.into_iter()
                .map(yaml_to_json)
                .collect::<Result<Vec<_>, _>>()?,
        ),
        Y::Mapping(m) => {
            let mut o = serde_json::Map::new();
            for (k, v) in m {
                let key = match k {
                    Y::String(s) => s,
                    Y::Null => "null".to_string(),
                    Y::Bool(_) | Y::Number(_) => match yaml_to_json(k)? {
                        Value::String(s) => s,
                        json => canonical_json(&json),
                    },
                    Y::Tagged(t) => match yaml_to_json(t.value)? {
                        Value::String(s) => s,
                        json => canonical_json(&json),
                    },
                    Y::Mapping(_) | Y::Sequence(_) => return Err(
                        "found a mapping or sequence used as a mapping key (unquoted template?)"
                            .to_string(),
                    ),
                };
                o.insert(key, yaml_to_json(v)?);
            }
            Value::Object(o)
        }
        Y::Tagged(t) => yaml_to_json(t.value)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sha256_of_empty_string() {
        assert_eq!(
            sha256_hex(""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }

    #[test]
    fn canonical_json_sorts_keys_and_strips_whitespace() {
        let v: Value =
            serde_json::from_str(r#"{"b": [1, {"z": null, "a": "x"}], "a": 1.5}"#).unwrap();
        assert_eq!(
            canonical_json(&v),
            r#"{"a":1.5,"b":[1,{"a":"x","z":null}]}"#
        );
    }

    /// The canonical text of a string, read again as YAML.
    fn read_again(s: &str) -> Value {
        let text = canonical_json(&Value::String(s.to_string()));
        match canon_yaml(&text) {
            Canon::Parsed(v) => v,
            Canon::Unparseable(e) => panic!("the canonical text of {s:?} is not YAML: {e}"),
        }
    }

    #[test]
    fn a_string_with_a_line_break_of_yaml_reads_again_as_itself_from_its_canonical_text() {
        // YAML reads each of these three raw characters as a line break, and folds it
        for c in ['\u{85}', '\u{2028}', '\u{2029}'] {
            let s = format!("a{c}b");
            assert_eq!(read_again(&s), Value::String(s.clone()), "{c:?}");
        }
    }

    #[test]
    fn a_string_with_a_character_that_yaml_does_not_allow_reads_again_as_itself() {
        // YAML refuses each of these as a raw character: DEL, a C1 control, U+FFFE, and U+FFFF
        for c in ['\u{7f}', '\u{80}', '\u{9f}', '\u{fffe}', '\u{ffff}'] {
            let s = format!("a{c}b");
            assert_eq!(read_again(&s), Value::String(s.clone()), "{c:?}");
        }
    }

    #[test]
    fn a_string_of_each_character_reads_again_as_itself_from_its_canonical_text() {
        let all: String = ('\0'..=char::MAX).collect();
        assert_eq!(read_again(&all), Value::String(all.clone()));
    }

    #[test]
    fn a_key_with_a_line_break_of_yaml_reads_again_as_itself_from_its_canonical_text() {
        let v = serde_json::json!({"a\u{2029}b": 1});
        match canon_yaml(&canonical_json(&v)) {
            Canon::Parsed(again) => assert_eq!(again, v),
            Canon::Unparseable(e) => panic!("not YAML: {e}"),
        }
    }

    #[test]
    fn the_canonical_text_of_a_string_escapes_only_what_yaml_cannot_hold_raw() {
        let kept = "é \u{a0} \u{2e80} \u{1f600} \u{10ffff}";
        assert_eq!(
            canonical_json(&Value::String(format!("tab\t quote\" slash\\ {kept}"))),
            format!(r#""tab\t quote\" slash\\ {kept}""#)
        );
        for c in [
            '\u{7f}', '\u{85}', '\u{9f}', '\u{2028}', '\u{2029}', '\u{fffe}',
        ] {
            assert_eq!(
                canonical_json(&Value::String(c.to_string())),
                format!("\"\\u{:04x}\"", c as u32)
            );
        }
    }

    #[test]
    fn the_hash_of_a_value_with_a_line_break_of_yaml_covers_the_text_with_its_escape() {
        // `\L` is U+2028 in a double-quoted YAML text
        let (hash, error) = hash_yaml("k: \"a\\Lb\"\n");
        assert!(error.is_none());
        let escaped = concat!(r#"{"k":"a\u"#, "2028", r#"b"}"#);
        assert_eq!(hash, sha256_hex(escaped));
        // 0.25.0 hashed the text with the character as it is
        assert_ne!(hash, sha256_hex("{\"k\":\"a\u{2028}b\"}"));
    }

    #[test]
    fn the_hash_of_a_value_with_no_such_character_is_the_hash_of_its_json() {
        let (hash, error) = hash_yaml("k: \"a\\tb\\u00e9\"\n");
        assert!(error.is_none());
        assert_eq!(hash, sha256_hex("{\"k\":\"a\\tb\u{e9}\"}"));
    }

    #[test]
    fn yaml_key_order_and_indentation_do_not_change_the_hash() {
        let a = "pack:\n  x: 1\n  y: \"2\"\nlist:\n  - a\n  - b\n";
        let b = "list:\n    - a\n    - b\npack:\n    y: \"2\"\n    x: 1\n";
        assert_eq!(hash_yaml(a).0, hash_yaml(b).0);
        assert!(hash_yaml(a).1.is_none());
    }

    #[test]
    fn multi_document_yaml_becomes_an_array() {
        match canon_yaml("a: 1\n---\nb: 2\n") {
            Canon::Parsed(Value::Array(docs)) => assert_eq!(docs.len(), 2),
            _ => panic!("expected an array of documents"),
        }
    }

    #[test]
    fn empty_and_comment_only_text_is_null() {
        assert!(matches!(canon_yaml(""), Canon::Parsed(Value::Null)));
        assert!(matches!(
            canon_yaml("# just a comment\n"),
            Canon::Parsed(Value::Null)
        ));
    }

    #[test]
    fn the_message_of_yaml_that_does_not_parse_is_its_first_line_cut_to_200_characters() {
        let long = format!("{}\nthe second line", "a".repeat(250));
        let cut = first_line(&long);
        assert_eq!(cut.chars().count(), 200);
        assert!(cut.ends_with('…'), "{cut}");
        assert_eq!(first_line("short\nnext"), "short");
    }

    #[test]
    fn unparseable_yaml_is_reported_and_hashed_as_text() {
        let text = "charts:\n  x:\n    replicas: {{ .spectro.var.replicas }}\n";
        let (h, err) = hash_yaml(text);
        assert!(err.is_some());
        assert_eq!(h, sha256_hex(text.trim()));
    }
}
