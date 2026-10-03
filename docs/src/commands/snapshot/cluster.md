# A cluster

`cluster:<cluster>` records every profile that a cluster runs, and
`cluster:<cluster>/<profile>` one of them. The record also carries the tiers of
the cluster, whether an update waits for it, and, with `--macros`, the digests
of its macro values. [strata snapshot](../snapshot.md) gives the other
subjects, what to expect, and the options.

## Record what a cluster runs

```text
{{#include ../../output/snapshot-cluster.txt}}
{{#include ../../output/snapshot-cluster-source.txt}}
```

A cluster that runs a recorded version has the root of that record: here
`088136c929bd`, the root of `aws-stack@1.1.1`. `cluster:<cluster>` records every
profile that the cluster runs, in the order that strata gives them: see
[The order of the profiles of a cluster](../../internals/palette/clusters.md#the-order-of-the-profiles-of-a-cluster).
`cluster:<cluster>/<profile>` records one. The record keeps the uid of the
cluster, so `verify` finds a renamed cluster.

`source.cluster.variables` holds the value that the cluster gives each
variable. The values are not part of the root. Strata writes no value for a
sensitive variable. See [Clusters give variables different values](../../use-cases/cluster-values.md).

## More of the cluster than its release

A cluster record also keeps what Palette reports of the cluster beside its
profiles, in tiers. Each tier has its own hash, and none is part of the root:

| Tier | What it holds | From |
| --- | --- | --- |
| `applied` | `spcHash` and `spcInfraHash`: the hashes that Palette computes of the spec it applied to the cluster. `spcHash` is its own for each cluster, so between two clusters `compare` reads `spcInfraHash` alone | the cluster document |
| `running` | each pack that the cluster reconciled, with its version and profile uid | the cluster document, `status.packs` |
| `policy` | the OS patch policy, the lifecycle, the machine health, the timezone, the cluster resources, the RBAC, and the FIPS mode | the cluster document, `spec.clusterConfig` |
| `shape` | the cloud type; each machine pool, with its instance type, size, zones, labels, taints, and architecture; and the cloud part that a person chooses. See [The shape on each cloud](#the-shape-on-each-cloud) | the cloud config: one more request, with `--shape` |

The record also keeps the labels of the cluster, in `source.cluster.labels`.
A tier is in the record only when Palette gives the part of the document that
it reads, so a record names the tiers that it carries.

`compare` of two cluster records names each tier that differs, and each field in
it. See [Two clusters on one release differ](../../use-cases/cluster-tiers.md).

## The shape on each cloud

Palette gives each cloud type its own cloud config. The `shape` tier reads the
fields of the cloud type of the cluster, and `cloudType` names that type. Each
machine pool holds its `name`, `isControlPlane`, `useControlPlaneAsWorker`,
`instanceType`, `size`, `minSize`, `maxSize`, `azs`, `labels`, `taints`,
`additionalLabels`, `additionalAnnotations`, `additionalTags`,
`nodeRepaveInterval`, `updateStrategy`, `skipK8sUpgrade`, and
`overrideKubeadmConfiguration`. `machinePoolProperties.archType` gives the
architecture of the nodes, `amd64` or `arm64`. The cloud part holds
`overrideClusterAPIConfig`.

Each cloud type adds these fields. A field that a cloud does not give is not
in the tier.

| Cloud type | Machine pool | Cloud part |
| --- | --- | --- |
| `aws` | `rootDeviceSize`, `capacityType`, `spotMarketOptions` | `region`, `sshKeyName`, `controlPlaneLoadBalancer`, `bastionDisabled` |
| `eks` | the fields of `aws`, `amiType`, `awsLaunchTemplate` | the fields of `aws`, `endpointAccess`, `addons`, `encryptionConfig` |
| `gcp`, `gke` | `rootDeviceSize`, `subnet` | `region`, `project`, `network`, `managedClusterConfig` |
| `azure`, `aks` | `osDisk`, `osType`, `osSku`, `isSystemNodePool`, `spotVMOptions` | `location`, `resourceGroup`, `subscriptionId`, `sshKey`, `vnetName`, `vnetResourceGroup`, `vnetCidrBlock`, `controlPlaneSubnet`, `workerSubnet`, `aadProfile`, `apiServerAccessProfile`, `infraLBConfig`, `enablePrivateCluster` |
| `vsphere` | `placements`: the compute cluster, the datastore, the folder, the resource pool, and the network of the pool | `placement`, `ntpServers`, `staticIp`, `sshKeys`, `controlPlaneEndpoint` |
| `edge-native` | `hosts`: the edge hosts of the pool; `osType` | `controlPlaneEndpoint`, `isTwoNodeCluster`, `ntpServers`, `overlayNetworkConfiguration`, `staticIp`, `sshKeys` |
| `maas` | `network`, `resourcePool`, `tags`, `useLxdVm` | `domain`, `enableLxdVm`, `ntpServers`, `staticIp`, `sshKeys` |
| another type | no more fields | no more fields |

The tier does not hold the ids that Palette gives each cluster: `subnetIds`,
`additionalSecurityGroups`, `vpcId`, and the `uid` of a vSphere placement. It
does not hold `instanceConfig`, which Palette calculates from the instance
type. Here the cluster `lab` runs on vSphere:

```text
{{#include ../../output/shape-vsphere-record.txt}}
{{#include ../../output/shape-vsphere.txt}}
```

A record that an older version of strata wrote has no `cloudType`. When one of
two records has none, `compare` and `verify` compare only the fields that the
older record holds. So the older record still verifies. Record the cluster
again to compare all the fields of its cloud.

## Whether an update waits

A cluster runs its own copy of each profile. When a person edits the profile in
place, the copy stays, and Palette tells the cluster that an update waits. The
record carries that statement in `source.cluster`:

- `updatesPending`: `true` when an update of a profile that the record covers
  waits, and `false` when none waits.
- `pendingUpdates[]`: for each change that waits, `profileUid`, `profile`, the
  `message` that Palette gives, such as `Manifest 'x' is updated in the pack
  'x(x)'`, and `since`.
- `updatesPendingError`: why strata could not read it. `updatesPending` is then
  absent, and the record is still taken.

These fields are not part of the root: they are Palette's statement about the
copy, and the copy is the same with or without them. `compare` and `list
clusters --applied` say when a cluster is behind. What Palette says for each
kind of change is in
[Cluster copies](../../internals/palette/clusters.md#cluster-copies).

## What a waiting update changes

`cluster:<cluster>@pending` records the profiles that the cluster runs after it
takes the update that waits, and not the copies that it runs now. Compare the
two before anybody accepts the update:

```text
{{#include ../../output/edited-pending-record.txt}}
{{#include ../../output/edited-pending.txt}}
```

Here the update brings one change: the reclaim policy of the CSI pack. The
record says `pending: true`, and it holds no `updatesPending`: that is a
statement about the copy that runs. When no update waits, the pending record
has the root of the cluster.

- Palette gives each profile of the cluster, and puts the profile as it is now
  in place of each profile that a waiting notification names. So the pending
  record covers the whole cluster, as a cluster record does.
- The manifests of each profile come from the profile: the cluster holds no
  copy of them yet.
- A profile that the cluster does not run yet has no variable values on the
  cluster, so the record holds none for it.
- `verify` of a pending record reads the update that waits again. Once the
  cluster takes the update, nothing waits, and the cluster runs the root of the
  pending record: a live run shows it. See
  [Cluster copies](../../internals/palette/clusters.md#cluster-copies).

## Macros

A macro is a name and a value that a tenant or a project defines. The values
and the manifests of a profile can read one as `{{.spectro.macro.<name>}}`, and
Palette gives it its value when it applies the profile. A record holds the text
with the name, so the root covers the name and not the value: an edit of a
macro does not move the root. Each report names the macros that the content
uses.

`--macros` records the value that Palette gives each of them, for one cluster.
It costs one request, and it needs a cluster ref:

```text
{{#include ../../output/macros-record.txt}}
{{#include ../../output/macros-digests.txt}}
```

The record holds `sha256:` of each value, and not the value: a macro can hold
a secret. `unresolved` is a macro that Palette gives no value. The digests are
not part of the root. When the macro gets another value, the record of the
version still verifies, and names the macro:

```text
{{#include ../../output/macros-verify-release.txt}}
```

`verify` on the cluster record reads the values again, and reports each macro
that has another value now:

```text
{{#include ../../output/macros-verify-cluster.txt}}
```

`compare` of two cluster records with the digests names each macro that
differs, and exits `1`, also when the roots are equal. What Palette does with a
running cluster when a macro changes is in
[Macros](../../internals/palette/profiles.md#macros).
