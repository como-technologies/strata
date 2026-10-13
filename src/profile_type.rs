//! The type of a profile: the four types of a cluster profile, `app` for an app profile, and
//! `pack` for a pack record.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// The `type` of a profile, as the `source` of a record names it.
///
/// Palette gives a cluster profile one of four types: `cluster`, `infra`, `add-on`, or `system`.
/// A record of an app profile has the type `app`, and a record of one pack version has the type
/// `pack`. Strata gives these two: neither is a cluster profile. A record keeps the text of the
/// type, and [`ProfileType::parse`] gives none for a text that is not one of the six. A
/// consumers report holds the type `app` of an app profile as this enum: strata gives it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum ProfileType {
    /// `cluster`: the core layers and the add-ons of a cluster.
    Cluster,
    /// `infra`: the core layers alone.
    Infra,
    /// `add-on`: add-ons alone.
    #[serde(rename = "add-on")]
    AddOn,
    /// `system`: a profile of the system of Palette.
    System,
    /// `app`: an app profile, whose tiers are its packs.
    App,
    /// `pack`: a record of one pack version from a registry.
    Pack,
}

impl ProfileType {
    /// The type as the documents name it: `cluster`, `infra`, `add-on`, `system`, `app`, or
    /// `pack`.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Cluster => "cluster",
            Self::Infra => "infra",
            Self::AddOn => "add-on",
            Self::System => "system",
            Self::App => "app",
            Self::Pack => "pack",
        }
    }

    /// The type that the documents name `text`, or none for another text.
    pub(crate) fn parse(text: &str) -> Option<Self> {
        match text {
            "cluster" => Some(Self::Cluster),
            "infra" => Some(Self::Infra),
            "add-on" => Some(Self::AddOn),
            "system" => Some(Self::System),
            "app" => Some(Self::App),
            "pack" => Some(Self::Pack),
            _ => None,
        }
    }

    /// True for a type whose profile builds a cluster. `cluster` and `infra` have the OS and
    /// the Kubernetes layers, so a cluster runs one of them before each add-on.
    pub(crate) fn builds_cluster(self) -> bool {
        matches!(self, Self::Cluster | Self::Infra)
    }

    /// True when `text` names a type that builds a cluster: see
    /// [`ProfileType::builds_cluster`]. False for a text that names no type.
    pub(crate) fn text_builds_cluster(text: &str) -> bool {
        Self::parse(text).is_some_and(Self::builds_cluster)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_profile_type_parses_and_prints_as_the_documents_name_it() {
        for kind in [
            ProfileType::Cluster,
            ProfileType::Infra,
            ProfileType::AddOn,
            ProfileType::System,
            ProfileType::App,
            ProfileType::Pack,
        ] {
            assert_eq!(ProfileType::parse(kind.as_str()), Some(kind));
            assert_eq!(
                serde_json::to_value(kind).unwrap(),
                serde_json::Value::from(kind.as_str()),
                "{kind:?}"
            );
        }
        assert_eq!(ProfileType::parse("addon"), None);
        assert_eq!(ProfileType::parse(""), None);
    }

    #[test]
    fn only_cluster_and_infra_build_a_cluster() {
        assert!(ProfileType::text_builds_cluster("cluster"));
        assert!(ProfileType::text_builds_cluster("infra"));
        for text in ["add-on", "system", "app", "pack", "", "other"] {
            assert!(!ProfileType::text_builds_cluster(text), "{text}");
        }
    }
}
