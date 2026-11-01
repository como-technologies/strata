//! A candidate file of any size, and the registry that serves its packs.
//!
//! The layers are the real packs of [`crate::real`], taken in turn. The file names each layer by
//! pack and version, as a team writes one, and leaves the values to the registry.

use std::path::Path;

use strata::api::MemoryApi;
use strata::candidate::{Candidate, CandidateLayer};

use crate::real;
use crate::routes::serve_registry_pack;

/// The candidate of `layers` layers. It names the registry of the real packs for all layers.
pub fn candidate(layers: usize) -> Candidate {
    let families = real::families();
    let registry_uid = families[0].at(0).registry_uid.clone();
    Candidate {
        name: "bench-candidate".to_string(),
        version: "1.0.0".to_string(),
        kind: "cluster".to_string(),
        cloud_type: "aws".to_string(),
        registry: None,
        registry_uid: Some(registry_uid),
        layers: (0..layers)
            .map(|index| {
                let pack = families[index % families.len()].at(index / families.len());
                CandidateLayer {
                    name: pack.name.clone(),
                    version: pack.version.clone(),
                    layer: None,
                    kind: None,
                    tag: None,
                    registry: None,
                    registry_uid: None,
                    values: None,
                    values_file: None,
                    params: Default::default(),
                    manifests: Vec::new(),
                }
            })
            .collect(),
        variables: Vec::new(),
    }
}

/// Write the candidate of `layers` layers as YAML to `path`.
///
/// # Panics
///
/// When the file cannot be written.
pub fn write(layers: usize, path: &Path) {
    strata::candidate::write(&candidate(layers), path, None)
        .expect("the candidate file is written");
}

/// The registry that holds every real pack: the listing and the entity of each pack.
pub fn registry() -> MemoryApi {
    real::families()
        .iter()
        .fold(MemoryApi::new(), |api, family| {
            let registry_uid = &family.at(0).registry_uid;
            serve_registry_pack(
                api,
                family.name,
                family.summaries.clone(),
                family.entity.clone(),
                registry_uid,
            )
        })
}
