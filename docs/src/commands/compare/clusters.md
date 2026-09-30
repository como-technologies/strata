# Two clusters

Two cluster records, a cluster against the update that waits for it, or a
cluster group against a member. [strata compare](../compare.md) gives the
other questions, the notes, what to expect, and the options.

## Compare two clusters by more than the release

Two cluster records that carry tiers are compared on each tier that both carry,
and on their labels. A `cluster tiers` line names the state of each, and a line
for each field that differs, as `<tier>.<path>`:

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

## Compare a cluster group with a member

When one side is a cluster group and the other is a `cluster:` ref that selects
no profile, the cluster side holds only the profiles that the group pins. The
question is whether the member runs the pin, and not what else it runs:

```text
{{#include ../../output/group-compare.txt}}
```

`tenant-a` runs one more profile of its own, and the comparison leaves it out.
A `cluster:<cluster>/<profile>` ref is not narrowed. See
[A cluster group pins a profile](../../use-cases/cluster-groups.md).
