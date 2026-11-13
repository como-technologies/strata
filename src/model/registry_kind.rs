//! The kind of a registry: `pack`, `helm`, or `oci`.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::closed_set::impl_closed_set;
use crate::model::pack_type::PackType;

/// A kind of registry, as the routes of Palette name it. The registry listing,
/// `GET /v1/registries/metadata`, names the kind of each registry with the same word.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum RegistryKind {
    /// A pack registry. It holds the packs of the type `spectro`.
    Pack,
    /// A Helm registry.
    Helm,
    /// An OCI registry.
    Oci,
}

// The segment of the kind in a route is the same text: `/v1/registries/{kind}/summary`.
impl_closed_set!(RegistryKind {
    Pack => "pack",
    Helm => "helm",
    Oci => "oci",
});

impl RegistryKind {
    /// The kind of the registry that holds a pack of the type `pack_type`: a pack registry holds
    /// the type `spectro`. None for a manifest pack, which no registry holds.
    pub(crate) fn of_pack_type(pack_type: PackType) -> Option<Self> {
        match pack_type {
            PackType::Spectro => Some(Self::Pack),
            PackType::Helm => Some(Self::Helm),
            PackType::Oci => Some(Self::Oci),
            PackType::Manifest => None,
        }
    }

    /// The type of a pack of a registry of the kind: `spectro` for a pack registry, else the
    /// word of the kind.
    pub(crate) fn pack_type(self) -> PackType {
        match self {
            Self::Pack => PackType::Spectro,
            Self::Helm => PackType::Helm,
            Self::Oci => PackType::Oci,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::closed_set::ClosedSet;

    #[test]
    fn a_kind_gives_the_type_of_its_packs_and_the_type_gives_the_kind() {
        assert_eq!(
            RegistryKind::of_pack_type(PackType::Spectro),
            Some(RegistryKind::Pack)
        );
        assert_eq!(
            RegistryKind::of_pack_type(PackType::Helm),
            Some(RegistryKind::Helm)
        );
        assert_eq!(RegistryKind::of_pack_type(PackType::Manifest), None);
        for &kind in RegistryKind::ALL {
            assert_eq!(RegistryKind::parse(kind.as_str()), Some(kind));
            assert_eq!(RegistryKind::of_pack_type(kind.pack_type()), Some(kind));
        }
        assert_eq!(RegistryKind::parse("spectro"), None);
    }
}
