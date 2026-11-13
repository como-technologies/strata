//! The registries of a listing: each registry that the key can see, by name.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::api::PaletteApi;
use crate::api::registry::{Registries, registry_kind};
use crate::closed_set::Open;
use crate::error::Result;
use crate::json::text;
use crate::model::registry_kind::RegistryKind;

/// One registry that the key can see.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ListedRegistry {
    /// Registry name, for `--registry` of `packs` and for a `pack:` ref.
    pub name: String,
    /// Registry uid. A ref takes it in place of a name that two registries have.
    pub uid: String,
    /// The kind of the registry: `pack`, `helm`, or `oci`, or the text of a kind of a later
    /// Palette release. Absent when the listing names no kind.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<Open<RegistryKind>>,
}

/// The registries of `GET /v1/registries/metadata`, by name and then by uid.
///
/// # Errors
///
/// Each error of the request, also a 404 or a 405: the listing is the answer, so a Palette with
/// no listing gives no list.
pub(super) fn list_registries<A: PaletteApi>(
    registries: &mut Registries<'_, A>,
) -> Result<Vec<ListedRegistry>> {
    let mut listed: Vec<ListedRegistry> = registries
        .metadata()?
        .iter()
        .map(|it| ListedRegistry {
            name: text(it, "/name"),
            uid: text(it, "/uid"),
            kind: registry_kind(it),
        })
        .collect();
    listed.sort_by(|a, b| a.name.cmp(&b.name).then_with(|| a.uid.cmp(&b.uid)));
    Ok(listed)
}
