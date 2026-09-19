# A cluster does not run its release

## The problem

A record says which release a cluster must run. A cluster can stay on an old
version. A cluster can move to a new version too early. A person can also edit
the pack values on the cluster. Strata hashes the copy of a profile on a cluster in the same way as
the profile. So a cluster that runs a recorded release has the root of the
record.

In this example, `dev` has three clusters:

| Cluster | Runs |
| --- | --- |
| `payments` | `aws-stack@1.1.0`, then the add-on profile `platform-addons@1.0.0` |
| `search` | `aws-stack@1.1.1` |
| `ledger` | `aws-stack@1.1.0`, with one value edited on the cluster |

Record the release first. Every step below compares a cluster against this file:

```text
{{#include ../output/cluster-release-record.txt}}
```

## 1. What each cluster runs

```text
{{#include ../output/cluster-applied.txt}}
```

`strata list clusters --applied` reads each cluster for the profiles it
applies. Start here: the list gives you the cluster refs and tells you which
cluster needs `/aws-stack` to select one profile.

## 2. A cluster that runs the release

```text
{{#include ../output/cluster-identical.txt}}
```

`/aws-stack` selects one profile of the cluster.

## 3. A cluster that runs two profiles

Without the selection, the cluster side covers each profile that the cluster runs.
The cluster side then holds the packs of both profiles:

```text
{{#include ../output/cluster-all-profiles.txt}}
```

`CHANGED` and exit `1`, with the add-on pack `ADDED` and its impact. The release
itself did not change: each of its five packs is `unchanged`. Select the
profile, unless the record covers each profile of the cluster.

## 4. A cluster on another version

```text
{{#include ../output/cluster-moved.txt}}
```

The cluster side reads `aws-stack@1.1.1 on cluster search`: it moved early. The
`consumers` line agrees: Palette does not list `search` for 1.1.0.

## 5. A cluster with an edit on the cluster

```text
{{#include ../output/cluster-edited.txt}}
```

The versions are equal, but the value differs. The note gives two causes. Run
`strata verify` on the record of the version to find which one. When `verify`
passes, the version did not change. A person then changed the values on the
cluster.

## 6. Record what the cluster runs

```text
{{#include ../output/cluster-run-record.txt}}
```

`verify` on a cluster record fetches the cluster again, with the same
selection.

## What to do

- Select the profile with `cluster:<cluster>/<profile>`, unless the record
  covers each profile of the cluster.
- Before you move a cluster that does not match its record, find why. Then move
  it to a recorded version.
- To see what a move changes on a cluster, compare the new record with the
  cluster. A pack the cluster does not have yet reads `REMOVED`, as section 3
  shows for the add-on profile.

## Palette behavior

See [What Palette does: cluster copies](../internals/palette.md#cluster-copies).
