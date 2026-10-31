//! The rules of the candidate file: the load and the check of the file, before any request.

use std::path::Path;

use serde_json::Value;

use super::{
    Candidate, LayerKind, LoadedCandidate, LoadedLayer, MINTED_LAYER, RegistryRef, export, gaps,
};
use crate::error::{Error, Result};
use crate::snapshot::pack::params_problem;

fn pick_registry(
    uid: &Option<String>,
    name: &Option<String>,
    invalid: impl Fn(String) -> Error,
    whom: &str,
) -> Result<Option<RegistryRef>> {
    match (uid, name) {
        (Some(_), Some(_)) => Err(invalid(format!(
            "{whom} sets both registry and registryUid"
        ))),
        (Some(u), None) => Ok(Some(RegistryRef::Uid(u.clone()))),
        (None, Some(n)) => Ok(Some(RegistryRef::Name(n.clone()))),
        (None, None) => Ok(None),
    }
}

/// Read and check a candidate file, and read the files it points at. Problems here need no
/// network: a file that does not parse or breaks a rule is `CandidateInvalid`, and a values or
/// manifest file that cannot be read is `Io` naming that file.
///
/// A file with `spec.template.packs` is a profile export of Palette. Strata reads it as the
/// candidate that it holds. The page "The candidate file" of the book gives the field of the
/// export that gives each field of the candidate.
pub fn load(path: &Path) -> Result<LoadedCandidate> {
    let yaml = crate::file::read_file(path)?;
    let invalid = invalid_at(path);
    let doc: Value = serde_yaml::from_str(&yaml).map_err(|e| invalid(e.to_string()))?;
    let candidate: Candidate = if export::is_export(&doc) {
        export::to_candidate(&doc).map_err(invalid)?
    } else {
        // parse the text again, and not `doc`: an error of the YAML parser names the line
        serde_yaml::from_str(&yaml).map_err(|e| invalid(e.to_string()))?
    };
    check(candidate, path)
}

/// The `CandidateInvalid` error of the candidate file `path`, from its message.
pub(super) fn invalid_at(path: &Path) -> impl Fn(String) -> Error + Copy + '_ {
    move |message| Error::CandidateInvalid {
        path: path.to_path_buf(),
        rule: None,
        message,
    }
}

/// Check a candidate that is in memory, and read the files it points at, relative to the
/// directory of `path`. `path` names the candidate: in a problem, and as the ref of the record.
/// [`load`] reads a file and then does this; a caller that builds the candidate itself, as
/// [`Candidate::from_snapshot`] does, starts here and writes no file.
pub fn check(candidate: Candidate, path: &Path) -> Result<LoadedCandidate> {
    let invalid = invalid_at(path);
    if candidate.name.is_empty() || candidate.version.is_empty() {
        return Err(invalid("name and version must not be empty".to_string()));
    }
    if candidate.layers.is_empty() {
        return Err(invalid("a candidate needs at least one layer".to_string()));
    }
    let base = path.parent().unwrap_or(Path::new("."));
    let read = |rel: &str| crate::file::read_file(&base.join(rel));
    let top = pick_registry(
        &candidate.registry_uid,
        &candidate.registry,
        invalid,
        "the candidate",
    )?;
    let mut layers = Vec::with_capacity(candidate.layers.len());
    for (i, l) in candidate.layers.iter().enumerate() {
        let whom = format!("layer {} ({})", i + 1, l.name);
        let registry = match l.kind {
            None => Some(
                pick_registry(&l.registry_uid, &l.registry, invalid, &whom)?
                    .or_else(|| top.clone())
                    .ok_or_else(|| {
                        invalid(format!(
                            "{whom} has no registry; set registry or registryUid on the layer or \
                             at the top"
                        ))
                    })?,
            ),
            // Palette makes the pack of a manifest layer from its own manifest pack: no registry
            // holds it, and the top-level registry does not apply to it
            Some(LayerKind::Manifest) => {
                if l.registry.is_some() || l.registry_uid.is_some() {
                    return Err(invalid(format!(
                        "{whom} is a manifest layer and names a registry; remove registry and \
                         registryUid from it"
                    )));
                }
                if l.manifests.is_empty() {
                    return Err(invalid(format!(
                        "{whom} is a manifest layer with no manifest; give it one manifest or more"
                    )));
                }
                // Palette takes the layer from its own manifest pack, and drops the layer of the
                // request, so another layer gives a record that the published version never has
                if let Some(layer) = l.layer.as_deref().filter(|&x| x != MINTED_LAYER.as_str()) {
                    return Err(invalid(format!(
                        "{whom} is a manifest layer with layer {layer}; Palette gives each manifest \
                         pack the layer {MINTED_LAYER}, so remove layer or set it to \
                         {MINTED_LAYER}"
                    )));
                }
                None
            }
        };
        if let Some(problem) = params_problem(&l.params) {
            return Err(invalid(format!("{whom}: {problem}")));
        }
        let values = match (&l.values, &l.values_file) {
            (Some(_), Some(_)) => {
                return Err(invalid(format!("{whom} sets both values and valuesFile")));
            }
            (Some(v), None) => Some(v.clone()),
            (None, Some(f)) => Some(read(f)?),
            (None, None) => None,
        };
        let mut manifests = Vec::with_capacity(l.manifests.len());
        for m in &l.manifests {
            let content = match (&m.content, &m.content_file) {
                (Some(c), None) => c.clone(),
                (None, Some(f)) => read(f)?,
                _ => {
                    return Err(invalid(format!(
                        "manifest {} of {whom} needs exactly one of content and contentFile",
                        m.name
                    )));
                }
            };
            manifests.push((m.name.clone(), content));
        }
        layers.push(LoadedLayer {
            registry,
            values,
            manifests,
        });
    }
    gaps::check_file(&candidate).map_err(|broken| broken.at(path))?;
    Ok(LoadedCandidate {
        path: path.to_path_buf(),
        candidate,
        layers,
    })
}
