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
| `policy` | the OS patch policy, the lifecycle, the machine health, the timezone, the FIPS mode, the role bindings, the namespaces, and how the cluster takes an update. See [The policy tier](#the-policy-tier) | the cluster document, and two more requests: the role bindings and the namespaces |
| `shape` | the cloud type; each machine pool, with its instance type, size, zones, labels, taints, and architecture; and the cloud part that a person chooses. See [The shape on each cloud](#the-shape-on-each-cloud) | the cloud config: one more request, with `--shape` |

A record of a cluster group carries a `policy` tier of its own: the config
that the group gives each virtual cluster. See
[The config that a cluster group gives](other-objects.md#the-config-that-a-cluster-group-gives).

The record also keeps the labels of the cluster, in `source.cluster.labels`.
A tier is in the record only when Palette gives the part of the document that
it reads, so a record names the tiers that it carries.

`compare` of two cluster records names each tier that differs, and each field in
it. See [Two clusters on one release differ](../../use-cases/cluster-tiers.md).
A tier difference does not change the exit of `compare` or `verify`.

## The policy tier

The `policy` tier holds the settings of the cluster:

| Field | What it holds | From |
| --- | --- | --- |
| `osPatch` | the OS patch policy | `spec.clusterConfig.machineManagementConfig.osPatchConfig` |
| `lifecycle` | whether a person paused the cluster | `spec.clusterConfig.lifecycleConfig` |
| `machineHealth` | the machine health checks | `spec.clusterConfig.machineHealthConfig` |
| `timezone` | the timezone of the scheduled tasks of the cluster | `spec.clusterConfig.timezone` |
| `fips` | the FIPS mode | `status.fips.mode` |
| `rbacs` | each role binding, with its `namespace`, `role`, `subjects`, and `type` | `GET /v1/spectroclusters/{uid}/config/rbacs` |
| `namespaces` | each namespace, with its `name`, `isRegex`, and `resourceAllocation` | `GET /v1/spectroclusters/{uid}/config/namespaces` |
| `updateWorkerPoolsInParallel` | `true` when Palette updates all the worker pools at one time, and `false` when it updates them one after the other | `spec.clusterConfig` |
| `controlPlaneHealthCheckTimeout` | how long Palette waits for a new control plane node to become ready, before it replaces the node | `spec.clusterConfig` |
| `clusterMetaAttribute` | the text that a person adds to the cluster as more data | `spec.clusterConfig` |
| `applySetting` | `DownloadAndInstall` when the cluster applies each update at once, and `DownloadAndInstallLater` when each update waits until a person applies it | `status.spcApply.actionType` |
| `agentUpgrades` | `paused` when a person stopped the upgrades of the agent of Palette on the cluster, and `allowed` when not | the annotation `spectroComponentsUpgradeForbidden` of the cluster |

```text
{{#include ../../output/tiers-policy.txt}}
```

A field that Palette does not give is `null`, empty, or `false`, and
`agentUpgrades` is `allowed`. The bindings
and the namespaces have no uid and no time. A binding is a set of subjects, so strata
sorts the subjects of each binding, and the bindings. When Palette answers
HTTP 404 for the bindings or the namespaces, the field is `null`, and not an
empty list.

A person can edit the subjects of a binding, or the quota of a namespace, and
the uid stays. The hash of the tier changes, and `compare` and `verify` name
the field. Here a person added the user `fred@example.com` to the binding of
the cluster `search`:

```text
{{#include ../../output/tiers-verify-search-lines.txt}}
```

A record that an older version of strata wrote holds `resources` and `rbac` in
place of `rbacs` and `namespaces`. They are the references to the bindings and
the namespaces, with their uids. When one of two records holds them, `compare` and
`verify` compare only `osPatch`, `lifecycle`, `machineHealth`, `timezone`, and
`fips`. So the older record still verifies. Record the cluster again to compare
all the fields.

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
| `openstack` | `flavorConfig`, `image`, `diskGiB`, `subnet` | `region`, `project`, `domain`, `sshKeyName`, `bastionDisabled`, `network`, `subnet`, `nodeCidr`, `dnsNameservers` |
| `apache-cloudstack` | `offering`, `template`, `networks` | `project`, `zones`, `sshKeyName`, `controlPlaneEndpoint`, `syncWithCKS` |
| another type | `rootDeviceSize`, `capacityType`, `spotMarketOptions`, `amiType`, `awsLaunchTemplate` | `region`, `project`, `sshKeyName`, `controlPlaneLoadBalancer`, `bastionDisabled`, `endpointAccess`, `addons`, `managedClusterConfig` |

On another type, the tier holds the fields that an older version of strata
recorded on each cloud. So the tier of that type holds no fewer fields than
before.

The tier does not hold the ids that Palette gives each cluster: `subnetIds`,
`additionalSecurityGroups`, `vpcId`, and the `uid` of a vSphere placement. It
does not hold `instanceConfig`, which Palette calculates from the instance
type, or on CloudStack from the offering. Here the cluster `lab` runs on
vSphere:

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

## Whether a repave waits

Palette can hold a repave of a cluster until a person approves it. A new value
of a macro in a field that repaves the cluster does this. While the repave
waits, Palette refuses an update of the profiles of the cluster, and a delete
of the cluster stays in `Deleting`. The record carries the repave in
`source.cluster`:

- `repavePending`: `true` when the cluster document says
  `status.repave.state: Pending`, and `false` when no repave waits.
- `pendingRepave`: only when a repave waits. `source` is what started the
  repave, such as `hubble` when Palette found the change. `since` is the time
  of the state. `reasons[]` has the `code`, the `message`, the `pack`, and the
  `keys` that differ, for each reason that Palette gives.

Strata reads `GET /v1/spectroclusters/{uid}/repave/status` only when a repave
waits. That answer holds the values of the pack on each side of the change.
The record keeps the names of the keys, and no value.

These fields are not part of the root: the copy that the cluster runs is the
same. `compare` and `verify` name the repave that waits:

```text
{{#include ../../output/repave-waits-verify-lines.txt}}
```

```text
{{#include ../../output/repave-waits-json.txt}}
```

What Palette does is in
[Macros](../../internals/palette/profiles.md#macros).

## What a waiting update changes

`cluster:<cluster>@pending` records the profiles that the cluster runs after it
takes the update that waits, and not the copies that it runs now. Compare the
two before anybody accepts the update:

```text
{{#include ../../output/edited-pending-record.txt}}
{{#include ../../output/edited-pending.txt}}
```

Here the update brings one change: the reclaim policy of the CSI pack. The
record says `pending: true`, and it holds no `updatesPending` and no
`repavePending`: these are statements about the cluster as it runs now. When no
update waits, the pending record has the root of the cluster.

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

`--macros` records the value that Palette gives each of them. It costs one
request. For a cluster, the value is the one that the cluster gets:

```text
{{#include ../../output/macros-record.txt}}
{{#include ../../output/macros-digests.txt}}
```

For a profile version, the value is the one that Palette gives at the scope of
the version, with no cluster. The record holds the digests in
`source.macroValues`:

```text
{{#include ../../output/macros-record-version.txt}}
{{#include ../../output/macros-digests-version.txt}}
```

For a cluster group, the value is the one that Palette gives for the group. The
record holds the digests in `source.cluster.macroValues`, as a cluster record
does:

```text
{{#include ../../output/macros-record-group.txt}}
{{#include ../../output/macros-digests-group.txt}}
```

The record holds `sha256:` of each value, and not the value: a macro can hold
a secret. `unresolved` is a macro that Palette gives no value. The digests are
not part of the root. When the macro gets another value, a record of the
version without the digests still verifies, and names the macro:

```text
{{#include ../../output/macros-verify-release.txt}}
```

`verify` on a record with the digests reads the values again, and reports each
macro that has another value now. A cluster record:

```text
{{#include ../../output/macros-verify-cluster.txt}}
```

A record of the version:

```text
{{#include ../../output/macros-verify-version.txt}}
```

A record of the cluster group:

```text
{{#include ../../output/macros-verify-group.txt}}
```

`compare` of two records with the digests names each macro that differs, and
exits `1`, also when the roots are equal. What Palette does with a
running cluster when a macro changes is in
[Macros](../../internals/palette/profiles.md#macros).
