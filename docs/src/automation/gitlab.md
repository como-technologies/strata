# GitLab CI

The scripts run on GitLab as they run on GitHub. Each host call goes through one
set of functions: `gh` for GitHub, and the REST API v4 with `curl` for GitLab.

| Script | On GitHub | On GitLab |
| --- | --- | --- |
| [`pr-comment.sh`](./pr-comments.md) | one comment on the pull request, and a label | one note on the merge request, and a label |
| [`verify-report.sh`](./drift-issues.md) | one issue per record that drifted | the same, as GitLab issues |
| [`merge-gate.sh`](./merge-gate.md) | a required status check | a job that "Pipelines must succeed" waits for |

`--host github` or `--host gitlab` picks the host. Without it, the CI variables
pick: `GITLAB_CI` or `CI_PROJECT_ID` means GitLab, and everything else means
GitHub. A terminal needs `--host gitlab` when no CI variable is set.

## The pipeline

`examples/ci/gitlab/strata.yml` holds the five jobs, ready to include. Copy
it into `.gitlab/strata.yml` and include it:

```yaml
include:
  - local: .gitlab/strata.yml

variables:
  STRATA_VERSION: v0.7.1
  STRATA_A: releases/dev.json        # the record that main has
  STRATA_B: candidate.yaml           # what this merge request proposes
  STRATA_RECORDS: "releases/**/*.json"
```

| Job | Runs on | Does |
| --- | --- | --- |
| `strata-comment` | each merge request pipeline | Compares `STRATA_A` with `STRATA_B`, and keeps one note and the label. |
| `strata-merge-gate` | each merge request pipeline | Checks the merge result. Make the merge button wait for it. |
| `strata-verify` | a pipeline schedule | Verifies `STRATA_RECORDS` and keeps one issue per record that drifted. |
| `strata-promote-gate` | a pipeline that carries `STRATA_PROMOTE` | Checks that the records of a promotion are one release. Manual. |
| `strata-retire-gate` | a pipeline that carries `STRATA_RETIRE` | Checks that no cluster runs the version. Manual. |

Add the schedule in CI/CD, Schedules. Turn the merge button on the gate in
Settings, Merge requests, "Pipelines must succeed".

## The variables

| Variable | Needed for | Note |
| --- | --- | --- |
| `GITLAB_TOKEN` | the note, the label, the issues | A project or group access token with the `api` scope. **The job token cannot write notes or issues.** Mask it. |
| `PALETTE_API_KEY` | verify, and a ref that is not a file | Mask it. |
| `PALETTE_PROJECT_UID` | a ref in a project | |
| `STRATA_TOKEN` | the download | A token that can read the strata releases, while that repository is private. |
| `STRATA_PROMOTE` | the promotion gate | The records, in promotion order. Give it on the pipeline run, not as a project variable: the job appears only in a pipeline that carries it. |
| `STRATA_RETIRE` | the retire gate | The version to retire, as `name@version`. Give it on the pipeline run, as above. |

The jobs read the merge request from `CI_MERGE_REQUEST_IID`, the project from
`CI_PROJECT_ID`, and the API from `CI_API_V4_URL`, so a self-hosted GitLab needs
no setting. `--repo group/subgroup/project` names a project from a terminal.

## Install strata in a job

`install-strata.sh` downloads one release, checks every file against
`SHA256SUMS`, and prints the path of the binary:

```bash
bash install-strata.sh --version v0.7.1 --dir .strata
export STRATA_BIN=$PWD/.strata/strata
.strata/merge-gate.sh "releases/*.json"
```

Each release carries the binary for four targets, `SHA256SUMS`, and the four
scripts, so one download gives a job everything. Use **v0.5.2** or later: it is
the first release with all four. `v0.5.1` carries three of them and no
`merge-gate.sh`, and `v0.5.0` carries none, so an older pipeline checks this
repository out at the tag and runs the scripts from `scripts/`. The installer
itself is a release file: the pipeline downloads it first. A token in `STRATA_TOKEN`,
`GITHUB_TOKEN` or `GH_TOKEN` reads a private repository; a public one needs
none.

| Option | Default | Function |
| --- | --- | --- |
| `--version` | `STRATA_VERSION`, or `latest` | The release. |
| `--dir DIR` | `.strata` | Where the files land. |
| `--repo OWNER/REPO` | `STRATA_REPO` | The repository of the release. |
| `--target TARGET` | from `uname` | The binary to take. |

## What is the same, and what is not

The reports, the exit codes and the markers are the same on both hosts. A note
on a merge request carries the marker `<!-- strata -->` on its first line, and a
drift issue carries `<!-- strata-verify: <path> -->`, so each run finds what the
last run wrote.

Both scripts create the label before they add it, with a description that says
what it means, so a project that does not have it yet gets it.
