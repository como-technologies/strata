# The observer shows an edit in place

## The problem

A person edits a published profile version through the API, and the number of the
version stays. The scheduled verify job of your pipeline finds the edit at its next
run, and it opens an issue. Between the edit and that run, a new cluster can take
the edited content. See [A published version is edited in place](./edited-in-place.md).

The observer runs in the cluster and checks every 15 minutes. It also keeps a page
that answers three more questions: who made the edit, whether this cluster took it,
and which pins have newer versions. It does this with the `strata` commands that
the other pages show. It adds no rule.

In this example, `dev` has `aws-stack@1.0.0` and `aws-stack@1.1.0`. The cluster
`payments` runs `aws-stack@1.0.0` and the add-on profile `platform-addons@1.0.0`.
The cluster `ledger` runs `aws-stack@1.1.0`.

**Who runs this.** The observer runs it in the cluster, and a person opens the
page. [The observer](../automation/observer.md) gives the install.

<div class="playground-demo">

`just playground observer-2` serves this tenant on this machine, with no key and
no cost. `just observer-playground` also runs the observer against it, and serves
its page. An edit in place changes the tenant, so the page has two seeds.
`observer-1` is the tenant before the edit.

</div>

## 1. The pipeline records what the clusters run

The pipeline of the team records each profile that its clusters run, and it writes
the records into the cluster as Secrets. The observer reads them as files:
`aws-stack_1.0.0.json.gz`, `aws-stack_1.1.0.json.gz`, and
`platform-addons_1.0.0.json.gz`, with `meta.json`. See
[Install](../automation/observer.md#install).

At this time, nobody has edited a version. The page shows no finding.

## 2. A person edits the version

A person changes the CSI reclaim policy of `aws-stack@1.0.0` from `Delete` to
`Retain`, through the API. The cluster `payments` has not taken the update. The
next content cycle shows the edit:

```text
{{#include ../output/observer-once.txt}}
```

Exit `1`, as with `strata verify`. The finding has the kind `published-changed`.
It names the pack `csi/csi-aws-ebs`, the impact, and who made the write. The
`firstSeen` time is the end of the first cycle that found it.

`clusterCopy` of the profile is `same`: `payments` still runs the content that the
record holds. The edit reaches the cluster only when somebody accepts the update.
[Read the two verdicts as a pair](./edited-in-place.md#read-the-two-verdicts-as-a-pair)
gives the choices for each case.

The page also lists the pins that have newer versions. They are not a fault, and
the page keeps them neutral:

```text
{{#include ../output/observer-newer.txt}}
```

See [The registry has newer versions](./behind.md).

## 3. A cluster differs from its release

The cluster `ledger` runs `aws-stack@1.1.0`, and a person changed its copy on the
cluster. The observer for `ledger` checks each record, so it reports the edit of
`aws-stack@1.0.0` too:

```text
{{#include ../output/observer-ledger.txt}}
```

The finding `cluster-differs` says that the copy of the cluster differs from the
record. See [A cluster does not run its release](./cluster-release.md).

## 4. Read the page

A person opens the page with a port-forward:

```bash
kubectl -n strata-observer port-forward deploy/strata-observer 8080
```

The overview shows the finding, the newer versions, and what the check did not
cover. The page of the profile shows its two sides, its events, and its packs. The
page of the pack shows the old and the new value of the reclaim policy. [The API of the observer](../reference/observer-api.md) names
the routes that carry each of them.

## What to do

- Do not repair the edited version in place: that is a second edit. Publish a new
  version with the change through review, and move the clusters to it. See
  [What to do](./edited-in-place.md#what-to-do).
- Read `who` first. It names the person and the time of the write.
- When the version goes back to its record, the finding goes in the next content
  cycle. A new record, written after a review, ends the finding too.
- A cluster that differs needs its own decision: see
  [A cluster does not run its release](./cluster-release.md).

## Limits

- The observer sees what Palette holds. It does not see an image that a vendor
  rebuilds under one tag, an edit on the cluster with `kubectl`, or what the
  workload runs. See [What the observer does not see](../automation/observer.md#what-the-observer-does-not-see).
- The check is as old as the last content cycle. The page shows the time of that
  cycle, and it shows `stale` when the cycle is more than two periods old.
