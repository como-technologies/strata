# Pull request comments

`scripts/ci/pr-comment.sh A B` compares two refs and posts the report as one
comment on a pull request. When the maximum impact reaches `repave`, it adds the
label `repave`.

```bash
scripts/ci/pr-comment.sh releases/1.0.0/dev.json candidate.json
```

On the next push, the script updates the same comment. It finds the comment by
the marker `<!-- strata -->` on the first line. It removes the label when the
impact no longer reaches the threshold.

## Try it first

`--dry-run` writes the comment to standard output, writes the label decision to
standard error, and calls no host. The run below sends the comment
to a file, so the block holds the label line alone:

```text
{{#include ../output/review-pr-comment.txt}}
```

## Ask Palette if the change repaves

The label comes from the impact table, and the table is a hint. For a cluster,
Palette answers the question itself. `--repave` asks Palette, as
[`compare --repave`](../commands/compare/repave.md) does, and puts the answer in
the comment. One ref must be a cluster, and the other a profile version that
Palette holds:

```bash
scripts/ci/pr-comment.sh cluster:payments aws-stack@1.1.6 --repave
```

The comment gets the `Palette:` line of the report. The label follows the answer
of Palette, and not the table. Here the table says `may-repave`, which is below
the threshold. Palette says repave, so the script adds the label. The note on
standard error says which one decided:

```text
{{#include ../output/repave-pr-comment.txt}}
```

When Palette says no repave, the script removes the label. A change of the CSI
values is `cluster-wide` in the table, and Palette says no repave:

```text
{{#include ../output/repave-pr-comment-csi.txt}}
```

Palette also says no repave for a new Kubernetes version, where the table says
`repave`. The script removes the label, and the comment carries the note that the
two disagree:

```text
{{#include ../output/repave-pr-comment-disagree.txt}}
```

| Palette says | The label |
| --- | --- |
| repave | The script adds the label, also where the table says otherwise. |
| no repave | The script removes the label, also where the table says otherwise. A threshold below `may-repave` still reads the table. |
| nothing | The table decides, as it does without `--repave`. |

Palette says nothing when the pair has no cluster, or when the profile side has
no uid in Palette, as a candidate has none. A request to Palette that fails has
the same result. The script says why on standard error, and the table decides:

```text
{{#include ../output/repave-pr-comment-gap.txt}}
```

`--repave` needs the key of Palette, because Palette answers for a cluster. The
script asks once for each rendering of the report, so Palette gets the question
twice. The question changes nothing in Palette.

## Post a preflight report

`--preflight FILE` posts the markdown report of
[`strata preflight`](../commands/preflight.md) in place of a comparison. Give no
ref, no `--label`, no `--threshold`, and no `--repave`:

```bash
strata preflight cluster.json --cloud eks --tier static --offline --out preflight
scripts/ci/pr-comment.sh --preflight preflight/report.md
```

The comment starts with the marker `<!-- strata-preflight -->`. It is a comment
of its own, so it and the comment of a comparison do not replace each other. A
later run finds the comment by its marker and updates it. The script sets no
label. [The everyday loop](../loop.md#5-before-a-build) shows the start of one.

The script exits `2` when `FILE` is missing or empty. Then the preflight wrote
no report, and the comment of the last push stays as it is.

## Options

| Option | Default | Function |
| --- | --- | --- |
| `--pr N` | from `gh` in a checkout of the branch, or `CI_MERGE_REQUEST_IID` on GitLab | The pull request, or the merge request. |
| `--repo OWNER/REPO` | `GITHUB_REPOSITORY`, or from `gh` | The repository, or the GitLab project. |
| `--host github\|gitlab` | from the CI variables | The code host. |
| `--label NAME` | `repave` | The label. |
| `--threshold LEVEL` | `repave` | The lowest [impact](../reference/impact.md) that adds the label. |
| `--repave` | | Ask Palette if the change repaves the cluster, and let its answer decide the label. |
| `--preflight FILE` | | Post the markdown report of `strata preflight` in `FILE`, in place of a comparison. Give no ref and none of the three options above. |
| `--dry-run` | | Print, and change nothing. |

The script is in this repository, at `scripts/ci/pr-comment.sh`. It needs `strata`
(or `STRATA_BIN`), `jq`, and the tool of the host: a `gh` with a login, or
`curl` and a `GITLAB_TOKEN`. See [GitLab CI](./gitlab.md). A ref that is not a file gets
one snapshot, so Palette gets one set of requests. Run from a terminal, the
script posts the same comment as the job.

## Exit status

| Exit | Meaning |
| --- | --- |
| `0` | The comment is posted or updated, and the label is set. A dry run also exits `0`. |
| `2` | The script could not run, or strata could not answer. |

A host call that fails ends the script with the status of that call.

## GitHub Actions

`examples/ci/github/strata.yml` is this job and the other four in one file,
ready to copy into `.github/workflows/`.

Call the reusable workflow. The caller needs `pull-requests: write`:

```yaml
on: pull_request
permissions:
  contents: read
  pull-requests: write
jobs:
  strata:
    uses: como-technologies/strata/.github/workflows/comment.yml@v0.24.0
    with:
      a: releases/1.1.0/dev.json
      b: releases/1.1.1/dev.json
      strata-version: v0.24.0
    secrets:
      strata-token: ${{ secrets.STRATA_READ_TOKEN }}     # while this repository is private
      palette-api-key: ${{ secrets.PALETTE_API_KEY }}   # only when a ref is not a snapshot file
```

| Input | Required | Function |
| --- | --- | --- |
| `a`, `b` | yes | The two refs, in the checkout of the caller. |
| `strata-version` | yes | The strata release, a tag of this repository. |
| `label` | no | The label. Default `repave`. |
| `threshold` | no | The impact that earns the label. Default `repave`. |
| `repave` | no | Ask Palette if the change repaves the cluster. Default `false`. |
| `palette-api` | no | A self-hosted Palette address. |

| Secret | Needed for |
| --- | --- |
| `strata-token` | A read token for this repository, while it is private. |
| `palette-api-key` | A ref that is not a snapshot file, and `repave`. |
| `palette-project-uid` | A ref that is not a snapshot file, in a project, and `repave`. |

The workflow downloads the release binary, checks the signature of `SHA256SUMS`,
checks the binary against it, and runs the script with the token of the caller.
[The reusable workflows](../install.md#the-reusable-workflows) tell what the
signature check needs. Another CI system installs strata
and runs the script.

## While this repository is private

A workflow of another repository can call this workflow only after two steps:

1. In this repository, open Settings, Actions, General, Access. Select
   "Accessible from repositories in the organization".
2. Give `strata-token`: a fine-grained token or a GitHub App token that can read
   the contents of this repository. The token of the caller cannot read another
   private repository, so without it the checkout and the download fail.
