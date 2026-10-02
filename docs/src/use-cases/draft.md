# Review a draft before its publish

## The problem

A person edits a published profile version in the console, or a pipeline
updates a version and publishes it in a second step. Until the publish, the
change waits in the draft of the version. The version keeps its published
content, and each cluster keeps what it runs.

The publish changes the version in place, with no new number. So the team must
see what the publish changes before it occurs, and must know after it that the
version holds the draft that the reviewer approved.

In this example, `dev` has `aws-stack@1.1.1`, and a release record of it is in
the repository.

**Who runs this.** A person at a terminal, or the job that comments on a change
request. The reviewer reads the comparison.

**Follow along.** `just playground draft-1` serves this tenant on this machine,
with no key and no cost. The draft and the publish change the tenant, so this
page has three seeds. Each section names the one it needs, and each seed serves
on its own port.

## 1. The version has no draft

The release record of the version:

```text
{{#include ../output/draft-release.txt}}
```

`<ref>@draft` reads the draft of the version. A version with no draft exits `2`
with the error `no-draft`:

```text
{{#include ../output/draft-none.txt}}
```

## 2. A person edits the version, and does not publish

**Follow along.** `just playground draft-2` is the same tenant with the draft.
Stop the first seed, and export what this one prints.

A person sets the reclaim policy of the CSI pack to `Retain`, and does not
publish. Compare the version with its draft:

```text
{{#include ../output/draft-compare.txt}}
```

Exit `1`. The `note` line says that the right side is the draft, and the pack
lines show what the publish changes: one value of the CSI layer. The impact
line says what the publish does to a cluster that takes the update.

The version keeps its content while the draft waits. Its release record
verifies:

```text
{{#include ../output/draft-version-holds.txt}}
```

## 3. Record the draft that the reviewer approves

The record of the draft holds the content that the publish gives:

```text
{{#include ../output/draft-record.txt}}
```

`source.template` is `draft`, and `source.ref` keeps the `@draft`:

```text
{{#include ../output/draft-template.txt}}
```

To change the draft in a change request, write it as a candidate file:

```text
{{#include ../output/draft-candidate.txt}}
```

The file holds the values and the manifests of the draft. See
[strata candidate](../commands/candidate.md).

## 4. After the publish, the version holds the draft

**Follow along.** `just playground draft-3` is the same tenant after the
publish. Stop the second seed, and export what this one prints.

The record of the draft verifies against the version:

```text
{{#include ../output/draft-published.txt}}
```

Exit `0`. The published version holds the content of the draft that the
reviewer approved. A change after the review gives `DRIFT`.

The release record from before the publish now reports `DRIFT`, because the
publish changed the version in place:

```text
{{#include ../output/draft-release-drift.txt}}
```

The publish empties the draft:

```text
{{#include ../output/draft-gone.txt}}
```

## What to do

- Before the publish, run `compare <ref> <ref>@draft`, and put the result in
  the review.
- Record the draft that the reviewer approves. After the publish, `verify` of
  that record proves that the version holds it.
- After the publish, record the version again as the new release record. The
  version keeps its number, so the old release record reports `DRIFT`.
- A publish is an edit in place. To keep each version fixed, publish a new
  version with the change: see
  [A published version is edited in place](./edited-in-place.md).

## Palette behavior

See [What Palette does: drafts](../internals/palette/profiles.md#drafts). The
live run is [`draft.sh`](../development/live-runs.md#draftsh).
