# The merge gate

`scripts/ci/merge-gate.sh` answers one question: **what do the records look like
after the merge button?** It merges the target branch into the checkout first,
so every answer is about the merge result, not about the branch as it was when
somebody opened it.

```bash
scripts/ci/merge-gate.sh "releases/*.json" --base main
```

A branch that sat for a day is the problem. Another merge request moved the
record. Palette changed the version that the record names. The comment on the
branch still shows yesterday's comparison, the merge is clean, and the cluster
build gets something that nobody reviewed.

## The three checks

| Check | What it catches | Exit |
| --- | --- | --- |
| Conflict | The branch does not merge the target branch. The report compares the two records, so you read the conflict as a release. | 1 |
| Behind | The merge result pins an older version of the profile, or of a pack, than the target branch has, and no commit declares a rollback. | 1 |
| Verify | A record of the merge result does not verify against Palette, or strata could not check it. | 1 |

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
merge request puts the label `rollback` on it, and the job runs with
`--allow-rollback`; both ways are reported, and both pass.

Read the evidence as evidence, not as proof. A rebase replays an old change on
the new base. Then "the branch wrote this record after the target branch moved"
is true of a branch that never read what it replaces. The declaration
is the part that makes it a decision, and it stays in the history of the
repository, where the next person can read why.

### A record that Palette no longer matches

The merge is clean and no pin moves back, and the gate is still red: the
profile that the record names was edited in place after the record was written.
See [A published version is edited in place](../use-cases/edited-in-place.md).

```text
{{#include ../output/merge-gate-drifted.txt}}
```

`--no-verify`, or no API key where strata looks for one (`PALETTE_API_KEY`,
`SPECTROCLOUD_APIKEY`, or `~/.palette.env`), leaves this check out. Then the
last line says `not verified` and why, in place of the count of records that
did not verify. See [Settings](../reference/settings.md).

The gate checks the record at the merge. Palette can change a published version
after that. Run `strata verify` on the record again in the job that builds the
cluster.

## Options

| Option | Default | Function |
| --- | --- | --- |
| `--base BRANCH` | `CI_MERGE_REQUEST_TARGET_BRANCH_NAME`, or `GITHUB_BASE_REF` | The target branch. |
| `--remote NAME` | `STRATA_REMOTE`, or `origin` | The remote to fetch the target branch from. |
| `--no-verify` | | Leave the verify check out. |
| `--allow-rollback` | | Accept a backwards pin, as the `rollback` label does. |

The script is in this repository, at `scripts/ci/merge-gate.sh`, and every
release carries it beside the binary. It needs `strata` (or `STRATA_BIN`),
`git`, `jq`, and `sort -V`. It
needs the history, so a job that clones with a depth gives it `GIT_DEPTH: 0` or
`fetch-depth: 0`. It writes to no host: the exit status is the gate, and
[the comment](./pr-comments.md) is what people read.

| Exit | Meaning |
| --- | --- |
| `0` | The merge result is clear. |
| `1` | The gate is red. The job fails, and the merge button waits. |
| `2` | The script could not run: no target branch, no git checkout, no branch of that name on the remote, no record matches, or strata could not read a record. |

A record that strata could not check is red, and not `2`. The report marks it
`ERROR`.

## Make the merge button wait

A git hook does not do this. `pre-commit` and `pre-push` run on the laptop of
one developer, and `--no-verify` skips them. They cannot see what the
target branch holds a minute before the merge. The gate belongs in the pipeline,
where the host holds the button.

`examples/ci/github/strata.yml` and `examples/ci/gitlab/strata.yml` hold this
gate and the other four jobs, ready to copy.

**GitHub.** Call the reusable workflow on `pull_request`, then make the job a
required status check in the branch protection rule of the default branch. Turn
on "Require branches to be up to date before merging". After a push to the
default branch, that setting blocks the merge until the pull request takes the
push, and the update of the pull request runs the check again. No setting re-runs
the check on a push to the default branch itself. A merge queue is the strongest
form: it runs the gate on the merge result of the queue. A queue run needs two
things. Add `merge_group:` to the events of the caller. Give the workflow the
`base` input as well, because the queue names the base as a full ref, which the
script does not read: `base: main`.

```yaml
on:
  pull_request:
  merge_group:                # a merge queue run; it needs the base input below
permissions:
  contents: read
jobs:
  gate:
    uses: como-technologies/strata/.github/workflows/merge-gate.yml@v0.12.0
    with:
      records: releases/**/*.json
      strata-version: v0.12.0
      base: main              # the default branch, for a merge queue run
    secrets:
      strata-token: ${{ secrets.STRATA_READ_TOKEN }}   # while this repository is private
      palette-api-key: ${{ secrets.PALETTE_API_KEY }}  # for the verify check
```

| Input | Required | Function |
| --- | --- | --- |
| `records` | yes | The records, or patterns, in the checkout of the caller. |
| `strata-version` | yes | The strata release, a tag of this repository. |
| `verify` | no | `false` leaves the verify check out. Default `true`. |
| `rollback-label` | no | The label that allows a backwards pin. Default `rollback`. |
| `base` | no | The branch to merge in, when no event names it. A merge queue run needs it. |
| `palette-api` | no | A self-hosted Palette address. |

The checkout of a pull request is already the merge result, so the gate finds
nothing to merge and checks what the button lands.

**GitLab.** Include [the pipeline](./gitlab.md), which runs `strata-merge-gate`
on each merge request pipeline. In Settings, Merge requests, turn on "Pipelines
must succeed". Merged results pipelines and merge trains run the pipeline on the
merge result, which is what this gate checks anyway.

See [A team pins each environment](../use-cases/fan-out.md) for the whole
pipeline: the comment, this gate, the record after the merge, and the scheduled
report.
