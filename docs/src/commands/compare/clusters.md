# Two clusters

Two cluster records, a cluster against the update that waits for it, a profile
version against a cluster that runs many profiles, or a cluster group against a
member. [strata compare](../compare.md) gives the other questions, the notes,
what to expect, and the options.

## Compare two clusters by more than the release

`compare` compares two cluster records of one kind on each tier that both
carry, and on their labels. A `cluster tiers` line names the state
of each, and a line for each field that differs, as `<tier>.<path>`:

```text
{{#include ../../output/tiers-differ-lines.txt}}
```

A tier difference does not change `same`, and does not change the exit: the
root says whether the release is the same. `--json` carries each tier as
`clusterTiers`, and a pack whose reconciled version differs from its profile as
`runningBehind`. See
[More of the cluster than its release](../snapshot/cluster.md#more-of-the-cluster-than-its-release).

## Compare a cluster with the update that waits

`cluster:<cluster>@pending` is the cluster after it takes the update that waits.
Compare it with the cluster to see what the update changes before anybody
accepts it:

```text
{{#include ../../output/edited-pending.txt}}
```

See [What a waiting update changes](../snapshot/cluster.md#what-a-waiting-update-changes).

Add `--repave` to ask Palette whether the cluster repaves when it takes the
update:

```text
{{#include ../../output/edited-repave-lines.txt}}
```

See [Ask about the update that waits](repave.md#ask-about-the-update-that-waits).

## Compare a profile version with a cluster

A cluster can run many profiles, such as a cluster profile and the add-on
profiles of each team. One side can be one version of a cluster profile, and
the other a `cluster:` ref that selects no profile. Then the cluster side holds
only the profile that the version replaces. The question is whether the cluster
runs the version, and not what else it runs. Here `payments` runs four
profiles, and the record holds `aws-stack@1.1.0`:

```text
{{#include ../../output/cluster-identical.txt}}
```

Exit `0`: the cluster runs the release. The comparison reads `cluster:payments`
as `cluster:payments/aws-stack`. `--json` names that ref in `source.ref` of the
cluster side, here `b.ref`:

```text
{{#include ../../output/cluster-many-profiles.txt}}
```

The version replaces the profile that has its uid, else the profile that has its
name. A version of another name that builds a cluster replaces the profile that
builds the cluster. `--repave` and `--validate` send this move to Palette. So
the report and the answer of Palette are about the same profile. The other
profiles give no `ADDED` and no `REMOVED` line:

```text
{{#include ../../output/cluster-move-asks.txt}}
```

The version of an add-on that the cluster does not run replaces no profile,
because the move adds it. Then the cluster side holds each profile of the
cluster. A `cluster:<cluster>/<profile>` ref keeps its own selection, and two
clusters stay whole. To compare each profile of a cluster, compare two records
of the cluster. See
[A cluster does not run its release](../../use-cases/cluster-release.md).

## A side with many profiles

The heading names a side with two or more profiles by their count. The record
joins their names and their versions with `+`, and a reader cannot pair them.
Here strata records each profile of `payments`, compares the record with the
cluster now, and verifies the record:

```text
{{#include ../../output/cluster-all-profiles-lines.txt}}
```

The verdict of `verify` gives the count too. `strata list clusters --detail`
names each profile and its version. `--json` gives them in
`a.cluster.profiles` and `b.cluster.profiles`.

## Compare a cluster group with a member

When one side is a cluster group and the other is a `cluster:` ref that selects
no profile, the cluster side holds only the profiles that the group pins. The
question is whether the member runs the pin, and not what else it runs:

```text
{{#include ../../output/group-compare.txt}}
```

`tenant-a` runs one more profile of its own, and the comparison omits it.
A `cluster:<cluster>/<profile>` ref keeps its own selection. See
[A cluster group pins a profile](../../use-cases/cluster-groups.md).

The tiers of a group hold what the group gives its virtual clusters, so
`compare` of a group with a member compares no tier. Two records of one group
compare their `policy` tiers: see
[The config that a cluster group gives](../snapshot/other-objects.md#the-config-that-a-cluster-group-gives).
