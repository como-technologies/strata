# A team pins each environment

## The problem

A dozen developers work in one repository. It holds one record for each
environment, and CI builds the clusters from what the default branch holds:

```text
releases/dev.json      releases/test.json      releases/stage.json      releases/prod.json
```

One record for each environment is the layout of this page. The blocks below
come from the pages that each step links to, and those pages keep one record
for each version, at `releases/<version>/<project>.json`. Strata reads a record
wherever it is: the layout is a choice of the team.

Each change is a pull request, which GitLab calls a merge request. This page
says pull request for both. Many are open at once, and each one was written
against the default branch as it was that morning. Five questions come up, and
strata answers each one in a job.

| Question | When | Job | What the team sees |
| --- | --- | --- | --- |
| What does this change do to the cluster? | each push to a pull request | `pr-comment.sh` | one comment, and a label on a repave |
| Is this pull request still right? | before the merge button | `merge-gate.sh` | a red check, with the reason |
| Did the environments get what the branch says? | every night | `verify-report.sh` | one issue per record that drifted |
| Which clusters use this profile version? | when a version is retired | `strata consumers` | the list, with the count |
| Did the environment get the release that was reviewed? | after each merge | the publish job: `snapshot`, then `same-release` | a failed job when the published version is not the reviewed one |

**Who runs this.** Jobs, almost entirely. A developer writes a record in a pull
request and reads a comment; the jobs below do the rest. Three of them are
drop-ins: [Pull request comments](../automation/pr-comments.md),
[The merge gate](../automation/merge-gate.md), and
[Drift issues](../automation/drift-issues.md). The retire question is
[The promotion gate and the retire gate](../automation/gates.md). The proof
after the merge goes in your own publish job, because the publish is yours.

<div class="playground-demo">

This page takes its blocks from the pages it links, so no one
page serves all of it. Each block below runs on the seed that its own page
names: `just playground review-1` for the comment,
`just playground review-2` and `just playground promote` for the proof after the
merge, and `just playground edited-in-place-2` for the night verify. The merge
gate reads a git repository, which the playground does not build.

</div>

## One record for each environment

A record is a snapshot of the profile version that the environment runs. The
pull request proposes the next one, as [a candidate
file](../reference/candidate-file.md) or as a record of a published version:

```bash
strata snapshot aws-stack@1.1.0 -o releases/dev.json
```

An environment moves forward when a pull request changes its record. Nothing
else changes a cluster, so the repository is the history of each environment.

## Each pull request carries its comparison

The comment job compares the record that the default branch has with what the
pull request proposes. [Review a change before you publish it](./review.md)
runs that job, and prints its command with the record names of that page:

```text
{{#include ../output/review-pr-comment.txt}}
```

The comment follows each push, and the label follows the impact: `repave` on a
change that replaces nodes. A reviewer reads the impact, not the diff of a JSON
file. See [Pull request comments](../automation/pr-comments.md), and
[GitLab CI](../automation/gitlab.md) for the same job as a pull request note.

## The merge button waits for the gate

A pull request that was right this morning is not right this evening. The
gate merges the default branch in first, and checks the merge result:

```text
{{#include ../output/merge-gate-behind.txt}}
```

The gate reads a git history, which the playground does not build. This block
comes from the repository that [The merge gate](../automation/merge-gate.md)
describes.

This branch would have pinned `aws-stack@1.0.0` over the `1.1.0` that another
pull request landed. Git merged it and gave no warning, because the developer
settled the conflict by keeping their own record. The cluster would have been
built one release back.

A rollback moves a pin backwards on purpose, and looks the same to git. The
gate reports the evidence of each backwards pin — whether the default branch
pinned that release before, and whether the branch wrote its record after the
default branch moved — and it passes when the commit that makes the change says
so:

```text
Rollback: releases/dev.json to aws-stack@1.0.0
1.1.0 repaved the nodes with a CSI driver that lost the volumes
```

The reason stays in the history, where the next person reads it. A team that
works in the pull request uses the `rollback` label instead.

The gate also catches what git cannot see: a record that the published version
no longer matches, because the version was edited in place. See
[The merge gate](../automation/merge-gate.md), which has the output and the
settings that hold the button on both hosts.

## After the merge, prove what shipped

The publish job does this, after each merge. The merge is not the release.
Record the version that the environment now runs, and make sure it is the
release that was reviewed:

```text
{{#include ../output/review-same.txt}}
```

Exit `0`: the record of the published version and the candidate that the team
reviewed are one release. A promotion to the next environment asks the same
question of the records it passes through, in promotion order:

```text
{{#include ../output/promote-same.txt}}
```

Give the records in promotion order. In the layout of this page that is
`strata same-release releases/dev.json releases/test.json releases/prod.json`.
See [Promote a release to another project](./promote.md).

## Every night, verify

The team is not the only writer of Palette. A person at the console can edit a
version in place, change a cluster by hand, or delete a profile. The scheduled
job checks each record against Palette, and keeps one issue open per record
that drifted.

The schedule covers the cluster records as well as the release records, so it
gives both patterns. A cluster record comes from the job that applies the
cluster, which runs `strata snapshot cluster:<name> -o runs/<name>.json` after
the apply; this page keeps only the environment records under `releases/`:

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

The issue closes itself on the first run where the record verifies again. See
[Drift issues](../automation/drift-issues.md).

## Across the fleet

| Question | Command |
| --- | --- |
| Do two environments run one release? | `strata same-release releases/dev.json releases/prod.json` |
| Do two clusters run one release? | `strata compare cluster:dev-a cluster:dev-b` |
| Which profiles carry this pack? | `strata list --pack csi-aws-ebs` |
| Which clusters use this profile version? | `strata consumers aws-stack@1.1.0` |
| What changed between two pack versions? | `strata compare 'pack:Public Repo/csi-aws-ebs@1.63.1' 'pack:Public Repo/csi-aws-ebs@1.65.0'` |
| Which version does the registry offer now? | `strata packs csi-aws-ebs --latest` |

The root of a record is what holds the environments together. Two records with
one root are one release, whatever the names and the uids say. A report that
lists the root of each environment says what runs where, one line for each.

## The whole pipeline

[GitLab CI](../automation/gitlab.md) names the comment job, the merge gate, and
the scheduled report on each host.

Both hosts also run the two gates on demand: see
[The promotion gate and the retire gate](../automation/gates.md). Every job runs
the same commands on either host, so a team that moves between them keeps the
reports, the markers and the exit codes.

## What to do

- Keep one record for each environment, and move an environment only with a
  pull request that changes its record.
- Run the comment job on each push, and make the gate a required check.
- Verify the records on a schedule, and keep one issue open for each record that
  drifted.
- Run `consumers` at the time of the delete, not before it.
- A team can keep one repository per pack, per profile, and per cluster. That
  team asks the same questions one level up, where a publish opens the pull
  requests. See [A publish fans out to what pins it](./publish-fan-out.md).
