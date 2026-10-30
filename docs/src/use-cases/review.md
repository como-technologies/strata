# Review a change before you publish it

## The problem

A pull request changes a profile: here, four infrastructure packs get new
versions. The new profile version (1.1.0) is not in Palette yet. The reviewer
must see which layers change, and whether the change replaces nodes, before
anybody publishes it.
After the publication, the team must know that Palette has the version that the
reviewer approved.

## Who runs each step

Two people and two jobs share this page. Read the step headings: they name the
actor. The author writes one file. The comment job runs steps 2 to 4 and posts
the result. The reviewer reads that comment. The publish job proves what shipped.

| Step | Who | When | Needs `PALETTE_API_KEY` |
| --- | --- | --- | --- |
| 1 | the author of the change | once, in the pull request | no |
| 2 | the [comment job](../automation/pr-comments.md) | each push to the pull request | yes |
| 3 | the [comment job](../automation/pr-comments.md) writes it, and the reviewer reads it | each push to the pull request | no |
| 4 | the [comment job](../automation/pr-comments.md) | each push to the pull request | no |
| 5 | the publish job | after the merge | yes |

**Follow along.** `just playground review-1` serves this tenant on this machine,
with no key and no cost. The publication in step 5 changes the tenant, so this
page has two seeds. Each section names the one it needs, and each seed serves on
its own port.

The project `dev` has `aws-stack@1.0.0`, and a record of it is already in the
repository, because the job that published 1.0.0 wrote one. The block below
writes that record, so this page has one to compare against:

```text
{{#include ../output/review-record.txt}}
```

## 1. The author writes the candidate file

The pull request holds a candidate file. A layer with no values gets the default
values of its pack.

```text
{{#include ../output/review-candidate.txt}}
```

[The candidate file](../reference/candidate-file.md) gives each field. To start
from a published version, use [strata candidate](../commands/candidate.md).

## 2. The job builds its snapshot

```text
{{#include ../output/review-snapshot.txt}}
```

Strata finds each pack in its registry. This root is the root that the
published version will have.

## 3. The job compares it with the last release

```text
{{#include ../output/review-markdown.txt:1:2}}
```

The book renders what that command writes, so the block below is the comment as
a reviewer reads it on the pull request. Each pack block opens when you click it.

{{#include ../output/review-markdown.txt:3:}}

Read the impact column first. Two layers say `node repave`: this release
replaces each node. A collapsible block below the table lists each changed
value, as `-` and `+` lines.

To list only the layers that replace nodes:

```text
{{#include ../output/review-repave.txt}}
```

## 4. The job posts the comparison

```text
{{#include ../output/review-pr-comment.txt}}
```

`--dry-run` writes the comment to a file and posts nothing. The file holds the
markdown of step 3, under a marker on its first line:

```text
{{#include ../output/review-comment-head.txt}}
```

The marker is an HTML comment, so a reader of the pull request does not see it.
The script reads it to find its own comment and edit that one, instead of adding
a second comment on the next push.

Without `--dry-run`, the script posts the comment and adds the label `repave`.
A label can require a second approver. See
[Pull request comments](../automation/pr-comments.md).

## 5. The publish job proves it is the approved version

**Follow along.** `just playground review-2` is the same tenant after somebody
published 1.1.0. Stop the first seed, and export what this one prints. Keep
`candidate.json` from step 2.

Record the published version, and ask whether it is the release the reviewer
approved:

```text
{{#include ../output/review-published.txt}}
{{#include ../output/review-same.txt}}
```

`IDENTICAL`: Palette has the version that the reviewer approved.

## Ask Palette about the version

Before the publish, `snapshot --from-candidate --validate` asks Palette if it
accepts the packs of the candidate. See
[Ask Palette before you publish](../commands/candidate.md#ask-palette-before-you-publish).

Palette answers two more questions only for a version that it holds, so they
follow the publish. Does the move of a cluster to the version repave the nodes?
Does Palette accept the packs on that cluster? See
[Ask Palette if the change repaves](../commands/compare/repave.md) and
[Ask Palette if it accepts the packs](../commands/compare/validate.md).
[Step 3 of the everyday loop](../loop.md#3-the-publish) shows both.

`same-release` is the question "are these one release?", where `compare` is the
question "what differs?". It takes two or more records, in the order a release
moves through them, and passes when each pair holds the same content. It reads
past the differences that carry no content — another uid, another project,
another tag name for one build — and `compare` reports those as changes because
its job is to report every difference. Use `same-release` for a gate, and
`compare` to read what a gate rejected.

## Palette behavior

A published profile version has the same root as its candidate, because Palette
puts the registry digest on each pack reference. See
[What Palette does](../internals/palette/packs.md#the-pack-ref-digest).

## What to do

- Keep the candidate file in the repository of the profile.
- When step 5 fails, somebody changed the version between the review and the
  publication. Do not use the version. Find the cause.
