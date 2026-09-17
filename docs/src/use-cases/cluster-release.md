# A cluster does not run its release

## The problem

A record says which release a cluster must run. A cluster can stay on an old
version, move to a new version too early, or get an edit of its pack values on
the cluster. Strata hashes the copy of a profile on a cluster in the same way as
the profile. So a cluster that runs a recorded release has the root of the
record.

In this example, `dev` has three clusters:

| Cluster | Runs |
| --- | --- |
| `payments` | `aws-pxk@1.1.0`, then the add-on profile `platform-addons@1.0.0` |
| `search` | `aws-pxk@1.1.1` |
| `ledger` | `aws-pxk@1.1.0`, with one value edited on the cluster |

The release record is `releases/1.1.0/dev.json`.

## 1. A cluster that runs the release

```text
{{#include ../output/cluster-identical.txt}}
```

`/aws-pxk` selects one profile of the cluster.

## 2. A cluster that runs two profiles

Without the selection, the snapshot covers each profile that the cluster runs.
The cluster side then holds the packs of both profiles:

```text
{{#include ../output/cluster-all-profiles.txt}}
```

`CHANGED` and exit `1`, with the add-on pack `ADDED`. The release itself did not
change: each of its five packs is `unchanged`. Select the profile, unless the
record covers each profile of the cluster.

## 3. A cluster on another version

```text
{{#include ../output/cluster-moved.txt}}
```

The cluster side reads `aws-pxk@1.1.1 on cluster search`: it moved early.

## 4. A cluster with an edit on the cluster

```text
{{#include ../output/cluster-edited.txt}}
```

The versions are equal, but the value differs. The note gives two causes. Run
`verify` on the record of the version to find which one: when `verify` passes,
the version did not change, so somebody edited the values on the cluster.

## 5. Record what the cluster runs

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
  cluster. The `-` lines are what the cluster does not have yet.

## Palette behavior

See [What Palette does: cluster copies](../internals/palette.md#cluster-copies).
