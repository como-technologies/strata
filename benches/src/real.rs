//! The real packs of `examples/packs/`.
//!
//! The directory holds the answers of the Public Repo registry for five packs, as `just
//! examples-fetch` saved them. The generator reads the same files that `just examples` builds
//! the example profiles from.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use serde_json::Value;
use strata::json::{arr, str_at, text};

/// The packs of the directory, in the order of the layers of a cluster: the OS, Kubernetes, the
/// network, the storage, and an add-on.
const NAMES: [&str; 5] = [
    "ubuntu-aws",
    "kubernetes",
    "cni-cilium-oss",
    "csi-aws-ebs",
    "cert-manager",
];

/// One version of a real pack, as the registry lists it.
#[derive(Debug, Clone)]
pub struct Pack {
    /// The name of the pack.
    pub name: String,
    /// The layer of the pack: `os`, `k8s`, `cni`, `csi`, or `addon`.
    pub layer: String,
    /// The version of the pack.
    pub version: String,
    /// The uid that the registry gives this version.
    pub pack_uid: String,
    /// The uid of the registry.
    pub registry_uid: String,
    /// The digest that the registry gives this version.
    pub digest: String,
    /// The default values of this version, as YAML.
    pub values: String,
}

/// A real pack at each version that `examples/packs/` holds, oldest first.
#[derive(Debug, Clone)]
pub struct Family {
    /// The name of the pack.
    pub name: &'static str,
    /// Each version with values, oldest first, and each one time.
    pub versions: Vec<Pack>,
    /// `GET /v1/packs?filters=spec.name=<name>`: one summary for each version in the registry.
    pub summaries: Value,
    /// `GET /v1/packs/<name>/registries/<uid>`: the tags and the values of the pack.
    pub entity: Value,
}

impl Family {
    /// The version at `revision`, counted from the oldest. A count past the newest version
    /// starts again at the oldest.
    pub fn at(&self, revision: usize) -> &Pack {
        &self.versions[revision % self.versions.len()]
    }

    fn read(dir: &Path, name: &'static str) -> Family {
        let summaries = read_json(&dir.join(name).join("_summaries.json"));
        let entity = read_json(&dir.join(name).join("_entity.json"));
        let mut versions: Vec<Pack> = Vec::new();
        for summary in arr(&summaries, "") {
            // the registry lists a version that `fetch-examples.sh` kept with its values; the
            // listing of the others has none
            let values = str_at(summary, "/spec/values");
            let pack_uid = str_at(summary, "/metadata/uid");
            if values.is_empty() || versions.iter().any(|v| v.pack_uid == pack_uid) {
                continue;
            }
            versions.push(Pack {
                name: name.to_string(),
                layer: text(summary, "/spec/layer"),
                version: text(summary, "/spec/version"),
                pack_uid: pack_uid.to_string(),
                registry_uid: text(summary, "/spec/registryUid"),
                digest: text(summary, "/spec/digest"),
                values: values.to_string(),
            });
        }
        versions.sort_by(|a, b| strata::reference::version::compare(&a.version, &b.version));
        Family {
            name,
            versions,
            summaries,
            entity,
        }
    }
}

/// The directory of the real packs.
pub fn packs_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../examples/packs")
}

/// The real packs, read one time.
///
/// # Panics
///
/// When `examples/packs/` is not there or holds a file that is not JSON. A benchmark has no input
/// without it, and no benchmark can run.
pub fn families() -> &'static [Family] {
    static FAMILIES: OnceLock<Vec<Family>> = OnceLock::new();
    FAMILIES.get_or_init(|| {
        let dir = packs_dir();
        NAMES.iter().map(|name| Family::read(&dir, name)).collect()
    })
}

fn read_json(path: &Path) -> Value {
    let text = std::fs::read_to_string(path).unwrap_or_else(|e| {
        panic!(
            "{}: {e}; run the benchmarks from a full clone",
            path.display()
        )
    });
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}
