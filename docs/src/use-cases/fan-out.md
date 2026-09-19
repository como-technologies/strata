# A team pins each environment

A dozen developers work in one repository. It holds one record for each
environment, and CI builds the clusters from what the default branch holds:

```text
releases/dev.json      releases/test.json      releases/stage.json      releases/prod.json
```

One record for each environment is the layout of this page. The blocks below
come from the pages that each step links to, and those pages keep one record
for each version, at `releases/<version>/<project>.json`. Strata reads a record
wherever it is: the layout is a choice of the team.

Each change is a merge request. Many are open at once, and each one was written
against the default branch as it was that morning. Four questions come up, and
strata answers each one in a job.

| Question | When | Job | What the team sees |
| --- | --- | --- | --- |
| What does this change do to the cluster? | each push to a merge request | `pr-comment.sh` | one comment, and a label on a repave |
| Is this merge request still right? | before the merge button | `merge-gate.sh` | a red check, with the reason |
| Did the environments get what the branch says? | every night | `verify-report.sh` | one issue per record that drifted |
| Which clusters carry this pack? | when a version is retired | `strata consumers` | the list, with the count |

## One record for each environment

A record is a snapshot of the profile version that the environment runs. The
merge request proposes the next one, as [a candidate
file](../reference/candidate-file.md) or as a record of a published version:

```bash
strata snapshot aws-stack@1.1.0 -o releases/dev.json
```

An environment moves forward when a merge request changes its record. Nothing
else changes a cluster, so the repository is the history of each environment.

## Each merge request carries its comparison

The comment job compares the record that the default branch has with what the
merge request proposes. [Review a change before you publish it](./review.md)
runs that job, and prints its command with the record names of that page:

```text
{{#include ../output/review-pr-comment.txt}}
```

The comment follows each push, and the label follows the impact: `repave` on a
change that replaces nodes. A reviewer reads the impact, not the diff of a JSON
file. See [Pull request comments](../automation/pr-comments.md), and
[GitLab CI](../automation/gitlab.md) for the same job as a merge request note.

## The merge button waits for the gate

A merge request that was right this morning is not right this evening. The
gate merges the default branch in first, and checks the merge result:

```text
{{#include ../output/merge-gate-behind.txt}}
```

The gate reads a git history, which the playground does not build. This block
comes from the repository that [The merge gate](../automation/merge-gate.md)
describes.

This branch would have pinned `aws-stack@1.0.0` over the `1.1.0` that another
merge request landed. Git merged it and gave no warning, because the developer
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
works in the merge request uses the `rollback` label instead.

The gate also catches what git cannot see: a record that Palette no longer
matches, because the version it names was edited in place. See
[The merge gate](../automation/merge-gate.md), which has the output and the
settings that hold the button on both hosts.

## After the merge, prove what shipped

The merge is not the release. Record the version that the environment now runs,
and make sure it is the release that was reviewed:

```text
{{#include ../output/review-same.txt}}
```

Exit `0`: the record of the published version and the candidate that the team
reviewed are one release. A promotion to the next environment asks the same
question of the records it passes through, in promotion order:

```bash
strata same-release releases/dev.json releases/test.json releases/prod.json
```

```text
{{#include ../output/promote-same.txt}}
```

Give the records in promotion order. See
[Promote a release to another project](./promote.md).

## Every night, verify

Palette is not a store that only the team writes to. A version can be edited in
place, a cluster can be changed by hand, and a profile can be deleted. The
scheduled job checks each record against Palette, and keeps one issue open per
record that drifted:

The schedule covers the cluster records as well as the release records, so it
gives both patterns:

````text
{{#include ../output/edited-report.txt}}
````

The issue closes itself on the first run where the record verifies again. See
[Drift issues](../automation/drift-issues.md).

## Across the fleet

| Question | Command |
| --- | --- |
| Do two environments run one release? | `strata same-release releases/dev.json releases/prod.json` |
| Do two clusters run one release? | `strata compare cluster:dev-a cluster:dev-b` |
| Which profiles carry a pack version? | `strata list --pack csi-aws-ebs` |
| Which clusters use this profile version? | `strata consumers aws-stack@1.1.0` |
| What changed between two pack versions? | `strata compare 'pack:Public Repo/csi-aws-ebs@1.63.1' 'pack:Public Repo/csi-aws-ebs@1.65.0'` |
| Which version does the registry offer now? | `strata packs csi-aws-ebs --latest` |

The root of a record is what holds the environments together. Two records with
one root are one release, whatever the names and the uids say. A report that
lists the root of each environment says what runs where, one line for each.

## The whole pipeline

| Host | Comment | Gate | Verify |
| --- | --- | --- | --- |
| GitHub | `comment.yml` on `pull_request` | `merge-gate.yml` as a required check | `verify.yml` on a schedule |
| GitLab | `strata-comment` | `strata-merge-gate`, with "Pipelines must succeed" | `strata-verify` on a pipeline schedule |

Both hosts run the same three scripts, so a team that moves between them keeps
the reports, the markers and the exit codes. See
[GitLab CI](../automation/gitlab.md).

A team that keeps one repository per pack, per profile and per cluster asks the
same questions one level up, where a publish opens the merge requests instead of
a person. See [A publish fans out to what pins it](./publish-fan-out.md).
