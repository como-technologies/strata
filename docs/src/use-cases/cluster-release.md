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
| `payments` | `aws-stack@1.1.0`, then three add-on profiles at 1.0.0: `observability`, `payments-apps`, and `platform-addons` |
| `search` | `aws-stack@1.1.1` |
| `ledger` | `aws-stack@1.1.0`, with one value edited on the cluster |

**Who runs this.** A person at a terminal for steps 1 to 5, with a Palette key:
every step reads Palette. Step 6 has two commands with two actors. The job that
applies a cluster writes the record, and no recipe ships for that one: it is one
`strata snapshot` after the apply. The scheduled verify job then checks that
record with every other one: see [Drift issues](../automation/drift-issues.md),
which ships `scripts/ci/verify-report.sh`.

<div class="playground-demo">

`just playground cluster-release` serves this tenant on this machine,
with no key and no cost. Nothing on this page changes the tenant, so one seed
serves every step.

</div>

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
its cluster, with the type in the last column. `payments` runs four: the cluster
profile `aws-stack@1.1.0` and three add-on profiles. Many clusters run more
profiles than the release. Step 3 shows what the comparison does with them.
[Refs](../reference/refs.md) gives every form of a cluster ref.

## 2. A cluster that runs the release

```text
{{#include ../output/cluster-identical.txt}}
```

`IDENTICAL` and exit `0`: this cluster runs the recorded release. The three
add-on profiles of `payments` are not in the comparison.

## 3. A cluster that runs many profiles

The record holds one profile version. So the cluster side holds only the
profile that the version replaces: here `aws-stack`, the profile with the same
name. `--json` names the ref that the comparison reads:

```text
{{#include ../output/cluster-many-profiles.txt}}
```

`cluster:payments/aws-stack` is that ref. It selects the profile after the
slash. A version of an add-on profile answers for that profile in the same
way:

```text
{{#include ../output/cluster-addon.txt}}
```

A move to the next version changes only the packs of `aws-stack`. `--repave`
and `--validate` ask Palette about the same move. The add-on profiles give no
`ADDED` and no `REMOVED` line:

```text
{{#include ../output/cluster-move-asks.txt}}
```

Exit `1`, because the roots differ. The two `palette` lines are the answers of
Palette. To compare each profile of the cluster, compare two records of the
cluster. See
[Compare a profile version with a cluster](../commands/compare/clusters.md#compare-a-profile-version-with-a-cluster).

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

- Compare the record of the release with `cluster:<cluster>`. The cluster side
  holds the profile of the record, and not the other profiles of the cluster.
- Before you move a cluster that does not match its record, find why. Then move
  it to a recorded version.
- To see what a move changes on a cluster, compare the new record with the
  cluster. A pack the cluster does not have yet reads `REMOVED`.

## Palette behavior

See [What Palette does: cluster copies](../internals/palette/clusters.md#cluster-copies).
