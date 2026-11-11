//! `behind`: the newer version of a profile that carries a newer version of a pin.
//!
//! A pipeline moves a change up three levels. A new pack version goes to a registry, a new profile
//! version pins it, and the definition of a cluster pins that profile version. So for each pin that
//! is behind, `behind` says which newer version of each profile that holds the pin pins a newer
//! version of the pack.
//!
//! No route of Palette names the profile versions that pin a version of a pack, and the profile
//! search takes no pack. So strata reads the template of each newer version from its document, as
//! `list --pack` does. It reads only the newer versions of a profile version that holds a pin that
//! is behind.

use std::collections::{BTreeMap, BTreeSet};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::documents::ProfileDocuments;
use super::newer::is_newer;
use super::profiles::{Held, ProfileKey, held_keys};
use super::{Check, Pin, PinKey, Pinned, ProfilePin};
use crate::api::{PaletteApi, recorded_packs};
use crate::error::{ErrorDocument, Result};
use crate::json::{str_at, text};
use crate::ops::scope_of;
use crate::reference::name_version;

/// A profile version that holds a pin that is behind, and the newer version of its profile that
/// carries a newer version of the pack.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PinCarrier {
    /// The profile name.
    pub profile: String,
    /// The version of the profile that the records hold.
    pub version: String,
    /// The uid of that version.
    pub uid: String,
    /// The newest of the newer versions of the profile that pins a newer version of the pack.
    /// Absent when no newer version does, and when strata could not read one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub carrier: Option<Carrier>,
    /// Why strata could not read the template of a version of the profile. The pin stays behind,
    /// and the exit does not change.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "Option<ErrorDocument>")]
    pub error: Option<Value>,
}

/// A newer version of a profile that pins a newer version of a pack.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Carrier {
    /// The version of the profile.
    pub version: String,
    /// The uid of that version.
    pub uid: String,
    /// The version of the pack that the template of that version pins.
    pub pack_version: String,
}

/// Give each pin of `pins` that is behind its carriers: one for each profile version of `held`
/// that a record of the pin holds, that has a newer version, and whose template holds the pack.
///
/// `documents` holds the documents of each scope that strata connected to. A profile version is in
/// the scope of its first record. A pin is in the scope of its own first record, so the two scopes
/// can differ.
pub(super) fn read_carriers<A: PaletteApi>(
    check: &Check<'_>,
    pins: &mut BTreeMap<PinKey, Pinned>,
    held: &BTreeMap<ProfileKey, Held>,
    documents: &mut BTreeMap<Option<String>, ProfileDocuments<'_, A>>,
) {
    for pinned in pins.values_mut().filter(|p| p.pin.behind) {
        let keys: BTreeSet<ProfileKey> = pinned
            .holders
            .iter()
            .filter_map(|&i| check.loaded.get(i))
            .flat_map(held_keys)
            .collect();
        for h in keys.iter().filter_map(|k| held.get(k)) {
            if !h.profile.behind {
                continue;
            }
            let scope = scope_of(&h.project).map(str::to_string);
            let Some(docs) = documents.get_mut(&scope) else {
                continue;
            };
            if let Some(found) = carrier_of(docs, &pinned.pin, &h.profile, check) {
                pinned.pin.carriers.push(found);
            }
        }
    }
}

/// The carrier of `pin` among the newer versions of `profile`, newest first. None when the
/// template of `profile` holds no pack of the name of the pin.
///
/// Strata stops at the first newer version that pins a version of the pack above the pin, by
/// [`is_newer`]. A read that fails stops the search too, and the answer gives its error.
fn carrier_of<A: PaletteApi>(
    docs: &mut ProfileDocuments<'_, A>,
    pin: &Pin,
    profile: &ProfilePin,
    check: &Check<'_>,
) -> Option<PinCarrier> {
    let mut found = PinCarrier {
        profile: profile.name.clone(),
        version: profile.version.clone(),
        uid: profile.uid.clone(),
        carrier: None,
        error: None,
    };
    match pack_version(docs, &profile.uid, &pin.name) {
        Ok(None) => return None,
        Ok(Some(_)) => {}
        Err(e) => {
            found.error = Some(e.to_json());
            return Some(found);
        }
    }
    check.progress.note(&format!(
        "reading which newer version of profile {} carries {} above {}",
        name_version(&profile.name, &profile.version),
        pin.name,
        pin.version
    ));
    for newer in &profile.newer {
        match pack_version(docs, &newer.uid, &pin.name) {
            Ok(Some(v)) if is_newer(&v, &pin.version, check.pre_release) => {
                found.carrier = Some(Carrier {
                    version: newer.version.clone(),
                    uid: newer.uid.clone(),
                    pack_version: v,
                });
                break;
            }
            Ok(_) => {}
            Err(e) => {
                found.error = Some(e.to_json());
                break;
            }
        }
    }
    Some(found)
}

/// The version of the pack `name` in the template of the profile version `uid`, from its document
/// in `docs`. None when the template holds no pack of that name. A template holds one pack of each
/// name: Palette refuses a pack name two times with `DuplicatePacksForbidden`.
fn pack_version<A: PaletteApi>(
    docs: &mut ProfileDocuments<'_, A>,
    uid: &str,
    name: &str,
) -> Result<Option<String>> {
    let doc = docs.get(uid)?;
    Ok(recorded_packs(doc)
        .iter()
        .find(|p| str_at(p, "/name") == name)
        .map(|p| text(p, "/version")))
}
