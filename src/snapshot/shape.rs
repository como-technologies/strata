//! The fields of the `shape` tier of a cluster record.
//!
//! The tier holds the machine pools and the cloud part of the cloud config of the cluster. Strata
//! reads them by the cloud type of the cluster.

use std::borrow::Cow;

use serde_json::{Map, Value, json};

use crate::api::RawCluster;
use crate::cloud::Cloud;
use crate::json::{arr, str_at, value_at};

/// The key of the shape fields that names the cloud type of the cluster. A record from before
/// strata read the fields by cloud type does not have it.
pub(super) const CLOUD_TYPE: &str = "cloudType";

/// The machine pool fields that a person chooses on each cloud. A cloud that does not give a
/// field has no such key in the tier. The ids that Palette gives each cluster, such as its
/// subnets and security groups, stay out.
pub(super) const POOL_FIELDS: &[&str] = &[
    "name",
    "isControlPlane",
    "useControlPlaneAsWorker",
    "instanceType",
    "size",
    "minSize",
    "maxSize",
    "azs",
    "labels",
    "taints",
    "additionalLabels",
    "additionalAnnotations",
    "additionalTags",
    "nodeRepaveInterval",
    "updateStrategy",
    "skipK8sUpgrade",
    "overrideKubeadmConfiguration",
    // the architecture of the nodes, `amd64` or `arm64`
    "machinePoolProperties",
];

/// The cloud part fields that a person chooses on each cloud.
pub(super) const CLOUD_FIELDS: &[&str] = &["overrideClusterAPIConfig"];

/// The key of the id that Palette gives each vSphere placement of a cluster.
const PLACEMENT_UID: &str = "uid";

/// The machine pool fields that a record from before strata read the fields by cloud type holds.
/// Do not change this list: `compare` and `verify` read an older record through it.
const LEGACY_POOL_FIELDS: &[&str] = &[
    "name",
    "isControlPlane",
    "useControlPlaneAsWorker",
    "instanceType",
    "size",
    "minSize",
    "maxSize",
    "azs",
    "labels",
    "taints",
    "additionalLabels",
    "additionalAnnotations",
    "additionalTags",
    "rootDeviceSize",
    "capacityType",
    "spotMarketOptions",
    "nodeRepaveInterval",
    "updateStrategy",
    "skipK8sUpgrade",
    "overrideKubeadmConfiguration",
    "amiType",
    "awsLaunchTemplate",
];

/// The cloud part fields that a record from before strata read the fields by cloud type holds.
/// Do not change this list.
const LEGACY_CLOUD_FIELDS: &[&str] = &[
    "region",
    "project",
    "sshKeyName",
    "controlPlaneLoadBalancer",
    "bastionDisabled",
    "endpointAccess",
    "addons",
    "managedClusterConfig",
    "overrideClusterAPIConfig",
];

/// The machine pool fields of the cloud `cloud`, in addition to [`POOL_FIELDS`].
///
/// Palette gives the cloud config of AKS in the model of Azure, and that of GKE in the model of
/// GCP. A cloud whose fields strata does not read by name gets the fields of an older record. So
/// the tier holds no fewer fields than before.
fn pool_fields(cloud: Cloud) -> &'static [&'static str] {
    match cloud {
        Cloud::Aws => &["rootDeviceSize", "capacityType", "spotMarketOptions"],
        // a managed node group: also the image family and the launch template of its nodes
        Cloud::Eks => &[
            "rootDeviceSize",
            "capacityType",
            "spotMarketOptions",
            "amiType",
            "awsLaunchTemplate",
        ],
        Cloud::Gcp | Cloud::Gke => &["rootDeviceSize", "subnet"],
        Cloud::Azure | Cloud::Aks => &[
            "osDisk",
            "osType",
            "osSku",
            "isSystemNodePool",
            "spotVMOptions",
        ],
        // the compute cluster, datastore, folder, resource pool, and network of the pool
        Cloud::Vsphere => &["placements"],
        // the edge hosts of the pool
        Cloud::EdgeNative => &["hosts", "osType"],
        Cloud::Maas => &["network", "resourcePool", "tags", "useLxdVm"],
        // the flavor, the image, and the root volume of each node, and the subnet of the pool
        Cloud::Openstack => &["flavorConfig", "image", "diskGiB", "subnet"],
        // Palette gives `instanceConfig` from the offering, so it stays out
        Cloud::Cloudstack => &["offering", "template", "networks"],
        Cloud::All | Cloud::Baremetal | Cloud::Nested | Cloud::Generic | Cloud::Custom => {
            LEGACY_POOL_FIELDS
        }
    }
}

