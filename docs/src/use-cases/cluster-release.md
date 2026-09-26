# A cluster does not run its release

## The problem

A record says which release a cluster must run. A cluster can stay on an old
version. A cluster can move to a new version too early. A person can also edit
the pack values on the cluster. Strata hashes the copy of a profile on a cluster
in the same way as the profile, so a cluster that runs a recorded release has
the root of the record.

In this example, `dev` has three clusters:

| Cluster | Runs |
| --- | --- |
| `payments` | `aws-stack@1.1.0`, then the add-on profile `platform-addons@1.0.0` |
| `search` | `aws-stack@1.1.1` |
| `ledger` | `aws-stack@1.1.0`, with one value edited on the cluster |

**Who runs this.** A person at a terminal for steps 1 to 5, with a Palette key:
every step reads Palette. Step 6 has two commands with two actors. The job that
applies a cluster writes the record, and no recipe ships for that one: it is one
`strata snapshot` after the apply. The scheduled verify job then checks that
record with every other one: see [Drift issues](../automation/drift-issues.md),
which ships `scripts/ci/verify-report.sh`.

**Follow along.** `just playground cluster-release` serves this tenant on this machine,
with no key and no cost. Nothing on this page changes the tenant, so one seed
serves every step.

Record the release first. Every step below compares a cluster against this file:

```text
{{#include ../output/cluster-release-record.txt}}
```

## 1. What each cluster runs

```text
{{#include ../output/cluster-applied.txt}}
```

`strata list clusters --applied` reads each cluster for the profiles it
applies. Start here, because the list gives you the two things the next steps
need.

**The ref of each cluster.** `cluster:64f1000000000000000000e1` is the ref of
`payments`. The name works in the same place, so `cluster:payments` is the same
cluster, and the steps below use the names because they read better.

**Which clusters run more than one profile.** The list puts each profile under
its cluster, with the type in the last column. `payments` runs two: the cluster
profile `aws-stack@1.1.0` and the add-on profile `platform-addons@1.0.0`. A ref
that names only the cluster covers every profile it runs, which step 3 shows. To
compare one of them, add the profile name to the ref after a slash:
`cluster:payments/aws-stack`. [Refs](../reference/refs.md) gives every form.

## 2. A cluster that runs the release

```text
{{#include ../output/cluster-identical.txt}}
```

`IDENTICAL` and exit `0`: this cluster runs the recorded release. The `/aws-stack`
of the ref kept the add-on profile out of the comparison.

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

Exit `1`. The cluster side reads `aws-stack@1.1.1 on cluster search`: it moved
early. The `consumers` line agrees: Palette does not list `search` for 1.1.0.
The rest of the report is what the move brought to the cluster: 1.1.1 declares a
required variable `platform_owner`, which 1.1.0 does not, and it adds a manifest
to `cert-manager`.

## 5. A cluster with an edit on the cluster

```text
{{#include ../output/cluster-edited.txt}}
```

Exit `1`. The versions are equal, but the value differs. The note gives two
causes: the version was edited in place, or somebody changed the values on the
cluster. To find which, verify the record of the version:

```text
{{#include ../output/cluster-edited-verify.txt}}
```

`IDENTICAL` and exit `0`: the version in Palette is the version you recorded. So
the version did not change, and the second cause is the one. Somebody changed
the values on `ledger`. Had this verify reported `DRIFT`, the first cause would
be the one: see
[A published version is edited in place](./edited-in-place.md).

## 6. Record what the cluster runs

```text
{{#include ../output/cluster-run-verify.txt}}
```

`verify` on a cluster record fetches the cluster again, with the same
selection. `OK` and exit `0`.

## What to do

- Select the profile with `cluster:<cluster>/<profile>`, unless the record
  covers each profile of the cluster.
- Before you move a cluster that does not match its record, find why. Then move
  it to a recorded version.
- To see what a move changes on a cluster, compare the new record with the
  cluster. A pack the cluster does not have yet reads `REMOVED`.

## Palette behavior

See [What Palette does: cluster copies](../internals/palette/clusters.md#cluster-copies).
