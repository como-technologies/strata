//! Canonical form and hashing. YAML is parsed and re-serialized as JSON with sorted keys and no
//! whitespace before hashing, so Palette's whitespace, key order and template expansion on read
//! do not move the hashes.

use serde::Deserialize;
use serde_json::Value;
use sha2::{Digest, Sha256};

/// Lowercase hex SHA-256 of `s`.
pub fn sha256_hex(s: &str) -> String {
    hex::encode(Sha256::digest(s.as_bytes()))
}

/// Sorted object keys, no whitespace, JSON string escaping. Sorting is done here so the result
/// does not depend on serde_json's map ordering feature.
pub fn canonical_json(v: &Value) -> String {
    let mut out = String::new();
    write_canon(v, &mut out);
    out
}

fn write_canon(v: &Value, out: &mut String) {
    match v {
        Value::Null => out.push_str("null"),
        Value::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
        Value::Number(n) => out.push_str(&n.to_string()),
        Value::String(s) => out.push_str(&serde_json::to_string(s).expect("string serializes")),
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
                out.push_str(&serde_json::to_string(k).expect("key serializes"));
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

/// YAML text to a JSON value: one document is that document, several documents become an array,
/// empty text is null. Map keys are stringified.
pub fn canon_yaml(text: &str) -> Canon {
    if text.trim().is_empty() {
        return Canon::Parsed(Value::Null);
    }
    let mut docs = Vec::new();
    for de in serde_yaml::Deserializer::from_str(text) {
        match serde_yaml::Value::deserialize(de) {
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

/// Check YAML text as Palette checks the values of a pack and the content of a manifest: it reads
/// the first document into a map (`ValidateFormat`, `hubble/util/yaml/yaml.go:960-966`). Empty
/// text and a null document pass. The error is the first line of the parser's message, or what the
/// document is in place of a map.
pub fn map_yaml(text: &str) -> Result<(), String> {
    let Some(de) = serde_yaml::Deserializer::from_str(text).next() else {
        return Ok(());
    };
    let doc = serde_yaml::Value::deserialize(de).map_err(|e| first_line(&e.to_string()))?;
    match yaml_to_json(doc)? {
        Value::Null | Value::Object(_) => Ok(()),
        Value::Array(_) => Err("the document is a list, not a map".to_string()),
        _ => Err("the document is a scalar, not a map".to_string()),
    }
}

/// Hash YAML text canonically; unparseable text is hashed as trimmed text and the error is returned.
pub fn hash_yaml(text: &str) -> (String, Option<String>) {
    match canon_yaml(text) {
        Canon::Parsed(v) => (sha256_hex(&canonical_json(&v)), None),
        Canon::Unparseable(e) => (sha256_hex(text.trim()), Some(e)),
    }
}

fn first_line(s: &str) -> String {
    s.lines().next().unwrap_or("").chars().take(200).collect()
}

/// Mapping keys must be scalars, as in PyYAML: an unquoted Go template such as
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
    fn map_yaml_takes_what_palette_reads_into_a_map() {
        // each case as gopkg.in/yaml.v3 v3.0.1 gives it to `Unmarshal` into a map, as Palette does
        for ok in [
            "",
            "   \n",
            "# only a comment",
            "~",
            "kind: Namespace",
            "1: a\n2: b",
            "true: x",
            "a: &x 1\nb: *x",
            "a: \"{{ .spectro.var.x }}\"",
            // Palette reads the first document only
            "a: 1\n---\nb: [",
        ] {
            assert_eq!(map_yaml(ok), Ok(()), "{ok:?}");
        }
        for bad in [
            "a: [b",
            "- a\n- b",
            "hello",
            "a: 1\na: 2",
            "a: b\n\tc: d",
            "replicas: {{ .spectro.var.replicas }}",
            "a:\n  ? [x]\n  : y",
        ] {
            assert!(map_yaml(bad).is_err(), "{bad:?}");
        }
        assert_eq!(
            map_yaml("- a").unwrap_err(),
            "the document is a list, not a map"
        );
    }

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
    fn unparseable_yaml_is_reported_and_hashed_as_text() {
        let text = "charts:\n  x:\n    replicas: {{ .spectro.var.replicas }}\n";
        let (h, err) = hash_yaml(text);
        assert!(err.is_some());
        assert_eq!(h, sha256_hex(text.trim()));
    }
}
