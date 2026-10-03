//! The profile export of Palette, read as a candidate.
//!
//! Palette writes one file for a profile version: the console exports it, and
//! `GET /v1/clusterprofiles/{uid}/export` gives it. The file holds the content of a candidate file
//! in another shape. [`super::load`] reads it when the file has `spec.template.packs`, so a team
//! that keeps the export in git does not write a second file for strata.

use serde_json::{Map, Value};

use super::{Candidate, CandidateLayer, CandidateManifest, MANIFEST, default_type};

/// The packs of an export. A file that has an array here is an export.
const PACKS: &str = "/spec/template/packs";

/// Whether `doc` is a profile export of Palette, and not a candidate file.
pub(super) fn is_export(doc: &Value) -> bool {
    doc.pointer(PACKS).is_some_and(Value::is_array)
}

/// The candidate that the export `doc` holds.
///
/// `metadata.name` and `spec.version` are the profile and its version, `spec.template.type` and
/// `cloudType` its type and cloud, and `spec.variables` its variables. Each pack is one layer, in
/// order. A pack of type `manifest` is a manifest layer with no registry. Each other pack names
/// its registry by `registry.metadata.uid`, else by `registry.metadata.name`.
///
/// The export holds no params. It omits values that are empty, so a pack with no values gets
/// empty values, and not the default values of the pack. [`super::check`] then checks the
/// candidate as it checks a candidate file.
///
/// # Errors
///
/// A message that names the field, when a field that holds text in an export holds another
/// value, such as a number, or when `spec.variables` is not a list.
pub(super) fn to_candidate(doc: &Value) -> Result<Candidate, String> {
    let packs = doc.pointer(PACKS).and_then(Value::as_array);
    let layers = packs
        .into_iter()
        .flatten()
        .enumerate()
        .map(|(i, p)| layer(p, &format!("spec.template.packs[{i}]")))
        .collect::<Result<Vec<_>, _>>()?;
    let kind = text(doc, "/spec/template/type", "")?;
    Ok(Candidate {
        name: text(doc, "/metadata/name", "")?,
        version: text(doc, "/spec/version", "")?,
        kind: if kind.is_empty() {
            default_type()
        } else {
            kind
        },
        cloud_type: text(doc, "/spec/template/cloudType", "")?,
        registry: None,
        registry_uid: None,
        layers,
        variables: list(doc, "/spec/variables")?,
    })
}

/// The layer of the pack `p` of an export. `at` names the pack in a message.
fn layer(p: &Value, at: &str) -> Result<CandidateLayer, String> {
    let nonempty = |s: String| Some(s).filter(|s| !s.is_empty());
    let manifest = text(p, "/type", at)? == MANIFEST;
    // Palette makes the pack of a manifest layer from its own manifest pack, so the export gives
    // it an empty registry
    let (registry_uid, registry) = if manifest {
        (None, None)
    } else {
        match nonempty(text(p, "/registry/metadata/uid", at)?) {
            Some(uid) => (Some(uid), None),
            None => (None, nonempty(text(p, "/registry/metadata/name", at)?)),
        }
    };
    let manifests = p
        .get("manifests")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .enumerate()
        .map(|(j, m)| {
            let at = format!("{at}.manifests[{j}]");
            Ok(CandidateManifest {
                name: text(m, "/name", &at)?,
                content: Some(text(m, "/content", &at)?),
                content_file: None,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    Ok(CandidateLayer {
        name: text(p, "/name", at)?,
        version: text(p, "/version", at)?,
        layer: nonempty(text(p, "/layer", at)?),
        kind: manifest.then(|| MANIFEST.to_string()),
        tag: nonempty(text(p, "/tag", at)?),
        registry,
        registry_uid,
        values: Some(text(p, "/values", at)?),
        values_file: None,
        params: Map::new(),
        manifests,
    })
}

/// The text at `ptr` in `v`, and empty text when `v` has no value there. `at` names `v` in the
/// message of a value that is not text.
fn text(v: &Value, ptr: &str, at: &str) -> Result<String, String> {
    match v.pointer(ptr) {
        None | Some(Value::Null) => Ok(String::new()),
        Some(Value::String(s)) => Ok(s.clone()),
        Some(other) => {
            let field = ptr.trim_start_matches('/').replace('/', ".");
            let field = if at.is_empty() {
                field
            } else {
                format!("{at}.{field}")
            };
            Err(format!(
                "{field} is {other} in the export, and not text; quote it"
            ))
        }
    }
}

/// The list at `ptr` in `doc`, and an empty list when `doc` has no value there. The export omits
/// a list that is empty.
fn list(doc: &Value, ptr: &str) -> Result<Vec<Value>, String> {
    match doc.pointer(ptr) {
        None | Some(Value::Null) => Ok(Vec::new()),
        Some(Value::Array(items)) => Ok(items.clone()),
        Some(other) => {
            let field = ptr.trim_start_matches('/').replace('/', ".");
            Err(format!("{field} is {other} in the export, and not a list"))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_file_with_spec_template_packs_is_an_export_and_a_candidate_file_is_not() {
        assert!(is_export(&json!({"spec": {"template": {"packs": []}}})));
        assert!(!is_export(&json!({"name": "barney", "layers": []})));
        assert!(!is_export(&json!({"spec": {"template": {"packs": {}}}})));
    }

    #[test]
    fn a_version_that_yaml_reads_as_a_number_is_refused_with_the_field_that_holds_it() {
        let doc = json!({"metadata": {"name": "barney"}, "spec": {"version": "1.0.0",
            "template": {"type": "add-on", "packs": [{"name": "fred", "version": 24.04}]}}});
        assert_eq!(
            to_candidate(&doc).unwrap_err(),
            "spec.template.packs[0].version is 24.04 in the export, and not text; quote it"
        );
    }

    #[test]
    fn an_export_whose_variables_are_not_a_list_is_candidate_invalid_and_names_the_field() {
        let dir = tempfile::tempdir().unwrap();
        let export = dir.path().join("export.yaml");
        std::fs::write(
            &export,
            "metadata:\n  name: barney\nspec:\n  version: 1.0.0\n  variables:\n    name: fred\n  \
             template:\n    packs:\n    - name: fred\n      version: 1.0.0\n",
        )
        .unwrap();
        match crate::candidate::load(&export).unwrap_err() {
            crate::Error::CandidateInvalid { message, .. } => assert_eq!(
                message,
                r#"spec.variables is {"name":"fred"} in the export, and not a list"#
            ),
            other => panic!("not candidate-invalid: {other}"),
        }
    }

    #[test]
    fn an_export_with_null_variables_has_no_variables() {
        let doc = json!({"metadata": {"name": "barney"}, "spec": {"version": "1.0.0",
            "variables": null, "template": {"packs": []}}});
        assert!(to_candidate(&doc).unwrap().variables.is_empty());
    }

    #[test]
    fn a_pack_with_no_registry_uid_names_its_registry_by_name() {
        let doc = json!({"metadata": {"name": "barney"}, "spec": {"version": "1.0.0",
            "template": {"packs": [{"name": "fred", "type": "spectro", "version": "1.0.0",
                "registry": {"metadata": {"uid": "", "name": "Public Repo"}}}]}}});
        let c = to_candidate(&doc).unwrap();
        assert_eq!(c.kind, "cluster");
        assert_eq!(c.layers[0].registry.as_deref(), Some("Public Repo"));
        assert_eq!(c.layers[0].registry_uid, None);
        assert_eq!(c.layers[0].values.as_deref(), Some(""));
    }
}
