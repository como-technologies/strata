# Edge hosts, cluster groups, and app profiles

Three more objects of Palette hold profiles, and strata records each:

```text
{{#include ../../output/edge-record.txt}}
{{#include ../../output/group-record.txt}}
{{#include ../../output/app-record.txt}}
```

- `edgehost:<host>` records the profiles bound to an edge host, as for a
  cluster. The record says `kind: edgeHost`. See
  [A profile runs on edge hosts](../../use-cases/edge-hosts.md).
- `clustergroup:<group>` records the profiles that a cluster group pins for its
  virtual clusters, and names the virtual clusters in `members[]`. The record
  says `kind: clusterGroup`. With `--macros`, it also holds the digests of the
  macro values that Palette gives for the group: see [Macros](cluster.md#macros).
  See [A cluster group pins a profile](../../use-cases/cluster-groups.md).
- `appprofile:<name>@<version>` records an app profile: each tier is a pack in
  the layer `app`, and the properties of a tier are its params. `source.type`
  is `app`. A tier has no digest. See
  [An app profile is updated in place](../../use-cases/app-profiles.md).

The params of a tier also hold `installOrder`, the install order of the tier,
and `containerRegistryUid`, the registry of its image, when Palette gives them.
The author of the app profile sets each one, as for a property. So the root
covers them, and a change of the install order changes the root. The record
lists the tiers by `installOrder`. A tier with no `installOrder` is at 0, and
tiers of one order keep the order in which Palette lists them. So an app
profile whose tiers have no install order keeps the root of an older record.

An edge host and a cluster group give no variable values, so their records hold
the variable definitions of each profile, as a profile record does. So a record
of an edge host has the root of the profile bound to it.

## The config that a cluster group gives

A cluster group gives each of its virtual clusters a config. The config holds
the limits of CPU, memory, and storage, the endpoint, the Kubernetes
distribution, and values in YAML. The record of the group keeps the config in the `policy` tier of
`source.cluster.tiers`, from `spec.clustersConfig` of the group:

```text
{{#include ../../output/group-policy.txt}}
```

| Field | What it holds |
| --- | --- |
| `endpointType` | how a person reaches each virtual cluster: `LoadBalancer` or `Ingress` |
| `kubernetesDistroType` | the Kubernetes distribution of each virtual cluster |
| `limitConfig` | the upper limits of each virtual cluster, as Palette gives them: `cpuMilliCore`, `memoryMiB`, `storageGiB`, and `overSubscription`, and `cpu` and `memory` in the older units |
| `values` | the values that the group applies to each virtual cluster, in canonical form, as a pack keeps its values. Each password holds `********` |
| `valuesHash` | the hash of `values`, with the mask. A comment or a change of the key order does not change it |
| `hostClusters[]` | the `uid` and the `endpointConfig` of each host cluster of the group, sorted by uid |

Palette gives the values as a person writes them, with no mask, and the values
have no schema. So strata finds a secret by its key. Strata writes `********`
over each value whose key ends with `password`, in capitals or not, at each
depth. Strata does not mask an empty value, a map, a list, or a value that holds
a variable. Here the values of the group hold `database.password`:

```text
{{#include ../../output/group-policy-values.txt}}
```

The hash covers the mask, so a new password does not change the tier. This holds
for values with no canonical form too, such as a template with no quotes. The
record then holds `values` as `null`, and `valuesHash` is the hash of the
masked text. Such values with no password keep the hash that strata 0.27.0 gave.

Here the values of a group hold a template with no quotes and a password:

```text
{{#include ../../output/group-template-values.txt}}
```

A person gives the group a new password. The record still verifies:

```text
{{#include ../../output/group-template-verify.txt}}
```

If strata cannot mask a password in such values, a snapshot, `verify`, and
`compare` give the error `serialize` and exit `2`. A record taken again gives the
same error. Quote the template, or change the block scalar in the values of the
group.

A record of strata 0.27.0 or before of a group with a password has the hash of
the text with each password in clear. So its tier differs from a new record.
Take the record again when strata can mask the password.

A record of strata 0.19 or before holds each password in clear. `compare` and `verify`
write the mask over its values before they compare, so the record still
compares. A secret under another key, such as a password in a URL, stays in
clear.

The tier is not part of the root. `verify` of the record reads the group again
and names each field that changed. `compare` of two records of a group does the
same. Here the group gives each virtual cluster 4 cores in place of 6:

```text
{{#include ../../output/group-policy-verify.txt}}
```

The pin is the same, so the root is the same and the exit is `0`. A live run
of [cluster-group.sh](../../development/live-runs.md#cluster-groupsh) gives the
same report for the same change.
