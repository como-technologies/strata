//! The cloud types of Palette, and the rule for a pack that supports a cloud.
//!
//! Palette knows a fixed set of cloud types. Each other cloud type is a custom cloud, which a
//! tenant registers. A pack names the clouds that it supports in `spec.cloudTypes`, and a
//! profile names its cloud in `spec.published.cloudType`.
//!
//! The create of a profile version refuses a pack that does not support the cloud of the
//! profile, with the code `PackNotSupported`. The function `supports` is that rule. `packs
//! --cloud`, `behind`, and the check of a candidate use it. The book states it in "A pack that
//! supports a cloud". Each name of a cloud type that the code compares or sends is in this module.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::closed_set::{ClosedSet, impl_closed_set};

/// The cloud type `all`: each cloud. A pack with it supports each cloud, and a profile of it
/// takes each pack.
pub const CLOUD_ALL: &str = "all";

/// The cloud type `custom`: each custom cloud. A pack with it supports each cloud that Palette
/// does not know.
pub const CLOUD_CUSTOM: &str = "custom";

/// The cloud type of an edge cluster. The engine `edge` of `strata-preflight` is this cloud type.
pub const CLOUD_EDGE_NATIVE: &str = "edge-native";

/// The cloud type of a virtual cluster. The cluster listing leaves out each cluster of this
/// cloud type, and the search does too, unless a condition on `environment` asks for it.
pub(crate) const CLOUD_NESTED: &str = "nested";

/// A cloud type that Palette knows, or a custom cloud.
///
/// Palette knows the cloud types of [`Cloud::All`] to [`Cloud::Generic`]. [`Cloud::Openstack`]
/// is a cloud type that Palette knew. A record from then holds it, and the rule of a pack reads
/// it as a custom cloud. Each other cloud type is [`Cloud::Custom`], which has the text `custom`
/// and does not keep the name of the custom cloud.
///
/// [`Cloud::of`] reads the text of a cloud type, and [`ClosedSet::as_str`] writes it. The table
/// of the texts is below the enum, and `strata-preflight` reads its engines from it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum Cloud {
    /// `all`, each cloud.
    All,
    /// `aws`.
    Aws,
    /// `eks`, a managed cluster on AWS.
    Eks,
    /// `gcp`.
    Gcp,
    /// `gke`, a managed cluster on GCP.
    Gke,
    /// `azure`.
    Azure,
    /// `aks`, a managed cluster on Azure.
    Aks,
    /// `vsphere`.
    Vsphere,
    /// `edge-native`.
    EdgeNative,
    /// `maas`.
    Maas,
    /// `apache-cloudstack`.
    #[serde(rename = "apache-cloudstack")]
    Cloudstack,
    /// `baremetal`.
    Baremetal,
    /// `nested`, a virtual cluster.
    Nested,
    /// `generic`.
    Generic,
    /// `openstack`, which Palette knew. A record from then holds it.
    Openstack,
    /// A cloud type that Palette does not know: a custom cloud.
    Custom,
}

impl_closed_set!(Cloud {
    All => CLOUD_ALL,
    Aws => "aws",
    Eks => "eks",
    Gcp => "gcp",
    Gke => "gke",
    Azure => "azure",
    Aks => "aks",
    Vsphere => "vsphere",
    EdgeNative => CLOUD_EDGE_NATIVE,
    Maas => "maas",
    Cloudstack => "apache-cloudstack",
    Baremetal => "baremetal",
    Nested => CLOUD_NESTED,
    Generic => "generic",
    Openstack => "openstack",
    Custom => CLOUD_CUSTOM,
});

impl Cloud {
    /// The cloud of the cloud type `cloud_type`, as a pack, a profile, or a cluster names it.
    /// A cloud type that Palette does not know is [`Cloud::Custom`].
    pub fn of(cloud_type: &str) -> Cloud {
        Cloud::parse(cloud_type).unwrap_or(Cloud::Custom)
    }

    /// The cloud whose account this cloud uses. A managed cluster uses the account of its
    /// cloud: an `eks` cluster uses an `aws` account, a `gke` cluster a `gcp` account, and an
    /// `aks` cluster an `azure` account. Each other cloud uses its own.
    pub fn account(self) -> Cloud {
        match self {
            Cloud::Eks => Cloud::Aws,
            Cloud::Gke => Cloud::Gcp,
            Cloud::Aks => Cloud::Azure,
            other => other,
        }
    }

