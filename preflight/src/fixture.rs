//! A fixture: a world document with the answers of Palette and of each command written in, for
//! the tests of the catalog. A fixture is a file. It can stand on its own, or say `_base` and
//! hold only what differs from the base:
//!
//! ```json
//! { "_base": "clean.json",
//!   "_note": "the one worker pool has size 0",
//!   "_patch": { "entity": { "spec": { "machinepoolconfig": [ { "poolConfig": { "name": "worker-pool", "size": 0 } } ] } } },
//!   "_delete": [ "/account/spec/sts" ] }
//! ```
//!
//! `_patch` merges as a change of a cluster merges (see [`crate::world::merge`]): a pool by its
//! name, a profile by its uid, an object key by key. `_delete` removes each JSON pointer. A base
//! can have a base of its own.

use std::path::Path;

use serde_json::Value;
use strata::json::{opt_text, strings};

use crate::error::{Error, Result};

/// Read a fixture, with its bases applied. The fixture gets the fields that the engine decides,
/// as [`crate::world::World::to_value`] gives them to a world of a run.
pub fn load(path: &Path) -> Result<Value> {
    let mut doc = load_depth(path, 0)?;
    crate::world::add_derived_fields(&mut doc);
    Ok(doc)
}

fn load_depth(path: &Path, depth: u32) -> Result<Value> {
    if depth > 8 {
        return Err(Error::Input {
            message: format!("{}: the chain of _base is longer than 8", path.display()),
        });
    }
    let text = std::fs::read_to_string(path).map_err(|e| crate::error::io(path, e))?;
    let mut doc: Value = serde_json::from_str(&text).map_err(|e| Error::Input {
        message: format!("{}: not JSON: {e}", path.display()),
    })?;
    let Some(base) = opt_text(&doc, "/_base") else {
        return Ok(doc);
    };
    let base_path = path.parent().unwrap_or(Path::new(".")).join(&base);
    let mut out = load_depth(&base_path, depth + 1)?;
    if let Some(patch) = doc.get("_patch") {
        crate::world::merge(&mut out, patch);
    }
    for pointer in strings(&doc, "/_delete") {
        delete_pointer(&mut out, &pointer);
    }
    // the fixture's own top-level keys beside the three, such as a `_note`, come along
    if let Some(obj) = doc.as_object_mut() {
        for k in ["_base", "_patch", "_delete"] {
            obj.remove(k);
        }
        for (k, v) in obj {
            out[k] = v.clone();
        }
    }
    Ok(out)
}

/// Remove the value at a JSON pointer, when it is there.
fn delete_pointer(doc: &mut Value, pointer: &str) {
    let Some((parent, key)) = pointer.rsplit_once('/') else {
        return;
    };
    let parent = if parent.is_empty() {
        Some(doc)
    } else {
        doc.pointer_mut(parent)
    };
    match parent {
        Some(Value::Object(o)) => {
            o.remove(key);
        }
        Some(Value::Array(a)) => {
            if let Ok(i) = key.parse::<usize>()
                && i < a.len()
            {
                a.remove(i);
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_fixture_with_a_base_merges_and_deletes() {
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path().join("clean.json");
        std::fs::write(
            &base,
            json!({"cloud": "eks", "entity": {"spec": {"machinepoolconfig": [
                {"poolConfig": {"name": "worker-pool", "size": 2}, "cloudConfig": {"azs": ["a", "b"]}}]}},
                "account": {"spec": {"sts": {"arn": "x"}, "credentialType": "sts"}}})
            .to_string(),
        )
        .unwrap();
        let fault = dir.path().join("one-zone.json");
        std::fs::write(
            &fault,
            json!({"_base": "clean.json", "_note": "one zone",
                "_patch": {"entity": {"spec": {"machinepoolconfig": [{"poolConfig": {"name": "worker-pool"}, "cloudConfig": {"azs": ["a"]}}]}}},
                "_delete": ["/account/spec/sts"]})
            .to_string(),
        )
        .unwrap();
        let w = load(&fault).unwrap();
        assert_eq!(
            w["entity"]["spec"]["machinepoolconfig"][0]["cloudConfig"]["azs"],
            json!(["a"])
        );
        assert_eq!(
            w["entity"]["spec"]["machinepoolconfig"][0]["poolConfig"]["size"], 2,
            "the base's field stays"
        );
        assert!(w["account"]["spec"].get("sts").is_none());
        assert_eq!(w["account"]["spec"]["credentialType"], "sts");
        assert_eq!(w["_note"], "one zone");
        assert!(w.get("_base").is_none());
        assert_eq!(
            w["accountKind"], "aws",
            "an eks fixture has the account of aws"
        );
        assert_eq!(w["routeCloud"], "eks");
    }
}
