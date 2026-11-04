# Two clusters on one release differ

## The problem

`compare` of two clusters says whether they run the same release. In an
incident, a person asks another question: what is different about these two
clusters? Two clusters can run one release and differ in their node pools, their
policy, their labels, or the pack version that each one reconciled.

Palette gives each of these in the cluster document, and in the cloud config of
the cluster. A cluster record keeps them in tiers, beside the release: see
[More of the cluster than its release](../commands/snapshot/cluster.md#more-of-the-cluster-than-its-release).

In this example, `payments` and `search` run `aws-stack@1.1.1`.

**Who runs this.** A person at a terminal during an incident, and a job that
keeps a record of each cluster.

<div class="playground-demo">

`just playground cluster-tiers-1` serves this tenant on this
machine, with no key and no cost. A person changes the clusters in step 3, so
this page has two seeds. Each section names the one it needs.

</div>

## 1. Record each cluster, with its shape

The `shape` tier costs one more request, so it needs `--shape`:

```text
{{#include ../output/tiers-record-payments.txt}}
{{#include ../output/tiers-record-search.txt}}
```

The record names each tier that it carries, with the hash of each, and the
labels of the cluster:

```text
{{#include ../output/tiers-show.txt}}
```

## 2. Compare the two records

```text
{{#include ../output/tiers-same-lines.txt}}
```

One release, and each tier is the same.

## 3. The clusters change

<div class="playground-demo">

`just playground cluster-tiers-2` is the same tenant after
four changes:

</div>

- The worker pool of `search` has three nodes and a taint.
- `search` has another timezone and one more label.
- A person added a user to the role binding of `search`.
- `payments` reconciled Kubernetes `1.36.1`, and its profile declares `1.36.2`.

Stop the first seed, and export what this one prints. Keep the records from
step 1.

`verify` of the record of `search` from step 1 names each change in the
`policy` tier. The verdict follows the release, so it is `OK`:

```text
{{#include ../output/tiers-verify-search-lines.txt}}
```

Record `search` again, and compare:

```text
{{#include ../output/tiers-record-search-again.txt}}
{{#include ../output/tiers-differ-lines.txt}}
```

The root is still `IDENTICAL`, and the exit is `0`: the release is the same.
`cluster tiers` names each tier that differs, and each field in it, as
`<tier>.<path>`. As JSON:

```text
{{#include ../output/tiers-differ-json.txt}}
```

## 4. A cluster behind its profile

`running` holds the pack version that the cluster reconciled. When it differs
from the version that the profile declares, each report on the cluster says so:

```text
{{#include ../output/tiers-behind-lines.txt}}
```

```text
{{#include ../output/tiers-behind-json.txt}}
```

`verify` of the record from step 1 reports the same change in the `running`
tier. The verdict follows the release, so it is `OK`:

```text
{{#include ../output/tiers-verify.txt}}
```

## What to do

- Record each cluster with `snapshot --shape`, and keep the records. A
  comparison of two records then answers "what is different", and not only "is
  the release the same".
- Treat a `running` difference from the profile as a cluster that has not
  finished an update, or that failed one: read its events in Palette.
- A tier difference does not change the exit of `compare` or `verify`. A job
  that must fail on one reads `clusterTiers` and `runningBehind` in the JSON.

## Palette behavior

The tiers come from `GET /v1/spectroclusters/{uid}`, and `shape` from
`GET /v1/cloudconfigs/{cloudType}/{configUid}`. See
[Cluster tiers](../internals/palette/clusters.md#cluster-tiers).
