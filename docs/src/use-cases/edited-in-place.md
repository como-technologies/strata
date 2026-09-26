# A published version is edited in place

## The problem

Palette accepts an edit of a published profile version through the API. The
version keeps its uid and its number, and no new version appears. The
`immutable-clusterprofiles` flag of the Spectro Cloud provider stops the edit only
in the provider, at plan time. The API accepts the edit with the flag on or off.
So the version name `aws-stack@1.1.1` can mean two contents at two times.

A cluster that runs the version keeps its own copy. A cluster that you create
after the edit gets the edited content. An update that a person accepts on the
first cluster brings the edit there too.

In this example, `dev` has `aws-stack@1.1.1`, and the cluster `payments` runs it.

**Who runs this.** The scheduled verify job finds this, and a person at a
terminal works out what changed. The job is
[Drift issues](../automation/drift-issues.md).

**Follow along.** `just playground edited-in-place-1` serves this tenant on this
machine, with no key and no cost. An edit in place changes the tenant, so this
page has three seeds. Each section names the one it needs, and each seed serves
on its own port.

## 1. Record the version and the cluster

```text
{{#include ../output/edited-record.txt}}
```

```text
{{#include ../output/edited-verify-ok.txt}}
```

## 2. A person edits the version: verify reports drift

**Follow along.** `just playground edited-in-place-2` is the same tenant after
the edit. Stop the first seed, and export what this one prints.

The person changed the CSI reclaim policy from `Delete` to `Retain`. Your
records from step 1 still hold the content from before it:

```text
{{#include ../output/edited-verify.txt}}
```

Exit `1`. The note says that both sides are the same uid and version: the
version was edited in place.

`--who` adds who made the edit, and when, from the audit log of Palette:

```text
{{#include ../output/edited-verify-who.txt}}
```

The record has what the content was, and Palette has who wrote it. See
[Who changed it](../commands/verify.md#who-changed-it), and what the log holds
for each writer at [The audit log](../internals/palette/profiles.md#the-audit-log).

The same comparison, from the record to the live version:

```text
{{#include ../output/edited-compare.txt}}
```

## 3. The running cluster keeps its copy

```text
{{#include ../output/edited-cluster-kept.txt}}
```

The edit did not reach `payments`. Palette says so too: the `note` line says
that an update waits for the cluster, and the listing marks the profile:

```text
{{#include ../output/edited-list-behind.txt}}
```

The cluster is behind its profile until it takes the update. A record of the
cluster says `updatesPending: true`: see
[Whether an update waits](../commands/snapshot.md#whether-an-update-waits).

Before anybody accepts the update, ask Palette if it replaces the nodes:

```text
{{#include ../output/edited-repave-lines.txt}}
```

The `palette` line is Palette's own answer for `payments`, and the level above
it is strata's hint. Here a change of the CSI values repaves nothing, and the
two agree. See
[Ask Palette if the change repaves](../commands/compare.md#ask-palette-if-the-change-repaves).

### Read the two verdicts as a pair

The release record and the cluster record answer different questions, and the
pair is the diagnosis. Run both before you act:

| `verify` of the release | `verify` of the cluster | What happened | What to do |
| --- | --- | --- | --- |
| `DRIFT` | `OK` | Palette changed, and the cluster has not taken the change | Fix Palette first. The cluster is safe until something updates it, and a new cluster gets the edit at once, as step 4 shows. |
| `DRIFT` | `DRIFT` | the change reached the cluster too | The cluster runs content that nobody reviewed. Step 6 shows this. |
| `OK` | `DRIFT` | the cluster differs from its record, and Palette does not | Somebody changed the cluster. See [A cluster does not run its release](./cluster-release.md). |

A changed cluster *value* is not in this table. `verify` exits `0` for it and
adds `1 variable value changed` to the verdict: see
[Clusters give variables different values](./cluster-values.md).

## 4. A new cluster gets the edit

The cluster `search` starts after the edit, on the same version:

```text
{{#include ../output/edited-two-clusters.txt}}
```

Two clusters on `aws-stack@1.1.1` run different content.

## 5. Check all records

```text
{{#include ../output/edited-bulk.txt}}
```

In a scheduled job, the report script opens one issue for each record that
drifted:

```text
{{#include ../output/edited-report.txt:1:3}}
```

The issue body is markdown, and the host renders it. This is what a person sees
in the issue:

{{#include ../output/edited-report.txt:4:38}}

The script then prints what it would do:

```text
{{#include ../output/edited-report.txt:40:}}
```

See [Drift issues](../automation/drift-issues.md).

## 6. The update reaches the old cluster

**Follow along.** `just playground edited-in-place-3` is the same tenant after
`payments` takes the update. Stop the second seed, and export what this one
prints.

A person accepts the update on `payments`. Now its record drifts too:

```text
{{#include ../output/edited-cluster-updated.txt}}
```

Exit `1`, and the same CSI value as step 2. Both verdicts now say `DRIFT`, which
is the second row of the table above: the cluster runs content that nobody
reviewed.

## What to do

- Do not repair the edited version in place: that is a second edit. Publish a
  new version with the change through review, and move the clusters to it.
- Run `verify` over each record on a schedule. It is the only check that
  catches an edit through the API.
- The record holds the release that the reviewers approved. When Palette differs
  from the record, Palette holds the error.

## Palette behavior

See [What Palette does: edits in place](../internals/palette/profiles.md#edits-in-place)
and [cluster copies](../internals/palette/clusters.md#cluster-copies).
