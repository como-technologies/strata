# A published version is edited in place

## The problem

Palette accepts an edit of a published profile version through the API. The
version keeps its uid and its number, and no new version appears. The
immutable-clusterprofiles flag of the Spectro Cloud provider stops the edit only
in the provider, at plan time. The API accepts the edit with the flag on or off.
So the version name `aws-stack@1.1.1` can mean two contents at two times.

A cluster that runs the version keeps its own copy. A cluster that you create
after the edit gets the edited content. An update that a person accepts on the
first cluster brings the edit there too.

In this example, `dev` has `aws-stack@1.1.1`, and the cluster `payments` runs it.

**Who runs this.** The scheduled verify job finds this, and a person at a
terminal works out what changed. The job is
[Drift issues](../automation/drift-issues.md).

**Follow along.** `just playground edited-in-place` serves this tenant on this machine,
with no key and no cost.

## 1. Record the version and the cluster

```text
{{#include ../output/edited-record.txt}}
```

```text
{{#include ../output/edited-verify-ok.txt}}
```

## 2. A person edits the version: verify reports drift

The person changes the CSI reclaim policy from `Delete` to `Retain`:

```text
{{#include ../output/edited-verify.txt}}
```

Exit `1`. The note says that both sides are the same uid and version: the
version was edited in place.

The same comparison, from the record to the live version:

```text
{{#include ../output/edited-compare.txt}}
```

## 3. The running cluster keeps its copy

```text
{{#include ../output/edited-cluster-kept.txt}}
```

The edit did not reach `payments`.

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

A person accepts the update on `payments`. Now its record drifts too:

```text
{{#include ../output/edited-cluster-updated.txt}}
```

## What to do

- Do not repair the edited version in place: that is a second edit. Publish a
  new version with the change through review, and move the clusters to it.
- Run `verify` over each record on a schedule. It is the only check that
  catches an edit through the API.
- The record holds the release that the reviewers approved. When Palette differs
  from the record, Palette holds the error.

## Palette behavior

See [What Palette does: edits in place](../internals/palette.md#edits-in-place)
and [cluster copies](../internals/palette.md#cluster-copies).
