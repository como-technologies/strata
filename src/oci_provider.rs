//! The provider type of an OCI registry: `pack`, `helm`, or `zarf`.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::closed_set::impl_closed_set;

/// The provider type of an OCI registry, as `spec.providerType` of the OCI summary listing names
/// it, and as the pack ref of a profile gives it in its annotations.
///
/// Palette follows a new digest in a different way for each provider type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum OciProvider {
    /// An OCI registry of packs. Palette syncs it.
    Pack,
    /// An OCI registry of Helm charts. Palette syncs it only when its sync is on.
    Helm,
    /// An OCI registry of Zarf packages. Palette syncs it only when its sync is on.
    Zarf,
}

impl_closed_set!(OciProvider {
    Pack => "pack",
    Helm => "helm",
    Zarf => "zarf",
});
