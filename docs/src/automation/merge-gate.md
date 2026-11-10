# The merge gate

`scripts/ci/merge-gate.sh` answers one question: **what do the records look like
after the merge button?** It merges the target branch into the checkout first.
So every answer is about the merge result. It is not about the branch as it was
when somebody opened it.

```bash
scripts/ci/merge-gate.sh "releases/*.json" --base main
```

A branch that sat for a day is the problem. Another merge request moved the
record. The version that the record names changed in Palette. The comment on
the branch still shows yesterday's comparison. The merge is clean. The cluster
build gets content from outside the review of the team.

## The three checks

| Check | What it catches | Exit |
| --- | --- | --- |
| Conflict | The branch does not merge the target branch. The report compares the two records, so you read the conflict as a release. | 1 |
| Behind | The merge result pins an older version of the profile, or of a pack, than the target branch has, and no commit declares a rollback. | 1 |
| Verify | A record of the merge result does not verify against Palette, holds a pack with no upstream digest, or strata could not check it. | 1 |

### A branch that replaces a newer pin

A developer opened a branch when `dev` pinned `aws-stack@1.0.0`. Another merge
request then pinned `aws-stack@1.1.0`. The developer merged `main` and kept
their own record, which git accepts with no message:

```text
{{#include ../output/merge-gate-behind.txt}}
```

The fix is to pin the version that the target branch has, and put the change on
top. Then the gate is clear:

```text
{{#include ../output/merge-gate-clear.txt}}
```

### The order of the versions

