//! The type of a pack: `spectro`, `helm`, `oci`, or `manifest`.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::closed_set::impl_closed_set;

/// The `type` of a pack, as a pack reference of Palette names it, as a record keeps it, and as
/// `packs` lists it.
///
/// A pack from a registry has the type of its registry:
///
/// - `spectro` from a pack registry.
/// - `helm` from a Helm registry.
/// - `oci` from an OCI registry.
///
/// A `manifest` pack comes from no
/// registry: Palette makes it from the manifests of the layer. A tier of an app profile has a
/// type of its own, such as `container`. It is not a pack type: a record keeps the text, and
/// strata reads no pack type from it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum PackType {
    /// `spectro`: a pack of a pack registry.
    Spectro,
    /// `helm`: a chart of a Helm registry.
    Helm,
    /// `oci`: a pack or a chart of an OCI registry.
    Oci,
    /// `manifest`: the manifests of a layer, from no registry.
    Manifest,
}

impl_closed_set!(PackType {
    Spectro => "spectro",
    Helm => "helm",
    Oci => "oci",
    Manifest => "manifest",
});

#[cfg(test)]
mod tests {
    use super::*;
    use crate::closed_set::ClosedSet;

    #[test]
    fn a_text_that_is_no_pack_type_reads_as_none() {
        assert_eq!(PackType::parse("container"), None);
        assert_eq!(PackType::parse(""), None);
    }
}
