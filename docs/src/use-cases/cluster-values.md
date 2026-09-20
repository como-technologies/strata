# Clusters give variables different values

## The problem

Profile variables are the input of a deployment. Each cluster gives each
variable a value. Two clusters on one release can give different values, and a
person can change a value on a cluster. The values are not part of the release,
so they do not change the root. But a team wants to see them.

In this example, `payments` and `search` run `aws-stack@1.1.1`. Each gives its own
team name to `platform_owner`.

**Who runs this.** A person at a terminal. The scheduled verify job does not
report a changed cluster value, so somebody must look.

**Follow along.** `just playground cluster-values` serves this tenant on this machine,
with no key and no cost.

## 1. Compare the clusters

```text
{{#include ../output/values-compare.txt}}
```

`IDENTICAL` and exit `0`: one release. The report has two rows about variables,
and they answer different questions. `variables` is the definitions the profile
holds, which are part of the release and are equal here. `cluster values` is the
value each cluster gives a variable, which is not part of the release. The
`cluster values CHANGED` block shows each value that differs, as
`<profile>.<variable>`. See [Report formats](../reference/formats.md).

## 2. The record keeps the values

Record the cluster first:

```text
{{#include ../output/cluster-values-record.txt}}
```

```text
{{#include ../output/values-record.txt}}
```

## 3. A value changes on the cluster

A person changes `platform_owner` on the cluster `payments` to `team-platform`,
in Palette. The record of the cluster keeps the cluster value from before that
change. The verdict counts each changed cluster value, and the report lists
them:

```text
{{#include ../output/values-verify.txt}}
```

Exit `0`: the release is the same. The verdict counts the changed values, and
the report lists each one.

## Sensitive variables

Strata never records the value of a sensitive variable, or a hash of it. The
record lists the variable with `sensitive: true`. So strata can show that a
sensitive variable was added or removed, but never that its value changed.

This covers profile variables. A pack's values and manifests go into the record
in full: see
[Where to keep a record](../commands/snapshot.md#where-to-keep-a-record).

## What to do

- Use `compare` between clusters to find a value that one cluster sets wrong.
- Do not treat a value change as drift. To keep a value fixed, make it part of
  the release: put it in the pack values, not in a variable.
- `verify` exits `0` for a changed cluster value, so the scheduled job opens no
  issue for it. To watch the values, read `records[].clusterVariables` from
  `strata verify --format json`.

## Palette behavior

A cluster holds its own value for each profile variable, and a person can change
one on the cluster. No live run records such a change: see
[What no run covers](../internals/palette.md#what-no-run-covers). What the runs
do show is that a cluster keeps its own copy of a profile, at
[Cluster copies](../internals/palette.md#cluster-copies).
