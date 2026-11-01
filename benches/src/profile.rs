//! The responses of Palette for a profile of any size, and the record of it.
//!
//! The packs are the real packs of [`crate::real`], taken in turn: the OS, Kubernetes, the
//! network, the storage, and an add-on, and then the next version of each. So the values have
//! the sizes and the shapes of the real values. A manifest is a config map of a few kilobytes.

use serde_json::{Value, json};
use strata::api::RawProfile;
use strata::snapshot::Snapshot;

use crate::real::{self, Pack};
use crate::{FETCHED_AT, routes};

/// The uid of the generated profile version.
pub const PROFILE_UID: &str = "64f0000000000000000000b1";

/// The name of the generated profile.
pub const PROFILE_NAME: &str = "bench-stack";

/// The version of the generated profile.
pub const PROFILE_VERSION: &str = "1.0.0";

/// The most packs, and the most manifests, that Palette takes in one profile.
pub const PALETTE_LIMIT: usize = 200;

/// The size of a profile.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Shape {
    /// How many packs the profile has.
    pub packs: usize,
    /// How many of the packs have one manifest: the first ones.
    pub manifests: usize,
}

impl Shape {
    /// A profile of `packs` packs and no manifest.
    pub const fn packs(packs: usize) -> Shape {
        Shape {
            packs,
            manifests: 0,
        }
    }

    /// The largest profile that Palette takes: 200 packs, and 200 manifests.
    pub const fn largest() -> Shape {
        Shape {
            packs: PALETTE_LIMIT,
            manifests: PALETTE_LIMIT,
        }
    }
}

/// What a profile changes against the profile of the same shape with [`Change::None`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Change {
    /// The same profile.
    None,
    /// The pack in the middle moves to the next version of its pack.
    OnePack,
    /// Each pack moves to the next version of its pack, and each manifest changes its content.
    EveryPack,
}

impl Change {
    /// True when the pack `index` of a profile of `shape` is not the pack of the same index in
    /// the profile with no change.
    fn moves(self, shape: Shape, index: usize) -> bool {
        match self {
            Change::None => false,
            Change::OnePack => index == shape.packs / 2,
            Change::EveryPack => true,
        }
    }
}

/// A pack of a profile, as `GET /v1/clusterprofiles/{uid}` lists it: the pack ref.
///
/// `name` is the name in the profile: the generator gives each pack a name of its own, so that
/// the key of each pack is one key.
pub fn pack_ref(real: &Pack, name: &str, manifest: Option<(&str, &str)>) -> Value {
    let manifests: Vec<Value> = manifest
        .map(|(name, uid)| json!({"name": name, "uid": uid}))
        .into_iter()
        .collect();
    json!({
        "name": name, "layer": real.layer, "type": "spectro", "tag": real.version,
        "version": real.version, "packUid": real.pack_uid, "registryUid": real.registry_uid,
        "digest": real.digest, "values": real.values, "manifests": manifests, "params": {},
        "presets": [],
    })
}

/// The config map that a manifest holds. `edited` changes one value of it.
fn manifest_content(index: usize, edited: bool) -> String {
    let mut text = format!(
        "apiVersion: v1\nkind: ConfigMap\nmetadata:\n  name: bench-{index:03}\n  namespace: platform\n\
         data:\n"
    );
    for key in 0..40 {
        let value = if edited && key == 0 {
            "edited"
        } else {
            "value"
        };
        text.push_str(&format!(
            "  key-{key:02}: {value}-{index:03}-{key:02}-0123456789\n"
        ));
    }
    text
}

/// The name of the pack `index` in the profile: the name of the real pack, and the place.
fn pack_name(real: &Pack, index: usize) -> String {
    format!("{}-{index:03}", real.name)
}

/// The name and the uid of the manifest of the pack `index`.
fn manifest_ids(index: usize) -> (String, String) {
    (
        format!("manifest-{index:03}"),
        format!("64f00000000000000000{index:04}"),
    )
}

/// The real pack at the place `index`, moved to the next version when the profile changes it.
fn pack_at(shape: Shape, change: Change, index: usize) -> &'static Pack {
    let families = real::families();
    let family = &families[index % families.len()];
    let revision = index / families.len();
    family.at(revision + usize::from(change.moves(shape, index)))
}

/// The responses of Palette for the profile of `shape` with `change`: its document, its pack
/// manifests, and its variables. The same call gives the same responses.
pub fn raw_profile(shape: Shape, change: Change) -> RawProfile {
    let mut refs = Vec::with_capacity(shape.packs);
    let mut entries = Vec::with_capacity(shape.manifests);
    for index in 0..shape.packs {
        let real = pack_at(shape, change, index);
        let name = pack_name(real, index);
        let manifest = (index < shape.manifests).then(|| manifest_ids(index));
        refs.push(pack_ref(
            real,
            &name,
            manifest.as_ref().map(|(n, u)| (n.as_str(), u.as_str())),
        ));
        if let Some((manifest_name, uid)) = manifest {
            let content = manifest_content(index, change.moves(shape, index));
            entries.push(json!({
                "metadata": {"name": name, "uid": real.pack_uid},
                "spec": {
                    "name": name, "layer": real.layer, "version": real.version,
                    "digest": real.digest, "registryUid": real.registry_uid,
                    "manifests": [{
                        "name": manifest_name, "uid": uid,
                        "digest": format!("sha1:{}", content.len()), "content": content,
                    }],
                },
                "status": {},
            }));
        }
    }
    RawProfile {
        profile: document(Value::Array(refs)),
        manifests: json!({
            "metadata": {"name": PROFILE_NAME, "uid": PROFILE_UID},
            "spec": {"packs": entries},
        }),
        variables: variables(),
        variables_unavailable: false,
        project_uid: None,
        fetched_at: None,
        manifest_entities: None,
        resolved_values: None,
    }
}

/// The document of the generated profile, with `packs` as its published packs.
pub fn document(packs: Value) -> Value {
    routes::profile_document(
        PROFILE_UID,
        PROFILE_NAME,
        PROFILE_VERSION,
        routes::template("aws", "cluster", packs),
        None,
    )
}

/// The variables of the profile: ten definitions, as a team writes for a stack.
fn variables() -> Value {
    let definitions: Vec<Value> = (0..10)
        .map(|n| {
            json!({
                "name": format!("variable_{n}"), "displayName": format!("Variable {n}"),
                "defaultValue": format!("default-{n}"), "format": "string", "inputType": "text",
                "required": true, "hidden": false, "immutable": false, "isSensitive": false,
                "options": [],
            })
        })
        .collect();
    json!({ "variables": definitions })
}

/// The record of the profile of `shape` with `change`, built from its responses.
///
/// # Panics
///
/// When the record does not build. The responses are the generator's own, so a panic is a bug of
/// the generator, or a change of the record that the generator must follow.
pub fn snapshot(shape: Shape, change: Change) -> Snapshot {
    Snapshot::from_raw(&raw_profile(shape, change), "bench-stack@1.0.0", FETCHED_AT)
        .expect("the generated responses build a record")
}
