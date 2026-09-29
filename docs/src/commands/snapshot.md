# strata snapshot

Record a profile version, a cluster, an edge host, a cluster group, an app
profile, or a pack version in a snapshot file, or each cluster of a set in a
directory. `compare`, `same-release`, `verify`, and `behind` read the file
later.

The examples use the project `dev`, which holds `aws-stack@1.1.0` and
`aws-stack@1.1.1`. The cluster `payments` runs 1.1.1.

## Record a profile version

```text
{{#include ../output/snapshot-profile.txt}}
```

Keep the file with the release, for example at
`releases/<version>/<project>.json` in a repository. The name is your choice.

The file holds the root and one hash for each pack. It also holds the values in
their canonical form and as Palette gave them, the manifests, the variables, and
the clusters that used the version. [The hashes and the root](../internals/hashes.md)
explains the hashes.

```text
{{#include ../output/snapshot-record.txt}}
```

`source.projectUid` is the project. `verify` checks the record there, also
after a rename of the project. [JSON documents](../reference/documents.md#strata-snapshot1)
lists each field.

## Record the newest version

```text
{{#include ../output/snapshot-latest.txt}}
{{#include ../output/snapshot-latest-version.txt}}
```

`<name>@latest` is the highest version in natural order: `22.04` is above
`20.04`, and `1.30.6` is above `1.29.10`.

## Record what a cluster runs

```text
{{#include ../output/snapshot-cluster.txt}}
{{#include ../output/snapshot-cluster-source.txt}}
```

A cluster that runs a recorded version has the root of that record: here
`088136c929bd`, the root of `aws-stack@1.1.1`. `cluster:<cluster>` records every
profile that the cluster runs, in the order that strata gives them: see
[The order of the profiles of a cluster](../internals/palette/clusters.md#the-order-of-the-profiles-of-a-cluster).
`cluster:<cluster>/<profile>` records one. The record keeps the uid of the
cluster, so `verify` finds a renamed cluster.

`pack:<name>@<version>` records one pack version from a registry, with the
values that the registry ships. `verify` on that record reads the registry
again, so it says whether a published pack version still serves the content it
served. See [What changed between two pack versions](../use-cases/pack-versions.md).

`source.cluster.variables` holds the value that the cluster gives each
variable. The values are not part of the root. Strata writes no value for a
sensitive variable. See [Clusters give variables different values](../use-cases/cluster-values.md).

## More of the cluster than its release

A cluster record also keeps what Palette reports of the cluster beside its
profiles, in tiers. Each tier has its own hash, and none is part of the root:

| Tier | What it holds | From |
| --- | --- | --- |
| `applied` | `spcHash` and `spcInfraHash`: the hashes that Palette computes of the spec it applied to the cluster. `spcHash` is its own for each cluster, so between two clusters `compare` reads `spcInfraHash` alone | the cluster document |
| `running` | each pack that the cluster reconciled, with its version and profile uid | the cluster document, `status.packs` |
| `policy` | the OS patch policy, the lifecycle, the machine health, the timezone, the cluster resources, the RBAC, and the FIPS mode | the cluster document, `spec.clusterConfig` |
| `shape` | each machine pool, with its instance type, size, zones, labels, and taints; the region and the project; and the settings of a managed control plane, on EKS and GKE | the cloud config: one more request, with `--shape` |

The record also keeps the labels of the cluster, in `source.cluster.labels`.
A tier is in the record only when Palette gives the part of the document that
it reads, so a record names the tiers that it carries. The `shape` tier leaves
out the ids that Palette gives each cluster, such as its subnets and its VPC.

`compare` of two cluster records names each tier that differs, and each field in
it. See [Two clusters on one release differ](../use-cases/cluster-tiers.md).

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
[Cluster copies](../internals/palette/clusters.md#cluster-copies).

## What a waiting update changes

`cluster:<cluster>@pending` records the profiles that the cluster runs after it
takes the update that waits, and not the copies that it runs now. Compare the
two before anybody accepts the update:

```text
{{#include ../output/edited-pending-record.txt}}
{{#include ../output/edited-pending.txt}}
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
  [Cluster copies](../internals/palette/clusters.md#cluster-copies).

## Edge hosts, cluster groups, and app profiles

Three more objects of Palette hold profiles, and strata records each:

```text
{{#include ../output/edge-record.txt}}
{{#include ../output/group-record.txt}}
{{#include ../output/app-record.txt}}
```

- `edgehost:<host>` records the profiles bound to an edge host, as for a
  cluster. The record says `kind: edgeHost`. See
  [A profile runs on edge hosts](../use-cases/edge-hosts.md).
- `clustergroup:<group>` records the profiles that a cluster group pins for its
  virtual clusters, and names the virtual clusters in `members[]`. The record
  says `kind: clusterGroup`. See
  [A cluster group pins a profile](../use-cases/cluster-groups.md).
- `appprofile:<name>@<version>` records an app profile: each tier is a pack in
  the layer `app`, and the properties of a tier are its params. `source.type`
  is `app`. A tier has no digest. See
  [An app profile is updated in place](../use-cases/app-profiles.md).

An edge host and a cluster group give no variable values, so their records hold
the variable definitions of each profile, as a profile record does. So a record
of an edge host has the root of the profile bound to it.

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
{{#include ../output/macros-record.txt}}
{{#include ../output/macros-digests.txt}}
```

The record holds `sha256:` of each value, and not the value: a macro can hold
a secret. `unresolved` is a macro that Palette gives no value. The digests are
not part of the root. When the macro gets another value, the record of the
version still verifies, and names the macro:

```text
{{#include ../output/macros-verify-release.txt}}
```

`verify` on the cluster record reads the values again, and reports each macro
that has another value now:

```text
{{#include ../output/macros-verify-cluster.txt}}
```

`compare` of two cluster records with the digests names each macro that
differs, and exits `1`, also when the roots are equal. What Palette does with a
running cluster when a macro changes is in
[Macros](../internals/palette/profiles.md#macros).

## Record each cluster of a set

`tag:<key>=<value>` and `filter:<name>` name a set of clusters: the clusters
with a label, or the clusters that a saved tag filter selects, in each project
the key can read. `--dir DIR` writes one record for each:

```text
{{#include ../output/fleet-snapshot.txt}}
```

Each record is `<cluster name>.json`, or `<name>.<uid>.json` when two clusters of
the set have one name, and each cluster is read in its own project. `--prune`
removes each cluster record of the directory whose cluster the query no longer
selects, and leaves every other file:

```text
{{#include ../output/fleet-snapshot-again.txt}}
```

Exit `0`, or `2` when strata could not record a cluster: the line of that
cluster says why, and the other records are written. See
[Find the fleet from Palette](../use-cases/fleet.md).

## Record without the network

`--raw-dir` saves the API responses and the time of the read. `--from-raw`
builds the same snapshot from them later, with no network and no key. The two
options exclude each other. An optional ref after `--from-raw` fills
`source.ref`. For a directory of a cluster, a ref of the form
`cluster:<uid>/<profile>` also selects that one profile. Strata checks the ref
against nothing else in the directory. The record keeps the time of the read,
not the time of the rebuild, so `verify` reports the version as unchanged since
Palette answered:

```text
{{#include ../output/snapshot-raw.txt}}
```

`scope.json` keeps the project. A directory with no `scope.json` builds a
snapshot at tenant scope.

## Record a version before you publish it

`--from-candidate` builds a snapshot from a candidate file. See
[strata candidate](./candidate.md) and
[Review a change before you publish it](../use-cases/review.md).

## Where to keep a record

A record is a file. Strata reads a record and writes a record. Strata does not
decide where you keep a record. Two places are usual, and each one answers a
different question:

| Where | What it gives | What it costs |
| --- | --- | --- |
| The repository, beside the code that publishes | A review, an author, and a date for each release. `git log` on the file says which commit pinned a release. | The job that publishes must push. |
| An object store, beside the state of the pipeline | Every job that publishes already writes there, and nothing new needs a token that pushes. | The history is the store's, not the repository's. |

A record holds each pack's values, params, and manifests in full. Only the value
of a sensitive profile variable is left out. So a record of a profile whose pack
values or manifests hold a secret carries that secret. Keep that record in the
object store, and not in the repository.

The book uses two layouts. The use cases keep one record for each version, at
`releases/<version>/<project>.json`, and one for each cluster, at
`runs/<cluster>.json`. [A team pins each environment](../use-cases/fan-out.md)
keeps one record for each environment, at `releases/<environment>.json`. Strata
reads both.

`verify` takes a pattern, so a directory of records is one command:
`strata verify "releases/*.json" --summary`. Read them from the store into a
directory first when they live outside the repository.

## What to expect

- Exit `0`, or `2` on an error.
- `--macros` and `--shape` need a `cluster:` ref. `--raw-dir` takes a profile or
  a cluster: a record read back from the directory does not say `@pending`,
  edge host, cluster group, or app profile.
- Without `-o`, the snapshot goes to standard output.
- `-o releases/1.1.0/dev.json` makes the directory that the path names, so a
  release directory needs no `mkdir` first.
- When a profile version has no published template, strata reads its draft.
  `source.template` names the one that it read: `published`, `draft`, `applied`
  for a cluster, `candidate`, or `registry` for a pack record.

## Options

```text
{{#include ../output/help-snapshot.txt}}
```

`-o` (`--output`) writes the record to a file, and without it the record goes
to standard output. `--compact` writes it on one line, and `-q` (`--quiet`)
drops the progress messages.