    /// True when Palette does not know this cloud type now: a custom cloud. A pack with the
    /// cloud type `custom` supports it.
    pub(crate) fn is_custom(self) -> bool {
        matches!(self, Cloud::Custom | Cloud::Openstack)
    }
}

/// True when a pack whose cloud types are `cloud_types` supports the cloud `cloud`.
///
/// This is the rule of the create of a profile version. Palette refuses a pack that breaks it
/// with the code `PackNotSupported`. A pack with no cloud type supports each cloud, and each
/// pack supports the cloud `all`. Each other pack supports the clouds of its cloud types. It
/// supports each cloud when one of them is `all`, and each custom cloud when one of them is
/// `custom`.
pub(crate) fn supports(cloud_types: &[String], cloud: &str) -> bool {
    let custom = Cloud::of(cloud).is_custom();
    cloud == CLOUD_ALL
        || cloud_types.is_empty()
        || cloud_types
            .iter()
            .any(|c| c == cloud || c == CLOUD_ALL || (custom && c == CLOUD_CUSTOM))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn clouds(names: &[&str]) -> Vec<String> {
        names.iter().map(|n| n.to_string()).collect()
    }

    #[test]
    fn a_pack_supports_a_cloud_by_the_rule_of_the_create_of_a_profile() {
        // each row: the cloud types of a pack, then whether it supports aws, a custom cloud, and
        // the cloud all
        let table: &[(&[&str], [bool; 3])] = &[
            (&[], [true, true, true]),
            (&[CLOUD_ALL], [true, true, true]),
            (&["aws"], [true, false, true]),
            (&["gcp", "aws"], [true, false, true]),
            (&["gcp"], [false, false, true]),
            (&[CLOUD_CUSTOM], [false, true, true]),
            (&["my-cloud"], [false, true, true]),
        ];
        for (types, [aws, custom, all]) in table {
            let types = clouds(types);
            assert_eq!(supports(&types, "aws"), *aws, "{types:?} on aws");
            assert_eq!(
                supports(&types, "my-cloud"),
                *custom,
                "{types:?} on my-cloud"
            );
            assert_eq!(supports(&types, CLOUD_ALL), *all, "{types:?} on all");
        }
        // Palette does not know openstack now, so a pack of custom supports it
        assert!(supports(&clouds(&[CLOUD_CUSTOM]), "openstack"));
        assert!(!supports(&clouds(&["aws"]), "openstack"));
    }

    #[test]
    fn a_managed_cluster_uses_the_account_of_its_cloud_and_each_other_cloud_uses_its_own() {
        assert_eq!(Cloud::Eks.account(), Cloud::Aws);
        assert_eq!(Cloud::Gke.account(), Cloud::Gcp);
        assert_eq!(Cloud::Aks.account(), Cloud::Azure);
        for cloud in [
            Cloud::Aws,
            Cloud::Gcp,
            Cloud::Azure,
            Cloud::EdgeNative,
            Cloud::Maas,
        ] {
            assert_eq!(cloud.account(), cloud);
        }
    }

    #[test]
    fn a_cloud_keeps_its_text_and_a_custom_cloud_reads_as_custom() {
        for cloud in Cloud::ALL {
            assert_eq!(Cloud::of(cloud.as_str()), *cloud);
        }
        assert_eq!(Cloud::of("my-cloud").as_str(), CLOUD_CUSTOM);
    }

    #[test]
    fn each_cloud_type_that_palette_knows_is_not_custom_and_each_other_one_is() {
        for name in [
            CLOUD_ALL,
            "aws",
            "eks",
            "gcp",
            "gke",
            "azure",
            "aks",
            "vsphere",
            CLOUD_EDGE_NATIVE,
            "maas",
            "apache-cloudstack",
            "baremetal",
            CLOUD_NESTED,
            "generic",
        ] {
            assert!(!Cloud::of(name).is_custom(), "{name}");
        }
        for name in ["openstack", "my-cloud", CLOUD_CUSTOM, ""] {
            assert!(Cloud::of(name).is_custom(), "{name}");
        }
        assert_eq!(Cloud::of("gke"), Cloud::Gke);
        assert_eq!(Cloud::of("aks"), Cloud::Aks);
    }
}
