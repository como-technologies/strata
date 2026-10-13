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

## Options

| Option | Default | Function |
| --- | --- | --- |
| `--pr N` | from `gh` in a checkout of the branch, or `CI_MERGE_REQUEST_IID` on GitLab | The pull request, or the merge request. |
| `--repo OWNER/REPO` | `GITHUB_REPOSITORY`, or from `gh` | The repository, or the GitLab project. |
| `--host github\|gitlab` | from the CI variables | The code host. |
| `--label NAME` | `repave` | The label. |
| `--threshold LEVEL` | `repave` | The lowest [impact](../reference/impact.md) that adds the label. |
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
    uses: como-technologies/strata/.github/workflows/comment.yml@v0.22.0
    with:
      a: releases/1.1.0/dev.json
      b: releases/1.1.1/dev.json
      strata-version: v0.22.0
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
| `palette-api` | no | A self-hosted Palette address. |

| Secret | Needed for |
| --- | --- |
| `strata-token` | A read token for this repository, while it is private. |
| `palette-api-key` | A ref that is not a snapshot file. |
| `palette-project-uid` | A ref that is not a snapshot file, in a project. |

The workflow downloads the release binary, checks it against `SHA256SUMS`, and
runs the script with the token of the caller. Another CI system installs strata
and runs the script.

## While this repository is private

A workflow of another repository can call this workflow only after two steps:

1. In this repository, open Settings, Actions, General, Access. Select
   "Accessible from repositories in the organization".
2. Give `strata-token`: a fine-grained token or a GitHub App token that can read
   the contents of this repository. The token of the caller cannot read another
   private repository, so without it the checkout and the download fail.