/// The cloud part fields of the cloud `cloud`, in addition to [`CLOUD_FIELDS`]. The network that
/// Palette makes for each cluster, such as its VPC, stays out.
fn cloud_fields(cloud: Cloud) -> &'static [&'static str] {
    match cloud {
        Cloud::Aws => &[
            "region",
            "sshKeyName",
            "controlPlaneLoadBalancer",
            "bastionDisabled",
        ],
        // a managed control plane: also who reaches its API server, its managed add-ons, and
        // the encryption of its secrets
        Cloud::Eks => &[
            "region",
            "sshKeyName",
            "controlPlaneLoadBalancer",
            "bastionDisabled",
            "endpointAccess",
            "addons",
            "encryptionConfig",
        ],
        // on GKE, `managedClusterConfig` holds the settings of the managed control plane
        Cloud::Gcp | Cloud::Gke => &["region", "project", "network", "managedClusterConfig"],
        Cloud::Azure | Cloud::Aks => &[
            "location",
            "resourceGroup",
            "subscriptionId",
            "sshKey",
            "vnetName",
            "vnetResourceGroup",
            "vnetCidrBlock",
            "controlPlaneSubnet",
            "workerSubnet",
            "aadProfile",
            "apiServerAccessProfile",
            "infraLBConfig",
            "enablePrivateCluster",
        ],
        Cloud::Vsphere => &[
            "placement",
            "ntpServers",
            "staticIp",
            "sshKeys",
            "controlPlaneEndpoint",
        ],
        Cloud::EdgeNative => &[
            "controlPlaneEndpoint",
            "isTwoNodeCluster",
            "ntpServers",
            "overlayNetworkConfiguration",
            "staticIp",
            "sshKeys",
        ],
        Cloud::Maas => &["domain", "enableLxdVm", "ntpServers", "staticIp", "sshKeys"],
        Cloud::Openstack => &[
            "region",
            "project",
            "domain",
            "sshKeyName",
            "bastionDisabled",
            "network",
            "subnet",
            "nodeCidr",
            "dnsNameservers",
        ],
        Cloud::Cloudstack => &[
            "project",
            "zones",
            "sshKeyName",
            "controlPlaneEndpoint",
            "syncWithCKS",
        ],
        Cloud::All | Cloud::Baremetal | Cloud::Nested | Cloud::Generic | Cloud::Custom => {
            LEGACY_CLOUD_FIELDS
        }
    }
}

/// The fields of the shape tier: the cloud type of the cluster document `cluster`, and the fields
/// of its cloud config `cloud_config` that a person chooses on that cloud. The pools are in name
/// order.
pub(crate) fn fields(cluster: &Value, cloud_config: &Value) -> Value {
    let cloud_type = RawCluster::cloud_type(cluster);
    let cloud = Cloud::of(cloud_type);
    let mut pools: Vec<Value> = arr(cloud_config, "/spec/machinePoolConfig")
        .iter()
        .map(|p| pick(p, &[POOL_FIELDS, pool_fields(cloud)]))
        .collect();
    pools.sort_by(|a, b| str_at(a, "/name").cmp(str_at(b, "/name")));
    let part = value_at(cloud_config, "/spec/clusterConfig");
    json!({
        CLOUD_TYPE: cloud_type,
        "cloud": pick(part, &[CLOUD_FIELDS, cloud_fields(cloud)]),
        "pools": pools,
    })
}