The gate does not order versions itself. `strata compare` orders them as Palette
does, by Semantic Versioning. It writes the direction of each move as
`versionMove` in its JSON. A pre-release is below its version. So the move from
`1.0.1-g0123456789ab` to `1.0.1` is a move forward, and the move from `1.0.1` to
`1.0.1-g0123456789ab` is a move back. See
[The order of versions](../reference/compatibility.md#the-order-of-versions).

### A rollback, and a branch that did not look

Both move a pin backwards, and only the author knows which one it is. The gate
does not guess. It prints the evidence, and it asks the author to say so.

| Evidence | A branch that did not look | A rollback |
| --- | --- | --- |
| Did the target branch pin this release before? | no | yes, with the date and the commit |
| Did the branch write its record after the target branch moved? | no | yes |

A branch that puts back what `dev` ran before `1.1.0`, and says nothing:

```text
{{#include ../output/merge-gate-rollback-red.txt}}
```

Every line of the evidence says rollback, and the gate is still red, because
no commit declares it. The same change, declared in the commit that makes it:

```text
roll dev back to aws-stack@1.0.0

Rollback: releases/dev.json to aws-stack@1.0.0
1.1.0 repaved the nodes with a CSI driver that lost the volumes
```

The gate reads the declaration, and passes:

```text
{{#include ../output/merge-gate-rollback.txt}}
```

The line names the record and the release that the merge result pins, so a
declaration covers that rollback and no other change. A team that works in the
merge request puts the label `rollback` on it. Then the job runs with
`--allow-rollback`. The report shows both ways, and both pass.

Read the evidence as evidence, not as proof. A rebase replays an old change on
the new base. Then "the branch wrote this record after the target branch moved"
is true of a branch that never read what it replaces. The declaration
is the part that makes it a decision. It stays in the history of the
repository, where the next person can read why.

### A record that the published version no longer matches

The merge is clean and no pin moves back, but the gate is still red. A
person edited the profile that the record names in place, after strata wrote the
record. See [A published version is edited in place](../use-cases/edited-in-place.md).

```text
{{#include ../output/merge-gate-drifted.txt}}
```

`--no-verify`, or no API key where strata looks for one (`PALETTE_API_KEY`,
`SPECTROCLOUD_APIKEY`, or `~/.palette.env`), omits this check. Then the
last line says `not verified` and why, in place of the count of records that
did not verify. See [Settings](../reference/settings.md).

A strata that is older than the script writes no `versionMove`. Then no pin reads
as backward, and the gate is green. So the script stops with exit `2` before it
checks anything. Here `STRATA_BIN` names an older strata:

```text
{{#include ../output/merge-gate-older-strata.txt}}
```

The gate checks the record at the merge. A published version can change after
that. Run `strata verify` on the record again in the job that builds the
cluster.

## Options

| Option | Default | Function |
| --- | --- | --- |
| `--base BRANCH` | `CI_MERGE_REQUEST_TARGET_BRANCH_NAME`, or `GITHUB_BASE_REF` | The target branch. |
| `--remote NAME` | `STRATA_REMOTE`, or `origin` | The remote to fetch the target branch from. |
| `--no-verify` | | Omit the verify check. |
| `--allow-rollback` | | Accept a backwards pin, as the `rollback` label does. |
| `--allow-no-digest` | | Accept a pack with no upstream digest, as `verify --allow-no-digest` does. Without it, the gate names the record `UNVERIFIED`. |

The script is in this repository, at `scripts/ci/merge-gate.sh`, and every
release carries it beside the binary. It needs `strata` (or `STRATA_BIN`),
`git`, `jq`, and `palette-key.sh` beside it, which looks for the key. It
needs the history, so a job that clones with a depth gives it `GIT_DEPTH: 0` or
`fetch-depth: 0`. It writes to no host: the exit status is the gate, and
[the comment](./pr-comments.md) is what people read.

| Exit | Meaning |
| --- | --- |
| `0` | The merge result is clear. |
| `1` | The gate is red. The job fails, and the merge button waits. |
| `2` | The script could not run. It found no target branch, no git checkout, or no branch of that name on the remote. It also stops when no record matches or when strata could not read a record. It stops when strata is older than the script, because that strata writes no `versionMove`. |

A record that strata could not check is red, and not `2`. The report marks it
`ERROR`.

## Make the merge button wait

A git hook does not do this. `pre-commit` and `pre-push` run on the laptop of
one developer, and `--no-verify` skips them. They cannot see what the
target branch holds a minute before the merge. The gate belongs in the pipeline,
where the host holds the button.

`examples/ci/github/strata.yml` and `examples/ci/gitlab/strata.yml` hold this
gate and the other four jobs, ready to copy.

**GitHub.** Call the reusable workflow on `pull_request`. Then make the job a
required status check in the branch protection rule of the default branch.
Enable "Require branches to be up to date before merging". After a push to the
default branch, that setting blocks the merge until the pull request takes the
push. The update of the pull request runs the check again. No setting re-runs
the check on a push to the default branch itself.

A merge queue is the strongest form. It runs the gate on the merge result of the
queue. A queue run needs two things. Add `merge_group:` to the events of the
caller. Give the workflow the `base` input as well, for example `base: main`.
The queue names the base as a full ref, and the script does not read a full ref.

```yaml
on:
  pull_request:
  merge_group:                # a merge queue run; it needs the base input below
permissions:
  contents: read
jobs:
  gate:
    uses: como-technologies/strata/.github/workflows/merge-gate.yml@v0.32.2
    with:
      records: releases/**/*.json
      strata-version: v0.32.2
      base: main              # the default branch, for a merge queue run
    secrets:
      palette-api-key: ${{ secrets.PALETTE_API_KEY }}  # for the verify check
```

| Input | Required | Function |
| --- | --- | --- |
| `records` | yes | The records, or patterns, in the checkout of the caller. |
| `strata-version` | yes | The strata release, a tag of this repository. |
| `verify` | no | `false` omits the verify check. Default `true`. |
| `rollback-label` | no | The label that allows a backwards pin. Default `rollback`. |
| `allow-no-digest` | no | Accept a pack with no upstream digest, as `verify --allow-no-digest` does. Default `false`. |
| `base` | no | The branch to merge in, when no event names it. A merge queue run needs it. |
| `palette-api` | no | A self-hosted Palette address. |

The checkout of a pull request is already the merge result, so the gate finds
nothing to merge and checks what the button lands.

**GitLab.** Include [the pipeline](./gitlab.md), which runs `strata-merge-gate`
on each merge request pipeline. The variable `STRATA_ALLOW_NO_DIGEST` takes the
place of the input `allow-no-digest`: see
[the variables](./gitlab.md#the-variables). In Settings, Merge requests, turn on
"Pipelines must succeed". Merged results pipelines and merge trains run the pipeline on the
merge result, which is what this gate checks anyway.

See [A team pins each environment](../use-cases/fan-out.md) for the whole
pipeline: the comment, this gate, the record after the merge, and the scheduled
report.
