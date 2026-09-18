# Drift issues

`scripts/ci/verify-report.sh` runs `strata verify` over the records. It keeps one
open GitHub issue for each record that drifted or that strata could not check.

```bash
scripts/ci/verify-report.sh 'releases/**/*.json' 'runs/*.json'
```

| Run | Issue |
| --- | --- |
| The first run that finds a record drifted | Opens an issue with the `verify` report. |
| Each later run while the record drifts | Updates that issue. |
| The first run where `verify` passes on the record again | Closes the issue with a comment. |

The script finds the issue of a record by a marker on the first line of its
body: `<!-- strata-verify: <path> -->`. It reads only the open issues with the
label `strata-drift`. It does not touch the issues of records that you did not
give.

## Try it first

`--dry-run` prints each issue body and the action, and calls no host:

````text
{{#include ../output/edited-report.txt}}
````

## Exit status

The script exits like `verify`, after it handles the issues: `0` when each
record is unchanged, `1` when a record drifted, `2` when a record could not be
checked.

## Options

| Option | Default | Function |
| --- | --- | --- |
| `--repo OWNER/REPO` | `GITHUB_REPOSITORY`, or from `gh` | The repository, or the GitLab project. |
| `--host github\|gitlab` | from the CI variables | The code host. |
| `--label NAME` | `strata-drift` | The label of the issues. |
| `--dry-run` | | Print, and change nothing. |

The script is in this repository, at `scripts/ci/verify-report.sh`. It needs
`strata`, `jq`, the Palette API key, and the tool of the host: a `gh` with a
login, or `curl` and a `GITLAB_TOKEN`. See [GitLab CI](./gitlab.md).

> **Schedule the runs minutes apart.** GitHub lists a new or closed issue about
> five seconds later. Two runs close together can open a second issue for one
> record.

## GitHub Actions

`examples/ci/github/strata.yml` is this job and the other two in one file,
ready to copy into `.github/workflows/`.

Call the reusable workflow from a scheduled workflow. The caller sets the
schedule and needs `issues: write`:

```yaml
on:
  schedule:
    - cron: "17 3 * * *"
  workflow_dispatch:
permissions:
  contents: read
  issues: write
jobs:
  strata:
    uses: como-technologies/strata/.github/workflows/verify.yml@v0.6.0
    with:
      records: releases/**/*.json
      strata-version: v0.6.0
    secrets:
      strata-token: ${{ secrets.STRATA_READ_TOKEN }}     # while this repository is private
      palette-api-key: ${{ secrets.PALETTE_API_KEY }}
```

| Input | Required | Function |
| --- | --- | --- |
| `records` | yes | Files or patterns, separated by spaces or new lines. Strata expands the patterns. |
| `strata-version` | yes | The strata release, a tag of this repository. |
| `label` | no | The label of the issues. Default `strata-drift`. |
| `palette-api` | no | The address of a self-hosted Palette. |

The secret `palette-api-key` is required. `strata-token` reads this repository
while it is private. Another scheduler runs the script directly.

[Pull request comments](./pr-comments.md#while-this-repository-is-private)
gives the two steps for a private repository. The live example ran this
workflow with `act`: see [Live runs](../development/live-runs.md#through-the-reusable-workflow).