/// The shape fields `a` and `b` of two records, in a form that compares. When one of them is from
/// before strata read the fields by cloud type, each keeps only the fields of that older record.
/// Two records of the current form compare as they are, so the fields are not copied.
pub(crate) fn comparable<'a>(a: &'a Value, b: &'a Value) -> (Cow<'a, Value>, Cow<'a, Value>) {
    let legacy = |v: &Value| v.get(CLOUD_TYPE).is_none();
    if legacy(a) || legacy(b) {
        (Cow::Owned(as_legacy(a)), Cow::Owned(as_legacy(b)))
    } else {
        (Cow::Borrowed(a), Cow::Borrowed(b))
    }
}

/// The shape fields `v` as a record from before strata read the fields by cloud type holds them.
fn as_legacy(v: &Value) -> Value {
    let pools: Vec<Value> = arr(v, "/pools")
        .iter()
        .map(|p| pick(p, &[LEGACY_POOL_FIELDS]))
        .collect();
    json!({
        "cloud": pick(value_at(v, "/cloud"), &[LEGACY_CLOUD_FIELDS]),
        "pools": pools,
    })
}

/// The keys of `v` that one of the lists `keys` names, without a null value, and without the uid
/// of a vSphere placement.
fn pick(v: &Value, keys: &[&[&str]]) -> Value {
    let mut out: Map<String, Value> = keys
        .iter()
        .flat_map(|k| k.iter())
        .filter_map(|k| v.get(*k).map(|x| (k.to_string(), x.clone())))
        .filter(|(_, x)| !x.is_null())
        .collect();
    if let Some(Value::Array(placements)) = out.get_mut("placements") {
        placements.iter_mut().for_each(without_placement_uid);
    }
    if let Some(placement) = out.get_mut("placement") {
        without_placement_uid(placement);
    }
    Value::Object(out)
}

fn without_placement_uid(placement: &mut Value) {
    if let Some(o) = placement.as_object_mut() {
        o.remove(PLACEMENT_UID);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_managed_cloud_reads_the_model_of_its_cloud_and_an_unknown_type_the_fields_of_an_older_record()
     {
        assert_eq!(pool_fields(Cloud::Aks), pool_fields(Cloud::Azure));
        assert_eq!(cloud_fields(Cloud::Aks), cloud_fields(Cloud::Azure));
        assert_eq!(pool_fields(Cloud::Gke), pool_fields(Cloud::Gcp));
        assert_eq!(cloud_fields(Cloud::Gke), cloud_fields(Cloud::Gcp));
        assert_ne!(cloud_fields(Cloud::Eks), cloud_fields(Cloud::Aws));
        for cloud in [
            Cloud::Custom,
            Cloud::Baremetal,
            Cloud::Nested,
            Cloud::Generic,
            Cloud::All,
        ] {
            assert_eq!(pool_fields(cloud), LEGACY_POOL_FIELDS, "{cloud:?}");
            assert_eq!(cloud_fields(cloud), LEGACY_CLOUD_FIELDS, "{cloud:?}");
        }
    }

    #[test]
    fn two_shape_records_of_the_current_form_compare_as_they_are_and_an_older_one_in_a_copy() {
        let current = json!({CLOUD_TYPE: "aws", "cloud": {"region": "us-east-1"}, "pools": []});
        let older = json!({"cloud": {"region": "us-east-1"}, "pools": []});
        let (a, b) = comparable(&current, &current);
        assert!(matches!((a, b), (Cow::Borrowed(_), Cow::Borrowed(_))));
        let (a, b) = comparable(&current, &older);
        assert!(matches!((a, b), (Cow::Owned(_), Cow::Owned(_))));
    }
}
