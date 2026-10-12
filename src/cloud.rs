//! The cloud types of Palette, and the rule for a pack that supports a cloud.
//!
//! Palette knows a fixed set of cloud types. Each other cloud type is a custom cloud, which a
//! tenant registers. A pack names the clouds that it supports in `spec.cloudTypes`, and a
//! profile names its cloud in `spec.published.cloudType`. The create of a profile version
//! refuses a pack that does not support the cloud of the profile, with the code
//! `PackNotSupported`. The function `supports` is that rule. `packs --cloud`, `behind`, and the
//! check of a candidate use it. The book states it in "A pack that supports a cloud".

/// The cloud type `all`: each cloud. A pack with it supports each cloud, and a profile of it
/// takes each pack.
pub const CLOUD_ALL: &str = "all";

/// The cloud type `custom`: each custom cloud. A pack with it supports each cloud that Palette
/// does not know.
pub const CLOUD_CUSTOM: &str = "custom";

/// A cloud type that Palette knows, or a custom cloud.
///
/// Palette knows the cloud types of [`Cloud::All`] to [`Cloud::Generic`]. [`Cloud::Openstack`]
/// is a cloud type that Palette knew: a record from then holds it, and the rule of a pack reads
/// it as a custom cloud. Each other cloud type is [`Cloud::Custom`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Cloud {
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

impl Cloud {
    /// The cloud of the cloud type `cloud_type`, as a pack, a profile, or a cluster names it.
    pub(crate) fn of(cloud_type: &str) -> Cloud {
        match cloud_type {
            CLOUD_ALL => Cloud::All,
            "aws" => Cloud::Aws,
            "eks" => Cloud::Eks,
            "gcp" => Cloud::Gcp,
            "gke" => Cloud::Gke,
            "azure" => Cloud::Azure,
            "aks" => Cloud::Aks,
            "vsphere" => Cloud::Vsphere,
            "edge-native" => Cloud::EdgeNative,
            "maas" => Cloud::Maas,
            "apache-cloudstack" => Cloud::Cloudstack,
            "baremetal" => Cloud::Baremetal,
            "nested" => Cloud::Nested,
            "generic" => Cloud::Generic,
            "openstack" => Cloud::Openstack,
            _ => Cloud::Custom,
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
/// pack supports the cloud `all`. Each other pack supports the clouds of its cloud types: each
/// cloud when one of them is `all`, and each custom cloud when one of them is `custom`.
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
            "edge-native",
            "maas",
            "apache-cloudstack",
            "baremetal",
            "nested",
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
