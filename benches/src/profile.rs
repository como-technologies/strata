//! The responses of Palette for a profile of any size, and the record of it.
//!
//! The packs are the real packs of [`crate::real`], taken in turn: the OS, Kubernetes, the
//! network, the storage, and an add-on. After five packs, the next version of each pack follows.
//! So the values have the sizes and the shapes of the real values. A manifest is a config map
//! of 40 keys.

use serde_json::{Value, json};
use strata::api::RawProfile;
use strata::snapshot::Snapshot;

use crate::real::{self, Pack};
use crate::routes::{self, PackFixture, manifest, variable};
use crate::{FETCHED_AT, PALETTE_LIMIT};

/// The uid of the generated profile version.
pub const PROFILE_UID: &str = "64f0000000000000000000b1";

/// The name of the generated profile.
pub const PROFILE_NAME: &str = "bench-stack";

/// The version of the generated profile.
pub const PROFILE_VERSION: &str = "1.0.0";

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

/// The pack `real` under the name `name` in the profile, as the fixtures state a pack.
///
/// The generator gives each pack a name of its own, so each pack has one key.
pub fn fixture<'a>(real: &'a Pack, name: &'a str) -> PackFixture<'a> {
    PackFixture::new(
        name,
        &real.layer,
        &real.version,
        &real.pack_uid,
        &real.registry_uid,
        &real.digest,
        &real.values,
    )
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

/// The manifest of the pack `index`: its config map, edited or not.
fn pack_manifest(index: usize, edited: bool) -> Value {
    let content = manifest_content(index, edited);
    manifest(
        &format!("manifest-{index:03}"),
        &format!("64f00000000000000000{index:04}"),
        &format!("sha1:{}", content.len()),
        &content,
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
        let pack = fixture(real, &name);
        let manifests: Vec<Value> = (index < shape.manifests)
            .then(|| pack_manifest(index, change.moves(shape, index)))
            .into_iter()
            .collect();
        refs.push(pack.pack_ref(&manifests));
        if !manifests.is_empty() {
            entries.push(pack.pack_manifests(manifests));
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
            variable(
                &format!("variable_{n}"),
                &format!("Variable {n}"),
                &format!("default-{n}"),
                false,
            )
        })
        .collect();
    json!({ "variables": definitions })
}

/// The record of the profile of `shape` with `change`, built from its responses.
///
/// # Panics
///
/// When the record does not build. The responses are the generator's own. So a panic is a bug of
/// the generator, or a change of the record that the generator must follow.
pub fn snapshot(shape: Shape, change: Change) -> Snapshot {
    Snapshot::from_raw(&raw_profile(shape, change), "bench-stack@1.0.0", FETCHED_AT)
        .expect("the generated responses build a record")
}
